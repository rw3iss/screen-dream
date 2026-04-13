//! Recording FFI functions.

use std::ffi::c_char;
use std::path::PathBuf;
use std::process::Command;
use std::ptr;
use std::time::Instant;

use domain::capture::RecordingConfig;
use domain::error::AppError;

use infrastructure::capture::{AudioCapture, PortalRecorder, RecordingPipeline};

use crate::core;
use crate::types::{
    from_c_str, to_c_string, SDError, SDRecordingConfig, SDRecordingHandle,
    SDRecordingStatus,
};

// ---------------------------------------------------------------------------
// Internal fields for SDRecordingHandle
// ---------------------------------------------------------------------------

/// Dual recorder support: xcap-based pipeline or portal-based recorder.
enum RecorderInner {
    Xcap {
        pipeline: RecordingPipeline,
        audio: Option<AudioCapture>,
    },
    Portal {
        recorder: PortalRecorder,
        audio: Option<AudioCapture>,
    },
}

/// The real contents of an SDRecordingHandle, hidden from FFI callers.
struct RecordingHandleInner {
    recorder: RecorderInner,
    start_time: Instant,
    frames_at_stop: u64,
    video_path: PathBuf,
    #[allow(dead_code)]
    audio_path: Option<PathBuf>,
}

// Re-define SDRecordingHandle here with the actual internal state.
// We shadow the placeholder from types.rs by using our own struct that
// is passed through the opaque pointer.

/// Start a recording session.
///
/// Takes a recording configuration and returns an opaque handle.
/// The caller must eventually call `sd_stop_recording` or `sd_free_recording_handle`.
///
/// On failure sets `*error` and returns null.
///
/// # Safety
/// * `config` must be a valid pointer to an `SDRecordingConfig`.
/// * `error` must be a valid pointer to a `*mut SDError` (may point to null).
#[no_mangle]
pub unsafe extern "C" fn sd_start_recording(
    config: *const SDRecordingConfig,
    error: *mut *mut SDError,
) -> *mut SDRecordingHandle {
    if config.is_null() {
        if !error.is_null() {
            unsafe {
                *error = SDError::from_app_error(AppError::Capture(
                    "config parameter is null".to_string(),
                ));
            }
        }
        return ptr::null_mut();
    }

    let cfg = unsafe { &*config };

    // Convert SDCaptureSource to domain CaptureSource.
    let domain_source = match cfg.source.to_domain() {
        Ok(s) => s,
        Err(e) => {
            if !error.is_null() {
                unsafe { *error = SDError::from_app_error(e) };
            }
            return ptr::null_mut();
        }
    };

    let output_path_str = unsafe { from_c_str(cfg.output_path) };
    if output_path_str.is_empty() {
        if !error.is_null() {
            unsafe {
                *error = SDError::from_app_error(AppError::Capture(
                    "output_path is empty".to_string(),
                ));
            }
        }
        return ptr::null_mut();
    }

    let video_codec = unsafe { from_c_str(cfg.video_codec) };
    let preset = unsafe { from_c_str(cfg.preset) };
    let mic_device_str = unsafe { from_c_str(cfg.microphone_device) };

    let recording_config = RecordingConfig {
        source: domain_source,
        fps: cfg.fps,
        video_codec: if video_codec.is_empty() {
            "libx264".to_string()
        } else {
            video_codec
        },
        crf: cfg.crf,
        preset: if preset.is_empty() {
            "fast".to_string()
        } else {
            preset
        },
        output_path: output_path_str.clone(),
        capture_microphone: cfg.capture_microphone,
        microphone_device: if mic_device_str.is_empty() {
            None
        } else {
            Some(mic_device_str.clone())
        },
    };

    // Resolve FFmpeg path.
    let state = core();
    let ffmpeg_path = match state.ffmpeg.ffmpeg_path() {
        Ok(p) => p,
        Err(e) => {
            if !error.is_null() {
                unsafe { *error = SDError::from_app_error(e) };
            }
            return ptr::null_mut();
        }
    };

    // Always use the RecordingPipeline (capture_frame loop → FFmpeg).
    // On KDE Wayland, capture_frame uses the already-running PipeWire stream
    // (initialized eagerly at startup). The PortalRecorder opens a separate
    // portal session which blocks and is redundant.
    let use_portal = false;

    // Optionally start audio capture (shared by both paths).
    let (audio, audio_path) = if cfg.capture_microphone {
        let audio_out = {
            let video_path = PathBuf::from(&output_path_str);
            let stem = video_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("recording");
            let parent = video_path.parent().unwrap_or(std::path::Path::new("."));
            parent.join(format!("{stem}_audio.wav"))
        };

        let device = if mic_device_str.is_empty() {
            None
        } else {
            Some(mic_device_str.as_str())
        };

        match AudioCapture::start(device, audio_out.clone()) {
            Ok(ac) => (Some(ac), Some(audio_out)),
            Err(e) => {
                // Audio failure is not fatal -- log and continue without audio.
                tracing::warn!("Failed to start audio capture: {e}");
                (None, None)
            }
        }
    } else {
        (None, None)
    };

    let recorder = if use_portal {
        // Load restore token from config dir (if available).
        let restore_token = load_portal_restore_token();
        tracing::info!(
            "Starting portal-based recording (restore_token={})",
            restore_token.is_some()
        );

        match PortalRecorder::start(
            ffmpeg_path,
            recording_config.clone(),
            restore_token,
        ) {
            Ok(r) => RecorderInner::Portal {
                recorder: r,
                audio,
            },
            Err(e) => {
                if !error.is_null() {
                    unsafe { *error = SDError::from_app_error(e) };
                }
                return ptr::null_mut();
            }
        }
    } else {
        // Start the xcap-based video recording pipeline.
        tracing::info!("Starting xcap-based recording pipeline");
        match RecordingPipeline::start(
            ffmpeg_path,
            state.capture.clone(),
            recording_config.clone(),
        ) {
            Ok(p) => RecorderInner::Xcap {
                pipeline: p,
                audio,
            },
            Err(e) => {
                if !error.is_null() {
                    unsafe { *error = SDError::from_app_error(e) };
                }
                return ptr::null_mut();
            }
        }
    };

    let inner = RecordingHandleInner {
        recorder,
        start_time: Instant::now(),
        frames_at_stop: 0,
        video_path: PathBuf::from(&output_path_str),
        audio_path,
    };

    // Store inner inside the opaque SDRecordingHandle.
    let handle = Box::new(SDRecordingHandle {
        inner: Box::into_raw(Box::new(inner)) as *mut std::ffi::c_void,
    });

    Box::into_raw(handle)
}

