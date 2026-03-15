use crate::html::html_to_markdown;
use lokb_core::{ContentHash, ContentType, Document, PrivacyLevel, Result};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use uuid::Uuid;

/// Extract chapters from an EPUB file as (Document, markdown_text) pairs.
/// EPUB = ZIP container with HTML chapters + OPF manifest.
pub fn extract_epub(epub_path: &Path, source_id: uuid::Uuid) -> Result<Vec<(Document, String)>> {
    let file = File::open(epub_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| lokb_core::Error::Storage(format!("invalid EPUB: {e}")))?;

    // First pass: collect all file names and contents
    let mut html_files: HashMap<String, String> = HashMap::new();
    let mut opf_content: Option<(String, String)> = None; // (dir, content)

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        let name = entry.name().to_string();

        if name.ends_with(".opf") {
            let dir = name
                .rsplit_once('/')
                .map(|(d, _)| d.to_string())
                .unwrap_or_default();
            let mut content = String::new();
            let _ = entry.read_to_string(&mut content);
            opf_content = Some((dir, content));
        } else if name.ends_with(".html") || name.ends_with(".xhtml") || name.ends_with(".htm") {
            let mut content = String::new();
            let _ = entry.read_to_string(&mut content);
            html_files.insert(name, content);
        }
    }

    // Get chapter order from OPF spine (or fallback to sorted filenames)
    let ordered_files = opf_content
        .as_ref()
        .and_then(|(dir, content)| parse_spine_order(dir, content))
        .unwrap_or_else(|| {
            let mut names: Vec<String> = html_files.keys().cloned().collect();
            names.sort();
            names
        });

    let book_title = opf_content
        .as_ref()
        .and_then(|(_, content)| parse_title(content))
        .unwrap_or_else(|| {
            epub_path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });

    let book_id = Uuid::now_v7();
    let mut documents = Vec::new();

    for (idx, file_name) in ordered_files.iter().enumerate() {
        // Try exact match, then basename match
        let html = html_files.get(file_name).or_else(|| {
            let basename = file_name.rsplit('/').next().unwrap_or(file_name);
            html_files
                .iter()
                .find(|(k, _)| k.ends_with(basename))
                .map(|(_, v)| v)
        });

        let html = match html {
            Some(h) => h,
            None => continue,
        };

        let markdown = html_to_markdown(html);
        if markdown.trim().is_empty() {
            continue;
        }

        let chapter_title =
            extract_chapter_title(&markdown).unwrap_or_else(|| format!("Chapter {}", idx + 1));
        let external_id = format!("chapter_{:03}", idx);

        let doc = Document {
            id: Uuid::now_v7(),
            source_id,
            external_id,
            parent_id: Some(book_id),
            depth: 1,
            title: chapter_title,
            content_type: ContentType::Book,
            language: None,
            content_hash: ContentHash::from_bytes(markdown.as_bytes()),
            content_size: markdown.len() as u64,
            created_at: chrono::Utc::now(),
            indexed_at: chrono::Utc::now(),
            privacy_level: PrivacyLevel::Internal,
        };
        documents.push((doc, markdown));
    }

    // Root document for the book
    let total_size: u64 = documents.iter().map(|(_, text)| text.len() as u64).sum();
    let root_doc = Document {
        id: book_id,
        source_id,
        external_id: "book".to_string(),
        parent_id: None,
        depth: 0,
        title: book_title,
        content_type: ContentType::Book,
        language: None,
        content_hash: ContentHash::from_bytes(&total_size.to_le_bytes()),
        content_size: total_size,
        created_at: chrono::Utc::now(),
        indexed_at: chrono::Utc::now(),
        privacy_level: PrivacyLevel::Internal,
    };

    let mut result = vec![(root_doc, String::new())];
    result.extend(documents);
    Ok(result)
}

/// Parse OPF spine to get chapter order.
fn parse_spine_order(opf_dir: &str, opf_content: &str) -> Option<Vec<String>> {
    let mut id_to_href: HashMap<String, String> = HashMap::new();

    for line in opf_content.lines() {
        let trimmed = line.trim();
        if trimmed.contains("<item")
            && trimmed.contains("application/xhtml")
            && let (Some(id), Some(href)) =
                (extract_attr(trimmed, "id"), extract_attr(trimmed, "href"))
        {
            let full_path = if opf_dir.is_empty() {
                href
            } else {
                format!("{opf_dir}/{href}")
            };
            id_to_href.insert(id, full_path);
        }
    }

    let mut ordered = Vec::new();
    for line in opf_content.lines() {
        let trimmed = line.trim();
        if trimmed.contains("<itemref")
            && let Some(idref) = extract_attr(trimmed, "idref")
            && let Some(href) = id_to_href.get(&idref)
        {
            ordered.push(href.clone());
        }
    }

    if ordered.is_empty() {
        None
    } else {
        Some(ordered)
    }
}

/// Extract title from OPF dc:title element.
fn parse_title(opf_content: &str) -> Option<String> {
    let start_tag = opf_content.find("<dc:title")?;
    let after_tag = opf_content[start_tag..].find('>')? + start_tag + 1;
    let end_tag = opf_content[after_tag..].find('<')? + after_tag;
    let title = opf_content[after_tag..end_tag].trim();
    if title.is_empty() {
        None
    } else {
        Some(title.to_string())
    }
}

fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let pattern = format!("{attr}=\"");
    let start = tag.find(&pattern)? + pattern.len();
    let end = tag[start..].find('"')? + start;
    Some(tag[start..end].to_string())
}

fn extract_chapter_title(markdown: &str) -> Option<String> {
    for line in markdown.lines() {
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("# ")
            && !title.is_empty()
        {
            return Some(title.to_string());
        }
    }
    None
}
