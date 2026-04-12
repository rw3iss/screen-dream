# Plan 5a: Rust FFI Layer -- Project Restructure & C Bindings

> **License:** GPLv3

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**App Name:** Screen Dream

**Goal:** Migrate the Rust backend from a Tauri-coupled workspace to a standalone `rust-core/` workspace, then build a C FFI crate (`screen_dream_ffi`) that exposes every backend capability to C/C++ callers. This enables the Qt6/C++ frontend (Plan 5b) to consume the Rust domain and infrastructure through a clean, stable C ABI.

**Architecture:** The existing two-crate architecture (domain + infrastructure) is preserved and moved. A new `ffi` crate sits on top, translating Rust types into `#[repr(C)]` structs, managing a global `CoreState` via `OnceLock`, and exposing `extern "C"` functions. Complex types (settings) cross the boundary as JSON strings. All heap-allocated returns include matching `sd_free_*` functions. Errors follow an out-parameter pattern (`*mut *mut SDError`).

**Tech Stack:** Rust 2021, cbindgen, C11 (test harness), CMake (test linking)

**Related documents:**
- `docs/plans/01-core-platform-infrastructure.md` -- original Tauri architecture
- `docs/plans/02-screen-capture-recording.md` -- capture/recording domain
- `docs/plans/05b-migration-qt-frontend.md` -- Qt6/C++ frontend (depends on this plan)

---

## Phase 1 -- Restructure Project

### Target File Structure

```
screen-recorder/
├── rust-core/
│   ├── Cargo.toml              # Workspace root
│   ├── crates/
│   │   ├── domain/             # MOVED from src-tauri/crates/domain/ (unchanged)
│   │   │   ├── Cargo.toml
│   │   │   └── src/
│   │   │       ├── lib.rs
│   │   │       ├── app_config.rs
│   │   │       ├── error.rs
│   │   │       ├── capture/
│   │   │       │   ├── mod.rs
│   │   │       │   ├── backend.rs
│   │   │       │   ├── recording.rs
│   │   │       │   └── source.rs
│   │   │       ├── ffmpeg/
│   │   │       │   ├── mod.rs
│   │   │       │   ├── codec.rs
│   │   │       │   ├── command.rs
│   │   │       │   └── provider.rs
│   │   │       ├── platform/
│   │   │       │   ├── mod.rs
│   │   │       │   └── detect.rs
│   │   │       └── settings/
│   │   │           ├── mod.rs
│   │   │           ├── model.rs
│   │   │           └── repository.rs
│   │   │
│   │   ├── infrastructure/     # MOVED from src-tauri/crates/infrastructure/ (unchanged)
│   │   │   ├── Cargo.toml
│   │   │   └── src/
│   │   │       ├── lib.rs
│   │   │       ├── capture/
│   │   │       │   ├── mod.rs
│   │   │       │   ├── audio_capture.rs
│   │   │       │   ├── recording_pipeline.rs
│   │   │       │   ├── screenshot.rs
│   │   │       │   └── xcap_backend.rs
│   │   │       ├── ffmpeg/
│   │   │       │   ├── mod.rs
│   │   │       │   ├── probe.rs
│   │   │       │   ├── process.rs
│   │   │       │   └── resolver.rs
│   │   │       └── settings/
│   │   │           ├── mod.rs
│   │   │           └── json_repository.rs
│   │   │
│   │   └── ffi/                # NEW -- C FFI bindings (Phase 2)
│   │       ├── Cargo.toml
│   │       ├── cbindgen.toml
│   │       └── src/
│   │           ├── lib.rs
│   │           ├── types.rs
│   │           ├── capture.rs
│   │           ├── recording.rs
│   │           ├── ffmpeg.rs
│   │           ├── settings.rs
│   │           └── platform.rs
│
├── docs/                       # KEPT
├── PLAN.md                     # KEPT
├── README.md                   # KEPT
├── Development.md              # KEPT
└── LICENSE                     # KEPT (if present)
```

**Removed directories:** `src-tauri/`, `src/`, `dist/`, `public/`, `node_modules/`, and all Node/Vite/Tauri config files (`package.json`, `pnpm-lock.yaml`, `tsconfig*.json`, `vite.config.ts`, `index.html`).

---

## Task 1: Restructure project -- move Rust crates

### Steps

- [ ] **1.1** Create the `rust-core/` directory and workspace `Cargo.toml`
- [ ] **1.2** Move `src-tauri/crates/domain/` to `rust-core/crates/domain/`
- [ ] **1.3** Move `src-tauri/crates/infrastructure/` to `rust-core/crates/infrastructure/`
- [ ] **1.4** Remove `src-tauri/` directory entirely
- [ ] **1.5** Remove `src/` directory (Preact frontend)
- [ ] **1.6** Remove Node/Vite/Tauri config files and build artifacts
- [ ] **1.7** Verify the workspace builds and tests pass

### File: `rust-core/Cargo.toml`

```toml
[workspace]
members = [
    "crates/domain",
    "crates/infrastructure",
]
resolver = "2"

[workspace.package]
version = "0.2.0"
edition = "2021"
license = "GPL-3.0"
authors = ["Screen Dream Contributors"]
```

### Commands

```bash
# 1.1 Create workspace
mkdir -p rust-core/crates

# 1.2-1.3 Move crates
cp -r src-tauri/crates/domain rust-core/crates/domain
cp -r src-tauri/crates/infrastructure rust-core/crates/infrastructure

# 1.4-1.6 Remove old directories and config files
rm -rf src-tauri/
rm -rf src/
rm -rf dist/
rm -rf public/
rm -rf node_modules/
rm -f package.json pnpm-lock.yaml tsconfig.json tsconfig.node.json vite.config.ts index.html

# 1.7 Verify
cd rust-core && cargo check --workspace
cd rust-core && cargo test --workspace
```

### Commit message

```
refactor: move Rust crates to rust-core/ workspace, remove Tauri/Preact

Migrate from Tauri-coupled layout to standalone Rust workspace.
Domain and infrastructure crates are moved unchanged.
Tauri app crate, Preact frontend, and Node tooling are removed.
```

---

## Phase 2 -- FFI Crate

## Task 2: Create FFI crate scaffold

### Steps

- [ ] **2.1** Create `rust-core/crates/ffi/Cargo.toml`
- [ ] **2.2** Create `rust-core/crates/ffi/cbindgen.toml`
- [ ] **2.3** Create `rust-core/crates/ffi/src/lib.rs` with `sd_init()` / `sd_shutdown()`
- [ ] **2.4** Add `"crates/ffi"` to the workspace members in `rust-core/Cargo.toml`
- [ ] **2.5** Verify: `cd rust-core && cargo build -p screen_dream_ffi`

### File: `rust-core/Cargo.toml` (updated)

```toml
[workspace]
members = [
    "crates/domain",
    "crates/infrastructure",
    "crates/ffi",
]
resolver = "2"

[workspace.package]
version = "0.2.0"
edition = "2021"
license = "GPL-3.0"
authors = ["Screen Dream Contributors"]
```

