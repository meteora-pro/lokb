//! Cluster-bundle zstd Content Store (ADR-001).
//!
//! Groups ~1000 documents per bundle, compresses as single zstd block.
//! Index: doc_id → (bundle_id, offset, len) for random access.

use lokb_core::Result;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const BUNDLE_TARGET_SIZE: usize = 1000; // docs per bundle

/// Bundle-based content store with zstd compression.
pub struct BundleContentStore {
    base_dir: PathBuf,
}

/// Index entry mapping document to its bundle location.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct BundleEntry {
    bundle_id: u32,
    offset: usize,
    length: usize,
}

/// Bundle index: maps external_id → location in bundle.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct BundleIndex {
    entries: HashMap<String, BundleEntry>,
    next_bundle_id: u32,
}

impl BundleContentStore {
    pub fn new(base_dir: &Path) -> Self {
        Self {
            base_dir: base_dir.to_path_buf(),
        }
    }

    fn source_dir(&self, source_name: &str) -> PathBuf {
        self.base_dir.join(source_name)
    }

    fn bundles_dir(&self, source_name: &str) -> PathBuf {
        self.source_dir(source_name).join("bundles")
    }

    fn index_path(&self, source_name: &str) -> PathBuf {
        self.source_dir(source_name).join("bundle_index.json")
    }

    fn bundle_path(&self, source_name: &str, bundle_id: u32) -> PathBuf {
        self.bundles_dir(source_name)
            .join(format!("{:04}.zst", bundle_id))
    }

    fn load_index(&self, source_name: &str) -> Result<BundleIndex> {
        let path = self.index_path(source_name);
        if path.exists() {
            let data = fs::read_to_string(&path)?;
            serde_json::from_str(&data).map_err(lokb_core::Error::Json)
        } else {
            Ok(BundleIndex::default())
        }
    }

    fn save_index(&self, source_name: &str, index: &BundleIndex) -> Result<()> {
        let path = self.index_path(source_name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(index)?;
        fs::write(path, data)?;
        Ok(())
    }

    /// Write multiple documents as bundles.
    /// Documents: Vec<(external_id, content)>
    pub fn write_batch(
        &self,
        source_name: &str,
        documents: &[(&str, &[u8])],
    ) -> Result<WriteMetrics> {
        let mut index = self.load_index(source_name)?;
        let bundles_dir = self.bundles_dir(source_name);
        fs::create_dir_all(&bundles_dir)?;

        let mut total_raw = 0u64;
        let mut total_compressed = 0u64;
        let mut bundles_created = 0u32;

        // Group into bundles of BUNDLE_TARGET_SIZE
        for chunk in documents.chunks(BUNDLE_TARGET_SIZE) {
            let bundle_id = index.next_bundle_id;
            index.next_bundle_id += 1;

            // Concatenate documents with length prefixes
            let mut raw_data = Vec::new();
            let mut entries = Vec::new();

            for (external_id, content) in chunk {
                let offset = raw_data.len();
                raw_data.extend_from_slice(content);
                let length = content.len();

                entries.push((
                    external_id.to_string(),
                    BundleEntry {
                        bundle_id,
                        offset,
                        length,
                    },
                ));
            }

            total_raw += raw_data.len() as u64;

            // Compress bundle
            let compressed = zstd::encode_all(&raw_data[..], 3)
                .map_err(|e| lokb_core::Error::Storage(format!("zstd compress: {e}")))?;
            total_compressed += compressed.len() as u64;

            // Write bundle file
            let bundle_path = self.bundle_path(source_name, bundle_id);
            fs::write(&bundle_path, &compressed)?;

            // Update index
            for (ext_id, entry) in entries {
                index.entries.insert(ext_id, entry);
            }

            bundles_created += 1;
        }

        self.save_index(source_name, &index)?;

        Ok(WriteMetrics {
            documents: documents.len(),
            bundles_created,
            raw_bytes: total_raw,
            compressed_bytes: total_compressed,
            compression_ratio: if total_compressed > 0 {
                total_raw as f64 / total_compressed as f64
            } else {
                0.0
            },
        })
    }

    /// Read a single document by external_id.
    pub fn read_document(&self, source_name: &str, external_id: &str) -> Result<Option<Vec<u8>>> {
        let index = self.load_index(source_name)?;
        let entry = match index.entries.get(external_id) {
            Some(e) => e,
            None => return Ok(None),
        };

        let bundle_path = self.bundle_path(source_name, entry.bundle_id);
        if !bundle_path.exists() {
            return Ok(None);
        }

        let compressed = fs::read(&bundle_path)?;
        let decompressed = zstd::decode_all(&compressed[..])
            .map_err(|e| lokb_core::Error::Storage(format!("zstd decompress: {e}")))?;

        if entry.offset + entry.length > decompressed.len() {
            return Err(lokb_core::Error::Storage(
                "bundle entry out of bounds".into(),
            ));
        }

        Ok(Some(
            decompressed[entry.offset..entry.offset + entry.length].to_vec(),
        ))
    }

    /// Check if a document exists.
    pub fn exists(&self, source_name: &str, external_id: &str) -> bool {
        self.load_index(source_name)
            .map(|idx| idx.entries.contains_key(external_id))
            .unwrap_or(false)
    }

    /// Delete all bundles for a source.
    pub fn delete_source(&self, source_name: &str) -> Result<()> {
        let dir = self.source_dir(source_name);
        if dir.exists() {
            fs::remove_dir_all(dir)?;
        }
        Ok(())
    }

    /// Total size of all bundles for a source.
    pub fn source_size(&self, source_name: &str) -> u64 {
        let bundles_dir = self.bundles_dir(source_name);
        if !bundles_dir.exists() {
            return 0;
        }
        fs::read_dir(bundles_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
                    .sum()
            })
            .unwrap_or(0)
    }

