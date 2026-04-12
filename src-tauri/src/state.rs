use std::sync::{Arc, Mutex};

use domain::ffmpeg::FfmpegProvider;
use domain::platform::PlatformInfo;
use domain::settings::SettingsRepository;
use infrastructure::capture::{AudioCapture, RecordingPipeline, XcapCaptureBackend};

/// Wrapper to make AudioCapture Send + Sync.
/// Safety: AudioCapture contains a cpal::Stream which is !Send+!Sync due to
/// platform marker types, but we only ever access it behind a Mutex from the
/// same logical context (Tauri command handlers). The stream itself is safe to
/// drop from any thread.
pub struct SendableAudioCapture(pub AudioCapture);

// SAFETY: AudioCapture is only accessed behind Mutex<Option<ActiveRecording>>.
// The cpal Stream's !Send is a conservative platform marker, not a real safety issue
// when protected by a Mutex.
unsafe impl Send for SendableAudioCapture {}
unsafe impl Sync for SendableAudioCapture {}

/// Active recording session, if any.
pub struct ActiveRecording {
    pub pipeline: RecordingPipeline,
    pub audio: Option<SendableAudioCapture>,
}

/// Central application state, managed by Tauri.
/// Holds references to all infrastructure services.
/// Each service is behind Arc for cheap cloning in async contexts.
pub struct AppState {
    pub ffmpeg: Arc<dyn FfmpegProvider>,
    pub settings: Arc<dyn SettingsRepository>,
    pub platform: PlatformInfo,
    pub capture: Arc<XcapCaptureBackend>,
    /// Currently active recording session (if any). Mutex for interior mutability.
    pub active_recording: Mutex<Option<ActiveRecording>>,
}