### File: `rust-core/crates/ffi/Cargo.toml`

```toml
[package]
name = "screen_dream_ffi"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "C FFI bindings for the Screen Dream Rust core"

[lib]
crate-type = ["cdylib"]

[dependencies]
domain = { path = "../domain" }
infrastructure = { path = "../infrastructure" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[build-dependencies]
cbindgen = "0.27"
```

### File: `rust-core/crates/ffi/cbindgen.toml`

```toml
language = "C"
header = """
/* screen_dream_ffi.h -- Auto-generated by cbindgen. DO NOT EDIT. */
/* License: GPL-3.0 */
"""

include_guard = "SCREEN_DREAM_FFI_H"
no_includes = false
sys_includes = ["stdint.h", "stdbool.h", "stddef.h"]

[export]
prefix = "SD"
include = []
exclude = []

[export.rename]
# Keep Rust naming as-is in C header

[fn]
rename_args = "None"

[enum]
rename_variants = "QualifiedScreamingSnakeCase"

[struct]
rename_fields = "None"
```

### File: `rust-core/crates/ffi/src/lib.rs`

```rust
//! # Screen Dream FFI
//!
//! C-compatible foreign function interface for the Screen Dream Rust core.
//! All public symbols are prefixed with `sd_` and use C-compatible types.
//!
//! ## Lifecycle
//! 1. Call `sd_init()` once at application startup.
//! 2. Use `sd_*` functions to interact with the core.
//! 3. Call `sd_shutdown()` once at application exit.
//!
//! ## Error handling
//! Functions that can fail accept a `*mut *mut SDError` out-parameter.
//! On success the pointer is left as-is (NULL). On failure an SDError is
//! allocated and written to the pointer. The caller must free it with
//! `sd_free_error()`.
//!
//! ## Memory ownership
//! Every `sd_*` function that returns a heap pointer has a corresponding
//! `sd_free_*` function. The caller MUST call the free function and MUST
//! NOT use libc `free()` directly.

pub mod types;
pub mod platform;
pub mod ffmpeg;
pub mod settings;
pub mod capture;
pub mod recording;

use std::ffi::CString;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use domain::ffmpeg::FfmpegProvider;
use domain::platform::PlatformInfo;
use domain::settings::SettingsRepository;
use infrastructure::capture::XcapCaptureBackend;
use infrastructure::ffmpeg::FfmpegResolver;
use infrastructure::settings::JsonSettingsRepository;

use types::SDError;

// ---------------------------------------------------------------------------
// Global core state
// ---------------------------------------------------------------------------

/// Holds all initialized backend services. Created by `sd_init()`,
/// accessed by every FFI function, destroyed by `sd_shutdown()`.
pub(crate) struct CoreState {
    pub platform: PlatformInfo,
    pub ffmpeg: Arc<FfmpegResolver>,
    pub settings: Arc<JsonSettingsRepository>,
    pub capture: Arc<XcapCaptureBackend>,
}

/// The single global instance. Populated by `sd_init()`.
static CORE: OnceLock<CoreState> = OnceLock::new();

/// Internal helper -- returns the core state or writes an error.
///
/// # Safety
/// The `error` pointer must be valid or null.
pub(crate) unsafe fn get_core(error: *mut *mut SDError) -> Option<&'static CoreState> {
    match CORE.get() {
        Some(state) => Some(state),
        None => {
            SDError::write(error, "sd_init() has not been called");
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Public API: lifecycle
// ---------------------------------------------------------------------------

/// Initialize the Screen Dream core.
///
/// Must be called exactly once before any other `sd_*` function.
/// Returns `true` on success, `false` on failure (error written to `error`).
///
/// # Safety
/// - `error` must be a valid pointer to a `*mut SDError` or NULL.
#[no_mangle]
pub unsafe extern "C" fn sd_init(error: *mut *mut SDError) -> bool {
    // Initialize tracing (logs to stderr, controlled by RUST_LOG env var).
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    tracing::info!("Screen Dream FFI: initializing core");

    // Detect platform.
    let platform = PlatformInfo::detect();
    tracing::info!(
        "Platform: {:?} / {:?} / {}",
        platform.os,
        platform.display_server,
        platform.arch
    );

    // Resolve config directory.
    let config_dir = match dirs_config_dir() {
        Some(dir) => dir,
        None => {
            SDError::write(error, "Failed to determine config directory");
            return false;
        }
    };

    // Create services.
    let ffmpeg = Arc::new(FfmpegResolver::new(None, None));
    let settings = Arc::new(JsonSettingsRepository::new(config_dir));
    let capture = Arc::new(XcapCaptureBackend::new(platform.clone()));

    let state = CoreState {
        platform,
        ffmpeg,
        settings,
        capture,
    };

    match CORE.set(state) {
        Ok(()) => {
            tracing::info!("Screen Dream FFI: core initialized successfully");
            true
        }
        Err(_) => {
            SDError::write(error, "sd_init() has already been called");
            false
        }
    }
}

/// Shut down the Screen Dream core and release resources.
///
/// After calling this function, no other `sd_*` functions may be called.
/// Currently a no-op for resource cleanup (OnceLock lives for the process
/// lifetime), but signals intent and allows future cleanup hooks.
///
/// # Safety
/// No special requirements.
#[no_mangle]
pub unsafe extern "C" fn sd_shutdown() {
    tracing::info!("Screen Dream FFI: shutdown requested");
    // OnceLock does not support reset, so we cannot truly drop the state.
    // In practice the process is exiting. Future versions may use a Mutex
    // if hot-reload is needed.
}

/// Return the library version as a C string. The caller must free with `sd_free_string()`.
///
/// # Safety
/// The returned pointer must be freed with `sd_free_string()`.
#[no_mangle]
pub unsafe extern "C" fn sd_version() -> *mut std::os::raw::c_char {
    let version = CString::new(env!("CARGO_PKG_VERSION")).unwrap_or_default();
    version.into_raw()
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Resolve the application config directory.
fn dirs_config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("screen-dream"))
}
```

### Commands

```bash
# Add ffi to workspace (edit Cargo.toml members list)
cd rust-core && cargo build -p screen_dream_ffi
```

### Commit message

```
feat(ffi): add FFI crate scaffold with sd_init/sd_shutdown lifecycle

Introduces the screen_dream_ffi crate (cdylib) with global CoreState
managed via OnceLock, tracing initialization, and cbindgen config
for auto-generating the C header.
```

---

## Task 3: FFI types module

### Steps

- [ ] **3.1** Create `rust-core/crates/ffi/src/types.rs` with all `#[repr(C)]` types
- [ ] **3.2** Verify: `cd rust-core && cargo build -p screen_dream_ffi`

### File: `rust-core/crates/ffi/src/types.rs`

```rust
//! C-compatible types for the Screen Dream FFI boundary.
//!
//! Every struct is `#[repr(C)]` so cbindgen can generate correct C headers.
//! Strings are passed as `*mut c_char` (owned, NUL-terminated).
//! Arrays are passed as pointer + count.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

// ============================= SDError =====================================

