use lokb_core::{Chunk, ContentType, Document, Result};
use lokb_pipeline::Chunker;
use uuid::Uuid;

/// Splits documents into chunks by markdown headings and paragraphs.
/// Articles/notes → split by ## headers, then by paragraph if too long.
/// Conversations → one chunk per document (already windowed in optimize).
/// Records → one chunk per document.
pub struct SemanticChunker {
    pub max_chunk_size: usize,
}

impl Default for SemanticChunker {
    fn default() -> Self {
        Self {
            max_chunk_size: 1500,
        }
    }
}

impl Chunker for SemanticChunker {
    fn chunk(&self, document: &Document, text: &str) -> Result<Vec<Chunk>> {
        match document.content_type {
            // Already windowed during optimize — one chunk per doc
            ContentType::Conversation | ContentType::Thread => {
                Ok(vec![make_chunk(document, text, 0, None)])
            }
            // Small records — one chunk per doc
            ContentType::MediaMeta
            | ContentType::GpsSegment
            | ContentType::Record
            | ContentType::Email => Ok(vec![make_chunk(document, text, 0, None)]),
            // Articles, books, papers — split by sections
            _ => Ok(self.split_by_sections(document, text)),
        }
    }
}

impl SemanticChunker {
    fn split_by_sections(&self, document: &Document, text: &str) -> Vec<Chunk> {
        let mut chunks = Vec::new();
        let mut current_section: Option<String> = None;
        let mut current_text = String::new();
        let mut chunk_start: usize = 0;

        for line in text.lines() {
            // Detect heading (## or ###)
            if line.starts_with("## ") || line.starts_with("### ") {
                // Flush current chunk if non-empty
                if !current_text.trim().is_empty() {
                    self.flush_chunk(
                        document,
                        &current_text,
                        chunk_start,
                        &current_section,
                        &mut chunks,
                    );
                }
                current_section = Some(line.trim_start_matches('#').trim().to_string());
                current_text = format!("{line}\n");
                chunk_start = chunks.len();
                continue;
            }

            current_text.push_str(line);
            current_text.push('\n');

            // Split if too long
            if current_text.len() > self.max_chunk_size {
                self.flush_chunk(
                    document,
                    &current_text,
                    chunk_start,
                    &current_section,
                    &mut chunks,
                );
                current_text.clear();
                chunk_start = chunks.len();
            }
        }

        // Flush remaining
        if !current_text.trim().is_empty() {
            self.flush_chunk(
                document,
                &current_text,
                chunk_start,
                &current_section,
                &mut chunks,
            );
        }

        // If no chunks created (empty doc), create one empty chunk
        if chunks.is_empty() {
            chunks.push(make_chunk(document, text, 0, None));
        }

        chunks
    }

    fn flush_chunk(
        &self,
        document: &Document,
        text: &str,
        chunk_index: usize,
        section: &Option<String>,
        chunks: &mut Vec<Chunk>,
    ) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        chunks.push(make_chunk(
            document,
            trimmed,
            chunk_index as u32,
            section.as_deref(),
        ));
    }
}

fn make_chunk(document: &Document, text: &str, index: u32, section: Option<&str>) -> Chunk {
    Chunk {
        id: Uuid::now_v7(),
        document_id: document.id,
        source_id: document.source_id,
        chunk_index: index,
        text: text.to_string(),
        byte_start: 0,
        byte_end: text.len() as u64,
        section_path: section.map(|s| s.to_string()),
        content_type: document.content_type,
        language: document.language.clone(),
        timestamp: None,
        latitude: None,
        longitude: None,
        privacy_level: document.privacy_level,
        entity_ids: vec![],
    }
}
