//! FM-index based exact substring search (ADR-008).
//!
//! Production implementation with:
//! - In-memory cache (index loaded once per process)
//! - Context extraction around matches
//! - Adaptive sampling based on corpus size
//! - Position map for resolving matches to source documents

use fm_index::{FMIndexWithLocate, MatchWithLocate, Search, SearchIndex, Text};
use lokb_core::Result;
use std::fs;
use std::path::Path;
use std::sync::Mutex;

/// FM-index for exact substring search across all ingested text.
pub struct SubstringIndex {
    /// Cached index — loaded once, reused for all queries
    inner: Mutex<Option<LoadedIndex>>,
    path: std::path::PathBuf,
}

struct LoadedIndex {
    index: FMIndexWithLocate<u8>,
    text: Vec<u8>,
    position_map: Vec<DocSpan>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct DocSpan {
    source: String,
    title: String,
    start: usize,
    end: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SavedData {
    text: Vec<u8>,
    position_map: Vec<DocSpan>,
}

/// A substring search hit with context.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SubstringHit {
    pub source: String,
    pub title: String,
    pub position: usize,
    pub context: String,
}

/// Metrics from building the FM-index.
#[derive(Debug, serde::Serialize)]
pub struct BuildMetrics {
    pub documents: usize,
    pub text_bytes: usize,
    pub index_bytes: usize,
    pub build_time_ms: u64,
}

impl SubstringIndex {
    /// Open index handle (lazy load — doesn't read from disk until first query).
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            inner: Mutex::new(None),
            path: path.to_path_buf(),
        })
    }

    /// Build FM-index from a collection of (source, title, text) documents.
    pub fn build(&self, documents: &[(&str, &str, &str)]) -> Result<BuildMetrics> {
        let start = std::time::Instant::now();

        let mut concatenated = Vec::new();
        let mut position_map = Vec::new();

        for (source, title, text) in documents {
            let start_pos = concatenated.len();
            // Replace null bytes (FM-index reserves 0 as terminator)
            let clean: Vec<u8> = text
                .as_bytes()
                .iter()
                .map(|&b| if b == 0 { b' ' } else { b })
                .collect();
            concatenated.extend_from_slice(&clean);
            let end_pos = concatenated.len();
            position_map.push(DocSpan {
                source: source.to_string(),
                title: title.to_string(),
                start: start_pos,
                end: end_pos,
            });
            concatenated.push(1); // separator between documents
        }

        if concatenated.is_empty() {
            let mut inner = self.inner.lock().unwrap();
            *inner = None;
            return Ok(BuildMetrics {
                documents: 0,
                text_bytes: 0,
                index_bytes: 0,
                build_time_ms: 0,
            });
        }

        // FM-index requires text to end with exactly one null byte
        if let Some(last) = concatenated.last_mut() {
            *last = 0;
        }

        let text_bytes = concatenated.len();

        // Adaptive sampling: small corpus → low sampling (fast locate),
        // large corpus → high sampling (less memory)
        let sampling_level = if text_bytes < 1_000_000 {
            2
        } else if text_bytes < 100_000_000 {
            8
        } else {
            32
        };

        // Save text + map for future loads
        let save_path = self.path.with_extension("fmdata");
        if let Some(parent) = save_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let saved = SavedData {
            text: concatenated.clone(),
            position_map: position_map.clone(),
        };
        let save_data = bincode::serialize(&saved)
            .map_err(|e| lokb_core::Error::Storage(format!("FM data serialize: {e}")))?;
        fs::write(&save_path, save_data)?;

        // Build FM-index (SA-IS algorithm, O(n))
        let text_obj = Text::new(concatenated.clone());
        let index = FMIndexWithLocate::new(&text_obj, sampling_level)
            .map_err(|e| lokb_core::Error::Storage(format!("FM-index build: {e}")))?;

        let index_bytes = index.heap_size();
        let build_time_ms = start.elapsed().as_millis() as u64;

        // Cache in memory
        let mut inner = self.inner.lock().unwrap();
        *inner = Some(LoadedIndex {
            index,
            text: concatenated,
            position_map,
        });

        Ok(BuildMetrics {
            documents: documents.len(),
            text_bytes,
            index_bytes,
            build_time_ms,
        })
    }

    /// Ensure index is loaded (lazy load from disk on first access).
    fn ensure_loaded(&self) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if inner.is_some() {
            return Ok(());
        }

        let save_path = self.path.with_extension("fmdata");
        if !save_path.exists() {
            return Ok(()); // no index built yet
        }

        let data = fs::read(&save_path)?;
        let saved: SavedData = bincode::deserialize(&data)
            .map_err(|e| lokb_core::Error::Storage(format!("FM data deserialize: {e}")))?;

        let text_len = saved.text.len();
        let sampling_level = if text_len < 1_000_000 {
            2
        } else if text_len < 100_000_000 {
            8
        } else {
            32
        };

        let text_obj = Text::new(saved.text.clone());
        let index = FMIndexWithLocate::new(&text_obj, sampling_level)
            .map_err(|e| lokb_core::Error::Storage(format!("FM-index rebuild: {e}")))?;

        *inner = Some(LoadedIndex {
            index,
            text: saved.text,
            position_map: saved.position_map,
        });

        Ok(())
    }

    /// Search for exact substring. Returns up to `limit` hits with context.
    pub fn search(&self, pattern: &str, limit: usize) -> Result<Vec<SubstringHit>> {
        self.ensure_loaded()?;

        let inner = self.inner.lock().unwrap();
        let loaded = match inner.as_ref() {
            Some(l) => l,
            None => return Ok(vec![]),
        };

        let search_result = loaded.index.search(pattern.as_bytes());
        let mut hits = Vec::new();
        let mut seen_docs = std::collections::HashSet::new();

        for m in search_result.iter_matches().take(limit * 10) {
            let pos = m.locate();
            if let Some(hit) = Self::resolve_position_with_context(
                &loaded.position_map,
                &loaded.text,
                pos,
                pattern.len(),
            ) {
                let doc_key = format!("{}:{}", hit.source, hit.title);
                if seen_docs.insert(doc_key) {
                    hits.push(hit);
                    if hits.len() >= limit {
                        break;
                    }
                }
            }
        }

        Ok(hits)
    }

    /// Count occurrences of pattern (fast, O(m), no locate).
    pub fn count(&self, pattern: &str) -> Result<usize> {
        self.ensure_loaded()?;

        let inner = self.inner.lock().unwrap();
        match inner.as_ref() {
            Some(l) => Ok(l.index.search(pattern.as_bytes()).count()),
            None => Ok(0),
        }
    }

    /// Check if index has been built.
    pub fn is_built(&self) -> bool {
        let save_path = self.path.with_extension("fmdata");
        save_path.exists()
    }

    fn resolve_position_with_context(
        position_map: &[DocSpan],
        text: &[u8],
        pos: usize,
        pattern_len: usize,
    ) -> Option<SubstringHit> {
        let doc = position_map
            .iter()
            .find(|d| pos >= d.start && pos < d.end)?;

        let local_pos = pos - doc.start;

        // Extract context: 60 chars before and after match
        let ctx_before = 60;
        let ctx_after = 60;
        let ctx_start = pos.saturating_sub(ctx_before);
        let ctx_end = (pos + pattern_len + ctx_after).min(doc.end).min(text.len());

        let context_bytes = &text[ctx_start..ctx_end];
        let context = String::from_utf8_lossy(context_bytes)
            .replace('\n', " ")
            .replace('\r', "");

        // Add ellipsis
        let prefix = if ctx_start > doc.start { "..." } else { "" };
        let suffix = if ctx_end < doc.end { "..." } else { "" };

        Some(SubstringHit {
            source: doc.source.clone(),
            title: doc.title.clone(),
            position: local_pos,
            context: format!("{prefix}{context}{suffix}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_and_search_with_context() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.fm");

        let index = SubstringIndex::open(&path).unwrap();

        let docs = vec![
            (
                "wiki",
                "Paris",
                "Paris is the capital of France. The Eiffel Tower is in Paris.",
            ),
            (
                "wiki",
                "Rust",
                "Rust is a systems programming language. Rust ensures memory safety.",
            ),
        ];

        let metrics = index.build(&docs).unwrap();
        assert_eq!(metrics.documents, 2);
        assert!(metrics.index_bytes > 0);

        // Exact substring search with context
        let hits = index.search("Eiffel Tower", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Paris");
        assert!(hits[0].context.contains("Eiffel Tower"));

        // Count
        assert_eq!(index.count("Paris").unwrap(), 2);
        assert_eq!(index.count("nonexistent").unwrap(), 0);
    }

    #[test]
    fn test_lazy_load_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.fm");

        // Build
        {
            let index = SubstringIndex::open(&path).unwrap();
            let docs = vec![("src", "doc1", "hello world foo bar")];
            index.build(&docs).unwrap();
        }

        // Reopen — lazy load on first search
        let index = SubstringIndex::open(&path).unwrap();
        assert!(index.is_built());
        let hits = index.search("hello", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].context.contains("hello world"));
    }

    #[test]
    fn test_empty_index() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.fm");
        let index = SubstringIndex::open(&path).unwrap();
        assert!(!index.is_built());
        assert_eq!(index.count("anything").unwrap(), 0);
        assert!(index.search("anything", 10).unwrap().is_empty());
    }

    #[test]
    fn test_special_characters() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("special.fm");
        let index = SubstringIndex::open(&path).unwrap();

        let docs = vec![(
            "math",
            "formulas",
            "The equation E=mc^2 changed physics. Also 3.14159 is pi.",
        )];
        index.build(&docs).unwrap();

        assert_eq!(index.count("E=mc^2").unwrap(), 1);
        assert_eq!(index.count("3.14159").unwrap(), 1);
        assert_eq!(index.count("E=mc").unwrap(), 1);
    }
}
