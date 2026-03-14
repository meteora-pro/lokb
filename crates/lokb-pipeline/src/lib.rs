use async_trait::async_trait;
use lokb_core::{Chunk, DataSourceId, Document, Entity, Relation, Result};
use std::time::Duration;

// ── Step Data ────────────────────────────────────────────────────────

/// Data flowing between pipeline steps.
#[derive(Debug)]
pub enum StepData {
    /// Raw bytes (file content)
    Bytes(Vec<u8>),
    /// Extracted text with metadata
    Text { content: String, document: Document },
    /// Multiple documents (batch)
    Documents(Vec<(Document, String)>),
    /// Entities extracted from structured sources
    Entities {
        entities: Vec<Entity>,
        relations: Vec<Relation>,
    },
}

/// Pipeline execution context.
pub struct PipelineContext {
    pub source_id: DataSourceId,
    pub source_name: String,
    pub data_dir: std::path::PathBuf,
}

// ── Optimize Pipeline (RAW → OPTIMIZED) ──────────────────────────────

/// Cost estimate for an optimize step.
#[derive(Debug, Clone)]
pub struct OptimizeEstimate {
    pub estimated_output_size: u64,
    pub compression_ratio: f64,
    pub compute_time: Duration,
    pub needs_gpu: bool,
}

/// Metrics after an optimize step completes.
#[derive(Debug, Clone)]
pub struct OptimizeMetrics {
    pub input_size: u64,
    pub output_size: u64,
    pub compression_ratio: f64,
    pub documents_processed: u64,
    pub compute_time: Duration,
    pub errors: u64,
}

/// A step in the Optimize Pipeline (RAW → OPTIMIZED).
/// Compresses data, extracts essence, normalizes format.
#[async_trait]
pub trait OptimizeStep: Send + Sync {
    fn name(&self) -> &str;
    fn estimate(&self, input_size: u64) -> OptimizeEstimate;
    async fn execute(&self, input: StepData, ctx: &PipelineContext) -> Result<StepData>;
}

/// Fallback behavior when a step fails.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub enum StepFallback {
    #[default]
    Skip,
    SkipDocument,
    Abort,
    Retry {
        max_attempts: u32,
        delay_ms: u64,
    },
}

// ── Enrichment Pipeline (OPTIMIZED → DERIVED) ────────────────────────

/// What kind of enrichment a step produces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnrichmentKind {
    Chunking,
    FullTextIndex,
    EmbeddingIndex,
    EntityExtraction,
    GeoIndex,
    TimelineIndex,
    Custom(String),
}

/// Cost estimate for an enrichment step.
#[derive(Debug, Clone)]
pub struct EnrichmentEstimate {
    pub storage_overhead: u64,
    pub compute_time: Duration,
    pub needs_gpu: bool,
    pub can_degrade: bool,
}

/// Metrics after an enrichment step completes.
#[derive(Debug, Clone)]
pub struct EnrichmentMetrics {
    pub chunks_processed: u64,
    pub storage_added: u64,
    pub compute_time: Duration,
}

/// A source of chunks for enrichment steps to consume.
#[async_trait]
pub trait ChunkSource: Send + Sync {
    async fn chunks(&self, source_id: DataSourceId) -> Result<Vec<Chunk>>;
}

/// A step in the Enrichment Pipeline (OPTIMIZED → DERIVED).
/// Expands data: builds indexes, vectors, entity graph.
#[async_trait]
pub trait EnrichmentStep: Send + Sync {
    fn name(&self) -> &str;
    fn enrichment_kind(&self) -> EnrichmentKind;
    fn estimate(&self, chunk_count: u64) -> EnrichmentEstimate;
    async fn execute(&self, source: &dyn ChunkSource, ctx: &PipelineContext) -> Result<()>;
    fn supports_incremental(&self) -> bool;
}

// ── Cross-Source Pipeline ────────────────────────────────────────────

/// Access to a source's derived data (for cross-source steps).
#[async_trait]
pub trait SourceAccess: Send + Sync {
    fn source_id(&self) -> DataSourceId;
    fn source_name(&self) -> &str;
    async fn entities(&self) -> Result<Vec<Entity>>;
    async fn chunks(&self) -> Result<Vec<Chunk>>;
}

/// A step that links data between multiple sources.
#[async_trait]
pub trait CrossSourceStep: Send + Sync {
    fn name(&self) -> &str;
    fn required_enrichments(&self) -> Vec<EnrichmentKind>;
    async fn execute(&self, sources: &[&dyn SourceAccess], ctx: &PipelineContext) -> Result<()>;
}

// ── Chunker ──────────────────────────────────────────────────────────

/// Strategy for splitting documents into searchable chunks.
pub trait Chunker: Send + Sync {
    fn chunk(&self, document: &Document, text: &str) -> Result<Vec<Chunk>>;
}

// ── Searcher ─────────────────────────────────────────────────────────

/// A search backend.
#[async_trait]
pub trait Searcher: Send + Sync {
    fn name(&self) -> &str;
    async fn search(&self, query: &lokb_core::SearchQuery) -> Result<Vec<lokb_core::ScoredChunk>>;
}
