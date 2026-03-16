//! FM-index based exact substring search (ADR-008).
//!
//! Production implementation with block-based building:
//! - Splits corpus into blocks of BLOCK_SIZE bytes
//! - Builds FM-index per block (fits in RAM)
//! - Searches across all blocks, merges results
//! - In-memory cache, context extraction, adaptive sampling

use fm_index::{FMIndexWithLocate, MatchWithLocate, Search, SearchIndex, Text};
use lokb_core::Result;
use std::fs;
use std::path::Path;
use std::sync::Mutex;

/// Max bytes per FM-index block. 500MB fits comfortably in 8GB RAM.
const BLOCK_SIZE: usize = 500 * 1024 * 1024;

/// FM-index for exact substring search across all ingested text.
pub struct SubstringIndex {
    inner: Mutex<Option<LoadedBlocks>>,
    path: std::path::PathBuf,
}

struct LoadedBlocks {
    blocks: Vec<LoadedBlock>,
}

struct LoadedBlock {
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
struct SavedBlock {
    text: Vec<u8>,
    position_map: Vec<DocSpan>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SavedData {
    blocks: Vec<SavedBlock>,
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
    pub blocks: usize,
}

impl SubstringIndex {
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            inner: Mutex::new(None),
            path: path.to_path_buf(),
        })
    }

    /// Build FM-index from documents. Automatically splits into blocks if >BLOCK_SIZE.
    pub fn build(&self, documents: &[(&str, &str, &str)]) -> Result<BuildMetrics> {
        let start = std::time::Instant::now();

        // Collect all document bytes with position maps
        let mut all_docs: Vec<(String, String, Vec<u8>)> = Vec::new();
        for (source, title, text) in documents {
            let clean: Vec<u8> = text
                .as_bytes()
                .iter()
                .map(|&b| if b == 0 { b' ' } else { b })
                .collect();
            all_docs.push((source.to_string(), title.to_string(), clean));
        }

        if all_docs.is_empty() {
            let mut inner = self.inner.lock().unwrap();
            *inner = None;
            return Ok(BuildMetrics {
                documents: 0,
                text_bytes: 0,
                index_bytes: 0,
                build_time_ms: 0,
                blocks: 0,
            });
        }

        // Split into blocks
        let mut saved_blocks = Vec::new();
        let mut current_text = Vec::new();
        let mut current_map = Vec::new();
        let mut total_text_bytes = 0usize;

        for (source, title, content) in &all_docs {
            // If adding this doc exceeds block size and we have content, flush block
            if !current_text.is_empty() && current_text.len() + content.len() > BLOCK_SIZE {
                // Finalize block: null-terminate
                if let Some(last) = current_text.last_mut() {
                    *last = 0;
                }
                saved_blocks.push(SavedBlock {
                    text: std::mem::take(&mut current_text),
                    position_map: std::mem::take(&mut current_map),
                });
            }

            let start_pos = current_text.len();
            current_text.extend_from_slice(content);
            let end_pos = current_text.len();
            current_map.push(DocSpan {
                source: source.clone(),
                title: title.clone(),
                start: start_pos,
                end: end_pos,
            });
            current_text.push(1); // separator
            total_text_bytes += content.len() + 1;
        }

        // Flush last block
        if !current_text.is_empty() {
            if let Some(last) = current_text.last_mut() {
                *last = 0;
            }
            saved_blocks.push(SavedBlock {
                text: current_text,
                position_map: current_map,
            });
        }

        let block_count = saved_blocks.len();
        eprintln!(
            "  FM-index: building {} block(s) from {} docs...",
            block_count,
            documents.len()
        );

        // Save to disk
        let save_path = self.path.with_extension("fmdata");
        if let Some(parent) = save_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let saved = SavedData {
            blocks: saved_blocks.clone(),
        };
        let save_data = bincode::serialize(&saved)
            .map_err(|e| lokb_core::Error::Storage(format!("FM serialize: {e}")))?;
        fs::write(&save_path, save_data)?;

        // Build FM-indexes for each block
        let mut loaded_blocks = Vec::new();
        let mut total_index_bytes = 0usize;

        for (i, block) in saved_blocks.into_iter().enumerate() {
            let sampling = adaptive_sampling(block.text.len());
            let text_obj = Text::new(block.text.clone());
            let index = FMIndexWithLocate::new(&text_obj, sampling)
                .map_err(|e| lokb_core::Error::Storage(format!("FM block {i}: {e}")))?;
            total_index_bytes += index.heap_size();
            loaded_blocks.push(LoadedBlock {
                index,
                text: block.text,
                position_map: block.position_map,
            });
        }

        let build_time_ms = start.elapsed().as_millis() as u64;

        let mut inner = self.inner.lock().unwrap();
        *inner = Some(LoadedBlocks {
            blocks: loaded_blocks,
        });

        Ok(BuildMetrics {
            documents: documents.len(),
            text_bytes: total_text_bytes,
            index_bytes: total_index_bytes,
            build_time_ms,
            blocks: block_count,
        })
    }

    fn ensure_loaded(&self) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if inner.is_some() {
            return Ok(());
        }

        let save_path = self.path.with_extension("fmdata");
        if !save_path.exists() {
            return Ok(());
        }

        let data = fs::read(&save_path)?;
        let saved: SavedData = bincode::deserialize(&data)
            .map_err(|e| lokb_core::Error::Storage(format!("FM deserialize: {e}")))?;

        let mut loaded_blocks = Vec::new();
        for block in saved.blocks {
            let sampling = adaptive_sampling(block.text.len());
            let text_obj = Text::new(block.text.clone());
            let index = FMIndexWithLocate::new(&text_obj, sampling)
                .map_err(|e| lokb_core::Error::Storage(format!("FM rebuild: {e}")))?;
            loaded_blocks.push(LoadedBlock {
                index,
                text: block.text,
                position_map: block.position_map,
            });
        }

        *inner = Some(LoadedBlocks {
            blocks: loaded_blocks,
        });
        Ok(())
    }

    /// Search across all blocks. Returns up to `limit` hits with context.
    pub fn search(&self, pattern: &str, limit: usize) -> Result<Vec<SubstringHit>> {
        self.ensure_loaded()?;

        let inner = self.inner.lock().unwrap();
        let loaded = match inner.as_ref() {
            Some(l) => l,
            None => return Ok(vec![]),
        };

        let mut all_hits = Vec::new();
        let mut seen_docs = std::collections::HashSet::new();

        for block in &loaded.blocks {
            let search_result = block.index.search(pattern.as_bytes());
            for m in search_result.iter_matches().take(limit * 10) {
                let pos = m.locate();
                if let Some(hit) =
                    resolve_position(&block.position_map, &block.text, pos, pattern.len())
                {
                    let doc_key = format!("{}:{}", hit.source, hit.title);
                    if seen_docs.insert(doc_key) {
                        all_hits.push(hit);
                        if all_hits.len() >= limit {
                            return Ok(all_hits);
                        }
                    }
                }
            }
        }

        Ok(all_hits)
    }

    /// Count occurrences across all blocks.
    pub fn count(&self, pattern: &str) -> Result<usize> {
        self.ensure_loaded()?;

        let inner = self.inner.lock().unwrap();
        match inner.as_ref() {
            Some(loaded) => {
                let total: usize = loaded
                    .blocks
                    .iter()
                    .map(|b| b.index.search(pattern.as_bytes()).count())
                    .sum();
                Ok(total)
            }
            None => Ok(0),
        }
    }

    pub fn is_built(&self) -> bool {
        self.path.with_extension("fmdata").exists()
    }
}

