use chrono::Utc;
use lokb_core::config;
use lokb_core::{
    ContentHash, ContentType, DataSource, DataSourceClass, PrivacyLevel, RawRetention, SyncStrategy,
};
use lokb_ingest::SemanticChunker;
use lokb_pipeline::Chunker;
use lokb_search::TantivyIndex;
use lokb_storage::{FileContentStore, SqliteCatalog};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use uuid::Uuid;

fn catalog_path() -> PathBuf {
    config::derived_dir().join("catalog.sqlite")
}

fn fts_path() -> PathBuf {
    config::derived_dir().join("fts")
}

fn open_catalog() -> io::Result<SqliteCatalog> {
    let path = catalog_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    SqliteCatalog::open(&path).map_err(|e| io::Error::other(e.to_string()))
}

fn open_content_store() -> FileContentStore {
    FileContentStore::new(config::source_content_dir().as_ref())
}

fn open_fts() -> io::Result<TantivyIndex> {
    TantivyIndex::open(&fts_path()).map_err(|e| io::Error::other(e.to_string()))
}

/// Metrics collected during source ingestion (ADR-002, ADR-003).
#[derive(Debug, Serialize)]
pub struct IngestMetrics {
    /// Total raw input size in bytes
    pub raw_input_bytes: u64,
    /// Total optimized output size in bytes
    pub optimized_bytes: u64,
    /// Compression ratio (raw / optimized)
    pub compression_ratio: f64,
    /// Number of documents processed
    pub documents_processed: u64,
    /// Number of chunks created
    pub chunks_created: u64,
    /// FTS index size after ingestion in bytes
    pub fts_index_bytes: u64,
    /// Total storage overhead from enrichment (FTS + catalog)
    pub enrichment_overhead_bytes: u64,
    /// Wall-clock time for optimize phase (ms)
    pub optimize_time_ms: u64,
    /// Wall-clock time for enrichment phase (ms)
    pub enrichment_time_ms: u64,
    /// Total wall-clock time (ms)
    pub total_time_ms: u64,
}

/// Save source config and ingest raw data. Returns metrics.
pub fn add_source(name: &str, raw: &str, format: &str, class: &str) -> io::Result<IngestMetrics> {
    use std::time::Instant;
    let total_start = Instant::now();

    config::init_dirs()?;

    let raw_path = PathBuf::from(raw);
    if !raw_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("raw path not found: {}", raw),
        ));
    }

    let raw_input_bytes = dir_size(&raw_path);

    let catalog = open_catalog()?;
    let content_store = open_content_store();

    // Check if source already exists (ADR-006: reject, suggest update)
    let existing = catalog
        .get_source(name)
        .map_err(|e| io::Error::other(e.to_string()))?;
    if existing.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("source '{}' already exists", name),
        ));
    }

    let source_id = Uuid::now_v7();
    let ds_class = match class {
        "public" => DataSourceClass::Public {
            license: None,
            web_url_template: None,
        },
        "personal" => DataSourceClass::Personal {
            owner: None,
            platform: None,
            contains_pii: true,
        },
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unknown class: {class}. Use 'public' or 'personal'"),
            ));
        }
    };

    // Register source first (documents reference it via FK)
    let source = DataSource {
        id: source_id,
        name: name.to_string(),
        class: ds_class,
        format: format.to_string(),
        sync_strategy: SyncStrategy::default(),
        raw_retention: RawRetention::default(),
        priority: if class == "personal" { 250 } else { 100 },
        document_count: 0,
        created_at: Utc::now(),
    };
    catalog
        .add_source(&source)
        .map_err(|e| io::Error::other(e.to_string()))?;

    // Phase 1: Optimize — ingest raw files into content store
    let optimize_start = Instant::now();
    let doc_count = ingest_raw(&raw_path, format, name, &content_store, source_id, &catalog)?;
    let optimize_time = optimize_start.elapsed();

    // Update document count
    catalog
        .update_source_doc_count(source_id, doc_count)
        .map_err(|e| io::Error::other(e.to_string()))?;

    let optimized_bytes = content_store.source_size(name);

    // Phase 2: Enrichment — chunk and build FTS index (single commit)
    let enrichment_start = std::time::Instant::now();
    let fts = open_fts()?;
    let chunker = SemanticChunker::default();
    let mut fts_writer = fts.writer().map_err(|e| io::Error::other(e.to_string()))?;

    let files = content_store
        .list_files(name)
        .map_err(|e| io::Error::other(e.to_string()))?;

    for (filename, content) in &files {
        let title = extract_doc_title(content, filename);
        let doc = lokb_core::Document {
            id: Uuid::now_v7(),
            source_id,
            external_id: filename.clone(),
            parent_id: None,
            depth: 0,
            title: title.clone(),
            content_type: if format == "telegram-export" {
                ContentType::Conversation
            } else {
                ContentType::Article
            },
            language: None,
            content_hash: ContentHash::from_bytes(content.as_bytes()),
            content_size: content.len() as u64,
            created_at: Utc::now(),
            indexed_at: Utc::now(),
            privacy_level: if class == "personal" {
                PrivacyLevel::Private
            } else {
                PrivacyLevel::Public
            },
        };
        let chunks = chunker
            .chunk(&doc, content)
            .map_err(|e| io::Error::other(e.to_string()))?;
        fts_writer
            .add_chunks(&chunks, name, &title)
            .map_err(|e| io::Error::other(e.to_string()))?;
    }

    let chunks_created = fts_writer
        .commit()
        .map_err(|e| io::Error::other(e.to_string()))? as u64;
    let enrichment_time = enrichment_start.elapsed();

    let fts_index_bytes = dir_size(&fts_path());
    let total_time = total_start.elapsed();

    Ok(IngestMetrics {
        raw_input_bytes,
        optimized_bytes,
        compression_ratio: if optimized_bytes > 0 {
            raw_input_bytes as f64 / optimized_bytes as f64
        } else {
            0.0
        },
        documents_processed: doc_count,
        chunks_created,
        fts_index_bytes,
        enrichment_overhead_bytes: fts_index_bytes + dir_size(&catalog_path()),
        optimize_time_ms: optimize_time.as_millis() as u64,
        enrichment_time_ms: enrichment_time.as_millis() as u64,
        total_time_ms: total_time.as_millis() as u64,
    })
}

