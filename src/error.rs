use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum DocodeError {
    #[error("{0}")]
    Message(String),
    #[error("I/O operation failed for {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("configuration error: {0}")]
    Config(String),
    #[error("security policy blocked the operation: {0}")]
    Safety(String),
    #[error("provider request failed: {0}")]
    Provider(String),
}

pub type Result<T> = std::result::Result<T, DocodeError>;

pub fn io_error(path: impl Into<PathBuf>, source: std::io::Error) -> DocodeError {
    DocodeError::Io {
        path: path.into(),
        source,
    }
}
