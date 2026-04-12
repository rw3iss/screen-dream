use std::path::{Path, PathBuf};

use domain::capture::{
    AvailableSources, CaptureBackend, CaptureSource, RecordingConfig, RecordingState,
    RecordingStatus,
};
use domain::error::AppError;
use infrastructure::capture::{
    list_audio_devices, AudioCapture, AudioDeviceInfo, RecordingPipeline,
};
use tauri::{Emitter, Manager, State};
use tracing::{error, info};

use crate::error::CommandResult;
use crate::state::{ActiveRecording, AppState, SendableAudioCapture};

/// Enumerate all available capture sources (monitors + windows).
#[tauri::command]
pub fn enumerate_sources(state: State<'_, AppState>) -> CommandResult<AvailableSources> {
    state.capture.enumerate_sources().map_err(Into::into)
}

/// Capture a screenshot from the given source and save it to a file.
/// Returns the path to the saved file.
#[tauri::command]
pub fn take_screenshot(
    state: State<'_, AppState>,
    source: CaptureSource,
    output_path: String,
) -> CommandResult<String> {
    let path = PathBuf::from(&output_path);
    let result =
        infrastructure::capture::capture_screenshot(state.capture.as_ref(), &source, &path)?;
    Ok(result.to_string_lossy().to_string())
}

/// Capture a screenshot and return it as a base64-encoded PNG string.
#[tauri::command]
pub fn take_screenshot_clipboard(
    state: State<'_, AppState>,
    source: CaptureSource,
) -> CommandResult<String> {
    infrastructure::capture::capture_screenshot_as_base64_png(state.capture.as_ref(), &source)
        .map_err(Into::into)
}

/// List available audio input devices.
#[tauri::command]
pub fn list_audio_devices_cmd() -> CommandResult<Vec<AudioDeviceInfo>> {
    list_audio_devices().map_err(Into::into)
}

/// Start a new recording session.
#[tauri::command]
pub fn start_recording(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
    config: RecordingConfig,
) -> CommandResult<()> {
    info!("start_recording command: {:?}", config);

    // Check if already recording.
    {
        let guard = state.active_recording.lock().map_err(|_| {
            AppError::Capture("Recording state lock poisoned".to_string())
        })?;
        if guard.is_some() {
            return Err(AppError::Capture(
                "A recording is already in progress. Stop it first.".to_string(),
            )
            .into());
        }
    }

    // Resolve FFmpeg path.
    let ffmpeg_path = state.ffmpeg.ffmpeg_path()?;

    // Emit starting event.
    let _ = app_handle.emit("recording-state-changed", RecordingState::Starting);

    // Start audio capture if requested.
    let audio = if config.capture_microphone {
        let audio_path = PathBuf::from(&config.output_path).with_extension("wav");
        match AudioCapture::start(config.microphone_device.as_deref(), audio_path) {
            Ok(capture) => {
                info!("Audio capture started");
                Some(SendableAudioCapture(capture))
            }
            Err(e) => {
                error!("Failed to start audio capture: {e}");
                // Don't fail the whole recording, just skip audio.
                let _ = app_handle.emit(
                    "recording-warning",
                    format!("Audio capture failed: {e}. Recording without audio."),
                );
                None
            }
        }
    } else {
        None
    };

    // Start the video recording pipeline.
    let pipeline =
        RecordingPipeline::start(ffmpeg_path, state.capture.clone(), config)?;

    // Store the active recording.
    {
        let mut guard = state.active_recording.lock().map_err(|_| {
            AppError::Capture("Recording state lock poisoned".to_string())
        })?;
        *guard = Some(ActiveRecording { pipeline, audio });
    }

    // Emit recording state.
    let _ = app_handle.emit("recording-state-changed", RecordingState::Recording);

    info!("Recording started");
    Ok(())
}