/// Stop a recording and get the final output file path.
///
/// If audio was captured, the video and audio are muxed together using FFmpeg.
/// The final path is written to `*out_path` (caller must free with `sd_free_string`).
///
/// On success returns `true`. On failure sets `*error` and returns `false`.
///
/// # Safety
/// * `handle` must have been returned by `sd_start_recording`.
/// * `out_path` must be a valid pointer to a `*mut c_char` (may point to null).
/// * `error` must be a valid pointer to a `*mut SDError` (may point to null).
#[no_mangle]
pub unsafe extern "C" fn sd_stop_recording(
    handle: *mut SDRecordingHandle,
    out_path: *mut *mut c_char,
    error: *mut *mut SDError,
) -> bool {
    if handle.is_null() {
        if !error.is_null() {
            unsafe {
                *error = SDError::from_app_error(AppError::Capture(
                    "handle is null".to_string(),
                ));
            }
        }
        return false;
    }

    let handle_ref = unsafe { &mut *handle };
    if handle_ref.inner.is_null() {
        if !error.is_null() {
            unsafe {
                *error = SDError::from_app_error(AppError::Capture(
                    "recording handle already consumed".to_string(),
                ));
            }
        }
        return false;
    }

    let mut inner = unsafe { Box::from_raw(handle_ref.inner as *mut RecordingHandleInner) };
    handle_ref.inner = ptr::null_mut();

    // Stop the recorder and extract audio handle.
    let (frames_captured, audio_wav_path) = match &mut inner.recorder {
        RecorderInner::Xcap { pipeline, audio } => {
            let pipeline_result = match pipeline.stop() {
                Ok(r) => r,
                Err(e) => {
                    if !error.is_null() {
                        unsafe { *error = SDError::from_app_error(e) };
                    }
                    return false;
                }
            };
            let wav = if let Some(ref mut ac) = audio {
                match ac.stop() {
                    Ok(p) => Some(p),
                    Err(e) => {
                        tracing::warn!("Failed to stop audio capture: {e}");
                        None
                    }
                }
            } else {
                None
            };
            (pipeline_result.frames_captured, wav)
        }
        RecorderInner::Portal { recorder, audio } => {
            let (pipeline_result, restore_token) = match recorder.stop() {
                Ok(r) => r,
                Err(e) => {
                    if !error.is_null() {
                        unsafe { *error = SDError::from_app_error(e) };
                    }
                    return false;
                }
            };
            // Save the restore token for future sessions.
            if let Some(token) = restore_token {
                save_portal_restore_token(&token);
            }
            let wav = if let Some(ref mut ac) = audio {
                match ac.stop() {
                    Ok(p) => Some(p),
                    Err(e) => {
                        tracing::warn!("Failed to stop audio capture: {e}");
                        None
                    }
                }
            } else {
                None
            };
            (pipeline_result.frames_captured, wav)
        }
    };

    inner.frames_at_stop = frames_captured;

    // If we have both video and audio, mux them together with FFmpeg.
    let final_path = if let Some(wav_path) = audio_wav_path {
        let state = core();
        match state.ffmpeg.ffmpeg_path() {
            Ok(ffmpeg_path) => {
                // Build muxed output path.
                let muxed_path = {
                    let stem = inner
                        .video_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("recording");
                    let ext = inner
                        .video_path
                        .extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or("mp4");
                    let parent = inner
                        .video_path
                        .parent()
                        .unwrap_or(std::path::Path::new("."));
                    parent.join(format!("{stem}_final.{ext}"))
                };

                let status = Command::new(&ffmpeg_path)
                    .args([
                        "-y",
                        "-i",
                        inner.video_path.to_str().unwrap_or(""),
                        "-i",
                        wav_path.to_str().unwrap_or(""),
                        "-c",
                        "copy",
                        muxed_path.to_str().unwrap_or(""),
                    ])
                    .output();

                match status {
                    Ok(output) if output.status.success() => {
                        // Clean up temporary files.
                        let _ = std::fs::remove_file(&inner.video_path);
                        let _ = std::fs::remove_file(&wav_path);
                        // Rename muxed to original path.
                        let _ = std::fs::rename(&muxed_path, &inner.video_path);
                        inner.video_path.clone()
                    }
                    Ok(output) => {
                        tracing::warn!(
                            "FFmpeg mux failed (exit {}): {}",
                            output.status,
                            String::from_utf8_lossy(&output.stderr)
                        );
                        // Fall back to video-only output.
                        inner.video_path.clone()
                    }
                    Err(e) => {
                        tracing::warn!("Failed to spawn FFmpeg for muxing: {e}");
                        inner.video_path.clone()
                    }
                }
            }
            Err(_) => {
                tracing::warn!("FFmpeg not available for muxing, returning video-only");
                inner.video_path.clone()
            }
        }
    } else {
        inner.video_path.clone()
    };

    if !out_path.is_null() {
        unsafe {
            *out_path = to_c_string(final_path.to_str().unwrap_or(""));
        }
    }

    true
}

