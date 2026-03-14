use async_trait::async_trait;
use lokb_core::{
    Chunk, DataSource, DataSourceId, Document, DocumentId, Entity, Relation, Result, StorageLayer,
};

// ── Catalog ──────────────────────────────────────────────────────────

/// Central registry of all documents and their state.
#[async_trait]
pub trait Catalog: Send + Sync {
    // Sources
    async fn add_source(&self, source: &DataSource) -> Result<()>;
    async fn get_source(&self, name: &str) -> Result<Option<DataSource>>;
    async fn list_sources(&self) -> Result<Vec<DataSource>>;
    async fn update_source(&self, source: &DataSource) -> Result<()>;

    // Documents
    async fn upsert_document(&self, doc: &Document) -> Result<()>;
    async fn get_document(&self, id: DocumentId) -> Result<Option<Document>>;
    async fn get_document_by_external_id(
        &self,
        source_id: DataSourceId,
        external_id: &str,
    ) -> Result<Option<Document>>;
    async fn list_documents(&self, source_id: DataSourceId) -> Result<Vec<Document>>;
    async fn delete_document(&self, id: DocumentId) -> Result<()>;
}

// ── Content Store ────────────────────────────────────────────────────

/// Storage for OPTIMIZED document text (cluster-bundle zstd).
#[async_trait]
pub trait ContentStore: Send + Sync {
    async fn write_document(
        &self,
        source_id: DataSourceId,
        doc_id: DocumentId,
        content: &[u8],
    ) -> Result<()>;
    async fn read_document(
        &self,
        source_id: DataSourceId,
        doc_id: DocumentId,
    ) -> Result<Option<Vec<u8>>>;
    async fn delete_document(&self, source_id: DataSourceId, doc_id: DocumentId) -> Result<()>;
}

// ── Chunk Store ──────────────────────────────────────────────────────

/// Storage for search chunks.
#[async_trait]
pub trait ChunkStore: Send + Sync {
    async fn upsert_chunks(&self, chunks: &[Chunk]) -> Result<()>;
    async fn get_chunks(&self, doc_id: DocumentId) -> Result<Vec<Chunk>>;
    async fn delete_chunks(&self, doc_id: DocumentId) -> Result<()>;
    async fn search_text(&self, query: &str, limit: usize) -> Result<Vec<Chunk>>;
}

// ── Entity Store ─────────────────────────────────────────────────────

/// Storage for knowledge graph entities and relations.
#[async_trait]
pub trait EntityStore: Send + Sync {
    async fn upsert_entity(&self, entity: &Entity) -> Result<()>;
    async fn get_entity(&self, name: &str) -> Result<Option<Entity>>;
    async fn upsert_relation(&self, relation: &Relation) -> Result<()>;
}

// ── Storage Status ───────────────────────────────────────────────────

/// Status of a storage layer.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LayerStatus {
    pub layer: StorageLayer,
    pub size_bytes: u64,
}

/// Aggregate storage status.
pub trait StorageStatus: Send + Sync {
    fn layer_sizes(&self) -> Result<Vec<LayerStatus>>;
}
