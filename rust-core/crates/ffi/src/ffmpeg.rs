//! FFmpeg status FFI functions.

use crate::core;
use crate::types::{to_c_string, SDError, SDFfmpegStatus};

/// Query FFmpeg availability and capabilities.
///
/// On success returns a heap-allocated `SDFfmpegStatus`. The caller must
/// free it with `sd_free_ffmpeg_status`.
///
/// On failure sets `*error` and returns null.
///
/// # Safety
/// * `sd_init` must have been called successfully.
/// * `error` must be a valid pointer to a `*mut SDError` (may point to null).
#[no_mangle]
pub unsafe extern "C" fn sd_get_ffmpeg_status(
    error: *mut *mut SDError,
) -> *mut SDFfmpegStatus {
    let state = core();

    // Check if FFmpeg path resolves (i.e. is available).
    let path_result = state.ffmpeg.ffmpeg_path();
    let available = path_result.is_ok();

    if !available {
        // FFmpeg not found -- return a status with available=false.
        let status = Box::new(SDFfmpegStatus {
            available: false,
            version: to_c_string(""),
            source_description: to_c_string(&state.ffmpeg.source_description()),
            video_encoders_json: to_c_string("[]"),
            audio_encoders_json: to_c_string("[]"),
        });
        return Box::into_raw(status);
    }

    // Query capabilities.
    match state.ffmpeg.capabilities() {
        Ok(caps) => {
            let video_json: Vec<String> = caps
                .video_encoders
                .iter()
                .map(|c| format!("\"{}\"", c.encoder_name()))
                .collect();
            let audio_json: Vec<String> = caps
                .audio_encoders
                .iter()
                .map(|c| format!("\"{}\"", c.encoder_name()))
                .collect();

            let status = Box::new(SDFfmpegStatus {
                available: true,
                version: to_c_string(&caps.version),
                source_description: to_c_string(&state.ffmpeg.source_description()),
                video_encoders_json: to_c_string(&format!("[{}]", video_json.join(","))),
                audio_encoders_json: to_c_string(&format!("[{}]", audio_json.join(","))),
            });
            Box::into_raw(status)
        }
        Err(e) => {
            if !error.is_null() {
                unsafe { *error = SDError::from_app_error(e) };
            }
            std::ptr::null_mut()
        }
    }
}

/// Free an `SDFfmpegStatus` returned by `sd_get_ffmpeg_status`.
///
/// # Safety
/// `status` must have been returned by `sd_get_ffmpeg_status`, or be null.
#[no_mangle]
pub unsafe extern "C" fn sd_free_ffmpeg_status(status: *mut SDFfmpegStatus) {
    if !status.is_null() {
        let s = unsafe { Box::from_raw(status) };
        unsafe {
            crate::sd_free_string(s.version);
            crate::sd_free_string(s.source_description);
            crate::sd_free_string(s.video_encoders_json);
            crate::sd_free_string(s.audio_encoders_json);
        }
    }
}
