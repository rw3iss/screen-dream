use std::path::PathBuf;

use super::codec::FfmpegCapabilities;
use crate::error::AppResult;

/// Abstraction over how we find and describe the FFmpeg binary.
/// Implemented by infrastructure layer (bundled sidecar, system PATH, etc.).
pub trait FfmpegProvider: Send + Sync {
    /// Returns the path to the ffmpeg binary.
    fn ffmpeg_path(&self) -> AppResult<PathBuf>;

    /// Returns the path to the ffprobe binary (if available).
    fn ffprobe_path(&self) -> AppResult<PathBuf>;

    /// Query the FFmpeg binary for its version and supported codecs.
    fn capabilities(&self) -> AppResult<FfmpegCapabilities>;

    /// Human-readable description of where FFmpeg was found.
    fn source_description(&self) -> String;
}
