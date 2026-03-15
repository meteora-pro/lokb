use async_trait::async_trait;
use lokb_core::Result;
use lokb_pipeline::{OptimizeEstimate, OptimizeStep, PipelineContext, StepData};
use std::time::Duration;

/// Converts HTML content to clean Markdown.
/// Used as a step in optimize pipelines for HTML-based sources
/// (Wikipedia ZIM articles, web pages, EPUB chapters).
pub struct HtmlToMarkdown;

#[async_trait]
impl OptimizeStep for HtmlToMarkdown {
    fn name(&self) -> &str {
        "html_to_markdown"
    }

    fn estimate(&self, input_size: u64) -> OptimizeEstimate {
        OptimizeEstimate {
            // HTML → Markdown typically 2-3x smaller (no tags)
            estimated_output_size: input_size / 2,
            compression_ratio: 2.0,
            compute_time: Duration::from_millis(input_size / 500_000 + 1),
            needs_gpu: false,
        }
    }

    async fn execute(&self, input: StepData, _ctx: &PipelineContext) -> Result<StepData> {
        match input {
            StepData::Text { content, document } => {
                let markdown = html_to_markdown(&content);
                Ok(StepData::Text {
                    content: markdown,
                    document,
                })
            }
            StepData::Documents(docs) => {
                let converted: Vec<_> = docs
                    .into_iter()
                    .map(|(doc, html)| {
                        let md = html_to_markdown(&html);
                        (doc, md)
                    })
                    .collect();
                Ok(StepData::Documents(converted))
            }
            other => Ok(other),
        }
    }
}

/// Convert HTML to Markdown using htmd.
pub fn html_to_markdown(html: &str) -> String {
    htmd::convert(html).unwrap_or_else(|_| {
        // Fallback: strip tags manually
        strip_html_tags(html)
    })
}

/// Simple fallback: strip HTML tags.
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_to_markdown() {
        let html = "<h1>Title</h1><p>Hello <strong>world</strong></p>";
        let md = html_to_markdown(html);
        assert!(md.contains("Title"));
        assert!(md.contains("**world**"));
    }

    #[test]
    fn test_strip_tags_fallback() {
        let html = "<p>Hello <b>world</b></p>";
        let result = strip_html_tags(html);
        assert_eq!(result, "Hello world");
    }
}
