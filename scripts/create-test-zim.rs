// Run with: cargo script scripts/create-test-zim.rs
// Or compile: rustc scripts/create-test-zim.rs -o /tmp/create-zim && /tmp/create-zim
use std::io::Write;

fn main() {
    let buf = build_test_zim();
    let path = "tests/fixtures/test.zim";
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(&buf).unwrap();
    println!("Created {} ({} bytes)", path, buf.len());
}

fn build_test_zim() -> Vec<u8> {
    let mime_types = b"text/html\0\0";
    let mime_pos: u64 = 80;

    let article1 = b"<h1>Rust Programming</h1><p>Rust is a systems programming language.</p>";
    let article2 = b"<h1>Wikipedia</h1><p>Wikipedia is a free online encyclopedia.</p>";

    let blob_count = 2;
    let offset_size = 4u32;
    let offsets_total = (blob_count + 1) * offset_size;
    let blob1_start = offsets_total;
    let blob2_start = blob1_start + article1.len() as u32;
    let blob2_end = blob2_start + article2.len() as u32;

    let mut cluster_data = Vec::new();
    cluster_data.push(1u8);
    cluster_data.extend_from_slice(&blob1_start.to_le_bytes());
    cluster_data.extend_from_slice(&blob2_start.to_le_bytes());
    cluster_data.extend_from_slice(&blob2_end.to_le_bytes());
    cluster_data.extend_from_slice(article1);
    cluster_data.extend_from_slice(article2);

    let cluster_pos: u64 = mime_pos + mime_types.len() as u64;

    let entry1_pos = cluster_pos + cluster_data.len() as u64;
    let mut entry1 = Vec::new();
    entry1.extend_from_slice(&0u16.to_le_bytes());
    entry1.push(0);
    entry1.push(b'C');
    entry1.extend_from_slice(&0u32.to_le_bytes());
    entry1.extend_from_slice(&0u32.to_le_bytes());
    entry1.extend_from_slice(&0u32.to_le_bytes());
    entry1.extend_from_slice(b"Rust_Programming\0Rust Programming\0");

    let entry2_pos = entry1_pos + entry1.len() as u64;
    let mut entry2 = Vec::new();
    entry2.extend_from_slice(&0u16.to_le_bytes());
    entry2.push(0);
    entry2.push(b'C');
    entry2.extend_from_slice(&0u32.to_le_bytes());
    entry2.extend_from_slice(&0u32.to_le_bytes());
    entry2.extend_from_slice(&1u32.to_le_bytes());
    entry2.extend_from_slice(b"Wikipedia\0Wikipedia\0");

    let url_ptr_pos = entry2_pos + entry2.len() as u64;
    let mut url_ptrs = Vec::new();
    url_ptrs.extend_from_slice(&entry1_pos.to_le_bytes());
    url_ptrs.extend_from_slice(&entry2_pos.to_le_bytes());

    let cluster_ptr_pos = url_ptr_pos + url_ptrs.len() as u64;
    let mut cluster_ptrs = Vec::new();
    cluster_ptrs.extend_from_slice(&cluster_pos.to_le_bytes());

    let checksum_pos = cluster_ptr_pos + cluster_ptrs.len() as u64;

    let mut header = vec![0u8; 80];
    header[0..4].copy_from_slice(&72173914u32.to_le_bytes());
    header[4..6].copy_from_slice(&6u16.to_le_bytes());
    header[24..28].copy_from_slice(&2u32.to_le_bytes());
    header[28..32].copy_from_slice(&1u32.to_le_bytes());
    header[32..40].copy_from_slice(&url_ptr_pos.to_le_bytes());
    header[40..48].copy_from_slice(&url_ptr_pos.to_le_bytes());
    header[48..56].copy_from_slice(&cluster_ptr_pos.to_le_bytes());
    header[56..64].copy_from_slice(&mime_pos.to_le_bytes());
    header[72..80].copy_from_slice(&checksum_pos.to_le_bytes());

    let mut buf = Vec::new();
    buf.extend_from_slice(&header);
    buf.extend_from_slice(mime_types);
    buf.extend_from_slice(&cluster_data);
    buf.extend_from_slice(&entry1);
    buf.extend_from_slice(&entry2);
    buf.extend_from_slice(&url_ptrs);
    buf.extend_from_slice(&cluster_ptrs);
    buf
}