/// Report from incremental update (ADR-006).
pub struct UpdateReport {
    pub new_count: u64,
    pub changed_count: u64,
    pub unchanged_count: u64,
    pub deleted_count: u64,
}

/// Incremental update of an existing source (ADR-006).
/// Compares content hashes, only processes new/changed documents.
pub fn update_source(name: &str, raw: &str) -> io::Result<UpdateReport> {
    config::init_dirs()?;

    let raw_path = PathBuf::from(raw);
    if !raw_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("raw path not found: {}", raw),
        ));
    }

    let catalog = open_catalog()?;
    let content_store = open_content_store();
    let fts = open_fts()?;

    let source = catalog
        .get_source(name)
        .map_err(|e| io::Error::other(e.to_string()))?
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("source '{}' not found. Use 'source add' first.", name),
            )
        })?;

    // Collect new raw files with their content hashes
    let new_files = scan_raw_files(&raw_path, &source.format)?;

    // Get existing external_ids from catalog
    let existing_ids: std::collections::HashSet<String> = catalog
        .list_external_ids(source.id)
        .map_err(|e| io::Error::other(e.to_string()))?
        .into_iter()
        .collect();

    let chunker = SemanticChunker::default();
    let ctx = IngestContext {
        source_name: name,
        source: &source,
        content_store: &content_store,
        catalog: &catalog,
        fts: &fts,
        chunker: &chunker,
    };

    let mut new_count = 0u64;
    let mut changed_count = 0u64;
    let mut unchanged_count = 0u64;

    let seen_ids: std::collections::HashSet<String> =
        new_files.iter().map(|f| f.external_id.clone()).collect();

    for file_info in &new_files {
        let existing_hash = catalog
            .get_content_hash(source.id, &file_info.external_id)
            .map_err(|e| io::Error::other(e.to_string()))?;

        let new_hash = ContentHash::from_bytes(file_info.content.as_bytes());

        match existing_hash {
            None => {
                // New document
                ingest_single_file(
                    &file_info.external_id,
                    &file_info.filename,
                    &file_info.content,
                    &ctx,
                )?;
                new_count += 1;
            }
            Some(old_hash) if old_hash != new_hash.0.as_slice() => {
                // Changed document — re-ingest
                ingest_single_file(
                    &file_info.external_id,
                    &file_info.filename,
                    &file_info.content,
                    &ctx,
                )?;
                changed_count += 1;
            }
            _ => {
                unchanged_count += 1;
            }
        }
    }

    // Detect deleted documents
    let deleted_count = existing_ids.difference(&seen_ids).count() as u64;
    for deleted_id in existing_ids.difference(&seen_ids) {
        catalog
            .delete_by_external_id(source.id, deleted_id)
            .map_err(|e| io::Error::other(e.to_string()))?;
    }

    // Update document count
    let total = catalog
        .document_count(source.id)
        .map_err(|e| io::Error::other(e.to_string()))?;
    catalog
        .update_source_doc_count(source.id, total)
        .map_err(|e| io::Error::other(e.to_string()))?;

    Ok(UpdateReport {
        new_count,
        changed_count,
        unchanged_count,
        deleted_count,
    })
}

