use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use lokb_core::Result;

/// Wrapper around fastembed for generating text embeddings.
/// Default model: multilingual-e5-small (384 dims, ~120MB).
pub struct Embedder {
    model: TextEmbedding,
    dimensions: usize,
}

impl Embedder {
    /// Create with default model (multilingual-e5-small).
    /// Downloads model on first use (~120MB).
    pub fn new() -> Result<Self> {
        Self::with_model(EmbeddingModel::MultilingualE5Small)
    }

    /// Create with a specific model.
    pub fn with_model(model: EmbeddingModel) -> Result<Self> {
        let dimensions = match model {
            EmbeddingModel::MultilingualE5Small => 384,
            EmbeddingModel::AllMiniLML6V2 => 384,
            EmbeddingModel::BGESmallENV15 => 384,
            _ => 384, // default fallback
        };

        let text_embedding =
            TextEmbedding::try_new(InitOptions::new(model).with_show_download_progress(true))
                .map_err(|e| {
                    lokb_core::Error::Storage(format!("embedding model init failed: {e}"))
                })?;

        Ok(Self {
            model: text_embedding,
            dimensions,
        })
    }

    /// Get embedding dimensions.
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }

    /// Embed a batch of texts. Returns Vec of f32 vectors.
    pub fn embed(&mut self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        // fastembed expects Vec<String>
        let documents: Vec<String> = texts.iter().map(|t| t.to_string()).collect();

        let embeddings = self
            .model
            .embed(documents, None)
            .map_err(|e| lokb_core::Error::Storage(format!("embedding failed: {e}")))?;

        Ok(embeddings)
    }

    /// Embed a single text.
    pub fn embed_one(&mut self, text: &str) -> Result<Vec<f32>> {
        let results = self.embed(&[text])?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| lokb_core::Error::Storage("no embedding returned".to_string()))
    }
}

/// Cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let c = vec![0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &c).abs() < 0.001);
    }
}