/// Pause an active recording.
///
/// On success returns `true`. On failure sets `*error` and returns `false`.
///
/// # Safety
/// `handle` must have been returned by `sd_start_recording` and not yet stopped.
#[no_mangle]
pub unsafe extern "C" fn sd_pause_recording(
    handle: *mut SDRecordingHandle,
    error: *mut *mut SDError,
) -> bool {
    if handle.is_null() {
        if !error.is_null() {
            unsafe {
                *error = SDError::from_app_error(AppError::Capture(
                    "handle is null".to_string(),
                ));
            }
        }
        return false;
    }

    let handle_ref = unsafe { &*handle };
    if handle_ref.inner.is_null() {
        if !error.is_null() {
            unsafe {
                *error = SDError::from_app_error(AppError::Capture(
                    "recording handle already consumed".to_string(),
                ));
            }
        }
        return false;
    }

    let inner = unsafe { &*(handle_ref.inner as *const RecordingHandleInner) };
    match &inner.recorder {
        RecorderInner::Xcap { pipeline, .. } => pipeline.pause(),
        RecorderInner::Portal { recorder, .. } => recorder.pause(),
    }
    true
}

/// Resume a paused recording.
///
/// On success returns `true`. On failure sets `*error` and returns `false`.
///
/// # Safety
/// `handle` must have been returned by `sd_start_recording` and not yet stopped.
#[no_mangle]
pub unsafe extern "C" fn sd_resume_recording(
    handle: *mut SDRecordingHandle,
    error: *mut *mut SDError,
) -> bool {
    if handle.is_null() {
        if !error.is_null() {
            unsafe {
                *error = SDError::from_app_error(AppError::Capture(
                    "handle is null".to_string(),
                ));
            }
        }
        return false;
    }

    let handle_ref = unsafe { &*handle };
    if handle_ref.inner.is_null() {
        if !error.is_null() {
            unsafe {
                *error = SDError::from_app_error(AppError::Capture(
                    "recording handle already consumed".to_string(),
                ));
            }
        }
        return false;
    }

    let inner = unsafe { &*(handle_ref.inner as *const RecordingHandleInner) };
    match &inner.recorder {
        RecorderInner::Xcap { pipeline, .. } => pipeline.resume(),
        RecorderInner::Portal { recorder, .. } => recorder.resume(),
    }
    true
}

