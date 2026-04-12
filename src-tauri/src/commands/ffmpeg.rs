use domain::ffmpeg::codec::FfmpegCapabilities;
use tauri::State;

use crate::state::AppState;

#[derive(serde::Serialize)]
pub struct FfmpegStatus {
    pub available: bool,
    pub source: String,
    pub capabilities: Option<FfmpegCapabilities>,
    pub error: Option<String>,
}

#[tauri::command]
pub fn get_ffmpeg_status(state: State<'_, AppState>) -> FfmpegStatus {
    let source = state.ffmpeg.source_description();

    match state.ffmpeg.capabilities() {
        Ok(caps) => FfmpegStatus {
            available: true,
            source,
            capabilities: Some(caps),
            error: None,
        },
        Err(e) => FfmpegStatus {
            available: false,
            source,
            capabilities: None,
            error: Some(e.to_string()),
        },
    }
}
