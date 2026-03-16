use lokb_core::{DataSource, DataSourceId, Document, DocumentId, Result};
use rusqlite::{Connection, params};
use std::path::Path;
use std::sync::Mutex;

/// SQLite-backed catalog implementing the Catalog trait (ADR-006).
/// Deduplication key: (source_id, external_id).
pub struct SqliteCatalog {
    conn: Mutex<Connection>,
}

impl SqliteCatalog {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        let catalog = Self {
            conn: Mutex::new(conn),
        };
        catalog.init_schema()?;
        Ok(catalog)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn =
            Connection::open_in_memory().map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        let catalog = Self {
            conn: Mutex::new(conn),
        };
        catalog.init_schema()?;
        Ok(catalog)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS sources (
                id              TEXT PRIMARY KEY,
                name            TEXT NOT NULL UNIQUE,
                class_json      TEXT NOT NULL,
                format          TEXT NOT NULL,
                sync_strategy   TEXT NOT NULL DEFAULT '\"Once\"',
                raw_retention   TEXT NOT NULL DEFAULT '\"Keep\"',
                priority        INTEGER NOT NULL DEFAULT 100,
                document_count  INTEGER NOT NULL DEFAULT 0,
                created_at      TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS documents (
                id              TEXT PRIMARY KEY,
                source_id       TEXT NOT NULL REFERENCES sources(id),
                external_id     TEXT NOT NULL,
                parent_id       TEXT,
                depth           INTEGER NOT NULL DEFAULT 0,
                title           TEXT NOT NULL,
                content_type    TEXT NOT NULL,
                language        TEXT,
                content_hash    BLOB NOT NULL,
                content_size    INTEGER NOT NULL DEFAULT 0,
                created_at      TEXT NOT NULL,
                indexed_at      TEXT NOT NULL,
                privacy_level   INTEGER NOT NULL DEFAULT 0,
                UNIQUE(source_id, external_id)
            );

            CREATE INDEX IF NOT EXISTS idx_documents_source ON documents(source_id);
            CREATE INDEX IF NOT EXISTS idx_documents_external ON documents(source_id, external_id);

            CREATE TABLE IF NOT EXISTS entities (
                id              TEXT PRIMARY KEY,
                canonical_name  TEXT NOT NULL,
                description     TEXT,
                entity_types    TEXT NOT NULL DEFAULT '[]',
                external_ids    TEXT NOT NULL DEFAULT '{}',
                latitude        REAL,
                longitude       REAL,
                mention_count   INTEGER NOT NULL DEFAULT 0
            );

            CREATE INDEX IF NOT EXISTS idx_entities_name ON entities(canonical_name);

            CREATE TABLE IF NOT EXISTS relations (
                subject_id      TEXT NOT NULL REFERENCES entities(id),
                predicate       TEXT NOT NULL,
                object_id       TEXT NOT NULL REFERENCES entities(id),
                source_id       TEXT,
                confidence      REAL NOT NULL DEFAULT 1.0
            );

            CREATE INDEX IF NOT EXISTS idx_relations_subject ON relations(subject_id);
            CREATE INDEX IF NOT EXISTS idx_relations_object ON relations(object_id);

            CREATE TABLE IF NOT EXISTS entity_mentions (
                entity_id       TEXT NOT NULL REFERENCES entities(id),
                document_id     TEXT NOT NULL REFERENCES documents(id),
                source_id       TEXT NOT NULL,
                mention_text    TEXT,
                UNIQUE(entity_id, document_id)
            );

            CREATE INDEX IF NOT EXISTS idx_mentions_entity ON entity_mentions(entity_id);
            CREATE INDEX IF NOT EXISTS idx_mentions_document ON entity_mentions(document_id);
            ",
        )
        .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        Ok(())
    }

    // ── Source operations ─────────────────────────────────────────

    pub fn add_source(&self, source: &DataSource) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let class_json = serde_json::to_string(&source.class)?;
        let sync_json = serde_json::to_string(&source.sync_strategy)?;
        let retention_json = serde_json::to_string(&source.raw_retention)?;
        conn.execute(
            "INSERT INTO sources (id, name, class_json, format, sync_strategy, raw_retention, priority, document_count, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                source.id.to_string(),
                source.name,
                class_json,
                source.format,
                sync_json,
                retention_json,
                source.priority,
                source.document_count,
                source.created_at.to_rfc3339(),
            ],
        )
        .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        Ok(())
    }

    pub fn get_source(&self, name: &str) -> Result<Option<DataSource>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, name, class_json, format, sync_strategy, raw_retention, priority, document_count, created_at FROM sources WHERE name = ?1")
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        let result = stmt
            .query_row(params![name], |row| Ok(row_to_source(row)))
            .optional()
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        match result {
            Some(s) => Ok(Some(s?)),
            None => Ok(None),
        }
    }

    pub fn list_sources(&self) -> Result<Vec<DataSource>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, name, class_json, format, sync_strategy, raw_retention, priority, document_count, created_at FROM sources ORDER BY name")
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| Ok(row_to_source(row)))
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        let mut sources = Vec::new();
        for row in rows {
            sources.push(row.map_err(|e| lokb_core::Error::Storage(e.to_string()))??);
        }
        Ok(sources)
    }

    pub fn update_source_doc_count(&self, source_id: DataSourceId, count: u64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sources SET document_count = ?1 WHERE id = ?2",
            params![count, source_id.to_string()],
        )
        .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        Ok(())
    }

    // ── Document operations ──────────────────────────────────────

    pub fn upsert_document(&self, doc: &Document) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let content_type_str = format!("{:?}", doc.content_type);
        conn.execute(
            "INSERT INTO documents (id, source_id, external_id, parent_id, depth, title, content_type, language, content_hash, content_size, created_at, indexed_at, privacy_level)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(source_id, external_id) DO UPDATE SET
                title = excluded.title,
                content_hash = excluded.content_hash,
                content_size = excluded.content_size,
                indexed_at = excluded.indexed_at",
            params![
                doc.id.to_string(),
                doc.source_id.to_string(),
                doc.external_id,
                doc.parent_id.map(|id| id.to_string()),
                doc.depth,
                doc.title,
                content_type_str,
                doc.language,
                doc.content_hash.0.as_slice(),
                doc.content_size,
                doc.created_at.to_rfc3339(),
                doc.indexed_at.to_rfc3339(),
                doc.privacy_level as i32,
            ],
        )
        .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        Ok(())
    }

    pub fn get_document_by_external_id(
        &self,
        source_id: DataSourceId,
        external_id: &str,
    ) -> Result<Option<DocumentId>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id FROM documents WHERE source_id = ?1 AND external_id = ?2")
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        let result = stmt
            .query_row(params![source_id.to_string(), external_id], |row| {
                let id_str: String = row.get(0)?;
                Ok(id_str)
            })
            .optional()
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        match result {
            Some(id_str) => {
                let id = id_str
                    .parse()
                    .map_err(|e: uuid::Error| lokb_core::Error::Storage(e.to_string()))?;
                Ok(Some(id))
            }
            None => Ok(None),
        }
    }

    /// Get content hash for a document by external_id (for incremental update, ADR-006).
    pub fn get_content_hash(
        &self,
        source_id: DataSourceId,
        external_id: &str,
    ) -> Result<Option<Vec<u8>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT content_hash FROM documents WHERE source_id = ?1 AND external_id = ?2")
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        let result = stmt
            .query_row(params![source_id.to_string(), external_id], |row| {
                row.get::<_, Vec<u8>>(0)
            })
            .optional()
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        Ok(result)
    }

    /// Get all external_ids for a source (for diff detection, ADR-006).
    pub fn list_external_ids(&self, source_id: DataSourceId) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT external_id FROM documents WHERE source_id = ?1")
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        let rows = stmt
            .query_map(params![source_id.to_string()], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        let mut ids = Vec::new();
        for row in rows {
            ids.push(row.map_err(|e| lokb_core::Error::Storage(e.to_string()))?);
        }
        Ok(ids)
    }

    /// Delete a document by source_id + external_id.
    pub fn delete_by_external_id(&self, source_id: DataSourceId, external_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM documents WHERE source_id = ?1 AND external_id = ?2",
            params![source_id.to_string(), external_id],
        )
        .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        Ok(())
    }

    /// Delete a source and all its documents.
    pub fn delete_source(&self, source_id: DataSourceId) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM documents WHERE source_id = ?1",
            params![source_id.to_string()],
        )
        .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        conn.execute(
            "DELETE FROM sources WHERE id = ?1",
            params![source_id.to_string()],
        )
        .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        Ok(())
    }

    pub fn document_count(&self, source_id: DataSourceId) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM documents WHERE source_id = ?1",
                params![source_id.to_string()],
                |row| row.get(0),
            )
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        Ok(count as u64)
    }

    // ── Entity operations ────────────────────────────────────────

    /// Upsert entity by canonical_name. Increments mention_count.
    pub fn upsert_entity(
        &self,
        id: &str,
        canonical_name: &str,
        description: Option<&str>,
        entity_types: &[String],
        external_ids: &std::collections::HashMap<String, String>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let types_json = serde_json::to_string(entity_types)?;
        let ext_json = serde_json::to_string(external_ids)?;
        conn.execute(
            "INSERT INTO entities (id, canonical_name, description, entity_types, external_ids, mention_count)
             VALUES (?1, ?2, ?3, ?4, ?5, 1)
             ON CONFLICT(id) DO UPDATE SET
                mention_count = mention_count + 1,
                description = COALESCE(excluded.description, entities.description)",
            params![id, canonical_name, description, types_json, ext_json],
        )
        .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        Ok(())
    }

    /// Add entity mention in a document.
    pub fn add_entity_mention(
        &self,
        entity_id: &str,
        document_id: DocumentId,
        source_id: DataSourceId,
        mention_text: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO entity_mentions (entity_id, document_id, source_id, mention_text)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                entity_id,
                document_id.to_string(),
                source_id.to_string(),
                mention_text,
            ],
        )
        .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        Ok(())
    }

    /// Get entity by name (case-insensitive search).
    pub fn get_entity_by_name(&self, name: &str) -> Result<Option<EntityInfo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, canonical_name, description, entity_types, external_ids, mention_count
                 FROM entities WHERE canonical_name = ?1 COLLATE NOCASE LIMIT 1",
            )
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        let result = stmt
            .query_row(params![name], |row| {
                Ok(EntityInfo {
                    id: row.get(0)?,
                    canonical_name: row.get(1)?,
                    description: row.get(2)?,
                    entity_types: row.get::<_, String>(3)?,
                    external_ids: row.get::<_, String>(4)?,
                    mention_count: row.get(5)?,
                })
            })
            .optional()
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        Ok(result)
    }

    /// Search entities by prefix.
    pub fn search_entities(&self, query: &str, limit: usize) -> Result<Vec<EntityInfo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, canonical_name, description, entity_types, external_ids, mention_count
                 FROM entities WHERE canonical_name LIKE ?1 COLLATE NOCASE
                 ORDER BY mention_count DESC LIMIT ?2",
            )
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        let pattern = format!("{query}%");
        let rows = stmt
            .query_map(params![pattern, limit as i64], |row| {
                Ok(EntityInfo {
                    id: row.get(0)?,
                    canonical_name: row.get(1)?,
                    description: row.get(2)?,
                    entity_types: row.get::<_, String>(3)?,
                    external_ids: row.get::<_, String>(4)?,
                    mention_count: row.get(5)?,
                })
            })
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        let mut entities = Vec::new();
        for row in rows {
            entities.push(row.map_err(|e| lokb_core::Error::Storage(e.to_string()))?);
        }
        Ok(entities)
    }

    /// Get relations for an entity.
    /// Add a relation between two entities.
    pub fn add_relation(
        &self,
        subject_id: &str,
        predicate: &str,
        object_id: &str,
        source_id: DataSourceId,
        confidence: f64,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO relations (subject_id, predicate, object_id, source_id, confidence)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                subject_id,
                predicate,
                object_id,
                source_id.to_string(),
                confidence,
            ],
        )
        .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        Ok(())
    }

    pub fn get_relations(&self, entity_id: &str) -> Result<Vec<RelationInfo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT r.predicate, e.canonical_name, r.confidence
                 FROM relations r
                 JOIN entities e ON e.id = r.object_id
                 WHERE r.subject_id = ?1
                 ORDER BY r.confidence DESC",
            )
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        let rows = stmt
            .query_map(params![entity_id], |row| {
                Ok(RelationInfo {
                    predicate: row.get(0)?,
                    target_name: row.get(1)?,
                    confidence: row.get(2)?,
                })
            })
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        let mut rels = Vec::new();
        for row in rows {
            rels.push(row.map_err(|e| lokb_core::Error::Storage(e.to_string()))?);
        }
        Ok(rels)
    }

    /// Get documents mentioning an entity.
    pub fn get_entity_documents(&self, entity_id: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT d.title FROM entity_mentions em
                 JOIN documents d ON d.id = em.document_id
                 WHERE em.entity_id = ?1
                 ORDER BY d.title LIMIT 50",
            )
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        let rows = stmt
            .query_map(params![entity_id], |row| row.get::<_, String>(0))
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        let mut titles = Vec::new();
        for row in rows {
            titles.push(row.map_err(|e| lokb_core::Error::Storage(e.to_string()))?);
        }
        Ok(titles)
    }

    /// Count total entities.
    pub fn entity_count(&self) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM entities", [], |row| row.get(0))
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        Ok(count as u64)
    }

    pub fn relation_count(&self) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM relations", [], |row| row.get(0))
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        Ok(count as u64)
    }
}

