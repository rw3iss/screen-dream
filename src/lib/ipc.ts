import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  FfmpegStatus,
  PlatformInfo,
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