/// Opaque error type returned by FFI functions.
/// Contains a heap-allocated error message string.
#[repr(C)]
pub struct SDError {
    pub message: *mut c_char,
}

impl SDError {
    /// Allocate an SDError on the heap and optionally write it to the out-pointer.
    ///
    /// # Safety
    /// `out` must be a valid `*mut *mut SDError` or null.
    pub(crate) unsafe fn write(out: *mut *mut SDError, msg: &str) {
        let c_msg = CString::new(msg).unwrap_or_else(|_| CString::new("unknown error").unwrap());
        let err = Box::new(SDError {
            message: c_msg.into_raw(),
        });
        if !out.is_null() {
            *out = Box::into_raw(err);
        }
    }

    /// Convenience: write an SDError from an AppError.
    pub(crate) unsafe fn write_app_error(
        out: *mut *mut SDError,
        err: &domain::error::AppError,
    ) {
        SDError::write(out, &err.to_string());
    }
}

/// Free an SDError allocated by any `sd_*` function.
///
/// # Safety
/// - `error` must have been returned by an `sd_*` function or be NULL.
/// - Must not be called twice on the same pointer.
#[no_mangle]
pub unsafe extern "C" fn sd_free_error(error: *mut SDError) {
    if error.is_null() {
        return;
    }
    let err = Box::from_raw(error);
    if !err.message.is_null() {
        let _ = CString::from_raw(err.message);
    }
}

// ============================= SDString helpers ============================

/// Free a C string returned by any `sd_*` function.
///
/// # Safety
/// - `s` must have been returned by an `sd_*` function or be NULL.
/// - Must not be called twice on the same pointer.
#[no_mangle]
pub unsafe extern "C" fn sd_free_string(s: *mut c_char) {
    if !s.is_null() {
        let _ = CString::from_raw(s);
    }
}

/// Internal helper: convert a Rust String to an owned `*mut c_char`.
/// Returns null on allocation failure (should not happen in practice).
pub(crate) fn string_to_c(s: &str) -> *mut c_char {
    CString::new(s)
        .unwrap_or_else(|_| CString::new("").unwrap())
        .into_raw()
}

/// Internal helper: convert a C string to a Rust &str.
/// Returns None if the pointer is null or not valid UTF-8.
///
/// # Safety
/// The pointer must be a valid NUL-terminated C string or null.
pub(crate) unsafe fn c_to_str<'a>(s: *const c_char) -> Option<&'a str> {
    if s.is_null() {
        return None;
    }
    CStr::from_ptr(s).to_str().ok()
}

// ============================= SDPlatformInfo ==============================

/// Platform information. All strings are owned and must be freed with
/// `sd_free_platform_info()`.
#[repr(C)]
pub struct SDPlatformInfo {
    /// Operating system: "linux", "macos", or "windows".
    pub os: *mut c_char,
    /// Display server: "x11", "wayland", "quartz", "win32", or "unknown".
    pub display_server: *mut c_char,
    /// CPU architecture: "x86_64", "aarch64", etc.
    pub arch: *mut c_char,
}

/// Free an SDPlatformInfo returned by `sd_get_platform_info()`.
///
/// # Safety
/// - `info` must have been returned by `sd_get_platform_info()` or be NULL.
#[no_mangle]
pub unsafe extern "C" fn sd_free_platform_info(info: *mut SDPlatformInfo) {
    if info.is_null() {
        return;
    }
    let info = Box::from_raw(info);
    sd_free_string(info.os);
    sd_free_string(info.display_server);
    sd_free_string(info.arch);
}

// ============================= SDMonitorInfo ===============================

/// Information about a single monitor.
#[repr(C)]
pub struct SDMonitorInfo {
    pub id: u32,
    /// Owned string. Free with the parent container's free function.
    pub name: *mut c_char,
    pub friendly_name: *mut c_char,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub scale_factor: f32,
    pub is_primary: bool,
}

// ============================= SDWindowInfo ================================

/// Information about a single window.
#[repr(C)]
pub struct SDWindowInfo {
    pub id: u32,
    pub pid: u32,
    /// Owned string.
    pub app_name: *mut c_char,
    pub title: *mut c_char,
    pub width: u32,
    pub height: u32,
    pub is_minimized: bool,
    pub is_focused: bool,
}

// ============================= SDAvailableSources ==========================

/// All available capture sources (monitors + windows).
#[repr(C)]
pub struct SDAvailableSources {
    /// Array of monitors. Length = `monitors_count`.
    pub monitors: *mut SDMonitorInfo,
    pub monitors_count: u32,
    /// Array of windows. Length = `windows_count`.
    pub windows: *mut SDWindowInfo,
    pub windows_count: u32,
    /// True if window enumeration is not available on this platform.
    pub windows_unavailable: bool,
    /// Human-readable reason (owned string, may be NULL).
    pub windows_unavailable_reason: *mut c_char,
}

/// Free an SDAvailableSources returned by `sd_enumerate_sources()`.
///
/// # Safety
/// - `sources` must have been returned by `sd_enumerate_sources()` or be NULL.
#[no_mangle]
pub unsafe extern "C" fn sd_free_available_sources(sources: *mut SDAvailableSources) {
    if sources.is_null() {
        return;
    }
    let sources = Box::from_raw(sources);

    // Free monitors array
    if !sources.monitors.is_null() {
        let monitors = Vec::from_raw_parts(
            sources.monitors,
            sources.monitors_count as usize,
            sources.monitors_count as usize,
        );
        for m in monitors {
            sd_free_string(m.name);
            sd_free_string(m.friendly_name);
        }
    }

    // Free windows array
    if !sources.windows.is_null() {
        let windows = Vec::from_raw_parts(
            sources.windows,
            sources.windows_count as usize,
            sources.windows_count as usize,
        );
        for w in windows {
            sd_free_string(w.app_name);
            sd_free_string(w.title);
        }
    }

    // Free reason string
    sd_free_string(sources.windows_unavailable_reason);
}

// ============================= SDCaptureSource =============================

/// Identifies what to capture. Tagged union with a type discriminant.
///
/// - `type_ == 0`: Screen capture. `id` = monitor ID.
/// - `type_ == 1`: Window capture. `id` = window ID.
/// - `type_ == 2`: Region capture. `id` = monitor ID, plus x/y/width/height.
#[repr(C)]
pub struct SDCaptureSource {
    /// 0 = Screen, 1 = Window, 2 = Region.
    pub type_: u8,
    /// Monitor ID (type 0, 2) or Window ID (type 1).
    pub id: u32,
    /// Region x offset (only used when type_ == 2).
    pub region_x: i32,
    /// Region y offset (only used when type_ == 2).
    pub region_y: i32,
    /// Region width (only used when type_ == 2).
    pub region_width: u32,
    /// Region height (only used when type_ == 2).
    pub region_height: u32,
}

