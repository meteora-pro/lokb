use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Root data directory. Overridable via LOKB_DATA_DIR env var.
pub fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("LOKB_DATA_DIR") {
        PathBuf::from(dir)
    } else {
        dirs().default
    }
}

struct Dirs {
    default: PathBuf,
}

fn dirs() -> Dirs {
    let base = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    Dirs {
        default: base.join(".local/share/lokb"),
    }
}

fn sources_dir() -> PathBuf {
    data_dir().join("sources")
}

fn source_dir() -> PathBuf {
    data_dir().join("source")
}

fn derived_dir() -> PathBuf {
    data_dir().join("derived")
}

fn cache_dir() -> PathBuf {
    data_dir().join("cache")
}

/// Ensure the base directory structure exists.
pub fn init_dirs() -> io::Result<()> {
    for dir in [sources_dir(), source_dir(), derived_dir(), cache_dir()] {
        fs::create_dir_all(dir)?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceConfig {
    pub name: String,
    pub raw_path: String,
    pub format: String,
    pub class: String,
    pub document_count: u64,
}

/// Save source config and ingest raw data.
pub fn add_source(name: &str, raw: &str, format: &str, class: &str) -> io::Result<()> {
    init_dirs()?;

    let raw_path = PathBuf::from(raw);
    if !raw_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("raw path not found: {}", raw),
        ));
    }

    let dest = source_dir().join(name);
    fs::create_dir_all(&dest)?;

    let doc_count = ingest_raw(&raw_path, format, &dest)?;

    let config = SourceConfig {
        name: name.to_string(),
        raw_path: raw.to_string(),
        format: format.to_string(),
        class: class.to_string(),
        document_count: doc_count,
    };

    let config_path = sources_dir().join(format!("{}.json", name));
    let json = serde_json::to_string_pretty(&config).map_err(io::Error::other)?;
    fs::write(config_path, json)?;

    Ok(())
}

/// Ingest raw files into source directory. Returns document count.
fn ingest_raw(raw_path: &Path, format: &str, dest: &Path) -> io::Result<u64> {
    match format {
        "markdown-dir" => ingest_markdown_dir(raw_path, dest),
        "telegram-export" => ingest_telegram(raw_path, dest),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported format: {}", format),
        )),
    }
}

/// Copy markdown files into source directory.
fn ingest_markdown_dir(raw_path: &Path, dest: &Path) -> io::Result<u64> {
    let mut count = 0;
    for entry in fs::read_dir(raw_path)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "md") {
            let file_name = path.file_name().unwrap();
            fs::copy(&path, dest.join(file_name))?;
            count += 1;
        }
    }
    Ok(count)
}

/// Parse Telegram export JSON and store messages as text documents.
fn ingest_telegram(raw_path: &Path, dest: &Path) -> io::Result<u64> {
    let json_path = raw_path.join("result.json");
    if !json_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "result.json not found in telegram export directory",
        ));
    }

    let content = fs::read_to_string(&json_path)?;
    let export: TelegramExport = serde_json::from_str(&content)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let chat_name = &export.name;
    let mut segments: Vec<Vec<&TelegramMessage>> = vec![];
    let mut current_segment: Vec<&TelegramMessage> = vec![];

    for msg in &export.messages {
        if msg.r#type != "message" {
            continue;
        }
        // Segment by 2-hour gaps
        if let Some(last) = current_segment.last()
            && time_gap_hours(&last.date, &msg.date) > 2.0
            && !current_segment.is_empty()
        {
            segments.push(current_segment);
            current_segment = vec![];
        }
        current_segment.push(msg);
    }
    if !current_segment.is_empty() {
        segments.push(current_segment);
    }

    let mut count = 0;
    for (i, segment) in segments.iter().enumerate() {
        let mut text = format!("[{}]\n", chat_name);
        for msg in segment {
            let from = msg.from.as_deref().unwrap_or("Unknown");
            let time = msg.date.split('T').next_back().unwrap_or(&msg.date);
            let msg_text = extract_text(&msg.text);
            text.push_str(&format!("{} [{}]: {}\n", from, time, msg_text));
        }
        let filename = format!("segment_{:04}.txt", i);
        fs::write(dest.join(&filename), &text)?;
        count += 1;
    }

    Ok(count)
}

fn time_gap_hours(a: &str, b: &str) -> f64 {
    // Simple comparison: parse "2024-03-15T14:20:00" format
    let parse = |s: &str| -> Option<f64> {
        let parts: Vec<&str> = s.split('T').collect();
        if parts.len() != 2 {
            return None;
        }
        let time_parts: Vec<&str> = parts[1].split(':').collect();
        if time_parts.len() < 2 {
            return None;
        }
        let date_parts: Vec<&str> = parts[0].split('-').collect();
        if date_parts.len() < 3 {
            return None;
        }
        let day: f64 = date_parts[2].parse().ok()?;
        let hours: f64 = time_parts[0].parse().ok()?;
        let minutes: f64 = time_parts[1].parse().ok()?;
        Some(day * 24.0 + hours + minutes / 60.0)
    };

    match (parse(a), parse(b)) {
        (Some(a), Some(b)) => (b - a).abs(),
        _ => 0.0,
    }
}

