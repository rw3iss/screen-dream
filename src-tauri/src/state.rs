use std::sync::Arc;

use domain::ffmpeg::FfmpegProvider;
use domain::platform::PlatformInfo;
use domain::settings::SettingsRepository;

/// Central application state, managed by Tauri.
/// Holds references to all infrastructure services.
/// Each service is behind Arc for cheap cloning in async contexts.
pub struct AppState {
    pub ffmpeg: Arc<dyn FfmpegProvider>,
    pub settings: Arc<dyn SettingsRepository>,
    pub platform: PlatformInfo,
}