impl SDCaptureSource {
    /// Convert to the domain CaptureSource enum.
    pub(crate) fn to_domain(&self) -> Option<domain::capture::CaptureSource> {
        use domain::capture::{CaptureSource, RegionSource, ScreenSource, WindowSource};
        match self.type_ {
            0 => Some(CaptureSource::Screen(ScreenSource {
                monitor_id: self.id,
            })),
            1 => Some(CaptureSource::Window(WindowSource {
                window_id: self.id,
            })),
            2 => Some(CaptureSource::Region(RegionSource {
                monitor_id: self.id,
                x: self.region_x,
                y: self.region_y,
                width: self.region_width,
                height: self.region_height,
            })),
            _ => None,
        }
    }
}

// ============================= SDFrame =====================================

/// A captured frame (raw RGBA pixel data).
#[repr(C)]
pub struct SDFrame {
    /// Pointer to RGBA pixel data (4 bytes per pixel, row-major).
    pub data: *mut u8,
    /// Length of `data` in bytes.
    pub data_len: u32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Free an SDFrame returned by `sd_capture_frame()`.
///
/// # Safety
/// - `frame` must have been returned by `sd_capture_frame()` or be NULL.
#[no_mangle]
pub unsafe extern "C" fn sd_free_frame(frame: *mut SDFrame) {
    if frame.is_null() {
        return;
    }
    let frame = Box::from_raw(frame);
    if !frame.data.is_null() {
        let _ = Vec::from_raw_parts(frame.data, frame.data_len as usize, frame.data_len as usize);
    }
}

// ============================= SDRecordingConfig ===========================

/// Configuration for starting a recording session.
/// All string fields are borrowed (caller retains ownership).
#[repr(C)]
pub struct SDRecordingConfig {
    /// What to capture.
    pub source: SDCaptureSource,
    /// Target frames per second (e.g. 30, 60).
    pub fps: u32,
    /// FFmpeg video codec string (e.g. "libx264"). Borrowed.
    pub video_codec: *const c_char,
    /// CRF quality value (0-51, lower = better).
    pub crf: u8,
    /// Encoding speed preset (e.g. "fast"). Borrowed.
    pub preset: *const c_char,
    /// Output file path. Borrowed.
    pub output_path: *const c_char,
    /// Whether to capture microphone audio.
    pub capture_microphone: bool,
    /// Microphone device name (NULL = default). Borrowed.
    pub microphone_device: *const c_char,
}

impl SDRecordingConfig {
    /// Convert to the domain RecordingConfig.
    ///
    /// # Safety
    /// All string pointers must be valid NUL-terminated C strings or NULL.
    pub(crate) unsafe fn to_domain(&self) -> Option<domain::capture::RecordingConfig> {
        let source = self.source.to_domain()?;
        let video_codec = c_to_str(self.video_codec)?.to_string();
        let preset = c_to_str(self.preset)?.to_string();
        let output_path = c_to_str(self.output_path)?.to_string();
        let microphone_device = if self.microphone_device.is_null() {
            None
        } else {
            c_to_str(self.microphone_device).map(|s| s.to_string())
        };

        Some(domain::capture::RecordingConfig {
            source,
            fps: self.fps,
            video_codec,
            crf: self.crf,
            preset,
            output_path,
            capture_microphone: self.capture_microphone,
            microphone_device,
        })
    }
}

// ============================= SDRecordingStatus ===========================

/// Current status of a recording session.
#[repr(C)]
pub struct SDRecordingStatus {
    /// 0=Idle, 1=Starting, 2=Recording, 3=Paused, 4=Stopping, 5=Completed, 6=Failed.
    pub state: u8,
    /// Duration in seconds.
    pub elapsed_seconds: f64,
    /// Number of frames captured so far.
    pub frames_captured: u64,
}

// ============================= SDFfmpegStatus ==============================

/// FFmpeg availability and capabilities.
#[repr(C)]
pub struct SDFfmpegStatus {
    /// True if FFmpeg was found and is usable.
    pub available: bool,
    /// Version string (owned, may be NULL if unavailable).
    pub version: *mut c_char,
    /// JSON array of video encoder names (owned string).
    pub video_encoders_json: *mut c_char,
    /// JSON array of audio encoder names (owned string).
    pub audio_encoders_json: *mut c_char,
}

/// Free an SDFfmpegStatus returned by `sd_get_ffmpeg_status()`.
///
/// # Safety
/// - `status` must have been returned by `sd_get_ffmpeg_status()` or be NULL.
#[no_mangle]
pub unsafe extern "C" fn sd_free_ffmpeg_status(status: *mut SDFfmpegStatus) {
    if status.is_null() {
        return;
    }
    let status = Box::from_raw(status);
    sd_free_string(status.version);
    sd_free_string(status.video_encoders_json);
    sd_free_string(status.audio_encoders_json);
}

// ============================= SDRecordingHandle ===========================

/// Opaque handle to a running recording session.
/// The internal state is managed by the FFI layer.
/// Free with `sd_free_recording_handle()` after stopping.
pub struct SDRecordingHandle {
    pub(crate) pipeline: std::sync::Mutex<infrastructure::capture::RecordingPipeline>,
    pub(crate) start_time: std::time::Instant,
    pub(crate) frames_captured: std::sync::atomic::AtomicU64,
}
```

### Commands

```bash
cd rust-core && cargo build -p screen_dream_ffi
```

### Commit message

```
feat(ffi): add #[repr(C)] types module with all FFI boundary types

Defines SDError, SDPlatformInfo, SDMonitorInfo, SDWindowInfo,
SDAvailableSources, SDCaptureSource, SDFrame, SDRecordingConfig,
SDRecordingStatus, SDFfmpegStatus, SDRecordingHandle, and all
corresponding sd_free_* functions.
```

---

## Task 4: FFI platform + FFmpeg functions

### Steps

- [ ] **4.1** Create `rust-core/crates/ffi/src/platform.rs`
- [ ] **4.2** Create `rust-core/crates/ffi/src/ffmpeg.rs`
- [ ] **4.3** Verify: `cd rust-core && cargo build -p screen_dream_ffi`

### File: `rust-core/crates/ffi/src/platform.rs`

```rust
//! Platform detection FFI functions.

use crate::types::{SDError, SDPlatformInfo, string_to_c};
use crate::get_core;

/// Get platform information (OS, display server, architecture).
///
/// Returns a heap-allocated SDPlatformInfo. The caller must free it with
/// `sd_free_platform_info()`.
///
/// # Safety
/// - `error` must be a valid `*mut *mut SDError` or NULL.
#[no_mangle]
pub unsafe extern "C" fn sd_get_platform_info(
    error: *mut *mut SDError,
) -> *mut SDPlatformInfo {
    let core = match get_core(error) {
        Some(c) => c,
        None => return std::ptr::null_mut(),
    };

    let os_str = format!("{:?}", core.platform.os).to_lowercase();
    let ds_str = format!("{:?}", core.platform.display_server).to_lowercase();

    let info = Box::new(SDPlatformInfo {
        os: string_to_c(&os_str),
        display_server: string_to_c(&ds_str),
        arch: string_to_c(&core.platform.arch),
    });

    Box::into_raw(info)
}
```

### File: `rust-core/crates/ffi/src/ffmpeg.rs`

```rust
//! FFmpeg status FFI functions.

use crate::types::{SDError, SDFfmpegStatus, string_to_c};
use crate::get_core;
use domain::ffmpeg::FfmpegProvider;

/// Query FFmpeg availability and capabilities.
///
/// Returns a heap-allocated SDFfmpegStatus. The caller must free it with
/// `sd_free_ffmpeg_status()`.
///
/// On failure, returns NULL and writes to `error`.
///
/// # Safety
/// - `error` must be a valid `*mut *mut SDError` or NULL.
#[no_mangle]
pub unsafe extern "C" fn sd_get_ffmpeg_status(
    error: *mut *mut SDError,
) -> *mut SDFfmpegStatus {
    let core = match get_core(error) {
        Some(c) => c,
        None => return std::ptr::null_mut(),
    };

    // Try to get capabilities. If FFmpeg is not found, return a "not available" status
    // rather than an error, since this is an expected condition.
    match core.ffmpeg.capabilities() {
        Ok(caps) => {
            let video_names: Vec<String> = caps
                .video_encoders
                .iter()
                .map(|c| format!("{:?}", c).to_lowercase())
                .collect();
            let audio_names: Vec<String> = caps
                .audio_encoders
                .iter()
                .map(|c| format!("{:?}", c).to_lowercase())
                .collect();

            let video_json = serde_json::to_string(&video_names).unwrap_or_else(|_| "[]".into());
            let audio_json = serde_json::to_string(&audio_names).unwrap_or_else(|_| "[]".into());

            let status = Box::new(SDFfmpegStatus {
                available: true,
                version: string_to_c(&caps.version),
                video_encoders_json: string_to_c(&video_json),
                audio_encoders_json: string_to_c(&audio_json),
            });
            Box::into_raw(status)
        }
        Err(domain::error::AppError::FfmpegNotFound(_)) => {
            // Not an error -- just not available.
            let status = Box::new(SDFfmpegStatus {
                available: false,
                version: std::ptr::null_mut(),
                video_encoders_json: string_to_c("[]"),
                audio_encoders_json: string_to_c("[]"),
            });
            Box::into_raw(status)
        }
        Err(e) => {
            SDError::write_app_error(error, &e);
            std::ptr::null_mut()
        }
    }
}
```

### Commands

```bash
cd rust-core && cargo build -p screen_dream_ffi
```

### Commit message

```
feat(ffi): add platform and FFmpeg status FFI functions

sd_get_platform_info() returns OS/display-server/arch.
sd_get_ffmpeg_status() probes FFmpeg availability and codec support,
returning a structured status rather than erroring when FFmpeg is
not installed.
```

---

## Task 5: FFI settings functions

### Steps

- [ ] **5.1** Create `rust-core/crates/ffi/src/settings.rs`
- [ ] **5.2** Verify: `cd rust-core && cargo build -p screen_dream_ffi`

### File: `rust-core/crates/ffi/src/settings.rs`

```rust
//! Settings FFI functions.
//!
//! Settings are passed across the FFI boundary as JSON strings to avoid
//! mapping the deeply nested `AppSettings` struct to C. The Qt side
//! deserializes with QJsonDocument.

use std::os::raw::c_char;

use domain::settings::SettingsRepository;

use crate::types::{SDError, c_to_str, string_to_c};
use crate::get_core;

/// Load application settings. Returns a JSON string.
///
/// The caller must free the returned string with `sd_free_string()`.
/// Returns NULL on error.
///
/// # Safety
/// - `error` must be a valid `*mut *mut SDError` or NULL.
#[no_mangle]
pub unsafe extern "C" fn sd_load_settings(
    error: *mut *mut SDError,
) -> *mut c_char {
    let core = match get_core(error) {
        Some(c) => c,
        None => return std::ptr::null_mut(),
    };

    match core.settings.load() {
        Ok(settings) => {
            match serde_json::to_string(&settings) {
                Ok(json) => string_to_c(&json),
                Err(e) => {
                    SDError::write(error, &format!("Failed to serialize settings: {e}"));
                    std::ptr::null_mut()
                }
            }
        }
        Err(e) => {
            SDError::write_app_error(error, &e);
            std::ptr::null_mut()
        }
    }
}

/// Save application settings from a JSON string.
///
/// Returns `true` on success, `false` on failure.
///
/// # Safety
/// - `json` must be a valid NUL-terminated C string.
/// - `error` must be a valid `*mut *mut SDError` or NULL.
#[no_mangle]
pub unsafe extern "C" fn sd_save_settings(
    json: *const c_char,
    error: *mut *mut SDError,
) -> bool {
    let core = match get_core(error) {
        Some(c) => c,
        None => return false,
    };

    let json_str = match c_to_str(json) {
        Some(s) => s,
        None => {
            SDError::write(error, "Invalid or NULL JSON string");
            return false;
        }
    };

    let settings: domain::settings::AppSettings = match serde_json::from_str(json_str) {
        Ok(s) => s,
        Err(e) => {
            SDError::write(error, &format!("Invalid settings JSON: {e}"));
            return false;
        }
    };

    match core.settings.save(&settings) {
        Ok(()) => true,
        Err(e) => {
            SDError::write_app_error(error, &e);
            false
        }
    }
}

/// Reset settings to defaults and save. Returns the default settings as JSON.
///
/// The caller must free the returned string with `sd_free_string()`.
/// Returns NULL on error.
///
/// # Safety
/// - `error` must be a valid `*mut *mut SDError` or NULL.
#[no_mangle]
pub unsafe extern "C" fn sd_reset_settings(
    error: *mut *mut SDError,
) -> *mut c_char {
    let core = match get_core(error) {
        Some(c) => c,
        None => return std::ptr::null_mut(),
    };

    match core.settings.reset() {
        Ok(settings) => {
            match serde_json::to_string(&settings) {
                Ok(json) => string_to_c(&json),
                Err(e) => {
                    SDError::write(error, &format!("Failed to serialize settings: {e}"));
                    std::ptr::null_mut()
                }
            }
        }
        Err(e) => {
            SDError::write_app_error(error, &e);
            std::ptr::null_mut()
        }
    }
}
```

### Commands

```bash
cd rust-core && cargo build -p screen_dream_ffi
```

### Commit message

```
feat(ffi): add settings FFI functions (load/save/reset as JSON)

Settings cross the FFI boundary as JSON strings to avoid mapping
the complex nested AppSettings struct to repr(C). The Qt side
uses QJsonDocument to parse.
```

---

## Task 6: FFI capture + screenshot functions

### Steps

- [ ] **6.1** Create `rust-core/crates/ffi/src/capture.rs`
- [ ] **6.2** Verify: `cd rust-core && cargo build -p screen_dream_ffi`

### File: `rust-core/crates/ffi/src/capture.rs`

```rust
//! Capture and screenshot FFI functions.

use std::os::raw::c_char;
use std::path::Path;

use domain::capture::CaptureBackend;

use crate::types::*;
use crate::get_core;

/// Enumerate all available capture sources (monitors + windows).
///
/// Returns a heap-allocated SDAvailableSources. The caller must free it
/// with `sd_free_available_sources()`.
///
/// # Safety
/// - `error` must be a valid `*mut *mut SDError` or NULL.
#[no_mangle]
pub unsafe extern "C" fn sd_enumerate_sources(
    error: *mut *mut SDError,
) -> *mut SDAvailableSources {
    let core = match get_core(error) {
        Some(c) => c,
        None => return std::ptr::null_mut(),
    };

    let sources = match core.capture.enumerate_sources() {
        Ok(s) => s,
        Err(e) => {
            SDError::write_app_error(error, &e);
            return std::ptr::null_mut();
        }
    };

    // Convert monitors
    let mut c_monitors: Vec<SDMonitorInfo> = sources
        .monitors
        .iter()
        .map(|m| SDMonitorInfo {
            id: m.id,
            name: string_to_c(&m.name),
            friendly_name: string_to_c(&m.friendly_name),
            width: m.width,
            height: m.height,
            x: m.x,
            y: m.y,
            scale_factor: m.scale_factor,
            is_primary: m.is_primary,
        })
        .collect();

    // Convert windows
    let mut c_windows: Vec<SDWindowInfo> = sources
        .windows
        .iter()
        .map(|w| SDWindowInfo {
            id: w.id,
            pid: w.pid,
            app_name: string_to_c(&w.app_name),
            title: string_to_c(&w.title),
            width: w.width,
            height: w.height,
            is_minimized: w.is_minimized,
            is_focused: w.is_focused,
        })
        .collect();

    let monitors_count = c_monitors.len() as u32;
    let windows_count = c_windows.len() as u32;

    let monitors_ptr = if c_monitors.is_empty() {
        std::ptr::null_mut()
    } else {
        let ptr = c_monitors.as_mut_ptr();
        std::mem::forget(c_monitors);
        ptr
    };

    let windows_ptr = if c_windows.is_empty() {
        std::ptr::null_mut()
    } else {
        let ptr = c_windows.as_mut_ptr();
        std::mem::forget(c_windows);
        ptr
    };

    let reason = sources
        .windows_unavailable_reason
        .as_deref()
        .map(string_to_c)
        .unwrap_or(std::ptr::null_mut());

    let result = Box::new(SDAvailableSources {
        monitors: monitors_ptr,
        monitors_count,
        windows: windows_ptr,
        windows_count,
        windows_unavailable: sources.windows_unavailable,
        windows_unavailable_reason: reason,
    });

    Box::into_raw(result)
}

/// Capture a single frame from the given source.
///
/// Returns a heap-allocated SDFrame with raw RGBA pixel data. The caller
/// must free it with `sd_free_frame()`.
///
/// # Safety
/// - `source` must be a valid pointer to an SDCaptureSource.
/// - `error` must be a valid `*mut *mut SDError` or NULL.
#[no_mangle]
pub unsafe extern "C" fn sd_capture_frame(
    source: *const SDCaptureSource,
    error: *mut *mut SDError,
) -> *mut SDFrame {
    let core = match get_core(error) {
        Some(c) => c,
        None => return std::ptr::null_mut(),
    };

    if source.is_null() {
        SDError::write(error, "source is NULL");
        return std::ptr::null_mut();
    }

    let domain_source = match (*source).to_domain() {
        Some(s) => s,
        None => {
            SDError::write(error, "Invalid capture source type");
            return std::ptr::null_mut();
        }
    };

    let frame = match core.capture.capture_frame(&domain_source) {
        Ok(f) => f,
        Err(e) => {
            SDError::write_app_error(error, &e);
            return std::ptr::null_mut();
        }
    };

    let data_len = frame.data.len() as u32;
    let mut data = frame.data.into_boxed_slice();
    let data_ptr = data.as_mut_ptr();
    std::mem::forget(data);

    let result = Box::new(SDFrame {
        data: data_ptr,
        data_len,
        width: frame.width,
        height: frame.height,
    });

    Box::into_raw(result)
}

/// Take a screenshot and save it to a file.
///
/// The output format is determined by the file extension (.png, .jpg, .webp).
/// Returns `true` on success, `false` on failure.
///
/// # Safety
/// - `source` must be a valid pointer to an SDCaptureSource.
/// - `path` must be a valid NUL-terminated C string.
/// - `error` must be a valid `*mut *mut SDError` or NULL.
#[no_mangle]
pub unsafe extern "C" fn sd_take_screenshot(
    source: *const SDCaptureSource,
    path: *const c_char,
    error: *mut *mut SDError,
) -> bool {
    let core = match get_core(error) {
        Some(c) => c,
        None => return false,
    };

    if source.is_null() {
        SDError::write(error, "source is NULL");
        return false;
    }

    let domain_source = match (*source).to_domain() {
        Some(s) => s,
        None => {
            SDError::write(error, "Invalid capture source type");
            return false;
        }
    };

    let path_str = match c_to_str(path) {
        Some(s) => s,
        None => {
            SDError::write(error, "Invalid or NULL path");
            return false;
        }
    };

    match infrastructure::capture::capture_screenshot(
        core.capture.as_ref(),
        &domain_source,
        Path::new(path_str),
    ) {
        Ok(_) => true,
        Err(e) => {
            SDError::write_app_error(error, &e);
            false
        }
    }
}

/// Take a screenshot and return it as a base64-encoded PNG string.
///
/// The caller must free the returned string with `sd_free_string()`.
/// Returns NULL on error.
///
/// # Safety
/// - `source` must be a valid pointer to an SDCaptureSource.
/// - `error` must be a valid `*mut *mut SDError` or NULL.
#[no_mangle]
pub unsafe extern "C" fn sd_take_screenshot_base64(
    source: *const SDCaptureSource,
    error: *mut *mut SDError,
) -> *mut c_char {
    let core = match get_core(error) {
        Some(c) => c,
        None => return std::ptr::null_mut(),
    };

    if source.is_null() {
        SDError::write(error, "source is NULL");
        return std::ptr::null_mut();
    }

    let domain_source = match (*source).to_domain() {
        Some(s) => s,
        None => {
            SDError::write(error, "Invalid capture source type");
            return std::ptr::null_mut();
        }
    };

    match infrastructure::capture::capture_screenshot_as_base64_png(
        core.capture.as_ref(),
        &domain_source,
    ) {
        Ok(b64) => string_to_c(&b64),
        Err(e) => {
            SDError::write_app_error(error, &e);
            std::ptr::null_mut()
        }
    }
}
```

### Commands

```bash
cd rust-core && cargo build -p screen_dream_ffi
```

### Commit message

```
feat(ffi): add capture and screenshot FFI functions

sd_enumerate_sources() lists monitors and windows.
sd_capture_frame() returns raw RGBA data.
sd_take_screenshot() saves to file, sd_take_screenshot_base64()
returns a base64-encoded PNG string.
```

---

## Task 7: FFI recording functions

### Steps

- [ ] **7.1** Create `rust-core/crates/ffi/src/recording.rs`
- [ ] **7.2** Verify: `cd rust-core && cargo build -p screen_dream_ffi`

### File: `rust-core/crates/ffi/src/recording.rs`

```rust
//! Recording session FFI functions.

use std::os::raw::c_char;
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;
use std::time::Instant;

use domain::ffmpeg::FfmpegProvider;

use crate::types::*;
use crate::get_core;

/// Start a recording session.
///
/// Returns an opaque SDRecordingHandle. The caller must eventually call
/// `sd_stop_recording()` and then `sd_free_recording_handle()`.
///
/// # Safety
/// - `config` must be a valid pointer to an SDRecordingConfig.
/// - `error` must be a valid `*mut *mut SDError` or NULL.
#[no_mangle]
pub unsafe extern "C" fn sd_start_recording(
    config: *const SDRecordingConfig,
    error: *mut *mut SDError,
) -> *mut SDRecordingHandle {
    let core = match get_core(error) {
        Some(c) => c,
        None => return std::ptr::null_mut(),
    };

    if config.is_null() {
        SDError::write(error, "config is NULL");
        return std::ptr::null_mut();
    }

    let domain_config = match (*config).to_domain() {
        Some(c) => c,
        None => {
            SDError::write(error, "Invalid recording config (bad source type or NULL strings)");
            return std::ptr::null_mut();
        }
    };

    // Get FFmpeg path.
    let ffmpeg_path = match core.ffmpeg.ffmpeg_path() {
        Ok(p) => p,
        Err(e) => {
            SDError::write_app_error(error, &e);
            return std::ptr::null_mut();
        }
    };

    // Start the recording pipeline.
    let pipeline = match infrastructure::capture::RecordingPipeline::start(
        ffmpeg_path,
        core.capture.clone(),
        domain_config,
    ) {
        Ok(p) => p,
        Err(e) => {
            SDError::write_app_error(error, &e);
            return std::ptr::null_mut();
        }
    };

    let handle = Box::new(SDRecordingHandle {
        pipeline: Mutex::new(pipeline),
        start_time: Instant::now(),
        frames_captured: AtomicU64::new(0),
    });

    Box::into_raw(handle)
}

/// Stop a recording session.
///
/// Writes the final output file path to `out_path` (caller must free with
/// `sd_free_string()`). Returns `true` on success.
///
/// # Safety
/// - `handle` must be a valid pointer returned by `sd_start_recording()`.
/// - `out_path` must be a valid `*mut *mut c_char` or NULL.
/// - `error` must be a valid `*mut *mut SDError` or NULL.
#[no_mangle]
pub unsafe extern "C" fn sd_stop_recording(
    handle: *mut SDRecordingHandle,
    out_path: *mut *mut c_char,
    error: *mut *mut SDError,
) -> bool {
    if handle.is_null() {
        SDError::write(error, "handle is NULL");
        return false;
    }

    let handle_ref = &*handle;
    let mut pipeline = match handle_ref.pipeline.lock() {
        Ok(p) => p,
        Err(e) => {
            SDError::write(error, &format!("Failed to lock pipeline: {e}"));
            return false;
        }
    };

    match pipeline.stop() {
        Ok(result) => {
            if !out_path.is_null() {
                *out_path = string_to_c(&result.output_path.to_string_lossy());
            }
            true
        }
        Err(e) => {
            SDError::write_app_error(error, &e);
            false
        }
    }
}

/// Pause a recording session.
///
/// # Safety
/// - `handle` must be a valid pointer returned by `sd_start_recording()`.
/// - `error` must be a valid `*mut *mut SDError` or NULL.
#[no_mangle]
pub unsafe extern "C" fn sd_pause_recording(
    handle: *mut SDRecordingHandle,
    error: *mut *mut SDError,
) -> bool {
    if handle.is_null() {
        SDError::write(error, "handle is NULL");
        return false;
    }

    let handle_ref = &*handle;
    match handle_ref.pipeline.lock() {
        Ok(pipeline) => {
            pipeline.pause();
            true
        }
        Err(e) => {
            SDError::write(error, &format!("Failed to lock pipeline: {e}"));
            false
        }
    }
}

/// Resume a paused recording session.
///
/// # Safety
/// - `handle` must be a valid pointer returned by `sd_start_recording()`.
/// - `error` must be a valid `*mut *mut SDError` or NULL.
#[no_mangle]
pub unsafe extern "C" fn sd_resume_recording(
    handle: *mut SDRecordingHandle,
    error: *mut *mut SDError,
) -> bool {
    if handle.is_null() {
        SDError::write(error, "handle is NULL");
        return false;
    }

    let handle_ref = &*handle;
    match handle_ref.pipeline.lock() {
        Ok(pipeline) => {
            pipeline.resume();
            true
        }
        Err(e) => {
            SDError::write(error, &format!("Failed to lock pipeline: {e}"));
            false
        }
    }
}

/// Get the current status of a recording session.
///
/// This is a non-allocating function that returns the status by value.
///
/// # Safety
/// - `handle` must be a valid pointer returned by `sd_start_recording()`.
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

