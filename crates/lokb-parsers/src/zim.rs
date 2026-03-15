//! Safe ZIM file reader.
//!
//! Implements the ZIM file format specification from openzim.org.
//! Uses memmap2 for memory-mapped I/O with proper bounds checking.

use lokb_core::Result;
use memmap2::Mmap;
use std::io::Read;
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
    fn parse(data: &[u8], offset: u64, file_len: u64) -> Result<Self> {
        let off = offset as usize;
        check_bounds(data, off, 4, file_len)?;

        let mime_type = read_u16(data, off);
        let _param_len = data[off + 2];
        let namespace = data[off + 3];

        if mime_type == 0xFFFF {
            // Redirect entry
            check_bounds(data, off, 12, file_len)?;
            let target_index = read_u32(data, off + 8);
            let (path, title) = read_path_title(data, off + 12, file_len)?;
            Ok(DirEntry::Redirect {
                namespace,
                target_index,
                path,
                title,
            })
        } else {
            // Content entry
            check_bounds(data, off, 16, file_len)?;
            let cluster_number = read_u32(data, off + 8);
            let blob_number = read_u32(data, off + 12);
            let (path, title) = read_path_title(data, off + 16, file_len)?;
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

/// Safe, read-only ZIM file reader with bounds checking.
pub struct ZimReader {
    mmap: Mmap,
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
        let file = std::fs::File::open(path)?;
        // SAFETY: We treat the mmap as read-only and don't modify the file
        // while it's mapped. The file is opened read-only.
        let mmap = unsafe { Mmap::map(&file)? };

        let header = ZimHeader::parse(&mmap)?;
        let mime_types = Self::parse_mime_types(&mmap, &header)?;

        Ok(Self {
            mmap,
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

    /// Get a directory entry by index.
    pub fn get_entry(&self, index: u32) -> Result<DirEntry> {
        let ptr_offset = self.header.path_ptr_pos + (index as u64) * 8;
        check_bounds(&self.mmap, ptr_offset as usize, 8, self.mmap.len() as u64)?;
        let entry_offset = read_u64(&self.mmap, ptr_offset as usize);
        DirEntry::parse(&self.mmap, entry_offset, self.mmap.len() as u64)
    }

    /// Get the content of a blob (article) by cluster and blob number.
    pub fn get_blob(&self, cluster_number: u32, blob_number: u32) -> Result<Vec<u8>> {
        if cluster_number >= self.header.cluster_count {
            return Err(lokb_core::Error::Storage(format!(
                "cluster {cluster_number} out of range (max {})",
                self.header.cluster_count
            )));
        }

        // Get cluster offset
        let cluster_ptr_offset = self.header.cluster_ptr_pos + (cluster_number as u64) * 8;
        check_bounds(
            &self.mmap,
            cluster_ptr_offset as usize,
            8,
            self.mmap.len() as u64,
        )?;
        let cluster_offset = read_u64(&self.mmap, cluster_ptr_offset as usize);

        // Get next cluster offset (or checksum pos) for size bound
        let cluster_end = if cluster_number + 1 < self.header.cluster_count {
            let next_ptr = cluster_ptr_offset + 8;
            read_u64(&self.mmap, next_ptr as usize)
        } else {
            self.header.checksum_pos
        };

        let cluster_size = cluster_end.saturating_sub(cluster_offset);
        if cluster_size == 0 || cluster_offset as usize >= self.mmap.len() {
            return Err(lokb_core::Error::Storage("empty or invalid cluster".into()));
        }

        let cluster_data =
            &self.mmap[cluster_offset as usize..(cluster_offset + cluster_size) as usize];

        // First byte: compression type
        let comp_byte = cluster_data[0];
        let compression = comp_byte & 0x0F;
        let extended = (comp_byte & 0x10) != 0;

        let compressed_data = &cluster_data[1..];

        // Decompress cluster
        let decompressed = match compression {
            1 => compressed_data.to_vec(), // uncompressed
            4 => decompress_xz(compressed_data)?,
            5 => decompress_zstd(compressed_data)?,
            _ => {
                return Err(lokb_core::Error::Storage(format!(
                    "unsupported compression type: {compression}"
                )));
            }
        };

        // Parse blob offsets
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

        if blob_number as usize >= blob_count {
            return Err(lokb_core::Error::Storage(format!(
                "blob {blob_number} out of range (max {blob_count})"
            )));
        }

        let blob_start = if extended {
            read_u64(&decompressed, blob_number as usize * 8) as usize
        } else {
            read_u32(&decompressed, blob_number as usize * 4) as usize
        };

        let blob_end = if (blob_number as usize + 1) < blob_count {
            if extended {
                read_u64(&decompressed, (blob_number as usize + 1) * 8) as usize
            } else {
                read_u32(&decompressed, (blob_number as usize + 1) * 4) as usize
            }
        } else {
            decompressed.len()
        };

        if blob_start > blob_end || blob_end > decompressed.len() {
            return Err(lokb_core::Error::Storage("invalid blob boundaries".into()));
        }

        Ok(decompressed[blob_start..blob_end].to_vec())
    }

    /// Iterate over all content articles (namespace C or A), yielding (path, title, html_content).
    pub fn articles(&self) -> Vec<ZimArticle> {
        let mut articles = Vec::new();
        for i in 0..self.header.entry_count {
            let entry = match self.get_entry(i) {
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
                    .mime_types
                    .get(*mime_type as usize)
                    .cloned()
                    .unwrap_or_default();

                // Only HTML articles
                if !mime.contains("html") {
                    continue;
                }

                let content = match self.get_blob(*cluster_number, *blob_number) {
                    Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
                    Err(_) => continue,
                };

                let display_title = if title.is_empty() {
                    path.clone()
                } else {
                    title.clone()
                };

                articles.push(ZimArticle {
                    path: path.clone(),
                    title: display_title,
                    mime_type: mime,
                    content,
                });
            }
        }
        articles
    }

    fn parse_mime_types(data: &[u8], header: &ZimHeader) -> Result<Vec<String>> {
        let mut pos = header.mime_list_pos as usize;
        let mut types = Vec::new();
        let max_types = 1024; // safety limit

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
            pos = end + 1; // skip null terminator
        }

        Ok(types)
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

fn check_bounds(data: &[u8], offset: usize, size: usize, file_len: u64) -> Result<()> {
    if offset + size > data.len() || offset + size > file_len as usize {
        return Err(lokb_core::Error::Storage(format!(
            "read out of bounds: offset={offset}, size={size}, file_len={file_len}"
        )));
    }
    Ok(())
}

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

fn read_path_title(data: &[u8], offset: usize, file_len: u64) -> Result<(String, String)> {
    let max_scan = std::cmp::min(data.len(), file_len as usize);
    if offset >= max_scan {
        return Ok((String::new(), String::new()));
    }

    // Read null-terminated path
    let path_end = data[offset..max_scan]
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(max_scan - offset);
    let path = String::from_utf8_lossy(&data[offset..offset + path_end]).to_string();

    // Read null-terminated title (after path's null terminator)
    let title_start = offset + path_end + 1;
    if title_start >= max_scan {
        return Ok((path, String::new()));
    }
    let title_end = data[title_start..max_scan]
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(max_scan - title_start);
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
    fn test_bounds_check() {
        let data = vec![0u8; 100];
        assert!(check_bounds(&data, 0, 100, 100).is_ok());
        assert!(check_bounds(&data, 0, 101, 100).is_err());
        assert!(check_bounds(&data, 99, 2, 100).is_err());
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
        let (path, title) = read_path_title(data.as_slice(), 0, data.len() as u64).unwrap();
        assert_eq!(path, "article/path");
        assert_eq!(title, "Article Title");
    }

    #[test]
    fn test_path_without_title() {
        let data = b"just-path\0\0";
        let (path, title) = read_path_title(data.as_slice(), 0, data.len() as u64).unwrap();
        assert_eq!(path, "just-path");
        assert_eq!(title, "");
    }
}
