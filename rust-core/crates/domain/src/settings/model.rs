use serde::{Deserialize, Serialize};

use crate::ffmpeg::codec::{AudioCodec, ContainerFormat, VideoCodec};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub recording: RecordingSettings,
    pub screenshot: ScreenshotSettings,
    pub export: ExportSettings,
    pub shortcuts: ShortcutSettings,
    pub ffmpeg: FfmpegSettings,
    pub general: GeneralSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingSettings {
    pub fps: u32,
    pub video_codec: VideoCodec,
    pub audio_codec: AudioCodec,
    pub container: ContainerFormat,
    pub crf: u8,
    pub preset: String,
    pub capture_cursor: bool,
    pub capture_audio: bool,
    pub capture_microphone: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotSettings {
    pub format: ScreenshotFormat,
    pub quality: u8,
    pub copy_to_clipboard: bool,
    pub save_to_disk: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScreenshotFormat {
    Png,
    Jpeg,
    Webp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportSettings {
    pub output_directory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutSettings {
    pub start_stop_recording: String,
    pub pause_resume_recording: String,
    pub take_screenshot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FfmpegSettings {
    /// "bundled", "system", or an absolute path
    pub source: String,
    pub custom_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralSettings {
    pub minimize_to_tray: bool,
    pub start_minimized: bool,
    pub show_notifications: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            recording: RecordingSettings {
                fps: 30,
                video_codec: VideoCodec::H264,
                audio_codec: AudioCodec::Aac,
                container: ContainerFormat::Mp4,
                crf: 23,
                preset: "fast".to_string(),
                capture_cursor: true,
                capture_audio: true,
                capture_microphone: false,
            },
            screenshot: ScreenshotSettings {
                format: ScreenshotFormat::Png,
                quality: 100,
                copy_to_clipboard: true,
                save_to_disk: true,
            },
            export: ExportSettings {
                output_directory: String::new(), // empty = ~/Pictures at runtime
            },
            shortcuts: ShortcutSettings {
                start_stop_recording: "CommandOrControl+Shift+R".to_string(),
                pause_resume_recording: "CommandOrControl+Shift+P".to_string(),
                take_screenshot: "CommandOrControl+Shift+S".to_string(),
            },
            ffmpeg: FfmpegSettings {
                source: "bundled".to_string(),
                custom_path: None,
            },
            general: GeneralSettings {
                minimize_to_tray: true,
                start_minimized: false,
                show_notifications: true,
            },
        }
    }
}