    let handle_ref = &*handle;
    let elapsed = handle_ref.start_time.elapsed().as_secs_f64();

    let (state, frames) = match handle_ref.pipeline.lock() {
        Ok(pipeline) => {
            let state = if pipeline.is_paused() {
                3 // Paused
            } else if pipeline.is_running() {
                2 // Recording
            } else {
                5 // Completed
            };
            (state, handle_ref.frames_captured.load(std::sync::atomic::Ordering::Relaxed))
        }
        Err(_) => (6, 0), // Failed
    };

    SDRecordingStatus {
        state,
        elapsed_seconds: elapsed,
        frames_captured: frames,
    }
}

/// Free a recording handle. Must be called after `sd_stop_recording()`.
///
/// # Safety
/// - `handle` must have been returned by `sd_start_recording()` or be NULL.
/// - Must not be called while the recording is still running.
#[no_mangle]
pub unsafe extern "C" fn sd_free_recording_handle(handle: *mut SDRecordingHandle) {
    if !handle.is_null() {
        let _ = Box::from_raw(handle);
    }
}

/// List available audio input devices as a JSON array.
///
/// Returns a JSON string like:
/// ```json
/// [{"name": "Built-in Mic", "is_default": true, "sample_rate": 44100, "channels": 1}, ...]
/// ```
///
/// The caller must free the returned string with `sd_free_string()`.
/// Returns NULL on error.
///
/// # Safety
/// - `error` must be a valid `*mut *mut SDError` or NULL.
#[no_mangle]
pub unsafe extern "C" fn sd_list_audio_devices(
    error: *mut *mut SDError,
) -> *mut c_char {
    match infrastructure::capture::list_audio_devices() {
        Ok(devices) => {
            match serde_json::to_string(&devices) {
                Ok(json) => string_to_c(&json),
                Err(e) => {
                    SDError::write(error, &format!("Failed to serialize audio devices: {e}"));
                    std::ptr::null_mut()
                }
            }
        }
        Err(e) => {
            SDError::write_app_error(error, &e);
            std::ptr::null_mut()
        }
    }
}
```

### Commands

```bash
cd rust-core && cargo build -p screen_dream_ffi
```

### Commit message

```
feat(ffi): add recording and audio device FFI functions

