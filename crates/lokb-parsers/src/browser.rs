//! Browser history parser (Chrome/Firefox SQLite).
//!
//! Extracts URLs, titles, visit counts and timestamps from browser databases.

use lokb_core::{ContentHash, ContentType, Document, PrivacyLevel, Result};
use std::path::Path;
use uuid::Uuid;

/// A parsed browser history entry.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub visit_count: i64,
    pub last_visit: String,
}

/// Parse Chrome History SQLite database.
pub fn parse_chrome_history(path: &Path, source_id: Uuid) -> Result<Vec<(Document, String)>> {
    parse_history_db(
        path,
        source_id,
        "SELECT url, title, visit_count, last_visit_time FROM urls ORDER BY last_visit_time DESC",
        true, // Chrome uses WebKit timestamp (microseconds since 1601-01-01)
    )
}

/// Parse Firefox places.sqlite database.
pub fn parse_firefox_history(path: &Path, source_id: Uuid) -> Result<Vec<(Document, String)>> {
    parse_history_db(
        path,
        source_id,
        "SELECT url, title, visit_count, last_visit_date FROM moz_places WHERE visit_count > 0 ORDER BY last_visit_date DESC",
        false, // Firefox uses microseconds since Unix epoch
    )
}

fn parse_history_db(
    path: &Path,
    source_id: Uuid,
    query: &str,
    is_chrome_timestamp: bool,
) -> Result<Vec<(Document, String)>> {
    // Copy database to temp (browser may have it locked)
    let temp_dir = std::env::temp_dir().join("lokb-browser-tmp");
    std::fs::create_dir_all(&temp_dir)?;
    let temp_path = temp_dir.join("history-copy.sqlite");
    std::fs::copy(path, &temp_path)?;

    let conn = rusqlite::Connection::open_with_flags(
        &temp_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|e| lokb_core::Error::Storage(format!("failed to open browser db: {e}")))?;

    let mut stmt = conn
        .prepare(query)
        .map_err(|e| lokb_core::Error::Storage(format!("query failed: {e}")))?;

    let entries: Vec<HistoryEntry> = stmt
        .query_map([], |row| {
            let url: String = row.get(0)?;
            let title: String = row.get::<_, Option<String>>(1)?.unwrap_or_default();
            let visit_count: i64 = row.get(2)?;
            let timestamp: i64 = row.get::<_, Option<i64>>(3)?.unwrap_or(0);

            let last_visit = if is_chrome_timestamp {
                // Chrome: microseconds since 1601-01-01
                let unix_us = timestamp - 11_644_473_600_000_000;
                chrono::DateTime::from_timestamp(unix_us / 1_000_000, 0)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_else(|| "unknown".to_string())
            } else {
                // Firefox: microseconds since Unix epoch
                chrono::DateTime::from_timestamp(timestamp / 1_000_000, 0)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_else(|| "unknown".to_string())
            };

            Ok(HistoryEntry {
                url,
                title,
                visit_count,
                last_visit,
            })
        })
        .map_err(|e| lokb_core::Error::Storage(format!("query exec failed: {e}")))?
        .filter_map(|r| r.ok())
        .collect();

    // Clean up temp
    let _ = std::fs::remove_file(&temp_path);

    // Group by domain for manageable document count
    let mut documents = Vec::new();
    for (idx, entry) in entries.iter().enumerate() {
        let title = if entry.title.is_empty() {
            &entry.url
        } else {
            &entry.title
        };

        let text = format!(
            "URL: {}\nTitle: {}\nVisits: {}\nLast visit: {}\n",
            entry.url, title, entry.visit_count, entry.last_visit,
        );

        let external_id = format!("url_{:06}", idx);
        let doc = Document {
            id: Uuid::now_v7(),
            source_id,
            external_id,
            parent_id: None,
            depth: 0,
            title: title.to_string(),
            content_type: ContentType::Record,
            language: None,
            content_hash: ContentHash::from_bytes(text.as_bytes()),
            content_size: text.len() as u64,
            created_at: chrono::Utc::now(),
            indexed_at: chrono::Utc::now(),
            privacy_level: PrivacyLevel::Private,
        };
        documents.push((doc, text));
    }

    Ok(documents)
}

/// Extract entity links from Wikipedia URLs in history.
pub fn extract_url_entities(entries: &[(Document, String)]) -> Vec<(String, String)> {
    let mut entities = Vec::new();
    for (_, text) in entries {
        for line in text.lines() {
            if let Some(url) = line.strip_prefix("URL: ")
                && let Some(article) = extract_wikipedia_article(url)
            {
                let entity_id = format!("wiki:{}", article.to_lowercase().replace(' ', "_"));
                entities.push((entity_id, article));
            }
        }
    }
    entities
}

/// Extract article name from Wikipedia URL.
fn extract_wikipedia_article(url: &str) -> Option<String> {
    // https://en.wikipedia.org/wiki/Paris → "Paris"
    let path = url.split("/wiki/").nth(1)?;
    if path.is_empty() || path.starts_with("Special:") || path.starts_with("File:") {
        return None;
    }
    Some(path.replace('_', " ").split('#').next()?.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_wikipedia_article() {
        assert_eq!(
            extract_wikipedia_article("https://en.wikipedia.org/wiki/Paris"),
            Some("Paris".to_string())
        );
        assert_eq!(
            extract_wikipedia_article("https://en.wikipedia.org/wiki/Eiffel_Tower"),
            Some("Eiffel Tower".to_string())
        );
        assert_eq!(
            extract_wikipedia_article("https://en.wikipedia.org/wiki/Special:Search"),
            None
        );
        assert_eq!(extract_wikipedia_article("https://google.com"), None);
    }

    #[test]
    fn test_extract_url_entities() {
        let docs = vec![(
            Document {
                id: Uuid::nil(),
                source_id: Uuid::nil(),
                external_id: "test".into(),
                parent_id: None,
                depth: 0,
                title: "test".into(),
                content_type: ContentType::Record,
                language: None,
                content_hash: ContentHash::from_bytes(b""),
                content_size: 0,
                created_at: chrono::Utc::now(),
                indexed_at: chrono::Utc::now(),
                privacy_level: PrivacyLevel::default(),
            },
            "URL: https://en.wikipedia.org/wiki/Rust_(programming_language)\nTitle: Rust\n"
                .to_string(),
        )];

        let entities = extract_url_entities(&docs);
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].1, "Rust (programming language)");
    }
}
