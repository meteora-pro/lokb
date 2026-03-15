//! CSV/TSV/JSONL data file parser.
//! Generates text descriptions + key statistics for search indexing.

use lokb_core::{ContentHash, ContentType, Document, PrivacyLevel, Result};
use std::path::Path;
use uuid::Uuid;

/// Parse a CSV/TSV file and generate text description with statistics.
pub fn parse_csv(path: &Path, source_id: Uuid) -> Result<(Document, String)> {
    let content = std::fs::read_to_string(path)?;
    let filename = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let delimiter = if filename.ends_with(".tsv") {
        '\t'
    } else {
        ','
    };

    let mut lines = content.lines();
    let header_line = match lines.next() {
        Some(h) => h,
        None => {
            return Err(lokb_core::Error::Storage("empty CSV file".into()));
        }
    };

    let headers: Vec<&str> = header_line.split(delimiter).collect();
    let col_count = headers.len();

    let mut row_count = 0u64;
    let mut sample_rows: Vec<Vec<String>> = Vec::new();

    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        row_count += 1;
        if sample_rows.len() < 5 {
            sample_rows.push(
                line.split(delimiter)
                    .map(|s| s.trim().to_string())
                    .collect(),
            );
        }
    }

    // Generate text description
    let mut text = format!("# Dataset: {filename}\n\n");
    text.push_str("## Schema\n\n");
    text.push_str(&format!("Columns ({col_count}):\n"));
    for (i, h) in headers.iter().enumerate() {
        text.push_str(&format!("- Column {}: {}\n", i + 1, h.trim()));
    }
    text.push_str("\n## Statistics\n\n");
    text.push_str(&format!("Total rows: {row_count}\n"));

    if !sample_rows.is_empty() {
        text.push_str("\n## Sample Data\n\n");
        // Header
        text.push_str("| ");
        for h in &headers {
            text.push_str(&format!("{} | ", h.trim()));
        }
        text.push('\n');
        text.push_str("| ");
        for _ in &headers {
            text.push_str("--- | ");
        }
        text.push('\n');
        // Rows
        for row in &sample_rows {
            text.push_str("| ");
            for val in row {
                text.push_str(&format!("{val} | "));
            }
            text.push('\n');
        }
    }

    let external_id = filename
        .trim_end_matches(".csv")
        .trim_end_matches(".tsv")
        .to_string();

    let doc = Document {
        id: Uuid::now_v7(),
        source_id,
        external_id,
        parent_id: None,
        depth: 0,
        title: format!("Dataset: {filename}"),
        content_type: ContentType::Record,
        language: None,
        content_hash: ContentHash::from_bytes(text.as_bytes()),
        content_size: text.len() as u64,
        created_at: chrono::Utc::now(),
        indexed_at: chrono::Utc::now(),
        privacy_level: PrivacyLevel::Public,
    };

    Ok((doc, text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_parse_csv() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.csv");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "name,age,city").unwrap();
        writeln!(f, "Alice,30,Paris").unwrap();
        writeln!(f, "Bob,25,London").unwrap();

        let (doc, text) = parse_csv(&path, Uuid::nil()).unwrap();
        assert!(text.contains("Dataset: test.csv"));
        assert!(text.contains("Total rows: 2"));
        assert!(text.contains("Alice"));
        assert_eq!(doc.content_type, ContentType::Record);
    }
}
