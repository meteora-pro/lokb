use lokb_parsers::zim::ZimReader;
use std::io::Write;
use std::path::PathBuf;

/// Build a minimal valid ZIM file for testing.
/// Structure: header + mime_types + 2 directory entries + 1 cluster + url_ptrs + cluster_ptrs
fn build_test_zim() -> Vec<u8> {
    let mut buf = Vec::new();

    // We'll build the file in stages, computing offsets as we go.
    // Layout:
    //   [0..80]      Header
    //   [80..]       MIME types
    //   [mime_end..]  Cluster data (1 cluster with 2 blobs)
    //   [cluster_end..] Directory entries (2 content entries)
    //   [entries_end..] URL pointer list
    //   [url_end..] Cluster pointer list

    // MIME types: "text/html\0\0"
    let mime_types = b"text/html\0\0";
    let mime_pos: u64 = 80;

    // Cluster: uncompressed, 2 blobs (article HTML)
    let article1_html =
        b"<h1>Test Article</h1><p>This is a test article about Rust programming.</p>";
    let article2_html = b"<h1>Second Article</h1><p>Wikipedia is an online encyclopedia.</p>";

    // Build uncompressed cluster
    // comp_byte=1 (uncompressed), then 4-byte offsets, then blob data
    let blob_count = 2;
    let offset_size = 4u32;
    let offsets_total = (blob_count + 1) * offset_size; // +1 for end marker
    let blob1_start = offsets_total;
    let blob2_start = blob1_start + article1_html.len() as u32;
    let blob2_end = blob2_start + article2_html.len() as u32;

    let mut cluster_data = Vec::new();
    cluster_data.push(1u8); // compression = 1 (none)
    cluster_data.extend_from_slice(&blob1_start.to_le_bytes());
    cluster_data.extend_from_slice(&blob2_start.to_le_bytes());
    cluster_data.extend_from_slice(&blob2_end.to_le_bytes());
    cluster_data.extend_from_slice(article1_html);
    cluster_data.extend_from_slice(article2_html);

    let cluster_pos: u64 = mime_pos + mime_types.len() as u64;

    // Directory entries
    let entry1_pos = cluster_pos + cluster_data.len() as u64;
    let mut entry1 = Vec::new();
    entry1.extend_from_slice(&0u16.to_le_bytes()); // mime_type = 0 (text/html)
    entry1.push(0); // param_len
    entry1.push(b'C'); // namespace
    entry1.extend_from_slice(&0u32.to_le_bytes()); // revision
    entry1.extend_from_slice(&0u32.to_le_bytes()); // cluster 0
    entry1.extend_from_slice(&0u32.to_le_bytes()); // blob 0
    entry1.extend_from_slice(b"Test_Article\0Test Article\0");

    let entry2_pos = entry1_pos + entry1.len() as u64;
    let mut entry2 = Vec::new();
    entry2.extend_from_slice(&0u16.to_le_bytes()); // mime_type = 0
    entry2.push(0);
    entry2.push(b'C');
    entry2.extend_from_slice(&0u32.to_le_bytes());
    entry2.extend_from_slice(&0u32.to_le_bytes()); // cluster 0
    entry2.extend_from_slice(&1u32.to_le_bytes()); // blob 1
    entry2.extend_from_slice(b"Second_Article\0Second Article\0");

    // URL pointer list (entry_count * 8 bytes)
    let url_ptr_pos = entry2_pos + entry2.len() as u64;
    let mut url_ptrs = Vec::new();
    url_ptrs.extend_from_slice(&entry1_pos.to_le_bytes());
    url_ptrs.extend_from_slice(&entry2_pos.to_le_bytes());

    // Cluster pointer list (cluster_count * 8 bytes)
    let cluster_ptr_pos = url_ptr_pos + url_ptrs.len() as u64;
    let mut cluster_ptrs = Vec::new();
    cluster_ptrs.extend_from_slice(&cluster_pos.to_le_bytes());

    let checksum_pos = cluster_ptr_pos + cluster_ptrs.len() as u64;

    // Build header
    let mut header = vec![0u8; 80];
    // Magic
    header[0..4].copy_from_slice(&72173914u32.to_le_bytes());
    // Version 6.0
    header[4..6].copy_from_slice(&6u16.to_le_bytes());
    header[6..8].copy_from_slice(&0u16.to_le_bytes());
    // UUID (zeros)
    // Entry count = 2
    header[24..28].copy_from_slice(&2u32.to_le_bytes());
    // Cluster count = 1
    header[28..32].copy_from_slice(&1u32.to_le_bytes());
    // Path pointer position
    header[32..40].copy_from_slice(&url_ptr_pos.to_le_bytes());
    // Title pointer position (same as path for simplicity)
    header[40..48].copy_from_slice(&url_ptr_pos.to_le_bytes());
    // Cluster pointer position
    header[48..56].copy_from_slice(&cluster_ptr_pos.to_le_bytes());
    // MIME list position
    header[56..64].copy_from_slice(&mime_pos.to_le_bytes());
    // Main page = 0
    header[64..68].copy_from_slice(&0u32.to_le_bytes());
    // Checksum position
    header[72..80].copy_from_slice(&checksum_pos.to_le_bytes());

    // Assemble file
    buf.extend_from_slice(&header);
    buf.extend_from_slice(mime_types);
    buf.extend_from_slice(&cluster_data);
    buf.extend_from_slice(&entry1);
    buf.extend_from_slice(&entry2);
    buf.extend_from_slice(&url_ptrs);
    buf.extend_from_slice(&cluster_ptrs);

    buf
}

