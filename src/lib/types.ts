// Mirrors domain::platform::detect
export type Os = "linux" | "macos" | "windows";
export type DisplayServer = "x11" | "wayland" | "quartz" | "win32" | "unknown";

export interface PlatformInfo {
  os: Os;
  display_server: DisplayServer;
  arch: string;
}

// Mirrors domain::ffmpeg::codec
export type VideoCodec = "h264" | "h265" | "vp9" | "av1";
export type AudioCodec = "aac" | "opus" | "mp3";
export type ContainerFormat = "mp4" | "webm" | "mkv" | "gif";
export type ScreenshotFormat = "png" | "jpeg" | "webp";

export interface FfmpegCapabilities {
  version: string;
  video_encoders: VideoCodec[];
  audio_encoders: AudioCodec[];
}

export interface FfmpegStatus {
  available: boolean;
  source: string;
  capabilities: FfmpegCapabilities | null;
  error: string | null;
}

// Mirrors domain::settings::model
export interface RecordingSettings {
  fps: number;
  video_codec: VideoCodec;
  audio_codec: AudioCodec;
  container: ContainerFormat;
  crf: number;
  preset: string;
  capture_cursor: boolean;
  capture_audio: boolean;
  capture_microphone: boolean;
}

export interface ScreenshotSettings {
  format: ScreenshotFormat;
  quality: number;
  copy_to_clipboard: boolean;
  save_to_disk: boolean;
}

export interface ExportSettings {
  output_directory: string;
}

export interface ShortcutSettings {
  start_stop_recording: string;
  pause_resume_recording: string;
  take_screenshot: string;
}

export interface FfmpegSettings {
  source: string;
  custom_path: string | null;
}

export interface GeneralSettings {
  minimize_to_tray: boolean;
  start_minimized: boolean;
  show_notifications: boolean;
}

export interface AppSettings {
  recording: RecordingSettings;
  screenshot: ScreenshotSettings;
  export: ExportSettings;
  shortcuts: ShortcutSettings;
  ffmpeg: FfmpegSettings;
  general: GeneralSettings;
}