/// Get the current status of a recording.
///
/// Returns an `SDRecordingStatus` by value (no heap allocation).
///
/// # Safety
/// `handle` must have been returned by `sd_start_recording`.
#[no_mangle]
pub unsafe extern "C" fn sd_get_recording_status(
    handle: *const SDRecordingHandle,
) -> SDRecordingStatus {
    if handle.is_null() {
        return SDRecordingStatus {
            state: 0, // Idle
            elapsed_seconds: 0.0,
            frames_captured: 0,
        };
    }

    let handle_ref = unsafe { &*handle };
    if handle_ref.inner.is_null() {
        // Handle was consumed (stopped).
        return SDRecordingStatus {
            state: 5, // Completed
            elapsed_seconds: 0.0,
            frames_captured: 0,
        };
    }

    let inner = unsafe { &*(handle_ref.inner as *const RecordingHandleInner) };
    let elapsed = inner.start_time.elapsed().as_secs_f64();

    let state = match &inner.recorder {
        RecorderInner::Xcap { pipeline, .. } => {
            if !pipeline.is_running() {
                5 // Completed
            } else if pipeline.is_paused() {
                3 // Paused
            } else {
                2 // Recording
            }
        }
        RecorderInner::Portal { recorder, .. } => {
            if !recorder.is_running() {
                5 // Completed
            } else if recorder.is_paused() {
                3 // Paused
            } else {
                2 // Recording
            }
        }
    };

    SDRecordingStatus {
        state,
        elapsed_seconds: elapsed,
        frames_captured: 0, // Not easily accessible without modifying pipeline; report elapsed time
    }
}

/// Free a recording handle.
///
/// If the recording is still in progress, it will be stopped first.
///
/// # Safety
/// `handle` must have been returned by `sd_start_recording`, or be null.
#[no_mangle]
pub unsafe extern "C" fn sd_free_recording_handle(handle: *mut SDRecordingHandle) {
    if handle.is_null() {
        return;
    }

    let h = unsafe { Box::from_raw(handle) };
    if !h.inner.is_null() {
        let mut inner = unsafe { Box::from_raw(h.inner as *mut RecordingHandleInner) };
        // Stop the recorder if still running.
        match &mut inner.recorder {
            RecorderInner::Xcap { pipeline, audio } => {
                if pipeline.is_running() {
                    let _ = pipeline.stop();
                }
                if let Some(ref mut ac) = audio {
                    let _ = ac.stop();
                }
            }
            RecorderInner::Portal { recorder, audio } => {
                if recorder.is_running() {
                    let _ = recorder.stop();
                }
                if let Some(ref mut ac) = audio {
                    let _ = ac.stop();
                }
            }
        }
    }
}

/// List available audio input devices as a JSON array.
///
/// On success returns a heap-allocated NUL-terminated JSON string.
/// The caller must free it with `sd_free_string`.
///
/// On failure sets `*error` and returns null.
///
/// # Safety
/// * `error` must be a valid pointer to a `*mut SDError` (may point to null).
#[no_mangle]
pub unsafe extern "C" fn sd_list_audio_devices(
    error: *mut *mut SDError,
) -> *mut c_char {
    match infrastructure::capture::list_audio_devices() {
        Ok(devices) => {
            match serde_json::to_string(&devices) {
                Ok(json) => to_c_string(&json),
                Err(e) => {
                    if !error.is_null() {
                        unsafe {
                            *error = SDError::from_app_error(AppError::Capture(format!(
                                "Failed to serialize audio devices: {e}"
                            )));
                        }
                    }
                    ptr::null_mut()
                }
            }
        }
        Err(e) => {
            if !error.is_null() {
                unsafe { *error = SDError::from_app_error(e) };
            }
            ptr::null_mut()
        }
    }
}

// ---------------------------------------------------------------------------
// Portal restore token persistence
// ---------------------------------------------------------------------------

/// Path for storing the portal restore token.
fn portal_token_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("com.screendream.app").join("portal_tokens.json"))
}

/// Load a saved portal restore token from disk.
fn load_portal_restore_token() -> Option<String> {
    let path = portal_token_path()?;
    let data = std::fs::read_to_string(&path).ok()?;
    let obj: serde_json::Value = serde_json::from_str(&data).ok()?;
    obj.get("restore_token")?.as_str().map(|s| s.to_string())
}

/// Save a portal restore token to disk for future sessions.
fn save_portal_restore_token(token: &str) {
    let Some(path) = portal_token_path() else {
        tracing::warn!("Cannot determine config dir for portal token");
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let obj = serde_json::json!({ "restore_token": token });
    match std::fs::write(&path, serde_json::to_string_pretty(&obj).unwrap_or_default()) {
        Ok(_) => tracing::debug!("Saved portal restore token to {}", path.display()),
        Err(e) => tracing::warn!("Failed to save portal restore token: {e}"),
    }
}