fn adaptive_sampling(text_len: usize) -> usize {
    if text_len < 1_000_000 {
        2
    } else if text_len < 100_000_000 {
        8
    } else {
        32
    }
}

fn resolve_position(
    position_map: &[DocSpan],
    text: &[u8],
    pos: usize,
    pattern_len: usize,
) -> Option<SubstringHit> {
    let doc = position_map
        .iter()
        .find(|d| pos >= d.start && pos < d.end)?;

    let local_pos = pos - doc.start;
    let ctx_start = pos.saturating_sub(60);
    let ctx_end = (pos + pattern_len + 60).min(doc.end).min(text.len());

    let context_bytes = &text[ctx_start..ctx_end];
    let context = String::from_utf8_lossy(context_bytes)
        .replace('\n', " ")
        .replace('\r', "");

    let prefix = if ctx_start > doc.start { "..." } else { "" };
    let suffix = if ctx_end < doc.end { "..." } else { "" };

    Some(SubstringHit {
        source: doc.source.clone(),
        title: doc.title.clone(),
        position: local_pos,
        context: format!("{prefix}{context}{suffix}"),
    })
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
        assert_eq!(metrics.blocks, 1);

        let hits = index.search("Eiffel Tower", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Paris");
        assert!(hits[0].context.contains("Eiffel Tower"));

        assert_eq!(index.count("Paris").unwrap(), 2);
        assert_eq!(index.count("nonexistent").unwrap(), 0);
    }

    #[test]
    fn test_lazy_load_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.fm");
        {
            let index = SubstringIndex::open(&path).unwrap();
            index
                .build(&[("src", "doc1", "hello world foo bar")])
                .unwrap();
        }
        let index = SubstringIndex::open(&path).unwrap();
        assert!(index.is_built());
        assert_eq!(index.count("hello").unwrap(), 1);
    }

    #[test]
    fn test_empty_index() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.fm");
        let index = SubstringIndex::open(&path).unwrap();
        assert!(!index.is_built());
        assert_eq!(index.count("anything").unwrap(), 0);
    }

    #[test]
    fn test_special_characters() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("special.fm");
        let index = SubstringIndex::open(&path).unwrap();
        index
            .build(&[("math", "formulas", "E=mc^2 and 3.14159 is pi.")])
            .unwrap();
        assert_eq!(index.count("E=mc^2").unwrap(), 1);
        assert_eq!(index.count("3.14159").unwrap(), 1);
    }
}
