//! Parallel ZIM ingestion pipeline.
//!
//! Architecture: Reader → Workers → Writer (channel-based pipeline).
//!
//! ```text
//! [ZIM Reader]  ──channel(32)──▶  [Worker Pool (N)]  ──channel(64)──▶  [Writer (1)]
//!    1 thread                      N threads                            1 thread
//!    I/O bound                     CPU bound                           I/O bound
//! ```
//!
//! - Reader: reads ZIM clusters, decompresses, sends raw HTML articles
//! - Workers: HTML→Markdown, chunking, wikilink extraction (CPU-heavy)
//! - Writer: FTS indexing, SQLite catalog, block commits (I/O)

use chrono::Utc;
use crossbeam_channel::{bounded, Receiver, Sender};
use lokb_core::{Chunk, ContentHash, ContentType, Document, PrivacyLevel};
use lokb_ingest::SemanticChunker;
use lokb_pipeline::Chunker;
use lokb_search::TantivyIndex;
use lokb_storage::{FileContentStore, SqliteCatalog};
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use uuid::Uuid;

// ── Pipeline data types ──────────────────────────────────────────────

/// Raw article from ZIM Reader → sent to Workers.
struct RawArticle {
    path: String,
    title: String,
    html: String,
}

/// Processed article from Workers → sent to Writer.
struct ProcessedArticle {
    doc: Document,
    markdown: String,
    chunks: Vec<Chunk>,
    wikilinks: HashMap<String, u32>,
}

/// Configuration for parallel pipeline.
pub struct PipelineConfig {
    /// Number of worker threads (default: num_cpus/2).
    pub threads: usize,
    /// Articles per block commit (default: 20000).
    pub block_size: u64,
    /// Whether source is large (>100K entries) — affects FTS truncation and file writes.
    pub large_source: bool,
    /// Privacy level for documents.
    pub privacy_level: PrivacyLevel,
    /// Whether to extract entities from wikilinks.
    pub extract_entities: bool,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        let threads = std::env::var("LOKB_THREADS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| num_cpus::get() / 2)
            .max(1);

        let block_size = std::env::var("LOKB_ZIM_BLOCK")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(20_000);

        Self {
            threads,
            block_size,
            large_source: false,
            privacy_level: PrivacyLevel::Public,
            extract_entities: true,
        }
    }
}

/// Metrics from parallel ingestion.
#[allow(dead_code)]
pub struct PipelineMetrics {
    pub doc_count: u64,
    pub entities_count: u64,
    pub blocks: u64,
    pub elapsed_secs: u64,
}

// ── Channel capacities ──────────────────────────────────────────────

const READER_CHANNEL_CAP: usize = 32;
const WRITER_CHANNEL_CAP: usize = 64;

// ── Public entry point ──────────────────────────────────────────────

/// Run parallel ZIM ingestion pipeline.
///
/// Spawns reader thread, N worker threads, runs writer in current thread.
/// Uses `std::thread::scope` for safe borrowing of shared references.
pub fn ingest_zim_parallel(
    raw_path: &Path,
    source_name: &str,
    content_store: &FileContentStore,
    source_id: Uuid,
    catalog: &SqliteCatalog,
    fts: &TantivyIndex,
    config: &PipelineConfig,
) -> io::Result<PipelineMetrics> {
    let reader = lokb_parsers::ZimReader::open(raw_path)
        .map_err(|e| io::Error::other(e.to_string()))?;

    let total_entries = reader.entry_count() as u64;
    eprintln!(
        "ZIM: {} entries, {} clusters, {} worker threads",
        reader.entry_count(),
        reader.cluster_count(),
        config.threads,
    );

    let shutdown = Arc::new(AtomicBool::new(false));
    let articles_processed = Arc::new(AtomicU64::new(0));

    // Install Ctrl+C handler
    let shutdown_flag = shutdown.clone();
    let _ = ctrlc::set_handler(move || {
        eprintln!("\nZIM: shutting down gracefully (finishing current block)...");
        shutdown_flag.store(true, Ordering::SeqCst);
    });

    let (reader_tx, reader_rx) = bounded::<RawArticle>(READER_CHANNEL_CAP);
    let (writer_tx, writer_rx) = bounded::<ProcessedArticle>(WRITER_CHANNEL_CAP);

    let start = std::time::Instant::now();

    // Use thread::scope so all threads can borrow local references safely.
    std::thread::scope(|scope| {
        // ── Reader thread ────────────────────────────────────────────
        let shutdown_r = shutdown.clone();
        scope.spawn(move || {
            reader_thread(&reader, reader_tx, &shutdown_r);
        });

        // ── Worker threads ───────────────────────────────────────────
        let large = config.large_source || total_entries > 100_000;
        let extract_ents = config.extract_entities && total_entries < 500_000;
        for _ in 0..config.threads {
            let rx = reader_rx.clone();
            let tx = writer_tx.clone();
            let ap = articles_processed.clone();
            let pl = config.privacy_level;
            scope.spawn(move || {
                worker_thread(rx, tx, source_id, large, extract_ents, pl, &ap);
            });
        }
        // Drop our copies so writer_rx sees channel close when all workers finish.
        drop(reader_rx);
        drop(writer_tx);

        // ── Writer runs in current thread ────────────────────────────
        writer_thread(
            writer_rx,
            source_name,
            content_store,
            source_id,
            catalog,
            fts,
            config,
            total_entries,
            &articles_processed,
            &start,
        )
    })
}

