//! Safe ZIM file reader.
//!
//! Implements the ZIM file format specification from openzim.org.
//! Uses file I/O with seek (not mmap) to handle files of any size
//! without excessive memory usage.

use lokb_core::Result;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const ZIM_MAGIC: u32 = 72_173_914; // 0x44D495A

// ── Header ───────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct ZimHeader {
    pub magic: u32,
    pub major_version: u16,
    pub minor_version: u16,
    pub uuid: [u8; 16],
    pub entry_count: u32,
    pub cluster_count: u32,
    pub path_ptr_pos: u64,
    pub title_ptr_pos: u64,
    pub cluster_ptr_pos: u64,
    pub mime_list_pos: u64,
    pub main_page: u32,
    pub checksum_pos: u64,
}

impl ZimHeader {
    fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 80 {
            return Err(lokb_core::Error::Storage(
                "ZIM file too small for header".into(),
            ));
        }

        let magic = read_u32(data, 0);
        if magic != ZIM_MAGIC {
            return Err(lokb_core::Error::Storage(format!(
                "invalid ZIM magic: {magic:#x}, expected {ZIM_MAGIC:#x}"
            )));
        }

        let mut uuid = [0u8; 16];
        uuid.copy_from_slice(&data[8..24]);

        Ok(Self {
            magic,
            major_version: read_u16(data, 4),
            minor_version: read_u16(data, 6),
            uuid,
            entry_count: read_u32(data, 24),
            cluster_count: read_u32(data, 28),
            path_ptr_pos: read_u64(data, 32),
            title_ptr_pos: read_u64(data, 40),
            cluster_ptr_pos: read_u64(data, 48),
            mime_list_pos: read_u64(data, 56),
            main_page: read_u32(data, 64),
            checksum_pos: read_u64(data, 72),
        })
    }
}

// ── Directory Entry ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum DirEntry {
    Content {
        mime_type: u16,
        namespace: u8,
        cluster_number: u32,
        blob_number: u32,
        path: String,
        title: String,
    },
    Redirect {
        namespace: u8,
        target_index: u32,
        path: String,
        title: String,
    },
}

impl DirEntry {
    fn parse(data: &[u8], file_len: u64) -> Result<Self> {
        if data.len() < 4 {
            return Err(lokb_core::Error::Storage("entry too small".into()));
        }

        let mime_type = read_u16(data, 0);
        let _param_len = data[2];
        let namespace = data[3];

        if mime_type == 0xFFFF {
            // Redirect entry
            if data.len() < 12 {
                return Err(lokb_core::Error::Storage("redirect entry too small".into()));
            }
            let target_index = read_u32(data, 8);
            let (path, title) = parse_path_title(&data[12..], file_len)?;
            Ok(DirEntry::Redirect {
                namespace,
                target_index,
                path,
                title,
            })
        } else {
            // Content entry
            if data.len() < 16 {
                return Err(lokb_core::Error::Storage("content entry too small".into()));
            }
            let cluster_number = read_u32(data, 8);
            let blob_number = read_u32(data, 12);
            let (path, title) = parse_path_title(&data[16..], file_len)?;
            Ok(DirEntry::Content {
                mime_type,
                namespace,
                cluster_number,
                blob_number,
                path,
                title,
            })
        }
    }

    pub fn namespace_char(&self) -> char {
        let ns = match self {
            DirEntry::Content { namespace, .. } => *namespace,
            DirEntry::Redirect { namespace, .. } => *namespace,
        };
        ns as char
    }

    pub fn path(&self) -> &str {
        match self {
            DirEntry::Content { path, .. } => path,
            DirEntry::Redirect { path, .. } => path,
        }
    }

    pub fn title(&self) -> &str {
        match self {
            DirEntry::Content { title, .. } => {
                if title.is_empty() {
                    self.path()
                } else {
                    title
                }
            }
            DirEntry::Redirect { title, .. } => {
                if title.is_empty() {
                    self.path()
                } else {
                    title
                }
            }
        }
    }

    pub fn is_content(&self) -> bool {
        matches!(self, DirEntry::Content { .. })
    }

    pub fn is_article(&self) -> bool {
        self.is_content() && (self.namespace_char() == 'C' || self.namespace_char() == 'A')
    }
}

// ── ZIM Reader ───────────────────────────────────────────────────────

/// Safe, read-only ZIM file reader using file I/O (not mmap).
/// Memory-efficient: reads only what's needed via seek+read.
pub struct ZimReader {
    file: std::sync::Mutex<std::fs::File>,
    file_len: u64,
    header: ZimHeader,
    mime_types: Vec<String>,
}

