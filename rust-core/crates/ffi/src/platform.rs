//! Platform information FFI functions.

use crate::core;
use crate::types::{to_c_string, SDPlatformInfo};

/// Returns platform information (OS, display server, architecture).
///
/// The returned struct contains heap-allocated strings. The caller must
/// free each string field with `sd_free_string` when done.
///
/// # Safety
/// `sd_init` must have been called successfully before calling this function.
#[no_mangle]
pub unsafe extern "C" fn sd_get_platform_info() -> SDPlatformInfo {
    let state = core();
    let p = &state.platform;

    SDPlatformInfo {
        os: to_c_string(&format!("{:?}", p.os)),
        display_server: to_c_string(&format!("{:?}", p.display_server)),
        arch: to_c_string(&p.arch),
    }
}