// ── Reader thread ────────────────────────────────────────────────────

fn reader_thread(
    reader: &lokb_parsers::ZimReader,
    tx: Sender<RawArticle>,
    shutdown: &AtomicBool,
) {
    for article in reader.article_iter() {
        if shutdown.load(Ordering::Relaxed) {
            eprintln!("ZIM reader: shutdown requested");
            break;
        }

        let title = if article.title.is_empty() {
            article.path.clone()
        } else {
            article.title.clone()
        };

        let raw = RawArticle {
            path: article.path,
            title,
            html: article.content,
        };

        // Bounded channel: blocks if workers are busy (backpressure).
        if tx.send(raw).is_err() {
            break; // channel closed
        }
    }
    // tx dropped here → workers will see channel close
}

// ── Worker thread ────────────────────────────────────────────────────

fn worker_thread(
    rx: Receiver<RawArticle>,
    tx: Sender<ProcessedArticle>,
    source_id: Uuid,
    large_source: bool,
    extract_entities: bool,
    privacy_level: PrivacyLevel,
    counter: &AtomicU64,
) {
    let chunker = SemanticChunker::default();

    for raw in rx {
        // catch_unwind: htmd can panic on malformed HTML
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            process_article(&raw, source_id, large_source, extract_entities, privacy_level, &chunker)
        }));

        match result {
            Ok(Some(processed)) => {
                counter.fetch_add(1, Ordering::Relaxed);
                if tx.send(processed).is_err() {
                    break; // writer closed
                }
            }
            Ok(None) => {
                // Empty markdown, skip
            }
            Err(_) => {
                // htmd panic, skip this article
            }
        }
    }
}

fn process_article(
    raw: &RawArticle,
    source_id: Uuid,
    large_source: bool,
    extract_entities: bool,
    privacy_level: PrivacyLevel,
    chunker: &SemanticChunker,
) -> Option<ProcessedArticle> {
    let markdown = lokb_parsers::html::html_to_markdown(&raw.html);
    if markdown.trim().is_empty() {
        return None;
    }

    // For large sources, truncate text for FTS to limit index size
    let fts_text = if large_source {
        let limit = 2000.min(markdown.len());
        &markdown[..markdown.floor_char_boundary(limit)]
    } else {
        &markdown
    };

    // Build Document in worker thread (Blake3 hash + UUID are CPU work)
    let doc = Document {
        id: Uuid::now_v7(),
        source_id,
        external_id: raw.path.clone(),
        parent_id: None,
        depth: 0,
        title: raw.title.clone(),
        content_type: ContentType::Article,
        language: None,
        content_hash: ContentHash::from_bytes(markdown.as_bytes()),
        content_size: markdown.len() as u64,
        created_at: Utc::now(),
        indexed_at: Utc::now(),
        privacy_level,
    };

    let chunks = chunker.chunk(&doc, fts_text).unwrap_or_default();

    let wikilinks = if extract_entities {
        lokb_parsers::extract_wikilinks(&markdown)
    } else {
        HashMap::new()
    };

    Some(ProcessedArticle {
        doc,
        markdown,
        chunks,
        wikilinks,
    })
}

