use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DaoError {
    #[error("failed to read file {path}: {source}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse toml config {path}: {source}")]
    ParseToml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("failed to create directory {path}: {source}")]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to create log file {path}: {source}")]
    CreateLogFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("logger initialization failed: {0}")]
    LoggerInit(#[from] tracing::subscriber::SetGlobalDefaultError),
}
