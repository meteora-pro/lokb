//! Tool execution context — shared resources for all tools.

use lokb_core::config;
use lokb_search::TantivyIndex;
use lokb_storage::{FileContentStore, SqliteCatalog};

/// Shared context for tool execution.
/// Holds references to catalog, content store, and search indexes.
pub struct ToolContext {
    pub catalog: SqliteCatalog,
    pub content_store: FileContentStore,
    pub fts: TantivyIndex,
}

impl ToolContext {
    /// Create context from default data directory.
    pub fn open() -> Result<Self, String> {
        let catalog_path = config::derived_dir().join("catalog.sqlite");
        if let Some(parent) = catalog_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let catalog = SqliteCatalog::open(&catalog_path).map_err(|e| format!("catalog: {e}"))?;
        let content_store = FileContentStore::new(config::source_content_dir().as_ref());
        let fts_path = config::derived_dir().join("fts");
        let fts = TantivyIndex::open(&fts_path).map_err(|e| format!("fts: {e}"))?;

        Ok(Self {
            catalog,
            content_store,
            fts,
        })
    }
}
