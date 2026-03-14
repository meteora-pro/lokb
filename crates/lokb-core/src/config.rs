use std::path::PathBuf;
use std::sync::OnceLock;

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Root data directory. Overridable via LOKB_DATA_DIR env var.
/// Cached after first call via OnceLock.
pub fn data_dir() -> &'static PathBuf {
    DATA_DIR.get_or_init(|| {
        if let Ok(dir) = std::env::var("LOKB_DATA_DIR") {
            PathBuf::from(dir)
        } else {
            default_data_dir()
        }
    })
}

fn default_data_dir() -> PathBuf {
    let base = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join(".local/share/lokb")
}

/// Standard subdirectories within data_dir
pub fn sources_config_dir() -> PathBuf {
    data_dir().join("sources")
}

pub fn source_content_dir() -> PathBuf {
    data_dir().join("source")
}

pub fn derived_dir() -> PathBuf {
    data_dir().join("derived")
}

pub fn cache_dir() -> PathBuf {
    data_dir().join("cache")
}

pub fn raw_dir() -> PathBuf {
    data_dir().join("raw")
}

pub fn models_dir() -> PathBuf {
    data_dir().join("models")
}

/// Ensure the base directory structure exists.
pub fn init_dirs() -> std::io::Result<()> {
    for dir in [
        sources_config_dir(),
        source_content_dir(),
        derived_dir(),
        cache_dir(),
        raw_dir(),
    ] {
        std::fs::create_dir_all(dir)?;
    }
    Ok(())
}
