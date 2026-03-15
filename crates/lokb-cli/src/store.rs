use chrono::Utc;
use lokb_core::config;
use lokb_core::{
    ContentHash, ContentType, DataSource, DataSourceClass, PrivacyLevel, RawRetention, SyncStrategy,
};
use lokb_ingest::SemanticChunker;
use lokb_pipeline::Chunker;
use lokb_search::{TantivyIndex, VectorIndex};
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

fn vector_path() -> PathBuf {
    config::derived_dir().join("vectors.json")
}

fn open_vectors() -> io::Result<VectorIndex> {
    VectorIndex::open(&vector_path()).map_err(|e| io::Error::other(e.to_string()))
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

    // Phase 3: Entity extraction from [[wikilinks]] (ADR-007 Pattern 1)
    let entities_extracted = extract_entities_from_files(&files, source_id, &catalog);

    if entities_extracted > 0 {
        eprintln!("  Entities: {entities_extracted} extracted from wikilinks");
    }

    // Phase 4: Optional embedding (background in future, inline for now)
    let embed_start = std::time::Instant::now();
    let vectors_created = match embed_chunks(name, &files, format, class, &chunker) {
        Ok(count) => count,
        Err(e) => {
            eprintln!("Warning: embedding skipped: {e}");
            0
        }
    };
    let embed_time = embed_start.elapsed();

    let fts_index_bytes = dir_size(&fts_path());
    let total_time = total_start.elapsed();

    if vectors_created > 0 {
        eprintln!(
            "  Embedding: {:.1}s ({} vectors, {} dims)",
            embed_time.as_secs_f64(),
            vectors_created,
            384
        );
    }

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

/// Embed chunks and store in vector index. Returns number of vectors created.
fn embed_chunks(
    source_name: &str,
    files: &[(String, String)],
    format: &str,
    class: &str,
    chunker: &SemanticChunker,
) -> io::Result<u64> {
    let mut embedder = lokb_embed::Embedder::new().map_err(|e| io::Error::other(e.to_string()))?;
    let mut vector_index = open_vectors()?;

    let privacy_level: u8 = if class == "personal" { 2 } else { 0 };
    let mut entries = Vec::new();

    for (filename, content) in files {
        let title = extract_doc_title(content, filename);
        let doc = lokb_core::Document {
            id: Uuid::now_v7(),
            source_id: Uuid::nil(), // dummy, only need for chunking
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
            privacy_level: PrivacyLevel::default(),
        };

        let chunks = chunker
            .chunk(&doc, content)
            .map_err(|e| io::Error::other(e.to_string()))?;

        // Batch embed chunk texts
        let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();
        if texts.is_empty() {
            continue;
        }

        let vectors = embedder
            .embed(&texts)
            .map_err(|e| io::Error::other(e.to_string()))?;

        for (chunk, vector) in chunks.iter().zip(vectors.into_iter()) {
            entries.push(lokb_search::vector::VectorEntry {
                chunk_id: chunk.id.to_string(),
                source_name: source_name.to_string(),
                title: title.clone(),
                text: chunk.text.clone(),
                vector,
                privacy_level,
            });
        }
    }

    let count = entries.len() as u64;
    vector_index
        .add_entries(entries)
        .map_err(|e| io::Error::other(e.to_string()))?;
    Ok(count)
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
        "zim" => ingest_zim(raw_path, source_name, content_store, source_id, catalog),
        "wikidata-json" => ingest_wikidata(raw_path, source_id, catalog),
        "mbox" => ingest_mbox(raw_path, source_name, content_store, source_id, catalog),
        "gpx" => ingest_gpx(raw_path, source_name, content_store, source_id, catalog),
        "exif-dir" => ingest_exif_dir(raw_path, source_name, content_store, source_id, catalog),
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

/// Extract articles from ZIM file (Wikipedia offline dump).
fn ingest_zim(
    raw_path: &Path,
    source_name: &str,
    content_store: &FileContentStore,
    source_id: Uuid,
    catalog: &SqliteCatalog,
) -> io::Result<u64> {
    let reader =
        lokb_parsers::ZimReader::open(raw_path).map_err(|e| io::Error::other(e.to_string()))?;

    eprintln!(
        "ZIM: {} entries, {} clusters",
        reader.entry_count(),
        reader.cluster_count()
    );

    let articles = reader.articles();
    eprintln!("ZIM: {} HTML articles found", articles.len());

    let mut count = 0u64;
    for article in &articles {
        // Convert HTML → Markdown
        let markdown = lokb_parsers::html::html_to_markdown(&article.content);
        if markdown.trim().is_empty() {
            continue;
        }

        let external_id = article.path.clone();
        let title = if article.title.is_empty() {
            article.path.clone()
        } else {
            article.title.clone()
        };
        let filename = format!("{}.md", sanitize_filename(&external_id));

        content_store
            .write_file(source_name, &filename, &markdown)
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
            content_hash: ContentHash::from_bytes(markdown.as_bytes()),
            content_size: markdown.len() as u64,
            created_at: Utc::now(),
            indexed_at: Utc::now(),
            privacy_level: PrivacyLevel::Public,
        };
        catalog
            .upsert_document(&doc)
            .map_err(|e| io::Error::other(e.to_string()))?;

        count += 1;
        if count.is_multiple_of(10000) {
            eprintln!("ZIM: {count} articles processed...");
        }
    }

    Ok(count)
}

/// Sanitize filename for content store (replace path separators).
fn sanitize_filename(name: &str) -> String {
    name.replace(['/', '\\', ':'], "_")
}

/// Parse GPX track file.
fn ingest_gpx(
    raw_path: &Path,
    source_name: &str,
    content_store: &FileContentStore,
    source_id: Uuid,
    catalog: &SqliteCatalog,
) -> io::Result<u64> {
    let segments = lokb_parsers::gpx::parse_gpx(raw_path, source_id)
        .map_err(|e| io::Error::other(e.to_string()))?;

    let mut count = 0;
    for (doc, text) in &segments {
        if text.is_empty() {
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

/// Extract EXIF metadata from photos in a directory.
fn ingest_exif_dir(
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
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        if !["jpg", "jpeg", "tiff", "tif"].contains(&ext.as_str()) {
            continue;
        }

        let filename = path.file_name().unwrap().to_string_lossy().to_string();
        let external_id = filename.clone();

        match lokb_parsers::exif::extract_exif(&path, source_id, &external_id) {
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

/// Parse MBOX email archive with threading.
fn ingest_mbox(
    raw_path: &Path,
    source_name: &str,
    content_store: &FileContentStore,
    source_id: Uuid,
    catalog: &SqliteCatalog,
) -> io::Result<u64> {
    let threads = lokb_parsers::mbox::parse_mbox(raw_path, source_id)
        .map_err(|e| io::Error::other(e.to_string()))?;

    // Extract contacts as Person entities
    let contacts = lokb_parsers::mbox::extract_contacts(&threads);
    for contact in &contacts {
        let entity_id = format!(
            "person:{}",
            contact
                .to_lowercase()
                .replace(' ', "_")
                .replace(['<', '>', '@'], "_")
        );
        let _ = catalog.upsert_entity(
            &entity_id,
            contact,
            None,
            &["Person".to_string()],
            &std::collections::HashMap::new(),
        );
    }
    if !contacts.is_empty() {
        eprintln!("  Contacts: {} people extracted from email", contacts.len());
    }

    let mut count = 0;
    for (doc, text) in &threads {
        if text.is_empty() {
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

    // Collect unique contacts for entity extraction
    let mut contacts: std::collections::HashSet<String> = std::collections::HashSet::new();

    let mut count = 0;
    for (i, segment) in segments.iter().enumerate() {
        let mut text = format!("[{}]\n", chat_name);
        for msg in segment {
            let from = msg.from.as_deref().unwrap_or("Unknown");
            let time = msg.date.split('T').next_back().unwrap_or(&msg.date);
            let msg_text = extract_text(&msg.text);

            // Track contacts
            if let Some(ref name) = msg.from {
                contacts.insert(name.clone());
            }

            // Format with reply/edited indicators
            let mut line = format!("{from} [{time}]");
            if msg.reply_to_message_id.is_some() {
                line.push_str(" (reply)");
            }
            if msg.edited.is_some() {
                line.push_str(" (edited)");
            }
            line.push_str(&format!(": {msg_text}\n"));
            text.push_str(&line);
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

    // Extract Person entities from contacts (ADR-007 Pattern 1)
    for contact_name in &contacts {
        let entity_id = format!("person:{}", contact_name.to_lowercase().replace(' ', "_"));
        let _ = catalog.upsert_entity(
            &entity_id,
            contact_name,
            None,
            &["Person".to_string()],
            &std::collections::HashMap::new(),
        );
    }
    if !contacts.is_empty() {
        eprintln!(
            "  Contacts: {} people extracted from Telegram",
            contacts.len()
        );
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
    #[allow(dead_code)]
    id: Option<i64>,
    r#type: String,
    date: String,
    from: Option<String>,
    text: serde_json::Value,
    reply_to_message_id: Option<i64>,
    edited: Option<String>,
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

/// Hybrid search: FTS (BM25) + optional vector similarity with RRF fusion.
pub fn search(
    query: &str,
    limit: usize,
    source_filter: Option<&str>,
    personal_only: bool,
    public_only: bool,
) -> io::Result<Vec<SearchResult>> {
    // FTS search
    let fts = open_fts()?;
    let fts_hits = fts
        .search(query, limit * 2, source_filter, personal_only, public_only)
        .map_err(|e| io::Error::other(e.to_string()))?;

    // Try vector search (optional — may not have embeddings)
    let vector_index = open_vectors()?;
    let vector_hits = if !vector_index.is_empty() {
        match lokb_embed::Embedder::new() {
            Ok(mut embedder) => match embedder.embed_one(query) {
                Ok(query_vec) => vector_index.search(
                    &query_vec,
                    limit * 2,
                    source_filter,
                    personal_only,
                    public_only,
                ),
                Err(_) => vec![],
            },
            Err(_) => vec![],
        }
    } else {
        vec![]
    };

    // If no vectors, return FTS only
    if vector_hits.is_empty() {
        return Ok(fts_hits
            .into_iter()
            .take(limit)
            .map(|hit| {
                let snippet = extract_snippet(&hit.text, 0, 200);
                SearchResult {
                    title: hit.title,
                    source: hit.source_name,
                    snippet,
                    score: hit.score as f64,
                }
            })
            .collect());
    }

    // Hybrid: Reciprocal Rank Fusion (RRF)
    // score(doc) = Σ 1 / (k + rank_i(doc))  where k=60
    let k = 60.0;
    let mut rrf_scores: std::collections::HashMap<String, (f64, SearchResult)> =
        std::collections::HashMap::new();

    for (rank, hit) in fts_hits.iter().enumerate() {
        let key = format!("{}:{}", hit.source_name, hit.title);
        let rrf = 1.0 / (k + rank as f64);
        rrf_scores
            .entry(key)
            .and_modify(|(score, _)| *score += rrf)
            .or_insert_with(|| {
                (
                    rrf,
                    SearchResult {
                        title: hit.title.clone(),
                        source: hit.source_name.clone(),
                        snippet: extract_snippet(&hit.text, 0, 200),
                        score: 0.0,
                    },
                )
            });
    }

    for (rank, hit) in vector_hits.iter().enumerate() {
        let key = format!("{}:{}", hit.source_name, hit.title);
        let rrf = 1.0 / (k + rank as f64);
        rrf_scores
            .entry(key)
            .and_modify(|(score, _)| *score += rrf)
            .or_insert_with(|| {
                (
                    rrf,
                    SearchResult {
                        title: hit.title.clone(),
                        source: hit.source_name.clone(),
                        snippet: extract_snippet(&hit.text, 0, 200),
                        score: 0.0,
                    },
                )
            });
    }

    let mut results: Vec<SearchResult> = rrf_scores
        .into_values()
        .map(|(score, mut r)| {
            r.score = score;
            r
        })
        .collect();
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(limit);

    Ok(results)
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
    pub entity_count: u64,
    pub relation_count: u64,
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

    let entity_count = catalog
        .entity_count()
        .map_err(|e| io::Error::other(e.to_string()))?;
    let relation_count = catalog
        .relation_count()
        .map_err(|e| io::Error::other(e.to_string()))?;

    Ok(FullStorageStatus {
        layers,
        total_bytes,
        sources,
        entity_count,
        relation_count,
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

/// Ingest Wikidata JSON dump into entity store.
fn ingest_wikidata(raw_path: &Path, source_id: Uuid, catalog: &SqliteCatalog) -> io::Result<u64> {
    let file = fs::File::open(raw_path)?;
    let reader: Box<dyn std::io::Read> = if raw_path.extension().is_some_and(|ext| ext == "gz") {
        Box::new(flate2::read::GzDecoder::new(file))
    } else {
        Box::new(file)
    };

    let config = lokb_parsers::wikidata::WikidataConfig::default();
    let mut count = 0u64;

    lokb_parsers::wikidata::parse_wikidata(reader, &config, |entity| {
        let en_label = entity.labels.get("en").cloned().unwrap_or_default();
        let entity_id = format!("wikidata:{}", entity.qid);

        let mut external_ids = std::collections::HashMap::new();
        external_ids.insert("wikidata".to_string(), entity.qid.clone());

        let entity_types: Vec<String> = entity.entity_type.iter().cloned().collect();

        let _ = catalog.upsert_entity(
            &entity_id,
            &en_label,
            entity.description.as_deref(),
            &entity_types,
            &external_ids,
        );

        // Add relations from properties
        for prop in &entity.properties {
            if let lokb_parsers::wikidata::WikidataValue::EntityRef(ref target_qid) = prop.value {
                let target_id = format!("wikidata:{target_qid}");
                let _ = catalog.add_relation(
                    &entity_id,
                    prop.property_label,
                    &target_id,
                    source_id,
                    1.0,
                );
            }
        }

        count += 1;
    })
    .map_err(|e| io::Error::other(e.to_string()))?;

    Ok(count)
}

/// Extract entities from [[wikilinks]] in markdown files.
/// Creates entities, records document mentions, and builds co-occurrence relations.
fn extract_entities_from_files(
    files: &[(String, String)],
    source_id: Uuid,
    catalog: &SqliteCatalog,
) -> u64 {
    let mut total = 0u64;

    for (filename, content) in files {
        let wikilinks = lokb_parsers::extract_wikilinks(content);
        if wikilinks.is_empty() {
            continue;
        }

        // Find document_id for this file
        let external_id = filename.trim_end_matches(".md").trim_end_matches(".txt");
        let doc_id = catalog
            .get_document_by_external_id(source_id, external_id)
            .ok()
            .flatten();

        let mut doc_entity_ids: Vec<String> = Vec::new();

        for entity_name in wikilinks.keys() {
            let entity_id = format!("wiki:{}", entity_name.to_lowercase().replace(' ', "_"));
            let _ = catalog.upsert_entity(
                &entity_id,
                entity_name,
                None,
                &[],
                &std::collections::HashMap::new(),
            );

            // Record mention in document
            if let Some(did) = doc_id {
                let _ = catalog.add_entity_mention(&entity_id, did, source_id, Some(entity_name));
            }

            doc_entity_ids.push(entity_id);
            total += 1;
        }

        // Build co-occurrence relations between entities in same document
        for i in 0..doc_entity_ids.len() {
            for j in (i + 1)..doc_entity_ids.len() {
                let _ = catalog.add_relation(
                    &doc_entity_ids[i],
                    "co_occurs_with",
                    &doc_entity_ids[j],
                    source_id,
                    1.0,
                );
            }
        }
    }
    total
}

/// Entity lookup result.
#[derive(Debug, Serialize)]
pub struct EntityCard {
    pub canonical_name: String,
    pub description: Option<String>,
    pub entity_types: Vec<String>,
    pub mention_count: i64,
    pub relations: Vec<lokb_storage::catalog::RelationInfo>,
    pub documents: Vec<String>,
}

/// Look up an entity by name.
pub fn entity_lookup(
    name: &str,
    include_relations: bool,
    include_documents: bool,
) -> io::Result<Option<EntityCard>> {
    let catalog = open_catalog()?;
    let entity = catalog
        .get_entity_by_name(name)
        .map_err(|e| io::Error::other(e.to_string()))?;

    match entity {
        None => Ok(None),
        Some(info) => {
            let relations = if include_relations {
                catalog
                    .get_relations(&info.id)
                    .map_err(|e| io::Error::other(e.to_string()))?
            } else {
                vec![]
            };

            let documents = if include_documents {
                catalog
                    .get_entity_documents(&info.id)
                    .map_err(|e| io::Error::other(e.to_string()))?
            } else {
                vec![]
            };

            let entity_types: Vec<String> =
                serde_json::from_str(&info.entity_types).unwrap_or_default();

            Ok(Some(EntityCard {
                canonical_name: info.canonical_name,
                description: info.description,
                entity_types,
                mention_count: info.mention_count,
                relations,
                documents,
            }))
        }
    }
}

/// Search entities by prefix.
pub fn entity_search(
    query: &str,
    limit: usize,
) -> io::Result<Vec<lokb_storage::catalog::EntityInfo>> {
    let catalog = open_catalog()?;
    catalog
        .search_entities(query, limit)
        .map_err(|e| io::Error::other(e.to_string()))
}

/// Result of a structured fact lookup.
#[derive(Debug, Serialize)]
pub struct FactAnswer {
    pub answer: String,
    pub entity: Option<String>,
    pub property: Option<String>,
    pub source: Option<String>,
}

/// Try to answer a fact query from the knowledge graph.
/// Parses patterns like "population of Paris", "capital of France".
pub fn fact_lookup(query: &str) -> io::Result<Option<FactAnswer>> {
    let catalog = open_catalog()?;
    let query_lower = query.to_lowercase();

    // Try to extract entity name from query
    // Patterns: "X of Y", "Y X", "what is Y"
    let entity_name = if let Some(pos) = query_lower.find(" of ") {
        Some(query[pos + 4..].trim().to_string())
    } else {
        // Try last word(s) as entity name
        let words: Vec<&str> = query.split_whitespace().collect();
        if words.len() >= 2 {
            Some(words[1..].join(" "))
        } else {
            None
        }
    };

    let entity_name = match entity_name {
        Some(n) => n,
        None => return Ok(None),
    };

    // Look up entity
    let entity = catalog
        .get_entity_by_name(&entity_name)
        .map_err(|e| io::Error::other(e.to_string()))?;

    let entity = match entity {
        Some(e) => e,
        None => return Ok(None),
    };

    // Get relations and try to match query pattern
    let relations = catalog
        .get_relations(&entity.id)
        .map_err(|e| io::Error::other(e.to_string()))?;

    // Extract property hint from query
    let property_hint = if let Some(pos) = query_lower.find(" of ") {
        query_lower[..pos].trim().to_string()
    } else {
        query_lower
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string()
    };

    // Try to find matching relation
    for rel in &relations {
        let pred_lower = rel.predicate.to_lowercase();
        if pred_lower.contains(&property_hint) || property_hint.contains(&pred_lower) {
            return Ok(Some(FactAnswer {
                answer: format!(
                    "{}: {} → {}",
                    entity.canonical_name, rel.predicate, rel.target_name
                ),
                entity: Some(entity.canonical_name.clone()),
                property: Some(rel.predicate.clone()),
                source: Some("knowledge_graph".to_string()),
            }));
        }
    }

    // If entity found but no matching relation, return entity description
    if let Some(ref desc) = entity.description {
        return Ok(Some(FactAnswer {
            answer: format!("{}: {}", entity.canonical_name, desc),
            entity: Some(entity.canonical_name),
            property: None,
            source: Some("knowledge_graph".to_string()),
        }));
    }

    Ok(None)
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
