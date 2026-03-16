use lokb_core::Result;

/// LLM generation options.
pub struct LlmOptions {
    pub max_tokens: usize,
    pub temperature: f32,
}

impl Default for LlmOptions {
    fn default() -> Self {
        Self {
            max_tokens: 512,
            temperature: 0.3,
        }
    }
}

/// Trait for LLM backends (ADR-005).
pub trait LlmBackend: Send + Sync {
    fn name(&self) -> &str;
    fn generate(&self, prompt: &str, options: &LlmOptions) -> Result<String>;
}

/// Skip backend — returns empty string. Used as fallback.
pub struct SkipBackend;

impl LlmBackend for SkipBackend {
    fn name(&self) -> &str {
        "skip"
    }
    fn generate(&self, _prompt: &str, _options: &LlmOptions) -> Result<String> {
        Ok(String::new())
    }
}

/// Ollama backend — calls local Ollama API.
pub struct OllamaBackend {
    host: String,
    model: String,
}

impl OllamaBackend {
    pub fn new(model: &str) -> Self {
        Self {
            host: std::env::var("OLLAMA_HOST")
                .unwrap_or_else(|_| "http://localhost:11434".to_string()),
            model: model.to_string(),
        }
    }
}

impl LlmBackend for OllamaBackend {
    fn name(&self) -> &str {
        "ollama"
    }

    fn generate(&self, prompt: &str, options: &LlmOptions) -> Result<String> {
        let url = format!("{}/api/generate", self.host);
        let body = serde_json::json!({
            "model": self.model,
            "prompt": prompt,
            "stream": false,
            "options": {
                "num_predict": options.max_tokens,
                "temperature": options.temperature,
            }
        });

        let response = reqwest::blocking::Client::new()
            .post(&url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(120))
            .send()
            .map_err(|e| lokb_core::Error::Storage(format!("Ollama request failed: {e}")))?;

        if !response.status().is_success() {
            return Err(lokb_core::Error::Storage(format!(
                "Ollama error: {}",
                response.status()
            )));
        }

        let json: serde_json::Value = response
            .json()
            .map_err(|e| lokb_core::Error::Storage(format!("Ollama response parse failed: {e}")))?;

        Ok(json["response"].as_str().unwrap_or("").to_string())
    }
}

/// OpenAI-compatible backend (works with vLLM, LM Studio, etc.)
pub struct OpenAiCompatibleBackend {
    base_url: String,
    model: String,
    api_key: Option<String>,
}

impl OpenAiCompatibleBackend {
    pub fn new(base_url: &str, model: &str, api_key: Option<String>) -> Self {
        Self {
            base_url: base_url.to_string(),
            model: model.to_string(),
            api_key,
        }
    }
}

impl LlmBackend for OpenAiCompatibleBackend {
    fn name(&self) -> &str {
        "openai_compatible"
    }

    fn generate(&self, prompt: &str, options: &LlmOptions) -> Result<String> {
        let url = format!("{}/v1/chat/completions", self.base_url);
        let body = serde_json::json!({
            "model": self.model,
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": options.max_tokens,
            "temperature": options.temperature,
        });

        let mut request = reqwest::blocking::Client::new()
            .post(&url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(120));

        if let Some(ref key) = self.api_key {
            request = request.bearer_auth(key);
        }

        let response = request
            .send()
            .map_err(|e| lokb_core::Error::Storage(format!("API request failed: {e}")))?;

        if !response.status().is_success() {
            return Err(lokb_core::Error::Storage(format!(
                "API error: {}",
                response.status()
            )));
        }

        let json: serde_json::Value = response
            .json()
            .map_err(|e| lokb_core::Error::Storage(format!("API response parse failed: {e}")))?;

        Ok(json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string())
    }
}

/// Create LLM backend from string spec.
/// Formats: "skip", "ollama:model", "openai:url:model[:key]"
pub fn create_backend(spec: &str) -> Result<Box<dyn LlmBackend>> {
    if spec == "skip" || spec.is_empty() {
        return Ok(Box::new(SkipBackend));
    }

    let parts: Vec<&str> = spec.splitn(3, ':').collect();
    match parts[0] {
        "ollama" => {
            let model = parts.get(1).unwrap_or(&"phi3");
            Ok(Box::new(OllamaBackend::new(model)))
        }
        "openai" => {
            let url = parts.get(1).ok_or_else(|| {
                lokb_core::Error::Storage("openai spec requires url: openai:url:model".into())
            })?;
            let model = parts.get(2).unwrap_or(&"gpt-3.5-turbo");
            let key = std::env::var("OPENAI_API_KEY").ok();
            Ok(Box::new(OpenAiCompatibleBackend::new(url, model, key)))
        }
        _ => Err(lokb_core::Error::Storage(format!(
            "unknown LLM backend: {spec}. Use: skip, ollama:model, openai:url:model"
        ))),
    }
}