/// Entity info from catalog.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EntityInfo {
    pub id: String,
    pub canonical_name: String,
    pub description: Option<String>,
    pub entity_types: String,
    pub external_ids: String,
    pub mention_count: i64,
}

/// Relation info from catalog.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RelationInfo {
    pub predicate: String,
    pub target_name: String,
    pub confidence: f64,
}

use rusqlite::OptionalExtension;

fn row_to_source(row: &rusqlite::Row) -> Result<DataSource> {
    let id_str: String = row
        .get(0)
        .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
    let name: String = row
        .get(1)
        .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
    let class_json: String = row
        .get(2)
        .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
    let format: String = row
        .get(3)
        .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
    let sync_json: String = row
        .get(4)
        .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
    let retention_json: String = row
        .get(5)
        .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
    let priority: i64 = row
        .get(6)
        .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
    let document_count: i64 = row
        .get(7)
        .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
    let created_at_str: String = row
        .get(8)
        .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;

    Ok(DataSource {
        id: id_str
            .parse()
            .map_err(|e: uuid::Error| lokb_core::Error::Storage(e.to_string()))?,
        name,
        class: serde_json::from_str(&class_json)?,
        format,
        sync_strategy: serde_json::from_str(&sync_json).unwrap_or_default(),
        raw_retention: serde_json::from_str(&retention_json).unwrap_or_default(),
        priority: priority as u32,
        document_count: document_count as u64,
        created_at: chrono::DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?,
    })
}