#[test]
fn test_read_test_zim() {
    let zim_data = build_test_zim();

    // Write to temp file
    let dir = tempfile::tempdir().unwrap();
    let zim_path = dir.path().join("test.zim");
    let mut f = std::fs::File::create(&zim_path).unwrap();
    f.write_all(&zim_data).unwrap();
    drop(f);

    // Open and read
    let reader = ZimReader::open(&zim_path).unwrap();
    assert_eq!(reader.entry_count(), 2);
    assert_eq!(reader.cluster_count(), 1);

    // Read entries
    let entry0 = reader.get_entry(0).unwrap();
    assert!(entry0.is_article());
    assert_eq!(entry0.path(), "Test_Article");
    assert_eq!(entry0.title(), "Test Article");

    let entry1 = reader.get_entry(1).unwrap();
    assert!(entry1.is_article());
    assert_eq!(entry1.path(), "Second_Article");
    assert_eq!(entry1.title(), "Second Article");

    // Read blobs
    let blob0 = reader.get_blob(0, 0).unwrap();
    let html0 = String::from_utf8(blob0).unwrap();
    assert!(html0.contains("<h1>Test Article</h1>"));
    assert!(html0.contains("Rust programming"));

    let blob1 = reader.get_blob(0, 1).unwrap();
    let html1 = String::from_utf8(blob1).unwrap();
    assert!(html1.contains("Second Article"));
    assert!(html1.contains("encyclopedia"));
}

#[test]
fn test_articles_iterator() {
    let zim_data = build_test_zim();
    let dir = tempfile::tempdir().unwrap();
    let zim_path = dir.path().join("test.zim");
    std::fs::write(&zim_path, &zim_data).unwrap();

    let reader = ZimReader::open(&zim_path).unwrap();
    let articles = reader.articles();

    assert_eq!(articles.len(), 2);
    assert_eq!(articles[0].title, "Test Article");
    assert!(articles[0].content.contains("Rust programming"));
    assert_eq!(articles[1].title, "Second Article");
    assert!(articles[1].content.contains("encyclopedia"));
}

#[test]
fn test_invalid_zim_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let bad_path = dir.path().join("bad.zim");
    std::fs::write(&bad_path, b"this is not a ZIM file").unwrap();

    let result = ZimReader::open(&bad_path);
    assert!(result.is_err());
}

#[test]
fn test_out_of_bounds_cluster() {
    let zim_data = build_test_zim();
    let dir = tempfile::tempdir().unwrap();
    let zim_path = dir.path().join("test.zim");
    std::fs::write(&zim_path, &zim_data).unwrap();

    let reader = ZimReader::open(&zim_path).unwrap();
    let result = reader.get_blob(999, 0);
    assert!(result.is_err());
}

#[test]
fn test_out_of_bounds_blob() {
    let zim_data = build_test_zim();
    let dir = tempfile::tempdir().unwrap();
    let zim_path = dir.path().join("test.zim");
    std::fs::write(&zim_path, &zim_data).unwrap();

    let reader = ZimReader::open(&zim_path).unwrap();
    let result = reader.get_blob(0, 999);
    assert!(result.is_err());
}
