//! MBOX email archive parser with threading support.

use lokb_core::{ContentHash, ContentType, Document, PrivacyLevel, Result};
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

/// A parsed email from MBOX.
#[derive(Debug, Clone)]
pub struct ParsedEmail {
    pub message_id: String,
    pub in_reply_to: Option<String>,
    pub from: String,
    pub to: String,
    pub subject: String,
    pub date: String,
    pub body: String,
}

/// A thread of emails grouped by In-Reply-To chain.
#[derive(Debug)]
pub struct EmailThread {
    pub subject: String,
    pub emails: Vec<ParsedEmail>,
}

/// Parse MBOX file into email threads.
pub fn parse_mbox(path: &Path, source_id: Uuid) -> Result<Vec<(Document, String)>> {
    let raw = std::fs::read(path)?;

    // Split MBOX by "From " lines at start of line
    let mut emails: Vec<ParsedEmail> = Vec::new();
    let mut current_start = 0;

    let content = String::from_utf8_lossy(&raw);
    let lines: Vec<&str> = content.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        if lines[i].starts_with("From ")
            && (i == 0 || lines[i - 1].is_empty() || i == current_start)
        {
            if i > current_start && current_start < lines.len() {
                let msg_text = lines[current_start..i].join("\n");
                if let Some(email) = parse_single_email(&msg_text) {
                    emails.push(email);
                }
            }
            current_start = i + 1; // skip "From " line
        }
        i += 1;
    }
    // Last email
    if current_start < lines.len() {
        let msg_text = lines[current_start..].join("\n");
        if let Some(email) = parse_single_email(&msg_text) {
            emails.push(email);
        }
    }

    // Group into threads by In-Reply-To
    let threads = group_into_threads(emails);

    // Create documents
    let mut documents = Vec::new();
    for (idx, thread) in threads.iter().enumerate() {
        let mut text = format!("# {}\n\n", thread.subject);
        let mut contacts: Vec<String> = Vec::new();

        for email in &thread.emails {
            text.push_str(&format!(
                "**From:** {} **To:** {} **Date:** {}\n\n",
                email.from, email.to, email.date
            ));
            text.push_str(&email.body);
            text.push_str("\n\n---\n\n");
            contacts.push(email.from.clone());
        }

        let external_id = format!("thread_{:04}", idx);
        let doc = Document {
            id: Uuid::now_v7(),
            source_id,
            external_id,
            parent_id: None,
            depth: 0,
            title: thread.subject.clone(),
            content_type: ContentType::EmailThread,
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

fn parse_single_email(raw: &str) -> Option<ParsedEmail> {
    let message = mail_parser::MessageParser::new().parse(raw.as_bytes())?;

    let message_id = message.message_id().unwrap_or("unknown").to_string();

    let in_reply_to = message.in_reply_to().as_text().map(|s| s.to_string());

    let from = message
        .from()
        .and_then(|f| f.first())
        .map(|a| {
            if let Some(name) = a.name() {
                format!("{} <{}>", name, a.address().unwrap_or(""))
            } else {
                a.address().unwrap_or("unknown").to_string()
            }
        })
        .unwrap_or_else(|| "unknown".to_string());

    let to = message
        .to()
        .and_then(|t| t.first())
        .map(|a| a.address().unwrap_or("unknown").to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let subject = message.subject().unwrap_or("(no subject)").to_string();
    let date = message
        .date()
        .map(|d| d.to_rfc3339())
        .unwrap_or_else(|| "unknown".to_string());

    // Get text body (prefer plain text over HTML)
    let body = message
        .body_text(0)
        .map(|s| s.to_string())
        .or_else(|| {
            message
                .body_html(0)
                .map(|s| crate::html::html_to_markdown(&s))
        })
        .unwrap_or_default();

    if body.trim().is_empty() && subject == "(no subject)" {
        return None;
    }

    Some(ParsedEmail {
        message_id,
        in_reply_to,
        from,
        to,
        subject,
        date,
        body,
    })
}

fn group_into_threads(emails: Vec<ParsedEmail>) -> Vec<EmailThread> {
    // Build message_id → index map
    let id_map: HashMap<String, usize> = emails
        .iter()
        .enumerate()
        .map(|(i, e)| (e.message_id.clone(), i))
        .collect();

    // Find thread roots (no in_reply_to, or in_reply_to not found)
    let mut thread_map: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut roots: Vec<usize> = Vec::new();

    for (i, email) in emails.iter().enumerate() {
        if let Some(ref reply_to) = email.in_reply_to
            && let Some(&parent_idx) = id_map.get(reply_to)
        {
            thread_map.entry(parent_idx).or_default().push(i);
            continue;
        }
        roots.push(i);
    }

    // Build threads from roots
    let mut threads = Vec::new();
    for root_idx in roots {
        let mut thread_emails = vec![emails[root_idx].clone()];
        // Collect replies (simple 1-level for now)
        if let Some(children) = thread_map.get(&root_idx) {
            for &child_idx in children {
                thread_emails.push(emails[child_idx].clone());
            }
        }
        let subject = thread_emails[0].subject.clone();
        threads.push(EmailThread {
            subject,
            emails: thread_emails,
        });
    }

    threads
}

/// Extract unique contact names from parsed emails.
pub fn extract_contacts(emails: &[(Document, String)]) -> Vec<String> {
    let mut contacts = std::collections::HashSet::new();
    for (_, text) in emails {
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("**From:** ")
                && let Some(name) = rest.split("**").next()
            {
                let name = name.trim();
                if !name.is_empty() {
                    contacts.insert(name.to_string());
                }
            }
        }
    }
    contacts.into_iter().collect()
}
