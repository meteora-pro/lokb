use lokb_core::{DocumentId, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// File-based content store. Each source has a directory with document files.
/// Future: replace with cluster-bundle zstd (ADR-001).
pub struct FileContentStore {
    base_dir: PathBuf,
}

impl FileContentStore {
    pub fn new(base_dir: &Path) -> Self {
        Self {
            base_dir: base_dir.to_path_buf(),
        }
    }

    fn source_dir(&self, source_name: &str) -> PathBuf {
        self.base_dir.join(source_name)
    }

    fn doc_path(&self, source_name: &str, doc_id: DocumentId) -> PathBuf {
        self.source_dir(source_name).join(doc_id.to_string())
    }

    pub fn write_document(
        &self,
        source_name: &str,
        doc_id: DocumentId,
        content: &[u8],
    ) -> Result<()> {
        let dir = self.source_dir(source_name);
        fs::create_dir_all(&dir)?;
        fs::write(self.doc_path(source_name, doc_id), content)?;
        Ok(())
    }

    pub fn read_document(&self, source_name: &str, doc_id: DocumentId) -> Result<Option<Vec<u8>>> {
        let path = self.doc_path(source_name, doc_id);
        if path.exists() {
            Ok(Some(fs::read(path)?))
        } else {
            Ok(None)
        }
    }

    /// Read by filename (legacy, for backward compatibility with current CLI).
    pub fn read_by_filename(&self, source_name: &str, filename: &str) -> Result<Option<String>> {
        for ext in ["md", "txt", ""] {
            let name = if ext.is_empty() {
                filename.to_string()
            } else {
                format!("{filename}.{ext}")
            };
            let path = self.source_dir(source_name).join(&name);
            if path.exists() {
                return Ok(Some(fs::read_to_string(path)?));
            }
        }
        Ok(None)
    }

    /// Copy a file into the content store (for markdown-dir ingestion).
    pub fn copy_file(&self, source_name: &str, src: &Path, filename: &str) -> Result<()> {
        let dir = self.source_dir(source_name);
        fs::create_dir_all(&dir)?;
        fs::copy(src, dir.join(filename))?;
        Ok(())
    }

    /// Write text content by filename.
    pub fn write_file(&self, source_name: &str, filename: &str, content: &str) -> Result<()> {
        let dir = self.source_dir(source_name);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join(filename), content)?;
        Ok(())
    }

    /// List all files in a source directory.
    pub fn list_files(&self, source_name: &str) -> Result<Vec<(String, String)>> {
        let dir = self.source_dir(source_name);
        if !dir.exists() {
            return Ok(vec![]);
        }
        let mut files = vec![];
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                let filename = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let content = fs::read_to_string(&path).unwrap_or_default();
                files.push((filename, content));
            }
        }
        Ok(files)
    }

    /// Calculate total size of a source directory.
    pub fn source_size(&self, source_name: &str) -> u64 {
        dir_size(&self.source_dir(source_name))
    }

    /// Calculate total size of all content.
    pub fn total_size(&self) -> u64 {
        dir_size(&self.base_dir)
    }
}

fn dir_size(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    fs::read_dir(path)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| {
                    let p = e.path();
                    if p.is_dir() {
                        dir_size(&p)
                    } else {
                        e.metadata().map(|m| m.len()).unwrap_or(0)
                    }
                })
                .sum()
        })
        .unwrap_or(0)
}
