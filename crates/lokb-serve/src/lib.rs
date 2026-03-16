//! HTTP API server and MCP server for lokb.

pub mod mcp;

use axum::{
    Router,
    extract::{Path, Query},
    http::StatusCode,
    response::Json,
    routing::get,
};
use lokb_core::config;
use lokb_storage::{FileContentStore, SqliteCatalog};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

struct AppState {
    catalog: SqliteCatalog,
    content_store: FileContentStore,
    fts: lokb_search::TantivyIndex,
}

/// Create the axum router.
pub fn create_router() -> Router {
    let catalog_path = config::derived_dir().join("catalog.sqlite");
    let fts_path = config::derived_dir().join("fts");

    let state = Arc::new(AppState {
        catalog: SqliteCatalog::open(&catalog_path).expect("failed to open catalog"),
        content_store: FileContentStore::new(config::source_content_dir().as_ref()),
        fts: lokb_search::TantivyIndex::open(&fts_path).expect("failed to open FTS index"),
    });

    Router::new()
        .route("/api/search", get(search_handler))
        .route("/api/sources", get(sources_handler))
        .route("/api/entity/{name}", get(entity_handler))
        .route("/api/status", get(status_handler))
        .route("/api/read/{source}/{doc_id}", get(read_handler))
        .with_state(state)
}

/// Start the HTTP server.
pub async fn serve(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let app = create_router();
    let addr = format!("0.0.0.0:{port}");
    eprintln!("lokb server listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

// ── Handlers ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    limit: Option<usize>,
    source: Option<String>,
}

#[derive(Serialize)]
struct SearchResponse {
    query: String,
    results: Vec<SearchHit>,
}

#[derive(Serialize)]
struct SearchHit {
    title: String,
    source: String,
    snippet: String,
    score: f32,
}

async fn search_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Query(params): Query<SearchQuery>,
) -> Json<SearchResponse> {
    let limit = params.limit.unwrap_or(20);
    let hits = state
        .fts
        .search(&params.q, limit, params.source.as_deref(), false, false)
        .unwrap_or_default();

    Json(SearchResponse {
        query: params.q,
        results: hits
            .into_iter()
            .map(|h| SearchHit {
                title: h.title,
                source: h.source_name,
                snippet: h.text[..h.text.len().min(200)].to_string(),
                score: h.score,
            })
            .collect(),
    })
}

#[derive(Serialize)]
struct SourceInfo {
    name: String,
    format: String,
    class: String,
    document_count: u64,
}

async fn sources_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> Json<Vec<SourceInfo>> {
    let sources = state.catalog.list_sources().unwrap_or_default();
    Json(
        sources
            .into_iter()
            .map(|s| SourceInfo {
                name: s.name,
                format: s.format,
                class: if s.class.is_public() {
                    "public".into()
                } else {
                    "personal".into()
                },
                document_count: s.document_count,
            })
            .collect(),
    )
}

async fn entity_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let entity = state
        .catalog
        .get_entity_by_name(&name)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match entity {
        Some(e) => {
            let relations = state.catalog.get_relations(&e.id).unwrap_or_default();
            let documents = state
                .catalog
                .get_entity_documents(&e.id)
                .unwrap_or_default();
            Ok(Json(serde_json::json!({
                "name": e.canonical_name,
                "description": e.description,
                "mention_count": e.mention_count,
                "relations": relations,
                "documents": documents,
            })))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

#[derive(Serialize)]
struct StatusResponse {
    sources: usize,
    entities: u64,
    relations: u64,
}

async fn status_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> Json<StatusResponse> {
    let sources = state.catalog.list_sources().unwrap_or_default();
    let entities = state.catalog.entity_count().unwrap_or(0);
    let relations = state.catalog.relation_count().unwrap_or(0);
    Json(StatusResponse {
        sources: sources.len(),
        entities,
        relations,
    })
}

async fn read_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Path((source, doc_id)): Path<(String, String)>,
) -> Result<String, StatusCode> {
    state
        .content_store
        .read_by_filename(&source, &doc_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)
}
