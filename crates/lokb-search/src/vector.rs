use lokb_core::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Simple file-based vector index. Brute-force cosine similarity search.
/// Future: replace with LanceDB IVF-PQ for millions of vectors.
pub struct VectorIndex {
    path: PathBuf,
    entries: Vec<VectorEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorEntry {
    pub chunk_id: String,
    pub source_name: String,
    pub title: String,
    pub text: String,
    pub vector: Vec<f32>,
    pub privacy_level: u8,
}

#[derive(Debug, Clone)]
pub struct VectorHit {
    pub chunk_id: String,
    pub source_name: String,
    pub title: String,
    pub text: String,
    pub score: f32,
}

impl VectorIndex {
    pub fn open(path: &Path) -> Result<Self> {
        let entries = if path.exists() {
            let data = fs::read(path)?;
            bincode_deserialize(&data)
        } else {
            vec![]
        };

        Ok(Self {
            path: path.to_path_buf(),
            entries,
        })
    }

    /// Add vectors to the index.
    pub fn add_entries(&mut self, new_entries: Vec<VectorEntry>) -> Result<()> {
        self.entries.extend(new_entries);
        self.save()
    }

    /// Save index to disk.
    fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let data = bincode_serialize(&self.entries);
        fs::write(&self.path, data)?;
        Ok(())
    }

    /// Brute-force cosine similarity search.
    pub fn search(
        &self,
        query_vector: &[f32],
        limit: usize,
        source_filter: Option<&str>,
        personal_only: bool,
        public_only: bool,
    ) -> Vec<VectorHit> {
        let mut scored: Vec<(f32, &VectorEntry)> = self
            .entries
            .iter()
            .filter(|e| {
                if personal_only && e.privacy_level == 0 {
                    return false;
                }
                if public_only && e.privacy_level > 0 {
                    return false;
                }
                if let Some(filter) = source_filter
                    && e.source_name != filter
                {
                    return false;
                }
                true
            })
            .map(|e| (cosine_similarity(query_vector, &e.vector), e))
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        scored
            .into_iter()
            .map(|(score, e)| VectorHit {
                chunk_id: e.chunk_id.clone(),
                source_name: e.source_name.clone(),
                title: e.title.clone(),
                text: e.text.clone(),
                score,
            })
            .collect()
    }

    /// Delete all entries for a source.
    pub fn delete_source(&mut self, source_name: &str) -> Result<()> {
        self.entries.retain(|e| e.source_name != source_name);
        self.save()
    }

    /// Number of vectors in the index.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

/// Simple serialization using JSON (replace with bincode for performance later).
fn bincode_serialize(entries: &[VectorEntry]) -> Vec<u8> {
    serde_json::to_vec(entries).unwrap_or_default()
}

fn bincode_deserialize(data: &[u8]) -> Vec<VectorEntry> {
    serde_json::from_slice(data).unwrap_or_default()
}
