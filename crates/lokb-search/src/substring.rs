//! FM-index based exact substring search (ADR-008).
//!
//! Provides O(m) substring search across the entire text corpus
//! where m is the pattern length. Built on Burrows-Wheeler Transform.

use fm_index::{FMIndexWithLocate, MatchWithLocate, Search, SearchIndex, Text};
use lokb_core::Result;
use std::fs;
use std::path::Path;

const SAMPLING_LEVEL: usize = 2;

/// FM-index for exact substring search across all ingested text.
pub struct SubstringIndex {
    index: Option<FMIndexWithLocate<u8>>,
    position_map: Vec<DocSpan>,
    path: std::path::PathBuf,
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

/// A substring search hit.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SubstringHit {
    pub source: String,
    pub title: String,
    pub position: usize,
    pub context: String,
}

impl SubstringIndex {
    /// Open existing index or create empty.
    pub fn open(path: &Path) -> Result<Self> {
        let save_path = path.with_extension("fmdata");

        if save_path.exists() {
            let data = fs::read(&save_path)?;
            let saved: SavedData = bincode::deserialize(&data)
                .map_err(|e| lokb_core::Error::Storage(format!("FM data deserialize: {e}")))?;

            // Rebuild FM-index from saved text
            let text = Text::new(saved.text);
            let index = FMIndexWithLocate::new(&text, SAMPLING_LEVEL)
                .map_err(|e| lokb_core::Error::Storage(format!("FM-index rebuild: {e}")))?;

            Ok(Self {
                index: Some(index),
                position_map: saved.position_map,
                path: path.to_path_buf(),
            })
        } else {
            Ok(Self {
                index: None,
                position_map: vec![],
                path: path.to_path_buf(),
            })
        }
    }

    /// Build FM-index from a collection of (source, title, text) documents.
    pub fn build(&mut self, documents: &[(&str, &str, &str)]) -> Result<BuildMetrics> {
        let start = std::time::Instant::now();

        let mut concatenated = Vec::new();
        let mut position_map = Vec::new();

        for (source, title, text) in documents {
            let start_pos = concatenated.len();
            // Replace any null bytes in text with spaces (FM-index reserves 0)
            let clean_text: Vec<u8> = text
                .as_bytes()
                .iter()
                .map(|&b| if b == 0 { b' ' } else { b })
                .collect();
            concatenated.extend_from_slice(&clean_text);
            let end_pos = concatenated.len();
            position_map.push(DocSpan {
                source: source.to_string(),
                title: title.to_string(),
                start: start_pos,
                end: end_pos,
            });
            // Separator between documents (byte 1, non-zero)
            concatenated.push(1);
        }
        // FM-index requires text to end with exactly one null byte
        if let Some(last) = concatenated.last_mut() {
            *last = 0;
        }

        if concatenated.is_empty() {
            self.index = None;
            self.position_map = vec![];
            return Ok(BuildMetrics {
                documents: 0,
                text_bytes: 0,
                index_bytes: 0,
                build_time_ms: 0,
            });
        }

        let text_bytes = concatenated.len();

        // Save text + map for future reloads
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

        // Build FM-index
        let text = Text::new(concatenated);
        let index = FMIndexWithLocate::new(&text, SAMPLING_LEVEL)
            .map_err(|e| lokb_core::Error::Storage(format!("FM-index build: {e}")))?;

        let index_bytes = index.heap_size();
        let build_time_ms = start.elapsed().as_millis() as u64;

        self.index = Some(index);
        self.position_map = position_map;

        Ok(BuildMetrics {
            documents: documents.len(),
            text_bytes,
            index_bytes,
            build_time_ms,
        })
    }

    /// Search for exact substring. Returns up to `limit` hits.
    pub fn search(&self, pattern: &str, limit: usize) -> Vec<SubstringHit> {
        let index = match &self.index {
            Some(idx) => idx,
            None => return vec![],
        };

        let search_result = index.search(pattern.as_bytes());
        let mut hits = Vec::new();
        let mut seen_docs = std::collections::HashSet::new();

        for m in search_result.iter_matches().take(limit * 5) {
            let pos = m.locate();
            if let Some(hit) = self.resolve_position(pos) {
                let doc_key = format!("{}:{}", hit.source, hit.title);
                if seen_docs.insert(doc_key) {
                    hits.push(hit);
                    if hits.len() >= limit {
                        break;
                    }
                }
            }
        }

        hits
    }

    /// Count occurrences of pattern (fast, O(m)).
    pub fn count(&self, pattern: &str) -> usize {
        match &self.index {
            Some(idx) => idx.search(pattern.as_bytes()).count(),
            None => 0,
        }
    }

    /// Check if index has been built.
    pub fn is_built(&self) -> bool {
        self.index.is_some()
    }

    fn resolve_position(&self, pos: usize) -> Option<SubstringHit> {
        let doc = self
            .position_map
            .iter()
            .find(|d| pos >= d.start && pos < d.end)?;

        let local_pos = pos - doc.start;

        Some(SubstringHit {
            source: doc.source.clone(),
            title: doc.title.clone(),
            position: local_pos,
            context: format!("at position {local_pos} in \"{}\"", doc.title),
        })
    }
}

/// Metrics from building the FM-index.
#[derive(Debug, serde::Serialize)]
pub struct BuildMetrics {
    pub documents: usize,
    pub text_bytes: usize,
    pub index_bytes: usize,
    pub build_time_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_and_search() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.fm");

        let mut index = SubstringIndex::open(&path).unwrap();

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

        // Exact substring search
        let hits = index.search("Eiffel Tower", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Paris");

        // Count
        assert_eq!(index.count("Paris"), 2);
        assert_eq!(index.count("nonexistent"), 0);
    }

    #[test]
    fn test_serialize_deserialize() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.fm");

        {
            let mut index = SubstringIndex::open(&path).unwrap();
            let docs = vec![("src", "doc1", "hello world")];
            index.build(&docs).unwrap();
        }

        // Reopen from disk
        let index = SubstringIndex::open(&path).unwrap();
        assert!(index.is_built());
        assert_eq!(index.count("hello"), 1);
    }

    #[test]
    fn test_empty_index() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.fm");
        let index = SubstringIndex::open(&path).unwrap();
        assert!(!index.is_built());
        assert_eq!(index.count("anything"), 0);
        assert!(index.search("anything", 10).is_empty());
    }
}
