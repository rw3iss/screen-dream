//! Capture FFI functions (source enumeration, frame capture, screenshots).

use std::ffi::c_char;
use std::ptr;

use crate::core;
use crate::types::{
    from_c_str, to_c_string, SDAvailableSources, SDCaptureSource, SDError, SDFrame,
    SDMonitorInfo, SDWindowInfo,
};

/// Enumerate all available capture sources (monitors and windows).
///
/// On success returns a heap-allocated `SDAvailableSources`. The caller
/// must free it with `sd_free_available_sources`.
///
/// On failure sets `*error` and returns null.
///
/// # Safety
/// * `sd_init` must have been called successfully.
/// * `error` must be a valid pointer to a `*mut SDError` (may point to null).
#[no_mangle]
pub unsafe extern "C" fn sd_enumerate_sources(
    error: *mut *mut SDError,
) -> *mut SDAvailableSources {
    let state = core();

    match state.capture.enumerate_sources() {
        Ok(sources) => {
            // Convert monitors.
            let mut monitors: Vec<SDMonitorInfo> = sources
                .monitors
                .iter()
                .map(SDMonitorInfo::from_domain)
                .collect();
            monitors.shrink_to_fit();

            // Convert windows.
            let mut windows: Vec<SDWindowInfo> = sources
                .windows
                .iter()
                .map(SDWindowInfo::from_domain)
                .collect();
            windows.shrink_to_fit();

            let monitors_count = monitors.len() as u32;
            let windows_count = windows.len() as u32;

            let monitors_ptr = if monitors.is_empty() {
                ptr::null_mut()
            } else {
                let ptr = monitors.as_mut_ptr();
                std::mem::forget(monitors);
                ptr
            };

            let windows_ptr = if windows.is_empty() {
                ptr::null_mut()
            } else {
                let ptr = windows.as_mut_ptr();
                std::mem::forget(windows);
                ptr
            };

            let result = Box::new(SDAvailableSources {
                monitors: monitors_ptr,
                monitors_count,
                windows: windows_ptr,
                windows_count,
                windows_unavailable: sources.windows_unavailable,
                windows_unavailable_reason: sources
                    .windows_unavailable_reason
                    .as_deref()
                    .map(to_c_string)
                    .unwrap_or(ptr::null_mut()),
            });

            Box::into_raw(result)
        }
        Err(e) => {
            if !error.is_null() {
                unsafe { *error = SDError::from_app_error(e) };
            }
            ptr::null_mut()
        }
    }
}

/// Free an `SDAvailableSources` returned by `sd_enumerate_sources`.
///
/// # Safety
/// `sources` must have been returned by `sd_enumerate_sources`, or be null.
#[no_mangle]
pub unsafe extern "C" fn sd_free_available_sources(sources: *mut SDAvailableSources) {
    if sources.is_null() {
        return;
    }
    let s = unsafe { Box::from_raw(sources) };

    // Free monitor strings and the monitors array.
    if !s.monitors.is_null() && s.monitors_count > 0 {
        let monitors = unsafe {
            Vec::from_raw_parts(s.monitors, s.monitors_count as usize, s.monitors_count as usize)
        };
        for m in &monitors {
            unsafe {
                crate::sd_free_string(m.name);
                crate::sd_free_string(m.friendly_name);
            }
        }
    }

    // Free window strings and the windows array.
    if !s.windows.is_null() && s.windows_count > 0 {
        let windows = unsafe {
            Vec::from_raw_parts(s.windows, s.windows_count as usize, s.windows_count as usize)
        };
        for w in &windows {
            unsafe {
                crate::sd_free_string(w.app_name);
                crate::sd_free_string(w.title);
                crate::sd_free_string(w.uuid);
            }
        }
    }

    // Free the reason string.
    unsafe {
        crate::sd_free_string(s.windows_unavailable_reason);
    }
}

/// Capture a single frame from the given source.
///
/// On success returns a heap-allocated `SDFrame` containing raw RGBA pixel data.
/// The caller must free it with `sd_free_frame`.
///
/// On failure sets `*error` and returns null.
///
/// # Safety
/// * `source` must be a valid pointer to an `SDCaptureSource`.
/// * `error` must be a valid pointer to a `*mut SDError` (may point to null).
#[no_mangle]
pub unsafe extern "C" fn sd_capture_frame(
    source: *const SDCaptureSource,
    error: *mut *mut SDError,
) -> *mut SDFrame {
    if source.is_null() {
        if !error.is_null() {
            unsafe {
                *error = SDError::from_app_error(domain::error::AppError::Capture(
                    "source parameter is null".to_string(),
                ));
            }
        }
        return ptr::null_mut();
    }

    let sd_source = unsafe { &*source };
    let domain_source = match sd_source.to_domain() {
        Ok(s) => s,
        Err(e) => {
            if !error.is_null() {
                unsafe { *error = SDError::from_app_error(e) };
            }
            return ptr::null_mut();
        }
    };

    let state = core();
    match state.capture.capture_frame(&domain_source) {
        Ok(frame) => {
            let data_len = frame.data.len() as u32;
            let width = frame.width;
            let height = frame.height;

            // Move the Vec data into a heap allocation we control.
            let mut data = frame.data.into_boxed_slice();
            let data_ptr = data.as_mut_ptr();
            std::mem::forget(data);

            let result = Box::new(SDFrame {
                data: data_ptr,
                data_len,
                width,
                height,
            });
            Box::into_raw(result)
        }
        Err(e) => {
            if !error.is_null() {
                unsafe { *error = SDError::from_app_error(e) };
            }
            ptr::null_mut()
        }
    }
}