sd_start_recording() returns an opaque handle used by stop/pause/resume.
sd_get_recording_status() returns state by value (no allocation).
sd_list_audio_devices() returns a JSON array of input devices.
```

---

## Task 8: Generate C header and verify

### Steps

- [ ] **8.1** Add a `build.rs` to the FFI crate that auto-generates the C header
- [ ] **8.2** Build the library and verify header generation
- [ ] **8.3** Write a small C test program that links the library
- [ ] **8.4** Build and run the C test

### File: `rust-core/crates/ffi/build.rs`

```rust
use std::env;
use std::path::PathBuf;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let output_dir = PathBuf::from(&crate_dir).join("include");

    // Create include directory if it doesn't exist.
    std::fs::create_dir_all(&output_dir).expect("Failed to create include directory");

    let config = cbindgen::Config::from_file(PathBuf::from(&crate_dir).join("cbindgen.toml"))
        .expect("Failed to read cbindgen.toml");

    cbindgen::Builder::new()
        .with_crate(crate_dir)
        .with_config(config)
        .generate()
        .expect("Failed to generate C bindings")
        .write_to_file(output_dir.join("screen_dream_ffi.h"));
}
```

### File: `rust-core/crates/ffi/tests/smoke_test.c`

```c
/* Minimal smoke test: init, query platform, shutdown. */
#include <stdio.h>
#include <stdlib.h>
#include <assert.h>
#include "../include/screen_dream_ffi.h"