    /// Total size of all sources.
    pub fn total_size(&self) -> u64 {
        if !self.base_dir.exists() {
            return 0;
        }
        fs::read_dir(&self.base_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().is_dir())
                    .map(|e| self.source_size(&e.file_name().to_string_lossy()))
                    .sum()
            })
            .unwrap_or(0)
    }

    /// Migrate from FileContentStore (plain files → bundles).
    pub fn migrate_from_files(&self, source_name: &str, files_dir: &Path) -> Result<WriteMetrics> {
        let mut documents: Vec<(String, Vec<u8>)> = Vec::new();

        if files_dir.exists() {
            for entry in fs::read_dir(files_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    let filename = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let content = fs::read(&path)?;
                    documents.push((filename, content));
                }
            }
        }

        let doc_refs: Vec<(&str, &[u8])> = documents
            .iter()
            .map(|(name, content)| (name.as_str(), content.as_slice()))
            .collect();

        self.write_batch(source_name, &doc_refs)
    }
}

/// Metrics from writing bundles.
#[derive(Debug, serde::Serialize)]
pub struct WriteMetrics {
    pub documents: usize,
    pub bundles_created: u32,
    pub raw_bytes: u64,
    pub compressed_bytes: u64,
    pub compression_ratio: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_and_read_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let store = BundleContentStore::new(dir.path());

        let docs = vec![
            ("doc1", b"Hello World".as_slice()),
            ("doc2", b"Rust Programming Language"),
            ("doc3", b"Knowledge Base"),
        ];

        let metrics = store.write_batch("test-source", &docs).unwrap();
        assert_eq!(metrics.documents, 3);
        assert_eq!(metrics.bundles_created, 1);
        assert!(metrics.compression_ratio > 0.0);

        // Read back
        let content = store.read_document("test-source", "doc1").unwrap().unwrap();
        assert_eq!(content, b"Hello World");

        let content = store.read_document("test-source", "doc3").unwrap().unwrap();
        assert_eq!(content, b"Knowledge Base");

        // Non-existent
        assert!(
            store
                .read_document("test-source", "nonexistent")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn test_multiple_bundles() {
        let dir = tempfile::tempdir().unwrap();
        let store = BundleContentStore::new(dir.path());

        // Create more docs than BUNDLE_TARGET_SIZE
        let mut docs: Vec<(String, Vec<u8>)> = Vec::new();
        for i in 0..2500 {
            docs.push((
                format!("doc_{i:04}"),
                format!("Content of document {i}").into_bytes(),
            ));
        }
        let doc_refs: Vec<(&str, &[u8])> = docs
            .iter()
            .map(|(name, content)| (name.as_str(), content.as_slice()))
            .collect();

        let metrics = store.write_batch("large", &doc_refs).unwrap();
        assert_eq!(metrics.documents, 2500);
        assert_eq!(metrics.bundles_created, 3); // 1000 + 1000 + 500

        // Read from different bundles
        let c0 = store.read_document("large", "doc_0000").unwrap().unwrap();
        assert_eq!(c0, b"Content of document 0");

        let c1500 = store.read_document("large", "doc_1500").unwrap().unwrap();
        assert_eq!(c1500, b"Content of document 1500");

        let c2499 = store.read_document("large", "doc_2499").unwrap().unwrap();
        assert_eq!(c2499, b"Content of document 2499");
    }

    #[test]
    fn test_delete_source() {
        let dir = tempfile::tempdir().unwrap();
        let store = BundleContentStore::new(dir.path());

        let docs = vec![("doc1", b"test".as_slice())];
        store.write_batch("src", &docs).unwrap();
        assert!(store.exists("src", "doc1"));

        store.delete_source("src").unwrap();
        assert!(!store.exists("src", "doc1"));
    }
}