/// Free an `SDFrame` returned by `sd_capture_frame`.
///
/// # Safety
/// `frame` must have been returned by `sd_capture_frame`, or be null.
#[no_mangle]
pub unsafe extern "C" fn sd_free_frame(frame: *mut SDFrame) {
    if frame.is_null() {
        return;
    }
    let f = unsafe { Box::from_raw(frame) };
    if !f.data.is_null() && f.data_len > 0 {
        // Reconstruct the boxed slice and drop it.
        unsafe {
            let _ = Box::from_raw(std::slice::from_raw_parts_mut(f.data, f.data_len as usize));
        }
    }
}

/// Capture a screenshot and save it to a file.
///
/// The output format is determined by the file extension (`.png`, `.jpg`, `.webp`).
///
/// On success returns `true`. On failure sets `*error` and returns `false`.
///
/// # Safety
/// * `source` must be a valid pointer to an `SDCaptureSource`.
/// * `output_path` must be a valid NUL-terminated UTF-8 string.
/// * `error` must be a valid pointer to a `*mut SDError` (may point to null).
#[no_mangle]
pub unsafe extern "C" fn sd_take_screenshot(
    source: *const SDCaptureSource,
    output_path: *const c_char,
    error: *mut *mut SDError,
) -> bool {
    if source.is_null() {
        if !error.is_null() {
            unsafe {
                *error = SDError::from_app_error(domain::error::AppError::Capture(
                    "source parameter is null".to_string(),
                ));
            }
        }
        return false;
    }
    if output_path.is_null() {
        if !error.is_null() {
            unsafe {
                *error = SDError::from_app_error(domain::error::AppError::Capture(
                    "output_path parameter is null".to_string(),
                ));
            }
        }
        return false;
    }

    let sd_source = unsafe { &*source };
    let domain_source = match sd_source.to_domain() {
        Ok(s) => s,
        Err(e) => {
            if !error.is_null() {
                unsafe { *error = SDError::from_app_error(e) };
            }
            return false;
        }
    };

    let path_str = unsafe { from_c_str(output_path) };
    let path = std::path::Path::new(&path_str);

    let state = core();

    // Use Spectacle for native-resolution screenshots on KDE.
    // Falls back to PipeWire-based capture on other compositors.
    tracing::info!("sd_take_screenshot: output_path={}", path.display());
    let result = if let Some(ref kwin) = state.kwin_capture {
        tracing::info!("sd_take_screenshot: using Spectacle path");
        kwin.capture_screenshot_spectacle(&domain_source)
            .and_then(|frame| {
                tracing::info!("Spectacle captured frame {}x{}, saving to '{}'", frame.width, frame.height, path.display());
                let fmt = infrastructure::capture::screenshot::ScreenshotFormat::from_extension(path)?;
                infrastructure::capture::screenshot::save_frame_to_file(&frame, path, fmt)?;
                Ok(path.to_path_buf())
            })
    } else {
        tracing::info!("sd_take_screenshot: no KWin backend, using PipeWire fallback");
        infrastructure::capture::capture_screenshot(
            state.capture.as_ref(),
            &domain_source,
            path,
        )
    };

    match result {
        Ok(ref p) => {
            tracing::info!("Screenshot saved successfully: {}", p.display());
            true
        }
        Err(e) => {
            tracing::error!("Screenshot failed: {e}");
            if !error.is_null() {
                unsafe { *error = SDError::from_app_error(e) };
            }
            false
        }
    }
}

/// Capture a screenshot and return it as a base64-encoded PNG string.
///
/// On success returns a heap-allocated NUL-terminated string.
/// The caller must free it with `sd_free_string`.
///
/// On failure sets `*error` and returns null.
///
/// # Safety
/// * `source` must be a valid pointer to an `SDCaptureSource`.
/// * `error` must be a valid pointer to a `*mut SDError` (may point to null).
#[no_mangle]
pub unsafe extern "C" fn sd_take_screenshot_base64(
    source: *const SDCaptureSource,
    error: *mut *mut SDError,
) -> *mut c_char {
    if source.is_null() {
        if !error.is_null() {
            unsafe {
                *error = SDError::from_app_error(domain::error::AppError::Capture(
                    "source parameter is null".to_string(),
                ));
            }
        }
        return ptr::null_mut();
    }

    let sd_source = unsafe { &*source };
    let domain_source = match sd_source.to_domain() {
        Ok(s) => s,
        Err(e) => {
            if !error.is_null() {
                unsafe { *error = SDError::from_app_error(e) };
            }
            return ptr::null_mut();
        }
    };

    let state = core();
    match infrastructure::capture::capture_screenshot_as_base64_png(
        state.capture.as_ref(),
        &domain_source,
    ) {
        Ok(base64) => to_c_string(&base64),
        Err(e) => {
            if !error.is_null() {
                unsafe { *error = SDError::from_app_error(e) };
            }
            ptr::null_mut()
        }
    }
}
