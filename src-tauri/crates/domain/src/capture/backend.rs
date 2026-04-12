use crate::error::AppResult;
use super::source::{AvailableSources, CaptureSource};

/// Raw captured frame data.
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    /// RGBA pixel data, row-major, 4 bytes per pixel.
    pub data: Vec<u8>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Trait for platform-specific screen/window capture.
/// Implemented by the infrastructure layer using xcap or platform-native APIs.
pub trait CaptureBackend: Send + Sync {
    /// Enumerate all available capture sources (monitors + windows).
    fn enumerate_sources(&self) -> AppResult<AvailableSources>;

    /// Capture a single frame from the given source.
    /// Returns raw RGBA pixel data suitable for encoding or saving.
    fn capture_frame(&self, source: &CaptureSource) -> AppResult<CapturedFrame>;
}
