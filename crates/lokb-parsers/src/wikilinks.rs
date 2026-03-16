use std::collections::HashMap;

/// Extract entity mentions from markdown [[wikilinks]].
/// Returns (entity_name, mention_count) pairs.
///
/// Handles:
/// - [[Page]] → "Page"
/// - [[Page|display text]] → "Page"
/// - [[Page#section]] → "Page"
/// - [[Page#section|display]] → "Page"
pub fn extract_wikilinks(text: &str) -> HashMap<String, u32> {
    let mut entities: HashMap<String, u32> = HashMap::new();
    let mut pos = 0;
    let bytes = text.as_bytes();

    while pos + 1 < bytes.len() {
        if bytes[pos] == b'[' && bytes[pos + 1] == b'[' {
            pos += 2;
            // Find closing ]]
            let start = pos;
            let mut depth = 1;
            while pos + 1 < bytes.len() && depth > 0 {
                if bytes[pos] == b']' && bytes[pos + 1] == b']' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                } else if bytes[pos] == b'[' && bytes[pos + 1] == b'[' {
                    depth += 1;
                    pos += 1;
                }
                pos += 1;
            }

            if depth == 0 {
                let link_text = &text[start..pos];
                if let Some(entity) = parse_wikilink(link_text) {
                    *entities.entry(entity).or_insert(0) += 1;
                }
                pos += 2; // skip ]]
            }
        } else {
            pos += 1;
        }
    }

    entities
}

/// Parse a single wikilink content: "Page|display" → "Page", "Page#section" → "Page"
fn parse_wikilink(link: &str) -> Option<String> {
    let link = link.trim();
    if link.is_empty() || link.starts_with(':') {
        return None; // skip interwiki and category links
    }

    // Take part before | (display text)
    let target = link.split('|').next().unwrap_or(link);

    // Take part before # (section)
    let page = target.split('#').next().unwrap_or(target);

    let page = page.trim();
    if page.is_empty() || page.len() > 200 {
        return None;
    }

    // Skip File: Image: Category: links
    if page.contains(':')
        && (page.starts_with("File") || page.starts_with("Image") || page.starts_with("Category"))
    {
        return None;
    }

    Some(page.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_wikilinks() {
        let text = "This is about [[Paris]] and [[France]].";
        let entities = extract_wikilinks(text);
        assert_eq!(entities.get("Paris"), Some(&1));
        assert_eq!(entities.get("France"), Some(&1));
    }

    #[test]
    fn test_display_text() {
        let text = "Built by [[Gustave Eiffel|Eiffel]].";
        let entities = extract_wikilinks(text);
        assert_eq!(entities.get("Gustave Eiffel"), Some(&1));
        assert!(entities.get("Eiffel").is_none());
    }

    #[test]
    fn test_section_links() {
        let text = "See [[Paris#Geography]] for more.";
        let entities = extract_wikilinks(text);
        assert_eq!(entities.get("Paris"), Some(&1));
    }

    #[test]
    fn test_multiple_mentions() {
        let text = "[[Paris]] is great. I love [[Paris]].";
        let entities = extract_wikilinks(text);
        assert_eq!(entities.get("Paris"), Some(&2));
    }

    #[test]
    fn test_skip_file_links() {
        let text = "[[File:photo.jpg]] and [[Category:Cities]]";
        let entities = extract_wikilinks(text);
        assert!(entities.is_empty());
    }

    #[test]
    fn test_no_wikilinks() {
        let text = "No links here. Just [regular](markdown) links.";
        let entities = extract_wikilinks(text);
        assert!(entities.is_empty());
    }
}