struct RawFileInfo {
    external_id: String,
    filename: String,
    content: String,
}

/// Scan raw files and return their external_ids + content.
/// Scan directory for files with given extensions, optionally transform content.
fn scan_dir_files(
    raw_path: &Path,
    extensions: &[&str],
    transform: Option<fn(&str) -> String>,
) -> io::Result<Vec<RawFileInfo>> {
    let mut files = vec![];
    for entry in fs::read_dir(raw_path)? {
        let entry = entry?;
        let path = entry.path();
        let ext_match = path
            .extension()
            .is_some_and(|ext| extensions.iter().any(|e| ext == *e));
        if !ext_match {
            continue;
        }
        let filename = path.file_name().unwrap().to_string_lossy().to_string();
        let mut external_id = filename.clone();
        for ext in extensions {
            external_id = external_id.trim_end_matches(&format!(".{ext}")).to_string();
        }
        let raw_content = fs::read_to_string(&path)?;
        let content = match transform {
            Some(f) => f(&raw_content),
            None => raw_content,
        };
        let store_filename = if transform.is_some() {
            format!("{external_id}.md")
        } else {
            filename
        };
        files.push(RawFileInfo {
            external_id,
            filename: store_filename,
            content,
        });
    }
    Ok(files)
}

fn scan_raw_files(raw_path: &Path, format: &str) -> io::Result<Vec<RawFileInfo>> {
    match format {
        "markdown-dir" => scan_dir_files(raw_path, &["md"], None),
        "plaintext-dir" => scan_dir_files(raw_path, &["txt"], None),
        "html-dir" => scan_dir_files(
            raw_path,
            &["html", "htm"],
            Some(lokb_parsers::html::html_to_markdown),
        ),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("incremental update not supported for format: {format}"),
        )),
    }
}

struct IngestContext<'a> {
    source_name: &'a str,
    source: &'a DataSource,
    content_store: &'a FileContentStore,
    catalog: &'a SqliteCatalog,
    fts: &'a TantivyIndex,
    chunker: &'a SemanticChunker,
}

/// Ingest a single file: write to content store, register in catalog, chunk, index.
fn ingest_single_file(
    external_id: &str,
    filename: &str,
    content: &str,
    ctx: &IngestContext<'_>,
) -> io::Result<()> {
    let title = extract_doc_title(content, filename);

    ctx.content_store
        .write_file(ctx.source_name, filename, content)
        .map_err(|e| io::Error::other(e.to_string()))?;

    let doc = lokb_core::Document {
        id: Uuid::now_v7(),
        source_id: ctx.source.id,
        external_id: external_id.to_string(),
        parent_id: None,
        depth: 0,
        title: title.clone(),
        content_type: ContentType::Article,
        language: Some("en".to_string()),
        content_hash: ContentHash::from_bytes(content.as_bytes()),
        content_size: content.len() as u64,
        created_at: Utc::now(),
        indexed_at: Utc::now(),
        privacy_level: if ctx.source.class.is_personal() {
            PrivacyLevel::Private
        } else {
            PrivacyLevel::Public
        },
    };

    ctx.catalog
        .upsert_document(&doc)
        .map_err(|e| io::Error::other(e.to_string()))?;

    let chunks = ctx
        .chunker
        .chunk(&doc, content)
        .map_err(|e| io::Error::other(e.to_string()))?;
    ctx.fts
        .index_chunks(&chunks, ctx.source_name, &title)
        .map_err(|e| io::Error::other(e.to_string()))?;

    Ok(())
}

