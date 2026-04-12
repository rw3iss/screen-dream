use serde::{Deserialize, Serialize};
use super::source::CaptureSource;

/// Recording lifecycle state, emitted to the frontend via Tauri events.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RecordingState {
    /// No recording in progress.
    Idle,
    /// Recording is starting (initializing pipeline).
    Starting,
    /// Actively recording.
    Recording,
    /// Recording is paused (can be resumed).
    Paused,
    /// Recording is stopping (finalizing file).
    Stopping,
    /// Recording completed successfully.
    Completed,
    /// Recording failed with an error message.
    Failed(String),
}

/// Configuration for a recording session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingConfig {
    /// What to capture (screen, window, or region).
    pub source: CaptureSource,
    /// Target frames per second (e.g., 30 or 60).
    pub fps: u32,
    /// Video codec to use (maps to FFmpeg encoder).
    pub video_codec: String,
    /// CRF quality value (0-51, lower = better).
    pub crf: u8,
    /// Encoding speed preset (e.g., "ultrafast", "fast", "medium").
    pub preset: String,
    /// Output file path.
    pub output_path: String,
    /// Whether to capture microphone audio.
    pub capture_microphone: bool,
    /// Microphone device name (if capture_microphone is true).
    pub microphone_device: Option<String>,
}

/// Status update emitted during recording.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingStatus {
    pub state: RecordingState,
    /// Duration of the recording so far, in seconds.
    pub elapsed_seconds: f64,
    /// Number of frames captured so far.
    pub frames_captured: u64,
    /// Output file path (available after completion).
    pub output_path: Option<String>,
}