/// Stop the current recording.
/// Returns the path to the output video file.
#[tauri::command]
pub async fn stop_recording(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<String> {
    info!("stop_recording command");

    let _ = app_handle.emit("recording-state-changed", RecordingState::Stopping);

    // Take the active recording out of state (under the lock).
    let mut recording = {
        let mut guard = state.active_recording.lock().map_err(|_| {
            AppError::Capture("Recording state lock poisoned".to_string())
        })?;
        guard.take().ok_or_else(|| {
            AppError::Capture("No recording in progress".to_string())
        })?
    };

    // Stop audio capture first.
    let audio_path = if let Some(ref mut audio) = recording.audio {
        match audio.0.stop() {
            Ok(path) => Some(path),
            Err(e) => {
                error!("Failed to stop audio capture: {e}");
                None
            }
        }
    } else {
        None
    };

    // Stop video recording (async).
    let pipeline_result = recording.pipeline.stop().await?;
    let video_path = pipeline_result.output_path;

    // If we have audio, mux it with the video using FFmpeg.
    let final_path = if let Some(audio_path) = audio_path {
        let ffmpeg_path = state.ffmpeg.ffmpeg_path()?;
        let muxed_path = mux_audio_video(&ffmpeg_path, &video_path, &audio_path)?;
        // Clean up temp files.
        let _ = std::fs::remove_file(&video_path);
        let _ = std::fs::remove_file(&audio_path);
        muxed_path
    } else {
        video_path.to_string_lossy().to_string()
    };

    let _ = app_handle.emit("recording-state-changed", RecordingState::Completed);

    info!("Recording completed: {final_path}");
    Ok(final_path)
}

/// Pause the current recording.
#[tauri::command]
pub fn pause_recording(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let guard = state.active_recording.lock().map_err(|_| {
        AppError::Capture("Recording state lock poisoned".to_string())
    })?;

    let recording = guard.as_ref().ok_or_else(|| {
        AppError::Capture("No recording in progress".to_string())
    })?;

    recording.pipeline.pause();
    let _ = app_handle.emit("recording-state-changed", RecordingState::Paused);

    info!("Recording paused");
    Ok(())
}

/// Resume the current recording.
#[tauri::command]
pub fn resume_recording(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let guard = state.active_recording.lock().map_err(|_| {
        AppError::Capture("Recording state lock poisoned".to_string())
    })?;

    let recording = guard.as_ref().ok_or_else(|| {
        AppError::Capture("No recording in progress".to_string())
    })?;

    recording.pipeline.resume();
    let _ = app_handle.emit("recording-state-changed", RecordingState::Recording);

    info!("Recording resumed");
    Ok(())
}

/// Get the current recording status.
#[tauri::command]
pub fn get_recording_status(state: State<'_, AppState>) -> CommandResult<RecordingStatus> {
    let guard = state.active_recording.lock().map_err(|_| {
        AppError::Capture("Recording state lock poisoned".to_string())
    })?;

    match guard.as_ref() {
        Some(recording) => {
            let state = if recording.pipeline.is_paused() {
                RecordingState::Paused
            } else if recording.pipeline.is_running() {
                RecordingState::Recording
            } else {
                RecordingState::Stopping
            };
            Ok(RecordingStatus {
                state,
                elapsed_seconds: 0.0,
                frames_captured: 0,
                output_path: Some(
                    recording
                        .pipeline
                        .output_path()
                        .to_string_lossy()
                        .to_string(),
                ),
            })
        }
        None => Ok(RecordingStatus {
            state: RecordingState::Idle,
            elapsed_seconds: 0.0,
            frames_captured: 0,
            output_path: None,
        }),
    }
}

/// Show the transparent overlay window for region selection.
#[tauri::command]
pub fn show_region_selector(app_handle: tauri::AppHandle) -> CommandResult<()> {
    let window = app_handle
        .get_webview_window("region-selector")
        .ok_or_else(|| AppError::Capture("Region selector window not found".to_string()))?;

    window.show().map_err(|e| {
        AppError::Capture(format!("Failed to show region selector: {e}"))
    })?;

    window.set_focus().map_err(|e| {
        AppError::Capture(format!("Failed to focus region selector: {e}"))
    })?;

    info!("Region selector overlay shown");
    Ok(())
}

/// Hide the overlay window (called after selection is made or cancelled).
#[tauri::command]
pub fn hide_region_selector(app_handle: tauri::AppHandle) -> CommandResult<()> {
    let window = app_handle
        .get_webview_window("region-selector")
        .ok_or_else(|| AppError::Capture("Region selector window not found".to_string()))?;

    window.hide().map_err(|e| {
        AppError::Capture(format!("Failed to hide region selector: {e}"))
    })?;

    info!("Region selector overlay hidden");
    Ok(())
}

/// Mux a video file and an audio WAV file into a final output using FFmpeg.
fn mux_audio_video(
    ffmpeg_path: &Path,
    video_path: &Path,
    audio_path: &Path,
) -> Result<String, AppError> {
    let output_path = video_path.with_file_name(format!(
        "{}_final.{}",
        video_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy(),
        video_path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
    ));

    info!(
        "Muxing video ({}) + audio ({}) -> {}",
        video_path.display(),
        audio_path.display(),
        output_path.display()
    );

    let output = std::process::Command::new(ffmpeg_path)
        .args([
            "-hide_banner",
            "-y",
            "-i",
            &video_path.to_string_lossy(),
            "-i",
            &audio_path.to_string_lossy(),
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-shortest",
            &output_path.to_string_lossy(),
        ])
        .output()
        .map_err(|e| AppError::FfmpegExecution(format!("Failed to run mux: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::FfmpegExecution(format!(
            "Audio/video mux failed: {}",
            stderr.lines().last().unwrap_or("unknown")
        )));
    }

    Ok(output_path.to_string_lossy().to_string())
}
