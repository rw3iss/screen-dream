//! C-compatible FFI types for Screen Dream.
//!
//! Every struct here is `#[repr(C)]` so it can be used directly from C/C++/Node.js.
//! Strings are represented as `*mut c_char` (owned, NUL-terminated).
//! Callers must free them with `sd_free_string`.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;

use domain::capture::{
    CaptureSource, MonitorInfo, RegionSource, ScreenSource, WindowInfo, WindowSource,
};
use domain::error::AppError;

// ---------------------------------------------------------------------------
// String helpers
// ---------------------------------------------------------------------------

/// Convert a Rust `&str` to an owned `*mut c_char`.
/// Returns `ptr::null_mut()` if the string contains interior NUL bytes.
pub fn to_c_string(s: &str) -> *mut c_char {
    match CString::new(s) {
        Ok(cs) => cs.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

/// Convert a `*const c_char` to an owned Rust `String`.
/// Returns an empty string if the pointer is null.
///
/// # Safety
/// The pointer must be a valid NUL-terminated C string or null.
pub unsafe fn from_c_str(s: *const c_char) -> String {
    if s.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned()
    }
}

// ---------------------------------------------------------------------------
// Error handling
// ---------------------------------------------------------------------------

/// FFI error type.  Caller must free with `sd_free_error`.
#[repr(C)]
pub struct SDError {
    pub kind: *mut c_char,
    pub message: *mut c_char,
}

impl SDError {
    /// Convert an `AppError` into a heap-allocated `*mut SDError`.
    pub fn from_app_error(e: AppError) -> *mut SDError {
        let (kind, message) = match &e {
            AppError::FfmpegNotFound(m) => ("FfmpegNotFound", m.as_str()),
            AppError::FfmpegExecution(m) => ("FfmpegExecution", m.as_str()),
            AppError::CodecUnavailable(m) => ("CodecUnavailable", m.as_str()),
            AppError::Settings(m) => ("Settings", m.as_str()),
            AppError::Platform(m) => ("Platform", m.as_str()),
            AppError::Io(m) => ("Io", m.as_str()),
            AppError::Capture(m) => ("Capture", m.as_str()),
            AppError::Encoding(m) => ("Encoding", m.as_str()),
        };
        let boxed = Box::new(SDError {
            kind: to_c_string(kind),
            message: to_c_string(message),
        });
        Box::into_raw(boxed)
    }
}

// ---------------------------------------------------------------------------
// Platform
// ---------------------------------------------------------------------------

#[repr(C)]
pub struct SDPlatformInfo {
    pub os: *mut c_char,
    pub display_server: *mut c_char,
    pub arch: *mut c_char,
}

// ---------------------------------------------------------------------------
// Monitors / Windows
// ---------------------------------------------------------------------------

#[repr(C)]
pub struct SDMonitorInfo {
    pub id: u32,
    pub name: *mut c_char,
    pub friendly_name: *mut c_char,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub scale_factor: f32,
    pub is_primary: bool,
}

impl SDMonitorInfo {
    pub fn from_domain(m: &MonitorInfo) -> Self {
        SDMonitorInfo {
            id: m.id,
            name: to_c_string(&m.name),
            friendly_name: to_c_string(&m.friendly_name),
            width: m.width,
            height: m.height,
            x: m.x,
            y: m.y,
            scale_factor: m.scale_factor,
            is_primary: m.is_primary,
        }
    }
}

#[repr(C)]
pub struct SDWindowInfo {
    pub id: u32,
    pub pid: u32,
    pub app_name: *mut c_char,
    pub title: *mut c_char,
    pub width: u32,
    pub height: u32,
    pub is_minimized: bool,
    pub is_focused: bool,
}

impl SDWindowInfo {
    pub fn from_domain(w: &WindowInfo) -> Self {
        SDWindowInfo {
            id: w.id,
            pid: w.pid,
            app_name: to_c_string(&w.app_name),
            title: to_c_string(&w.title),
            width: w.width,
            height: w.height,
            is_minimized: w.is_minimized,
            is_focused: w.is_focused,
        }
    }
}

#[repr(C)]
pub struct SDAvailableSources {
    pub monitors: *mut SDMonitorInfo,
    pub monitors_count: u32,
    pub windows: *mut SDWindowInfo,
    pub windows_count: u32,
    pub windows_unavailable: bool,
    pub windows_unavailable_reason: *mut c_char,
}

// ---------------------------------------------------------------------------
// Capture
// ---------------------------------------------------------------------------

/// C-compatible capture source descriptor.
///
/// `source_type`: 0 = Screen, 1 = Window, 2 = Region.
///
/// For Screen: only `monitor_id` is used.
/// For Window: only `window_id` is used.
/// For Region: `monitor_id`, `region_x`, `region_y`, `region_width`, `region_height` are used.
#[repr(C)]
pub struct SDCaptureSource {
    pub source_type: u32,
    pub monitor_id: u32,
    pub window_id: u32,
    pub region_x: i32,
    pub region_y: i32,
    pub region_width: u32,
    pub region_height: u32,
}

impl SDCaptureSource {
    /// Convert this C struct to the domain `CaptureSource` enum.
    pub fn to_domain(&self) -> Result<CaptureSource, AppError> {
        match self.source_type {
            0 => Ok(CaptureSource::Screen(ScreenSource {
                monitor_id: self.monitor_id,
            })),
            1 => Ok(CaptureSource::Window(WindowSource {
                window_id: self.window_id,
            })),
            2 => Ok(CaptureSource::Region(RegionSource {
                monitor_id: self.monitor_id,
                x: self.region_x,
                y: self.region_y,
                width: self.region_width,
                height: self.region_height,
            })),
            other => Err(AppError::Capture(format!(
                "Invalid source_type: {other} (expected 0=Screen, 1=Window, 2=Region)"
            ))),
        }
    }
}

/// Raw captured frame data.
#[repr(C)]
pub struct SDFrame {
    pub data: *mut u8,
    pub data_len: u32,
    pub width: u32,
    pub height: u32,
}

// ---------------------------------------------------------------------------
// Recording
// ---------------------------------------------------------------------------

/// Configuration for starting a recording session.
/// String fields are borrowed (`*const c_char`) -- the caller retains ownership.
#[repr(C)]
pub struct SDRecordingConfig {
    pub source: SDCaptureSource,
    pub fps: u32,
    pub video_codec: *const c_char,
    pub crf: u8,
    pub preset: *const c_char,
    pub output_path: *const c_char,
    pub capture_microphone: bool,
    pub microphone_device: *const c_char,
}

/// Recording status update.
///
/// `state`: 0=Idle, 1=Starting, 2=Recording, 3=Paused, 4=Stopping, 5=Completed, 6=Failed.
#[repr(C)]
pub struct SDRecordingStatus {
    pub state: u32,
    pub elapsed_seconds: f64,
    pub frames_captured: u64,
}

// ---------------------------------------------------------------------------
// FFmpeg
// ---------------------------------------------------------------------------

/// FFmpeg availability and capabilities.
/// The `video_encoders_json` and `audio_encoders_json` fields contain JSON arrays
/// of encoder name strings for easy consumption in higher-level languages.
#[repr(C)]
pub struct SDFfmpegStatus {
    pub available: bool,
    pub version: *mut c_char,
    pub source_description: *mut c_char,
    pub video_encoders_json: *mut c_char,
    pub audio_encoders_json: *mut c_char,
}

// ---------------------------------------------------------------------------
// Opaque handles
// ---------------------------------------------------------------------------

/// Opaque handle for an in-progress recording session.
/// The internal fields are not exposed across the FFI boundary.
/// The `inner` pointer is managed by the recording module.
pub struct SDRecordingHandle {
    pub(crate) inner: *mut std::ffi::c_void,
}

// ---------------------------------------------------------------------------
// Free functions (exported as `extern "C"`)
// ---------------------------------------------------------------------------

/// Free a string allocated by the Rust FFI layer.
///
/// # Safety
/// `s` must have been returned by a Screen Dream FFI function, or be null.
#[no_mangle]
pub unsafe extern "C" fn sd_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            drop(CString::from_raw(s));
        }
    }
}

/// Free an `SDError` allocated by the Rust FFI layer.
///
/// # Safety
/// `e` must have been returned by a Screen Dream FFI function, or be null.
#[no_mangle]
pub unsafe extern "C" fn sd_free_error(e: *mut SDError) {
    if !e.is_null() {
        let err = unsafe { Box::from_raw(e) };
        unsafe {
            sd_free_string(err.kind);
            sd_free_string(err.message);
        }
    }
}