/// An article extracted from a ZIM file.
#[derive(Debug, Clone)]
pub struct ZimArticle {
    pub path: String,
    pub title: String,
    pub mime_type: String,
    pub content: String,
}

impl ZimReader {
    /// Open a ZIM file for reading.
    pub fn open(path: &Path) -> Result<Self> {
        let mut file = std::fs::File::open(path)?;
        let file_len = file.metadata()?.len();

        // Disable OS page cache for this file (critical for large files to prevent OOM).
        // macOS: F_NOCACHE=48, Linux: would use posix_fadvise(DONTNEED).
        #[cfg(target_os = "macos")]
        {
            use std::os::unix::io::AsRawFd;
            unsafe {
                libc::fcntl(file.as_raw_fd(), libc::F_NOCACHE, 1);
            }
        }

        // Read header (80 bytes)
        let mut header_buf = [0u8; 80];
        file.read_exact(&mut header_buf)?;
        let header = ZimHeader::parse(&header_buf)?;

        // Read mime types
        file.seek(SeekFrom::Start(header.mime_list_pos))?;
        let mut mime_buf = vec![0u8; 4096]; // mime list is small
        let n = file.read(&mut mime_buf)?;
        mime_buf.truncate(n);
        let mime_types = Self::parse_mime_types(&mime_buf)?;

        Ok(Self {
            file: std::sync::Mutex::new(file),
            file_len,
            header,
            mime_types,
        })
    }

    /// Get the header.
    pub fn header(&self) -> &ZimHeader {
        &self.header
    }

    /// Number of entries.
    pub fn entry_count(&self) -> u32 {
        self.header.entry_count
    }

    /// Number of clusters.
    pub fn cluster_count(&self) -> u32 {
        self.header.cluster_count
    }

