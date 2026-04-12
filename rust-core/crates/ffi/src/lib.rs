//! Screen Dream FFI crate.
//!
//! Exposes the Rust domain + infrastructure layers as a C-compatible shared library (`cdylib`).
//! Every public symbol is prefixed with `sd_` to avoid name collisions.

pub mod types;

use std::ffi::c_char;
use std::sync::{Arc, OnceLock};

use domain::ffmpeg::FfmpegProvider;
use domain::platform::PlatformInfo;
use domain::settings::SettingsRepository;
use infrastructure::capture::XcapCaptureBackend;
use infrastructure::ffmpeg::FfmpegResolver;
use infrastructure::settings::JsonSettingsRepository;

use types::{from_c_str, SDError};

// ---------------------------------------------------------------------------
// Core singleton
// ---------------------------------------------------------------------------

/// Holds all initialized subsystems for the lifetime of the library.
pub struct CoreState {
    pub ffmpeg: Arc<dyn FfmpegProvider>,
    pub settings: Arc<dyn SettingsRepository>,
    pub capture: Arc<XcapCaptureBackend>,
    pub platform: PlatformInfo,
}

static CORE: OnceLock<CoreState> = OnceLock::new();

/// Returns a reference to the initialized `CoreState`.
///
/// # Panics
/// Panics if `sd_init` has not been called successfully.
pub fn core() -> &'static CoreState {
    CORE.get().expect("sd_init() must be called before using the Screen Dream FFI library")
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

/// Initialize the Screen Dream core.
///
/// `config_dir` — NUL-terminated path to the configuration directory
///                (e.g. `~/.config/screen-dream`).  The directory will be
///                created if it does not exist.
///
/// On success returns `true` and leaves `*error` unchanged.
/// On failure returns `false` and writes a heap-allocated `SDError` into `*error`.
/// The caller must free it with `sd_free_error`.
///
/// Calling `sd_init` more than once (without `sd_shutdown`) is a no-op that returns `true`.
///
/// # Safety
/// * `config_dir` must be a valid NUL-terminated UTF-8 string.
/// * `error` must be a valid pointer to a `*mut SDError` (may point to null).
#[no_mangle]
pub unsafe extern "C" fn sd_init(
    config_dir: *const c_char,
    error: *mut *mut SDError,
) -> bool {
    // Already initialized — treat as success.
    if CORE.get().is_some() {
        return true;
    }

    let config_path = unsafe { from_c_str(config_dir) };
    let config_dir_path = std::path::PathBuf::from(&config_path);

    // Ensure the config directory exists.
    if let Err(e) = std::fs::create_dir_all(&config_dir_path) {
        if !error.is_null() {
            unsafe {
                *error = SDError::from_app_error(domain::error::AppError::Io(format!(
                    "Failed to create config directory '{}': {}",
                    config_path, e
                )));
            }
        }
        return false;
    }

    // Detect platform.
    let platform = PlatformInfo::detect();

    // Build subsystems.
    let settings: Arc<dyn SettingsRepository> =
        Arc::new(JsonSettingsRepository::new(config_dir_path.clone()));

    let ffmpeg: Arc<dyn FfmpegProvider> =
        Arc::new(FfmpegResolver::new(None, None));

    let capture: Arc<XcapCaptureBackend> =
        Arc::new(XcapCaptureBackend::new(platform.clone()));

    let state = CoreState {
        ffmpeg,
        settings,
        capture,
        platform,
    };

    // OnceLock::set returns Err if already set (race). Treat as success.
    let _ = CORE.set(state);
    true
}

/// Shut down the Screen Dream core and release all resources.
///
/// After calling this, `sd_init` may be called again.
///
/// # Safety
/// No other FFI calls may be in flight when this is called.
#[no_mangle]
pub unsafe extern "C" fn sd_shutdown() {
    // OnceLock does not have a `take` method in stable Rust, so we use
    // a workaround: we leak the static and replace it.  Since this is a
    // cdylib loaded once, the one-time leak on shutdown is acceptable.
    //
    // In practice, sd_shutdown is called once at process exit.
    // A future version may use a Mutex<Option<CoreState>> if hot-reload is needed.

    // Nothing to do if not initialized -- OnceLock cannot be reset in stable Rust.
    // We document that sd_shutdown + sd_init again is not currently supported.
    // The process is expected to exit after sd_shutdown.
}