/// Ingest raw files. Returns document count.
fn ingest_raw(
    raw_path: &Path,
    format: &str,
    source_name: &str,
    content_store: &FileContentStore,
    source_id: Uuid,
    catalog: &SqliteCatalog,
) -> io::Result<u64> {
    match format {
        "markdown-dir" => ingest_text_dir(
            raw_path,
            &["md"],
            None,
            source_name,
            content_store,
            source_id,
            catalog,
        ),
        "telegram-export" => {
            ingest_telegram(raw_path, source_name, content_store, source_id, catalog)
        }
        "plaintext-dir" => ingest_text_dir(
            raw_path,
            &["txt"],
            None,
            source_name,
            content_store,
            source_id,
            catalog,
        ),
        "html-dir" => ingest_text_dir(
            raw_path,
            &["html", "htm"],
            Some(lokb_parsers::html::html_to_markdown),
            source_name,
            content_store,
            source_id,
            catalog,
        ),
        "epub" => ingest_epub(raw_path, source_name, content_store, source_id, catalog),
        "pdf-dir" => ingest_pdf_dir(raw_path, source_name, content_store, source_id, catalog),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported format: {}", format),
        )),
    }
}

/// Generic text directory ingestion: read files, optionally transform, store and register.
#[allow(clippy::too_many_arguments)]
fn ingest_text_dir(
    raw_path: &Path,
    extensions: &[&str],
    transform: Option<fn(&str) -> String>,
    source_name: &str,
    content_store: &FileContentStore,
    source_id: Uuid,
    catalog: &SqliteCatalog,
) -> io::Result<u64> {
    let mut count = 0;
    for entry in fs::read_dir(raw_path)? {
        let entry = entry?;
        let path = entry.path();
        let ext_match = path
            .extension()
            .is_some_and(|ext| extensions.iter().any(|e| ext == *e));
        if !ext_match {
            continue;
        }

        let raw_content = fs::read_to_string(&path)?;
        let content = match transform {
            Some(f) => f(&raw_content),
            None => raw_content,
        };

        let filename = path.file_name().unwrap().to_string_lossy().to_string();
        let mut external_id = filename.clone();
        for ext in extensions {
            external_id = external_id.trim_end_matches(&format!(".{ext}")).to_string();
        }
        let store_filename = if transform.is_some() {
            format!("{external_id}.md")
        } else {
            filename
        };
        let title = extract_doc_title(&content, &store_filename);

        content_store
            .write_file(source_name, &store_filename, &content)
            .map_err(|e| io::Error::other(e.to_string()))?;

        let doc = lokb_core::Document {
            id: Uuid::now_v7(),
            source_id,
            external_id,
            parent_id: None,
            depth: 0,
            title,
            content_type: ContentType::Article,
            language: Some("en".to_string()),
            content_hash: ContentHash::from_bytes(content.as_bytes()),
            content_size: content.len() as u64,
            created_at: Utc::now(),
            indexed_at: Utc::now(),
            privacy_level: PrivacyLevel::Public,
        };
        catalog
            .upsert_document(&doc)
            .map_err(|e| io::Error::other(e.to_string()))?;

        count += 1;
    }
    Ok(count)
}

/// Parse Telegram export JSON and store messages as text documents.
/// Extract chapters from EPUB and register in catalog.
/// Extract text from PDF files in a directory.
fn ingest_pdf_dir(
    raw_path: &Path,
    source_name: &str,
    content_store: &FileContentStore,
    source_id: Uuid,
    catalog: &SqliteCatalog,
) -> io::Result<u64> {
    let mut count = 0;
    for entry in fs::read_dir(raw_path)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "pdf") {
            continue;
        }

        let filename = path.file_name().unwrap().to_string_lossy().to_string();
        let external_id = filename.trim_end_matches(".pdf").to_string();

        match lokb_parsers::extract_pdf(&path, source_id, &external_id) {
            Ok((doc, text)) => {
                let md_filename = format!("{external_id}.md");
                content_store
                    .write_file(source_name, &md_filename, &text)
                    .map_err(|e| io::Error::other(e.to_string()))?;
                catalog
                    .upsert_document(&doc)
                    .map_err(|e| io::Error::other(e.to_string()))?;
                count += 1;
            }
            Err(e) => {
                eprintln!("Warning: skipping {filename}: {e}");
            }
        }
    }
    Ok(count)
}