    /// Read bytes from file at given offset.
    fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        let mut file = self.file.lock().unwrap();
        file.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0u8; len];
        file.read_exact(&mut buf).map_err(|e| {
            lokb_core::Error::Storage(format!("read at offset {offset}, len {len}: {e}"))
        })?;
        Ok(buf)
    }

    /// Get a directory entry by index.
    pub fn get_entry(&self, index: u32) -> Result<DirEntry> {
        let ptr_offset = self.header.path_ptr_pos + (index as u64) * 8;
        let ptr_buf = self.read_at(ptr_offset, 8)?;
        let entry_offset = read_u64(&ptr_buf, 0);

        // Read enough for entry header + path + title (512 bytes should be enough)
        let read_len = std::cmp::min(512, (self.file_len - entry_offset) as usize);
        let entry_buf = self.read_at(entry_offset, read_len)?;
        DirEntry::parse(&entry_buf, self.file_len)
    }

    /// Get the content of a blob (article) by cluster and blob number.
    pub fn get_blob(&self, cluster_number: u32, blob_number: u32) -> Result<Vec<u8>> {
        let blobs = self.decompress_cluster(cluster_number)?;
        let idx = blob_number as usize;
        if idx >= blobs.len() {
            return Err(lokb_core::Error::Storage(format!(
                "blob {blob_number} out of range (max {})",
                blobs.len()
            )));
        }
        Ok(blobs[idx].clone())
    }

    /// Iterate over all content articles (namespace C or A).
    /// NOTE: Collects all articles into memory. For large ZIM files, use `article_iter()` instead.
    pub fn articles(&self) -> Vec<ZimArticle> {
        self.article_iter().collect()
    }

    /// Streaming iterator over articles. Memory-efficient for large files.
    pub fn article_iter(&self) -> ZimArticleIter<'_> {
        ZimArticleIter {
            reader: self,
            entry_index: 0,
            cluster_cache: None,
        }
    }

    /// Count HTML articles without loading content.
    pub fn article_count(&self) -> usize {
        let mut count = 0;
        for i in 0..self.header.entry_count {
            let entry = match self.get_entry(i) {
                Ok(e) => e,
                Err(_) => continue,
            };
            if !entry.is_article() {
                continue;
            }
            if let DirEntry::Content { mime_type, .. } = &entry {
                let mime = self
                    .mime_types
                    .get(*mime_type as usize)
                    .cloned()
                    .unwrap_or_default();
                if mime.contains("html") {
                    count += 1;
                }
            }
        }
        count
    }

    /// Decompress a full cluster, returning all blobs.
    fn decompress_cluster(&self, cluster_number: u32) -> Result<Vec<Vec<u8>>> {
        if cluster_number >= self.header.cluster_count {
            return Err(lokb_core::Error::Storage(format!(
                "cluster {cluster_number} out of range"
            )));
        }

        // Read cluster pointer(s)
        let ptr_offset = self.header.cluster_ptr_pos + (cluster_number as u64) * 8;
        let cluster_offset;
        let cluster_end;

        if cluster_number + 1 < self.header.cluster_count {
            let ptrs_buf = self.read_at(ptr_offset, 16)?;
            cluster_offset = read_u64(&ptrs_buf, 0);
            cluster_end = read_u64(&ptrs_buf, 8);
        } else {
            let ptr_buf = self.read_at(ptr_offset, 8)?;
            cluster_offset = read_u64(&ptr_buf, 0);
            cluster_end = self.header.checksum_pos;
        }

        let cluster_size = cluster_end.saturating_sub(cluster_offset) as usize;
        if cluster_size == 0 {
            return Err(lokb_core::Error::Storage("empty cluster".into()));
        }

        // Read compressed cluster data
        let cluster_data = self.read_at(cluster_offset, cluster_size)?;

        let comp_byte = cluster_data[0];
        let compression = comp_byte & 0x0F;
        let extended = (comp_byte & 0x10) != 0;

        let compressed_data = &cluster_data[1..];

        let decompressed = match compression {
            1 => compressed_data.to_vec(),
            4 => decompress_xz(compressed_data)?,
            5 => decompress_zstd(compressed_data)?,
            _ => {
                return Err(lokb_core::Error::Storage(format!(
                    "unsupported compression type: {compression}"
                )));
            }
        };

        // Parse all blob offsets
        let offset_size: usize = if extended { 8 } else { 4 };
        if decompressed.len() < offset_size {
            return Err(lokb_core::Error::Storage(
                "cluster too small for blob offsets".into(),
            ));
        }

        let first_offset = if extended {
            read_u64(&decompressed, 0) as usize
        } else {
            read_u32(&decompressed, 0) as usize
        };

        if first_offset == 0 || first_offset % offset_size != 0 {
            return Err(lokb_core::Error::Storage(
                "invalid first blob offset".into(),
            ));
        }

        let blob_count = first_offset / offset_size;
        let mut blobs = Vec::with_capacity(blob_count);

        for b in 0..blob_count {
            let blob_start = if extended {
                read_u64(&decompressed, b * 8) as usize
            } else {
                read_u32(&decompressed, b * 4) as usize
            };

            let blob_end = if b + 1 < blob_count {
                if extended {
                    read_u64(&decompressed, (b + 1) * 8) as usize
                } else {
                    read_u32(&decompressed, (b + 1) * 4) as usize
                }
            } else {
                decompressed.len()
            };

            if blob_start <= blob_end && blob_end <= decompressed.len() {
                blobs.push(decompressed[blob_start..blob_end].to_vec());
            } else {
                blobs.push(Vec::new());
            }
        }

        Ok(blobs)
    }

    fn parse_mime_types(data: &[u8]) -> Result<Vec<String>> {
        let mut pos = 0;
        let mut types = Vec::new();
        let max_types = 1024;

        while pos < data.len() && types.len() < max_types {
            let end = data[pos..]
                .iter()
                .position(|&b| b == 0)
                .map(|p| pos + p)
                .unwrap_or(data.len());

            if end == pos {
                break; // empty string = end of list
            }

            let mime = String::from_utf8_lossy(&data[pos..end]).to_string();
            types.push(mime);
            pos = end + 1;
        }

        Ok(types)
    }
}

// ── Article Iterator ─────────────────────────────────────────────────

/// Lazy streaming iterator over ZIM articles.
/// Uses file I/O — constant memory regardless of ZIM file size.
pub struct ZimArticleIter<'a> {
    reader: &'a ZimReader,
    entry_index: u32,
    cluster_cache: Option<(u32, Vec<Vec<u8>>)>,
}

impl<'a> Iterator for ZimArticleIter<'a> {
    type Item = ZimArticle;

