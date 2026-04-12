use domain::error::AppError;
use serde::Serialize;

/// Wrapper so AppError can be returned from #[tauri::command] functions.
/// Tauri v2 auto-implements `Into<InvokeError>` for any type that
/// implements `Serialize`, so we just need this to be `Serialize`.
#[derive(Debug, Serialize)]
pub struct CommandError {
    pub kind: String,
    pub message: String,
}

impl From<AppError> for CommandError {
    fn from(err: AppError) -> Self {
        let kind = match &err {
            AppError::FfmpegNotFound(_) => "ffmpeg_not_found",
            AppError::FfmpegExecution(_) => "ffmpeg_execution",
            AppError::CodecUnavailable(_) => "codec_unavailable",
            AppError::Settings(_) => "settings",
            AppError::Platform(_) => "platform",
            AppError::Io(_) => "io",
            AppError::Capture(_) => "capture",
            AppError::Encoding(_) => "encoding",
        }
        .to_string();

        CommandError {
            kind,
            message: err.to_string(),
        }
    }
}

/// Shorthand result type for Tauri commands.
pub type CommandResult<T> = Result<T, CommandError>;
