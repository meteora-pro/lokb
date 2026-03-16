//! Shared tool functions for CLI, MCP Server, and HTTP API.
//!
//! Each tool is a pure function: typed input → typed output.
//! Transport layers (CLI, MCP, HTTP) call these and handle formatting.

use serde::{Deserialize, Serialize};

mod context;
pub use context::ToolContext;

// ── Tool Inputs/Outputs ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SearchInput {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    pub source: Option<String>,
    #[serde(default)]
    pub personal_only: bool,
    #[serde(default)]
    pub public_only: bool,
}

fn default_limit() -> usize {
    20
}

#[derive(Debug, Serialize)]
pub struct SearchOutput {
    pub query: String,
    pub results: Vec<SearchHit>,
}

#[derive(Debug, Serialize)]
pub struct SearchHit {
    pub title: String,
    pub source: String,
    pub snippet: String,
    pub score: f64,
}

#[derive(Debug, Deserialize)]
pub struct ReadInput {
    pub source: String,
    pub doc_id: String,
}

#[derive(Debug, Serialize)]
pub struct ReadOutput {
    pub source: String,
    pub doc_id: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct EntityInput {
    pub name: String,
    #[serde(default)]
    pub relations: bool,
    #[serde(default)]
    pub documents: bool,
}

#[derive(Debug, Serialize)]
pub struct EntityOutput {
    pub found: bool,
    pub entity: Option<EntityCard>,
    pub suggestions: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct EntityCard {
    pub name: String,
    pub description: Option<String>,
    pub mention_count: i64,
    pub relations: Vec<lokb_storage::catalog::RelationInfo>,
    pub documents: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SourceListOutput {
    pub sources: Vec<SourceInfo>,
}

#[derive(Debug, Serialize)]
pub struct SourceInfo {
    pub name: String,
    pub format: String,
    pub class: String,
    pub document_count: u64,
}

#[derive(Debug, Serialize)]
pub struct StatusOutput {
    pub sources: usize,
    pub entities: u64,
    pub relations: u64,
    pub total_bytes: u64,
}

// ── Tool Functions ───────────────────────────────────────────────────

/// Search across all sources (FTS).
pub fn tool_search(ctx: &ToolContext, input: &SearchInput) -> Result<SearchOutput, String> {
    let hits = ctx
        .fts
        .search(
            &input.query,
            input.limit,
            input.source.as_deref(),
            input.personal_only,
            input.public_only,
        )
        .map_err(|e| e.to_string())?;

    Ok(SearchOutput {
        query: input.query.clone(),
        results: hits
            .into_iter()
            .map(|h| SearchHit {
                title: h.title,
                source: h.source_name,
                snippet: h.text[..h.text.len().min(300)].to_string(),
                score: h.score as f64,
            })
            .collect(),
    })
}

/// Read a document.
pub fn tool_read(ctx: &ToolContext, input: &ReadInput) -> Result<ReadOutput, String> {
    let content = ctx
        .content_store
        .read_by_filename(&input.source, &input.doc_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| {
            format!(
                "document '{}' not found in source '{}'",
                input.doc_id, input.source
            )
        })?;

    Ok(ReadOutput {
        source: input.source.clone(),
        doc_id: input.doc_id.clone(),
        content,
    })
}

/// Entity lookup.
pub fn tool_entity(ctx: &ToolContext, input: &EntityInput) -> Result<EntityOutput, String> {
    let entity = ctx
        .catalog
        .get_entity_by_name(&input.name)
        .map_err(|e| e.to_string())?;

    match entity {
        Some(info) => {
            let relations = if input.relations {
                ctx.catalog
                    .get_relations(&info.id)
                    .map_err(|e| e.to_string())?
            } else {
                vec![]
            };
            let documents = if input.documents {
                ctx.catalog
                    .get_entity_documents(&info.id)
                    .map_err(|e| e.to_string())?
            } else {
                vec![]
            };

            Ok(EntityOutput {
                found: true,
                entity: Some(EntityCard {
                    name: info.canonical_name,
                    description: info.description,
                    mention_count: info.mention_count,
                    relations,
                    documents,
                }),
                suggestions: vec![],
            })
        }
        None => {
            let suggestions = ctx
                .catalog
                .search_entities(&input.name, 5)
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|e| e.canonical_name)
                .collect();

            Ok(EntityOutput {
                found: false,
                entity: None,
                suggestions,
            })
        }
    }
}

/// List sources.
pub fn tool_sources(ctx: &ToolContext) -> Result<SourceListOutput, String> {
    let sources = ctx.catalog.list_sources().map_err(|e| e.to_string())?;
    Ok(SourceListOutput {
        sources: sources
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
    })
}

/// Status overview.
pub fn tool_status(ctx: &ToolContext) -> Result<StatusOutput, String> {
    let sources = ctx.catalog.list_sources().map_err(|e| e.to_string())?;
    let entities = ctx.catalog.entity_count().map_err(|e| e.to_string())?;
    let relations = ctx.catalog.relation_count().map_err(|e| e.to_string())?;

    Ok(StatusOutput {
        sources: sources.len(),
        entities,
        relations,
        total_bytes: 0, // simplified for MCP
    })
}

/// List available tools (for MCP tools/list).
pub fn available_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "search".into(),
            description: "Search the knowledge base using full-text search".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query"},
                    "limit": {"type": "integer", "description": "Max results (default 20)"},
                    "source": {"type": "string", "description": "Filter by source name"},
                    "personal_only": {"type": "boolean"},
                    "public_only": {"type": "boolean"}
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "read".into(),
            description: "Read a document by source and document ID".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "source": {"type": "string", "description": "Source name"},
                    "doc_id": {"type": "string", "description": "Document ID"}
                },
                "required": ["source", "doc_id"]
            }),
        },
        ToolDefinition {
            name: "entity".into(),
            description: "Look up an entity in the knowledge graph".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Entity name"},
                    "relations": {"type": "boolean", "description": "Include relations"},
                    "documents": {"type": "boolean", "description": "Include mentioning documents"}
                },
                "required": ["name"]
            }),
        },
        ToolDefinition {
            name: "sources".into(),
            description: "List all data sources".into(),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
        },
        ToolDefinition {
            name: "status".into(),
            description: "Get knowledge base status (source count, entities, relations)".into(),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
        },
    ]
}

#[derive(Debug, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub parameters: serde_json::Value,
}