int main(void) {
    SDError *err = NULL;

    /* Initialize the core. */
    printf("Calling sd_init()...\n");
    bool ok = sd_init(&err);
    if (!ok) {
        fprintf(stderr, "sd_init failed: %s\n", err ? err->message : "unknown");
        sd_free_error(err);
        return 1;
    }
    printf("sd_init() succeeded.\n");

    /* Get library version. */
    char *version = sd_version();
    printf("Library version: %s\n", version);
    sd_free_string(version);

    /* Get platform info. */
    SDPlatformInfo *info = sd_get_platform_info(&err);
    if (!info) {
        fprintf(stderr, "sd_get_platform_info failed: %s\n",
                err ? err->message : "unknown");
        sd_free_error(err);
        sd_shutdown();
        return 1;
    }
    printf("Platform: os=%s, display_server=%s, arch=%s\n",
           info->os, info->display_server, info->arch);
    sd_free_platform_info(info);

    /* Check FFmpeg status. */
    err = NULL;
    SDFfmpegStatus *ffmpeg = sd_get_ffmpeg_status(&err);
    if (ffmpeg) {
        printf("FFmpeg available: %s\n", ffmpeg->available ? "yes" : "no");
        if (ffmpeg->available) {
            printf("FFmpeg version: %s\n", ffmpeg->version);
            printf("Video encoders: %s\n", ffmpeg->video_encoders_json);
            printf("Audio encoders: %s\n", ffmpeg->audio_encoders_json);
        }
        sd_free_ffmpeg_status(ffmpeg);
    } else {
        fprintf(stderr, "sd_get_ffmpeg_status failed: %s\n",
                err ? err->message : "unknown");
        sd_free_error(err);
    }

    /* Shutdown. */
    sd_shutdown();
    printf("sd_shutdown() succeeded. All OK.\n");

    return 0;
}
```

### File: `rust-core/crates/ffi/tests/CMakeLists.txt`

```cmake
cmake_minimum_required(VERSION 3.16)
project(screen_dream_ffi_smoke_test C)

