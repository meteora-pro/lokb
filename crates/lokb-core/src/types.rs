use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Identifiers ──────────────────────────────────────────────────────

pub type DataSourceId = Uuid;
pub type DocumentId = Uuid;
pub type ChunkId = Uuid;
pub type EntityId = Uuid;

// ── DataSource ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSource {
    pub id: DataSourceId,
    pub name: String,
    pub class: DataSourceClass,
    pub format: String,
    pub sync_strategy: SyncStrategy,
    pub raw_retention: RawRetention,
    pub priority: u32,
    pub document_count: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DataSourceClass {
    Public {
        license: Option<String>,
        web_url_template: Option<String>,
    },
    Personal {
        owner: Option<String>,
        platform: Option<String>,
        contains_pii: bool,
    },
}

impl DataSourceClass {
    pub fn is_public(&self) -> bool {
        matches!(self, Self::Public { .. })
    }

    pub fn is_personal(&self) -> bool {
        matches!(self, Self::Personal { .. })
    }

    pub fn is_exportable(&self) -> bool {
        self.is_public()
    }
}

// ── Document ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: DocumentId,
    pub source_id: DataSourceId,
    pub external_id: String,
    pub parent_id: Option<DocumentId>,
    pub depth: u8,
    pub title: String,
    pub content_type: ContentType,
    pub language: Option<String>,
    pub content_hash: ContentHash,
    pub content_size: u64,
    pub created_at: DateTime<Utc>,
    pub indexed_at: DateTime<Utc>,
    pub privacy_level: PrivacyLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentType {
    Article,
    Book,
    Paper,
    Note,
    Webpage,
    CodeFile,
    Conversation,
    Thread,
    EmailThread,
    Email,
    MediaMeta,
    GpsTrackFile,
    GpsSegment,
    Record,
}

// ── Chunk ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub id: ChunkId,
    pub document_id: DocumentId,
    pub source_id: DataSourceId,
    pub chunk_index: u32,
    pub text: String,
    pub byte_start: u64,
    pub byte_end: u64,
    pub section_path: Option<String>,
    pub content_type: ContentType,
    pub language: Option<String>,
    pub timestamp: Option<i64>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub privacy_level: PrivacyLevel,
    pub entity_ids: Vec<EntityId>,
}

// ── Entity ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: EntityId,
    pub canonical_name: String,
    pub labels: std::collections::HashMap<String, Vec<String>>,
    pub description: Option<String>,
    pub entity_types: Vec<String>,
    pub external_ids: std::collections::HashMap<String, String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub mention_count: u32,
}

// ── Relation ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub subject_id: EntityId,
    pub predicate: String,
    pub object_id: EntityId,
    pub source_id: DataSourceId,
    pub confidence: f32,
}

// ── Privacy ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PrivacyLevel {
    #[default]
    Public = 0,
    Internal = 1,
    Private = 2,
    Secret = 3,
}

// ── Content Hash ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentHash(pub [u8; 32]);

impl ContentHash {
    pub fn from_bytes(data: &[u8]) -> Self {
        Self(*blake3::hash(data).as_bytes())
    }

    pub fn hex(&self) -> String {
        hex::encode(self.0)
    }
}

// ── Sync & Retention ─────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum SyncStrategy {
    #[default]
    Once,
    Incremental {
        change_detection: ChangeMethod,
        deletion_policy: DeletionPolicy,
    },
    FileWatch {
        debounce_secs: u64,
        change_detection: ChangeMethod,
    },
    FullReload,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum ChangeMethod {
    #[default]
    ContentHash,
    FileModTime,
    ModTimeThenHash,
    IncrementalId {
        last_seen: String,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum DeletionPolicy {
    Delete,
    #[default]
    SoftDelete,
    KeepAll,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum RawRetention {
    DeleteAfterOptimize,
    #[default]
    Keep,
    ExternalReference(String),
    KeepVersions(u32),
}

// ── Search ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub text: String,
    pub source_filter: Option<DataSourceId>,
    pub class_filter: Option<ClassFilter>,
    pub privacy_max: PrivacyLevel,
    pub limit: usize,
}

#[derive(Debug, Clone, Copy)]
pub enum ClassFilter {
    PublicOnly,
    PersonalOnly,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScoredChunk {
    pub chunk: Chunk,
    pub score: f64,
    pub source_name: String,
}

// ── Storage Layers ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorageLayer {
    Raw,
    Source,
    Derived,
    Cache,
}

// hex encoding for ContentHash
mod hex {
    pub fn encode(bytes: [u8; 32]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}
