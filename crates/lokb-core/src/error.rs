use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("source '{0}' not found")]
    SourceNotFound(String),

    #[error("source '{0}' already exists")]
    SourceAlreadyExists(String),

    #[error("document '{1}' not found in source '{0}'")]
    DocumentNotFound(String, String),

    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("pipeline error in step '{step}': {message}")]
    Pipeline { step: String, message: String },

    #[error("budget exceeded: {0}")]
    BudgetExceeded(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