fn ingest_epub(
    raw_path: &Path,
    source_name: &str,
    content_store: &FileContentStore,
    source_id: Uuid,
    catalog: &SqliteCatalog,
) -> io::Result<u64> {
    let chapters = lokb_parsers::extract_epub(raw_path, source_id)
        .map_err(|e| io::Error::other(e.to_string()))?;

    let mut count = 0;
    for (doc, text) in &chapters {
        if text.is_empty() && doc.depth == 0 {
            // Root document — register in catalog but no content file
            catalog
                .upsert_document(doc)
                .map_err(|e| io::Error::other(e.to_string()))?;
            continue;
        }
        let filename = format!("{}.md", doc.external_id);
        content_store
            .write_file(source_name, &filename, text)
            .map_err(|e| io::Error::other(e.to_string()))?;
        catalog
            .upsert_document(doc)
            .map_err(|e| io::Error::other(e.to_string()))?;
        count += 1;
    }
    Ok(count)
}

fn ingest_telegram(
    raw_path: &Path,
    source_name: &str,
    content_store: &FileContentStore,
    source_id: Uuid,
    catalog: &SqliteCatalog,
) -> io::Result<u64> {
    let json_path = raw_path.join("result.json");
    if !json_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "result.json not found in telegram export directory",
        ));
    }

    let raw_content = fs::read_to_string(&json_path)?;
    let export: TelegramExport = serde_json::from_str(&raw_content)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let chat_name = &export.name;
    let mut segments: Vec<Vec<&TelegramMessage>> = vec![];
    let mut current_segment: Vec<&TelegramMessage> = vec![];

    for msg in &export.messages {
        if msg.r#type != "message" {
            continue;
        }
        if let Some(last) = current_segment.last()
            && time_gap_hours(&last.date, &msg.date) > 2.0
            && !current_segment.is_empty()
        {
            segments.push(current_segment);
            current_segment = vec![];
        }
        current_segment.push(msg);
    }
    if !current_segment.is_empty() {
        segments.push(current_segment);
    }

    let mut count = 0;
    for (i, segment) in segments.iter().enumerate() {
        let mut text = format!("[{}]\n", chat_name);
        for msg in segment {
            let from = msg.from.as_deref().unwrap_or("Unknown");
            let time = msg.date.split('T').next_back().unwrap_or(&msg.date);
            let msg_text = extract_text(&msg.text);
            text.push_str(&format!("{} [{}]: {}\n", from, time, msg_text));
        }
        let filename = format!("segment_{:04}.txt", i);
        let external_id = format!("segment_{:04}", i);

        content_store
            .write_file(source_name, &filename, &text)
            .map_err(|e| io::Error::other(e.to_string()))?;

        let doc = lokb_core::Document {
            id: Uuid::now_v7(),
            source_id,
            external_id,
            parent_id: None,
            depth: 0,
            title: format!("{} segment {}", chat_name, i),
            content_type: ContentType::Conversation,
            language: None,
            content_hash: ContentHash::from_bytes(text.as_bytes()),
            content_size: text.len() as u64,
            created_at: Utc::now(),
            indexed_at: Utc::now(),
            privacy_level: PrivacyLevel::Private,
        };
        catalog
            .upsert_document(&doc)
            .map_err(|e| io::Error::other(e.to_string()))?;

        count += 1;
    }

    Ok(count)
}