// ── Writer thread ────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn writer_thread(
    rx: Receiver<ProcessedArticle>,
    source_name: &str,
    content_store: &FileContentStore,
    source_id: Uuid,
    catalog: &SqliteCatalog,
    fts: &TantivyIndex,
    config: &PipelineConfig,
    total_entries: u64,
    articles_processed: &AtomicU64,
    start: &std::time::Instant,
) -> io::Result<PipelineMetrics> {
    let mut fts_writer = Some(
        fts.writer().map_err(|e| io::Error::other(e.to_string()))?,
    );

    catalog
        .begin_batch()
        .map_err(|e| io::Error::other(e.to_string()))?;

    let mut count = 0u64;
    let mut block_num = 0u64;
    let mut entities_count = 0u64;
    let large_source = config.large_source || total_entries > 100_000;

    // Timing accumulators for profiling (per block)
    let mut t_fts = std::time::Duration::ZERO;
    let mut t_catalog = std::time::Duration::ZERO;
    let mut t_files = std::time::Duration::ZERO;
    let mut t_entity = std::time::Duration::ZERO;
    let mut t_hash = std::time::Duration::ZERO;

    // Pre-allocate empty HashMap for entity upsert (avoid allocation per call)
    let empty_ext_ids: HashMap<String, String> = HashMap::new();
    let empty_types: Vec<String> = Vec::new();

    for processed in rx {
        // Document already built in worker thread (Blake3 hash + UUID computed there)
        let doc = processed.doc;

        // Write sampled files to content store (every 100th for large sources)
        if !large_source || count.is_multiple_of(100) {
            let t0 = std::time::Instant::now();
            let filename = format!("{}.md", sanitize_filename(&doc.external_id));
            let _ = content_store.write_file(source_name, &filename, &processed.markdown);
            t_files += t0.elapsed();
        }

        // hash/doc creation done in worker — timing kept for profiling parity
        let t0 = std::time::Instant::now();
        t_hash += t0.elapsed();

        // Catalog upsert
        let t0 = std::time::Instant::now();
        catalog
            .upsert_document(&doc)
            .map_err(|e| io::Error::other(e.to_string()))?;
        t_catalog += t0.elapsed();

        // FTS index chunks
        let t0 = std::time::Instant::now();
        if let Some(ref mut w) = fts_writer {
            w.add_chunks(&processed.chunks, source_name, &doc.title)
                .map_err(|e| io::Error::other(e.to_string()))?;
        }
        t_fts += t0.elapsed();

        // Entity extraction
        let t0 = std::time::Instant::now();
        for entity_name in processed.wikilinks.keys() {
            let entity_id = format!("wiki:{}", entity_name.to_lowercase().replace(' ', "_"));
            let _ = catalog.upsert_entity(
                &entity_id,
                entity_name,
                None,
                &empty_types,
                &empty_ext_ids,
            );
            let _ = catalog.add_entity_mention(&entity_id, doc.id, source_id, Some(entity_name));
            entities_count += 1;
        }
        t_entity += t0.elapsed();

        count += 1;

        // ── Block boundary ───────────────────────────────────────
        if count.is_multiple_of(config.block_size) {
            catalog
                .commit_batch()
                .map_err(|e| io::Error::other(e.to_string()))?;

            let old_writer = fts_writer.take().unwrap();
            let chunks_in_block = old_writer
                .commit()
                .map_err(|e| io::Error::other(e.to_string()))?;

            block_num += 1;
            let elapsed = start.elapsed().as_secs();
            let rate = if elapsed > 0 { count / elapsed } else { count };
            let processed_total = articles_processed.load(Ordering::Relaxed);
            eprintln!(
                "ZIM: block {block_num} — {count} written, {processed_total} processed, \
                 {chunks_in_block} chunks ({rate}/s) \
                 [fts={:.0}ms cat={:.0}ms hash={:.0}ms ent={:.0}ms files={:.0}ms]",
                t_fts.as_millis(),
                t_catalog.as_millis(),
                t_hash.as_millis(),
                t_entity.as_millis(),
                t_files.as_millis(),
            );

            // Reset timers for next block
            t_fts = std::time::Duration::ZERO;
            t_catalog = std::time::Duration::ZERO;
            t_files = std::time::Duration::ZERO;
            t_entity = std::time::Duration::ZERO;
            t_hash = std::time::Duration::ZERO;

            fts_writer = Some(
                fts.writer().map_err(|e| io::Error::other(e.to_string()))?,
            );

            catalog
                .begin_batch()
                .map_err(|e| io::Error::other(e.to_string()))?;
        }
    }

    // Final commit
    catalog
        .commit_batch()
        .map_err(|e| io::Error::other(e.to_string()))?;
    if let Some(w) = fts_writer.take() {
        let _ = w.commit().map_err(|e| io::Error::other(e.to_string()))?;
    }

    let elapsed = start.elapsed().as_secs();
    let rate = if elapsed > 0 { count / elapsed } else { count };
    eprintln!(
        "ZIM: done — {count} articles in {} blocks ({rate}/s, {elapsed}s, {} threads)",
        block_num + 1,
        config.threads,
    );

    Ok(PipelineMetrics {
        doc_count: count,
        entities_count,
        blocks: block_num + 1,
        elapsed_secs: elapsed,
    })
}

fn sanitize_filename(name: &str) -> String {
    name.replace(['/', '\\', ':'], "_")
}