/// Extract text from Telegram message text field (can be string or array).
fn extract_text(text: &serde_json::Value) -> String {
    match text {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(parts) => parts
            .iter()
            .map(|p| match p {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Object(obj) => obj
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                _ => String::new(),
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

#[derive(Deserialize)]
struct TelegramExport {
    name: String,
    messages: Vec<TelegramMessage>,
}

#[derive(Deserialize)]
struct TelegramMessage {
    r#type: String,
    date: String,
    from: Option<String>,
    text: serde_json::Value,
}

/// List all configured sources.
pub fn list_sources() -> io::Result<Vec<SourceConfig>> {
    let dir = sources_dir();
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut sources = vec![];
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            let content = fs::read_to_string(&path)?;
            if let Ok(config) = serde_json::from_str::<SourceConfig>(&content) {
                sources.push(config);
            }
        }
    }
    sources.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(sources)
}

/// Search result from store.
#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub title: String,
    pub source: String,
    pub snippet: String,
    pub score: f64,
}

/// Simple text search across all documents.
pub fn search(
    query: &str,
    personal_only: bool,
    public_only: bool,
) -> io::Result<Vec<SearchResult>> {
    let sources = list_sources()?;
    let query_lower = query.to_lowercase();
    let mut results = vec![];

    for source in &sources {
        if personal_only && source.class != "personal" {
            continue;
        }
        if public_only && source.class != "public" {
            continue;
        }

        let src_dir = source_dir().join(&source.name);
        if !src_dir.exists() {
            continue;
        }

        for entry in fs::read_dir(&src_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let content = fs::read_to_string(&path).unwrap_or_default();
            let content_lower = content.to_lowercase();

            if let Some(pos) = content_lower.find(&query_lower) {
                let title = extract_title(&content, &path);
                let snippet = extract_snippet(&content, pos, 200);

                // Simple scoring: count occurrences
                let score = content_lower.matches(&query_lower).count() as f64;

                results.push(SearchResult {
                    title,
                    source: source.name.clone(),
                    snippet,
                    score,
                });
            }
        }
    }

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(results)
}

fn extract_title(content: &str, path: &Path) -> String {
    // Try to get first markdown heading
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(heading) = trimmed.strip_prefix("# ") {
            return heading.to_string();
        }
    }
    // Fallback to filename
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Untitled".to_string())
}

fn extract_snippet(content: &str, pos: usize, max_len: usize) -> String {
    let start = pos.saturating_sub(max_len / 2);
    let end = (pos + max_len / 2).min(content.len());

    let snippet = &content[start..end];
    // Trim to word boundaries
    let snippet = snippet.trim();
    if start > 0 {
        format!("...{snippet}...")
    } else if end < content.len() {
        format!("{snippet}...")
    } else {
        snippet.to_string()
    }
}

/// Read a document by source:doc_id reference.
pub fn read_document(doc_ref: &str) -> io::Result<String> {
    let parts: Vec<&str> = doc_ref.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected format: source_name:document_id",
        ));
    }
    let (source_name, doc_id) = (parts[0], parts[1]);
    let src_dir = source_dir().join(source_name);

    // Try exact filename match with common extensions
    for ext in ["md", "txt", ""] {
        let filename = if ext.is_empty() {
            doc_id.to_string()
        } else {
            format!("{}.{}", doc_id, ext)
        };
        let path = src_dir.join(&filename);
        if path.exists() {
            return fs::read_to_string(path);
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!(
            "document '{}' not found in source '{}'",
            doc_id, source_name
        ),
    ))
}

/// Storage layer stats.
#[derive(Debug, Serialize)]
pub struct StorageLayer {
    pub name: String,
    pub size_bytes: u64,
}

/// Calculate storage status.
pub fn storage_status() -> io::Result<Vec<StorageLayer>> {
    Ok(vec![
        StorageLayer {
            name: "source".to_string(),
            size_bytes: dir_size(&source_dir()),
        },
        StorageLayer {
            name: "derived".to_string(),
            size_bytes: dir_size(&derived_dir()),
        },
        StorageLayer {
            name: "cache".to_string(),
            size_bytes: dir_size(&cache_dir()),
        },
    ])
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

/// Export public sources to a tar.zst file.
pub fn export(output: &str, include_personal: bool) -> io::Result<()> {
    let sources = list_sources()?;
    let output_path = PathBuf::from(output);

    // Collect files to export
    let mut exported_sources: Vec<String> = vec![];
    for source in &sources {
        if !include_personal && source.class == "personal" {
            continue;
        }
        exported_sources.push(source.name.clone());
    }

    // Create a simple manifest as the export (tar.zst would need extra deps)
    let manifest = serde_json::json!({
        "version": "0.1.0",
        "sources": exported_sources,
    });
    let json = serde_json::to_string_pretty(&manifest).map_err(io::Error::other)?;
    fs::write(output_path, json)?;

    Ok(())
}
