pub mod audio_capture;
#[allow(dead_code)]
mod drm_backend; // kept for future AMD/Intel use, not re-exported
pub mod kwin_backend;
pub mod pipewire_capture;
pub mod portal_recorder;
pub mod portal_screenshot;
pub mod recording_pipeline;
pub mod screenshot;
pub mod spectacle_backend;
pub mod xcap_backend;

pub use audio_capture::*;
pub use kwin_backend::*;
pub use pipewire_capture::*;
pub use portal_recorder::*;
pub use recording_pipeline::*;
pub use screenshot::*;
pub use xcap_backend::*;