    fn next(&mut self) -> Option<ZimArticle> {
        while self.entry_index < self.reader.header.entry_count {
            let i = self.entry_index;
            self.entry_index += 1;

            let entry = match self.reader.get_entry(i) {
                Ok(e) => e,
                Err(_) => continue,
            };

            if !entry.is_article() {
                continue;
            }

            if let DirEntry::Content {
                cluster_number,
                blob_number,
                path,
                title,
                mime_type,
                ..
            } = &entry
            {
                let mime = self
                    .reader
                    .mime_types
                    .get(*mime_type as usize)
                    .cloned()
                    .unwrap_or_default();

                if !mime.contains("html") {
                    continue;
                }

                // Cache cluster: decompress only when cluster_number changes
                let need_decompress = match &self.cluster_cache {
                    Some((cached_num, _)) => *cached_num != *cluster_number,
                    None => true,
                };

                if need_decompress {
                    self.cluster_cache = None; // free old cache first
                    match self.reader.decompress_cluster(*cluster_number) {
                        Ok(blobs) => {
                            self.cluster_cache = Some((*cluster_number, blobs));
                        }
                        Err(_) => continue,
                    }
                }

                let blobs = &self.cluster_cache.as_ref().unwrap().1;
                let blob_idx = *blob_number as usize;
                if blob_idx >= blobs.len() {
                    continue;
                }

                let content = String::from_utf8_lossy(&blobs[blob_idx]).to_string();
                let display_title = if title.is_empty() {
                    path.clone()
                } else {
                    title.clone()
                };

                return Some(ZimArticle {
                    path: path.clone(),
                    title: display_title,
                    mime_type: mime,
                    content,
                });
            }
        }
        None
    }
}

// ── Decompression ────────────────────────────────────────────────────

const MAX_DECOMPRESSED_SIZE: usize = 256 * 1024 * 1024; // 256MB limit per cluster

fn decompress_xz(data: &[u8]) -> Result<Vec<u8>> {
    let decoder = xz2::read::XzDecoder::new(data);
    let mut result = Vec::new();
    decoder
        .take(MAX_DECOMPRESSED_SIZE as u64)
        .read_to_end(&mut result)
        .map_err(|e| lokb_core::Error::Storage(format!("XZ decompression failed: {e}")))?;
    Ok(result)
}

fn decompress_zstd(data: &[u8]) -> Result<Vec<u8>> {
    let decoder = zstd::Decoder::new(data)
        .map_err(|e| lokb_core::Error::Storage(format!("zstd init failed: {e}")))?;
    let mut result = Vec::new();
    decoder
        .take(MAX_DECOMPRESSED_SIZE as u64)
        .read_to_end(&mut result)
        .map_err(|e| lokb_core::Error::Storage(format!("zstd decompression failed: {e}")))?;
    Ok(result)
}

// ── Safe binary reading helpers ──────────────────────────────────────

fn read_u16(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

fn read_u32(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

fn read_u64(data: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
        data[offset + 4],
        data[offset + 5],
        data[offset + 6],
        data[offset + 7],
    ])
}

fn parse_path_title(data: &[u8], _file_len: u64) -> Result<(String, String)> {
    if data.is_empty() {
        return Ok((String::new(), String::new()));
    }

    // Read null-terminated path
    let path_end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    let path = String::from_utf8_lossy(&data[..path_end]).to_string();

    // Read null-terminated title (after path's null terminator)
    let title_start = path_end + 1;
    if title_start >= data.len() {
        return Ok((path, String::new()));
    }
    let title_end = data[title_start..]
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(data.len() - title_start);
    let title = String::from_utf8_lossy(&data[title_start..title_start + title_end]).to_string();

    Ok((path, title))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_magic_validation() {
        let bad_data = vec![0u8; 80];
        let result = ZimHeader::parse(&bad_data);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("invalid ZIM magic")
        );
    }

    #[test]
    fn test_header_too_small() {
        let data = vec![0u8; 10];
        let result = ZimHeader::parse(&data);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too small"));
    }

    #[test]
    fn test_read_helpers() {
        let data = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        assert_eq!(read_u16(&data, 0), 0x0201);
        assert_eq!(read_u32(&data, 0), 0x04030201);
        assert_eq!(read_u64(&data, 0), 0x0807060504030201);
    }

    #[test]
    fn test_decompress_zstd() {
        let original = b"Hello, ZIM World! This is a test of zstd compression.";
        let compressed = zstd::encode_all(&original[..], 3).unwrap();
        let decompressed = decompress_zstd(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_path_title_parsing() {
        let data = b"article/path\0Article Title\0rest of data";
        let (path, title) = parse_path_title(data.as_slice(), 100).unwrap();
        assert_eq!(path, "article/path");
        assert_eq!(title, "Article Title");
    }

    #[test]
    fn test_path_without_title() {
        let data = b"just-path\0\0";
        let (path, title) = parse_path_title(data.as_slice(), 100).unwrap();
        assert_eq!(path, "just-path");
        assert_eq!(title, "");
    }
}
