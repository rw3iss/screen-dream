import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppSettings,
  AudioDeviceInfo,
  AvailableSources,
  CaptureSource,
  FfmpegStatus,
  PlatformInfo,
  RecordingConfig,
  RecordingState,
  RecordingStatus,
} from "./types";

// Platform
export const getPlatformInfo = () =>
  invoke<PlatformInfo>("get_platform_info");

// Settings
export const getSettings = () =>
  invoke<AppSettings>("get_settings");

export const saveSettings = (settings: AppSettings) =>
  invoke<void>("save_settings", { settings });

export const resetSettings = () =>
  invoke<AppSettings>("reset_settings");

// FFmpeg
export const getFfmpegStatus = () =>
  invoke<FfmpegStatus>("get_ffmpeg_status");

// Capture & Recording
export const enumerateSources = () =>
  invoke<AvailableSources>("enumerate_sources");

export const takeScreenshot = (source: CaptureSource, outputPath: string) =>
  invoke<string>("take_screenshot", { source, outputPath });

export const takeScreenshotClipboard = (source: CaptureSource) =>
  invoke<string>("take_screenshot_clipboard", { source });

export const listAudioDevices = () =>
  invoke<AudioDeviceInfo[]>("list_audio_devices_cmd");

export const startRecording = (config: RecordingConfig) =>
  invoke<void>("start_recording", { config });

export const stopRecording = () =>
  invoke<string>("stop_recording");

export const pauseRecording = () =>
  invoke<void>("pause_recording");

export const resumeRecording = () =>
  invoke<void>("resume_recording");

export const getRecordingStatus = () =>
  invoke<RecordingStatus>("get_recording_status");

// Region selector overlay
export const showRegionSelector = () =>
  invoke<void>("show_region_selector");

export const hideRegionSelector = () =>
  invoke<void>("hide_region_selector");

// Event listeners
export const onRecordingState = (
  callback: (state: RecordingState) => void,
): Promise<UnlistenFn> =>
  listen<RecordingState>("recording-state-changed", (event) => {
    callback(event.payload);
  });

export const onRecordingWarning = (
  callback: (message: string) => void,
): Promise<UnlistenFn> =>
  listen<string>("recording-warning", (event) => {
    callback(event.payload);
  });