fn time_gap_hours(a: &str, b: &str) -> f64 {
    let parse = |s: &str| -> Option<f64> {
        let parts: Vec<&str> = s.split('T').collect();
        if parts.len() != 2 {
            return None;
        }
        let time_parts: Vec<&str> = parts[1].split(':').collect();
        if time_parts.len() < 2 {
            return None;
        }
        let date_parts: Vec<&str> = parts[0].split('-').collect();
        if date_parts.len() < 3 {
            return None;
        }
        let day: f64 = date_parts[2].parse().ok()?;
        let hours: f64 = time_parts[0].parse().ok()?;
        let minutes: f64 = time_parts[1].parse().ok()?;
        Some(day * 24.0 + hours + minutes / 60.0)
    };

    match (parse(a), parse(b)) {
        (Some(a), Some(b)) => (b - a).abs(),
        _ => 0.0,
    }
}

fn extract_text(text: &serde_json::Value) -> String {
    match text {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(parts) => parts
            .iter()
            .map(|p| match p {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Object(obj) => obj
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                _ => String::new(),
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

#[derive(Deserialize)]
struct TelegramExport {
    name: String,
    messages: Vec<TelegramMessage>,
}

#[derive(Deserialize)]
struct TelegramMessage {
    r#type: String,
    date: String,
    from: Option<String>,
    text: serde_json::Value,
}

/// Detailed status of a single source.
#[derive(Debug, Serialize)]
pub struct SourceStatus {
    pub name: String,
    pub format: String,
    pub class: String,
    pub document_count: u64,
    pub content_bytes: u64,
    pub created_at: String,
}

pub fn source_status(name: &str) -> io::Result<SourceStatus> {
    let catalog = open_catalog()?;
    let content_store = open_content_store();

    let source = catalog
        .get_source(name)
        .map_err(|e| io::Error::other(e.to_string()))?
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("source '{name}' not found"),
            )
        })?;

    Ok(SourceStatus {
        name: source.name.clone(),
        format: source.format.clone(),
        class: if source.class.is_public() {
            "public".to_string()
        } else {
            "personal".to_string()
        },
        document_count: source.document_count,
        content_bytes: content_store.source_size(&source.name),
        created_at: source.created_at.to_rfc3339(),
    })
}

/// Delete a source: catalog entries, content files, FTS index entries.
pub fn delete_source(name: &str) -> io::Result<()> {
    let catalog = open_catalog()?;
    let content_store = open_content_store();

    let source = catalog
        .get_source(name)
        .map_err(|e| io::Error::other(e.to_string()))?
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("source '{name}' not found"),
            )
        })?;

    // Delete FTS entries for this source
    let fts = open_fts()?;
    fts.delete_source(&source.id.to_string())
        .map_err(|e| io::Error::other(e.to_string()))?;

    // Delete content files
    content_store
        .delete_source(name)
        .map_err(|e| io::Error::other(e.to_string()))?;

    // Delete from catalog (documents + source)
    catalog
        .delete_source(source.id)
        .map_err(|e| io::Error::other(e.to_string()))?;

    Ok(())
}

/// List all configured sources (from SQLite catalog).
pub fn list_sources() -> io::Result<Vec<SourceListItem>> {
    let catalog = open_catalog()?;
    let sources = catalog
        .list_sources()
        .map_err(|e| io::Error::other(e.to_string()))?;
    Ok(sources
        .into_iter()
        .map(|s| SourceListItem {
            name: s.name,
            format: s.format,
            class: if s.class.is_public() {
                "public".to_string()
            } else {
                "personal".to_string()
            },
            document_count: s.document_count,
        })
        .collect())
}

/// Source info for JSON output (backward compatible with E2E tests).
#[derive(Debug, Serialize)]
pub struct SourceListItem {
    pub name: String,
    pub format: String,
    pub class: String,
    pub document_count: u64,
}

/// Search result (backward compatible with E2E tests).
#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub title: String,
    pub source: String,
    pub snippet: String,
    pub score: f64,
}

/// Full-text search via Tantivy BM25.
pub fn search(
    query: &str,
    limit: usize,
    source_filter: Option<&str>,
    personal_only: bool,
    public_only: bool,
) -> io::Result<Vec<SearchResult>> {
    let fts = open_fts()?;
    let hits = fts
        .search(query, limit, source_filter, personal_only, public_only)
        .map_err(|e| io::Error::other(e.to_string()))?;

    Ok(hits
        .into_iter()
        .map(|hit| {
            let snippet = extract_snippet(&hit.text, 0, 200);
            SearchResult {
                title: hit.title,
                source: hit.source_name,
                snippet,
                score: hit.score as f64,
            }
        })
        .collect())
}

