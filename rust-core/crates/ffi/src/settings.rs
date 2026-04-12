//! Settings FFI functions.
//!
//! Settings cross the FFI boundary as JSON strings to avoid having to
//! define `#[repr(C)]` structs for the entire settings tree.

use std::ffi::c_char;

use crate::core;
use crate::types::{from_c_str, to_c_string, SDError};

/// Load application settings and return them as a JSON string.
///
/// On success returns a heap-allocated NUL-terminated JSON string.
/// The caller must free it with `sd_free_string`.
///
/// On failure sets `*error` and returns null.
///
/// # Safety
/// * `sd_init` must have been called successfully.
/// * `error` must be a valid pointer to a `*mut SDError` (may point to null).
#[no_mangle]
pub unsafe extern "C" fn sd_load_settings(
    error: *mut *mut SDError,
) -> *mut c_char {
    let state = core();

    match state.settings.load() {
        Ok(settings) => {
            match serde_json::to_string(&settings) {
                Ok(json) => to_c_string(&json),
                Err(e) => {
                    if !error.is_null() {
                        unsafe {
                            *error = SDError::from_app_error(
                                domain::error::AppError::Settings(format!(
                                    "Failed to serialize settings: {e}"
                                )),
                            );
                        }
                    }
                    std::ptr::null_mut()
                }
            }
        }
        Err(e) => {
            if !error.is_null() {
                unsafe { *error = SDError::from_app_error(e) };
            }
            std::ptr::null_mut()
        }
    }
}

/// Save application settings from a JSON string.
///
/// On success returns `true`. On failure sets `*error` and returns `false`.
///
/// # Safety
/// * `json` must be a valid NUL-terminated UTF-8 string.
/// * `error` must be a valid pointer to a `*mut SDError` (may point to null).
#[no_mangle]
pub unsafe extern "C" fn sd_save_settings(
    json: *const c_char,
    error: *mut *mut SDError,
) -> bool {
    if json.is_null() {
        if !error.is_null() {
            unsafe {
                *error = SDError::from_app_error(domain::error::AppError::Settings(
                    "json parameter is null".to_string(),
                ));
            }
        }
        return false;
    }

    let json_str = unsafe { from_c_str(json) };

    let settings: domain::settings::AppSettings = match serde_json::from_str(&json_str) {
        Ok(s) => s,
        Err(e) => {
            if !error.is_null() {
                unsafe {
                    *error = SDError::from_app_error(domain::error::AppError::Settings(
                        format!("Failed to parse settings JSON: {e}"),
                    ));
                }
            }
            return false;
        }
    };

    let state = core();
    match state.settings.save(&settings) {
        Ok(()) => true,
        Err(e) => {
            if !error.is_null() {
                unsafe { *error = SDError::from_app_error(e) };
            }
            false
        }
    }
}

/// Reset settings to defaults and return the new defaults as a JSON string.
///
/// On success returns a heap-allocated NUL-terminated JSON string.
/// The caller must free it with `sd_free_string`.
///
/// On failure sets `*error` and returns null.
///
/// # Safety
/// * `sd_init` must have been called successfully.
/// * `error` must be a valid pointer to a `*mut SDError` (may point to null).
#[no_mangle]
pub unsafe extern "C" fn sd_reset_settings(
    error: *mut *mut SDError,
) -> *mut c_char {
    let state = core();

    match state.settings.reset() {
        Ok(settings) => {
            match serde_json::to_string(&settings) {
                Ok(json) => to_c_string(&json),
                Err(e) => {
                    if !error.is_null() {
                        unsafe {
                            *error = SDError::from_app_error(
                                domain::error::AppError::Settings(format!(
                                    "Failed to serialize settings: {e}"
                                )),
                            );
                        }
                    }
                    std::ptr::null_mut()
                }
            }
        }
        Err(e) => {
            if !error.is_null() {
                unsafe { *error = SDError::from_app_error(e) };
            }
            std::ptr::null_mut()
        }
    }
}