set(CMAKE_C_STANDARD 11)

# Path to the generated header
include_directories(${CMAKE_CURRENT_SOURCE_DIR}/../include)

# Path to the built cdylib
# Adjust based on cargo build output location
set(FFI_LIB_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../../target/debug")

add_executable(smoke_test smoke_test.c)

# Link against the cdylib
target_link_libraries(smoke_test
    ${FFI_LIB_DIR}/libscreen_dream_ffi.so
    pthread
    dl
    m
)
```

### Commands

```bash
# 8.1-8.2 Build library (build.rs runs cbindgen automatically)
cd rust-core && cargo build -p screen_dream_ffi

# Verify header was generated
ls rust-core/crates/ffi/include/screen_dream_ffi.h

# 8.3-8.4 Build and run C smoke test
cd rust-core/crates/ffi/tests
mkdir -p build && cd build
cmake ..
make
LD_LIBRARY_PATH=../../../../target/debug ./smoke_test
```

### Commit message

```
feat(ffi): add build.rs for auto-generated C header and smoke test

build.rs invokes cbindgen to produce screen_dream_ffi.h on every
build. A minimal C program verifies init/platform/ffmpeg/shutdown
round-trips correctly through the FFI boundary.
```

---

## Summary of FFI API Surface

| Function | Returns | Free with |
|---|---|---|
| `sd_init(error)` | `bool` | -- |
| `sd_shutdown()` | `void` | -- |
| `sd_version()` | `*mut c_char` | `sd_free_string()` |
| `sd_get_platform_info(error)` | `*mut SDPlatformInfo` | `sd_free_platform_info()` |
| `sd_get_ffmpeg_status(error)` | `*mut SDFfmpegStatus` | `sd_free_ffmpeg_status()` |
| `sd_load_settings(error)` | `*mut c_char` (JSON) | `sd_free_string()` |
| `sd_save_settings(json, error)` | `bool` | -- |
| `sd_reset_settings(error)` | `*mut c_char` (JSON) | `sd_free_string()` |
| `sd_enumerate_sources(error)` | `*mut SDAvailableSources` | `sd_free_available_sources()` |
| `sd_capture_frame(source, error)` | `*mut SDFrame` | `sd_free_frame()` |
| `sd_take_screenshot(source, path, error)` | `bool` | -- |
| `sd_take_screenshot_base64(source, error)` | `*mut c_char` | `sd_free_string()` |
| `sd_start_recording(config, error)` | `*mut SDRecordingHandle` | `sd_free_recording_handle()` |
| `sd_stop_recording(handle, out_path, error)` | `bool` | -- |
| `sd_pause_recording(handle, error)` | `bool` | -- |
| `sd_resume_recording(handle, error)` | `bool` | -- |
| `sd_get_recording_status(handle)` | `SDRecordingStatus` (by value) | -- |
| `sd_list_audio_devices(error)` | `*mut c_char` (JSON) | `sd_free_string()` |
| `sd_free_error(error)` | `void` | -- |
| `sd_free_string(s)` | `void` | -- |

## Error Convention

All fallible functions accept `*mut *mut SDError` as their last parameter. On success the pointer is untouched. On failure an `SDError` is heap-allocated and written through the pointer. The caller checks the return value (NULL pointer or `false`) and reads the error message from `err->message`. The caller must free the error with `sd_free_error()`.

```c
SDError *err = NULL;
SDPlatformInfo *info = sd_get_platform_info(&err);
if (!info) {
    fprintf(stderr, "Error: %s\n", err->message);
    sd_free_error(err);
    return;
}
// use info...
sd_free_platform_info(info);
```