fn extract_doc_title(content: &str, filename: &str) -> String {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(heading) = trimmed.strip_prefix("# ") {
            return heading.to_string();
        }
    }
    filename
        .trim_end_matches(".md")
        .trim_end_matches(".txt")
        .to_string()
}

fn extract_snippet(content: &str, pos: usize, max_len: usize) -> String {
    let start = pos.saturating_sub(max_len / 2);
    let end = (pos + max_len / 2).min(content.len());
    let snippet = &content[start..end];
    let snippet = snippet.trim();
    if start > 0 {
        format!("...{snippet}...")
    } else if end < content.len() {
        format!("{snippet}...")
    } else {
        snippet.to_string()
    }
}

/// Read a document by source:doc_id reference.
pub fn read_document(doc_ref: &str) -> io::Result<String> {
    let parts: Vec<&str> = doc_ref.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected format: source_name:document_id",
        ));
    }
    let (source_name, doc_id) = (parts[0], parts[1]);
    let content_store = open_content_store();

    content_store
        .read_by_filename(source_name, doc_id)
        .map_err(|e| io::Error::other(e.to_string()))?
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "document '{}' not found in source '{}'",
                    doc_id, source_name
                ),
            )
        })
}

/// Storage layer stats.
#[derive(Debug, Serialize)]
pub struct StorageLayerInfo {
    pub name: String,
    pub size_bytes: u64,
}

/// Per-source storage breakdown.
#[derive(Debug, Serialize)]
pub struct SourceStorageInfo {
    pub name: String,
    pub class: String,
    pub documents: u64,
    pub content_bytes: u64,
}

/// Full storage status with per-source breakdown (ADR-003).
#[derive(Debug, Serialize)]
pub struct FullStorageStatus {
    pub layers: Vec<StorageLayerInfo>,
    pub total_bytes: u64,
    pub sources: Vec<SourceStorageInfo>,
}

/// Calculate storage status with per-source breakdown.
pub fn storage_status() -> io::Result<FullStorageStatus> {
    let catalog = open_catalog()?;
    let content_store = open_content_store();

    let layers = vec![
        StorageLayerInfo {
            name: "source".to_string(),
            size_bytes: content_store.total_size(),
        },
        StorageLayerInfo {
            name: "derived".to_string(),
            size_bytes: dir_size(config::derived_dir().as_ref()),
        },
        StorageLayerInfo {
            name: "cache".to_string(),
            size_bytes: dir_size(config::cache_dir().as_ref()),
        },
    ];
    let total_bytes: u64 = layers.iter().map(|l| l.size_bytes).sum();

    let sources_list = catalog
        .list_sources()
        .map_err(|e| io::Error::other(e.to_string()))?;
    let sources = sources_list
        .iter()
        .map(|s| SourceStorageInfo {
            name: s.name.clone(),
            class: if s.class.is_public() {
                "public".to_string()
            } else {
                "personal".to_string()
            },
            documents: s.document_count,
            content_bytes: content_store.source_size(&s.name),
        })
        .collect();

    Ok(FullStorageStatus {
        layers,
        total_bytes,
        sources,
    })
}

fn dir_size(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    fs::read_dir(path)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| {
                    let p = e.path();
                    if p.is_dir() {
                        dir_size(&p)
                    } else {
                        e.metadata().map(|m| m.len()).unwrap_or(0)
                    }
                })
                .sum()
        })
        .unwrap_or(0)
}

/// Export public sources.
pub fn export(output: &str, include_personal: bool) -> io::Result<()> {
    let catalog = open_catalog()?;
    let sources = catalog
        .list_sources()
        .map_err(|e| io::Error::other(e.to_string()))?;
    let output_path = PathBuf::from(output);

    let exported_sources: Vec<String> = sources
        .iter()
        .filter(|s| include_personal || s.class.is_exportable())
        .map(|s| s.name.clone())
        .collect();

    let manifest = serde_json::json!({
        "version": "0.1.0",
        "sources": exported_sources,
    });
    let json = serde_json::to_string_pretty(&manifest).map_err(io::Error::other)?;
    fs::write(output_path, json)?;

    Ok(())
}
