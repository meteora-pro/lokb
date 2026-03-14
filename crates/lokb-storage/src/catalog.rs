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
