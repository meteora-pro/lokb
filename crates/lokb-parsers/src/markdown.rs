use async_trait::async_trait;
use lokb_core::{ContentHash, ContentType, Document, PrivacyLevel, Result};
use lokb_pipeline::{OptimizeEstimate, OptimizeStep, PipelineContext, StepData};
use std::fs;
use std::time::Duration;
use uuid::Uuid;

/// Passthrough step for markdown files: reads .md files from a directory
/// and outputs Documents with their text content.
/// Compression ratio ≈ 1.0 (no transformation, just ingestion).
pub struct MarkdownPassthrough;

#[async_trait]
impl OptimizeStep for MarkdownPassthrough {
    fn name(&self) -> &str {
        "markdown_passthrough"
    }

    fn estimate(&self, input_size: u64) -> OptimizeEstimate {
        OptimizeEstimate {
            estimated_output_size: input_size,
            compression_ratio: 1.0,
            compute_time: Duration::from_millis(input_size / 1_000_000 + 1),
            needs_gpu: false,
        }
    }

    async fn execute(&self, input: StepData, ctx: &PipelineContext) -> Result<StepData> {
        match input {
            StepData::Bytes(path_bytes) => {
                // Input: path to directory as bytes
                let dir_path = String::from_utf8_lossy(&path_bytes);
                let dir = std::path::Path::new(dir_path.as_ref());

                let mut documents = Vec::new();
                for entry in fs::read_dir(dir)? {
                    let entry = entry?;
                    let path = entry.path();
                    if path.extension().is_some_and(|ext| ext == "md") {
                        let content = fs::read_to_string(&path)?;
                        let filename = path.file_name().unwrap().to_string_lossy().to_string();
                        let external_id = filename.trim_end_matches(".md").to_string();
                        let title = extract_title(&content).unwrap_or(external_id.clone());

                        let doc = Document {
                            id: Uuid::now_v7(),
                            source_id: ctx.source_id,
                            external_id,
                            parent_id: None,
                            depth: 0,
                            title,
                            content_type: ContentType::Article,
                            language: Some("en".to_string()),
                            content_hash: ContentHash::from_bytes(content.as_bytes()),
                            content_size: content.len() as u64,
                            created_at: chrono::Utc::now(),
                            indexed_at: chrono::Utc::now(),
                            privacy_level: PrivacyLevel::Public,
                        };
                        documents.push((doc, content));
                    }
                }
                Ok(StepData::Documents(documents))
            }
            other => Ok(other),
        }
    }
}

fn extract_title(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(heading) = trimmed.strip_prefix("# ") {
            return Some(heading.to_string());
        }
    }
    None
}
