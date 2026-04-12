use serde::Serialize;
use thiserror::Error;

/// Unified error type for the entire application domain.
/// Each variant maps to a category of failure. The `String` payloads
/// carry human-readable context — callers use `with_context()` or
/// format strings to describe what went wrong.
#[derive(Debug, Error, Serialize, Clone)]
#[serde(tag = "kind", content = "message")]
pub enum AppError {
    #[error("FFmpeg not found: {0}")]
    FfmpegNotFound(String),

    #[error("FFmpeg execution failed: {0}")]
    FfmpegExecution(String),

    #[error("Codec not available: {0}")]
    CodecUnavailable(String),

    #[error("Settings error: {0}")]
    Settings(String),

    #[error("Platform error: {0}")]
    Platform(String),

    #[error("I/O error: {0}")]
    Io(String),

    #[error("Capture error: {0}")]
    Capture(String),

    #[error("Encoding error: {0}")]
    Encoding(String),
}

pub type AppResult<T> = Result<T, AppError>;

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Io(err.to_string())
    }
}
