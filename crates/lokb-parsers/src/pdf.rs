use lokb_core::{ContentHash, ContentType, Document, PrivacyLevel, Result};
use std::path::Path;
use uuid::Uuid;

/// Extract text from a PDF file and return as a Document.
pub fn extract_pdf(
    pdf_path: &Path,
    source_id: uuid::Uuid,
    external_id: &str,
) -> Result<(Document, String)> {
    let bytes = std::fs::read(pdf_path)?;
    let text = std::panic::catch_unwind(|| pdf_extract::extract_text_from_mem(&bytes))
        .map_err(|_| lokb_core::Error::Storage("PDF extraction panicked".to_string()))?
        .map_err(|e| lokb_core::Error::Storage(format!("PDF extraction failed: {e}")))?;

    let text = clean_pdf_text(&text);

    let title = extract_pdf_title(&text).unwrap_or_else(|| {
        pdf_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    });

    let doc = Document {
        id: Uuid::now_v7(),
        source_id,
        external_id: external_id.to_string(),
        parent_id: None,
        depth: 0,
        title,
        content_type: ContentType::Paper,
        language: None,
        content_hash: ContentHash::from_bytes(text.as_bytes()),
        content_size: text.len() as u64,
        created_at: chrono::Utc::now(),
        indexed_at: chrono::Utc::now(),
        privacy_level: PrivacyLevel::Internal,
    };

    Ok((doc, text))
}

/// Clean up extracted PDF text: fix ligatures, normalize whitespace, remove header/footer noise.
fn clean_pdf_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut prev_empty = false;

    for line in text.lines() {
        let trimmed = line.trim();

        // Skip likely headers/footers (page numbers, short repeated lines)
        if trimmed.len() < 4 && trimmed.chars().all(|c| c.is_ascii_digit() || c == '-') {
            continue;
        }

        if trimmed.is_empty() {
            if !prev_empty {
                result.push('\n');
                prev_empty = true;
            }
        } else {
            result.push_str(trimmed);
            result.push('\n');
            prev_empty = false;
        }
    }

    // Fix common PDF ligature issues
    result = result.replace("ﬁ", "fi");
    result = result.replace("ﬂ", "fl");
    result = result.replace("ﬀ", "ff");
    result = result.replace("ﬃ", "ffi");
    result = result.replace("ﬄ", "ffl");

    result.trim().to_string()
}

/// Try to extract title from first non-empty lines of PDF text.
fn extract_pdf_title(text: &str) -> Option<String> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let first = lines.next()?.trim();

    // If first line is short enough, it's likely the title
    if first.len() < 200 && !first.ends_with('.') {
        return Some(format!("# {first}"));
    }
    None
}
