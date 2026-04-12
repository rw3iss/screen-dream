use std::io::Cursor;
use std::path::{Path, PathBuf};

use domain::capture::{CaptureBackend, CaptureSource, CapturedFrame};
use domain::error::{AppError, AppResult};
use image::ImageFormat;
use tracing::{debug, info};

/// Supported screenshot output formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenshotFormat {
    Png,
    Jpeg,
    WebP,
}

impl ScreenshotFormat {
    /// Determine format from file extension.
    pub fn from_extension(path: &Path) -> AppResult<Self> {
        match path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .as_deref()
        {
            Some("png") => Ok(ScreenshotFormat::Png),
            Some("jpg" | "jpeg") => Ok(ScreenshotFormat::Jpeg),
            Some("webp") => Ok(ScreenshotFormat::WebP),
            _ => Err(AppError::Capture(
                "Unsupported screenshot format. Use .png, .jpg, or .webp".to_string(),
            )),
        }
    }

    fn to_image_format(self) -> ImageFormat {
        match self {
            ScreenshotFormat::Png => ImageFormat::Png,
            ScreenshotFormat::Jpeg => ImageFormat::Jpeg,
            ScreenshotFormat::WebP => ImageFormat::WebP,
        }
    }
}

/// Capture a screenshot from the given source and save it to a file.
///
/// The output format is determined by the file extension of `output_path`.
/// Supported formats: PNG, JPEG, WebP.
///
/// Returns the path to the saved file.
pub fn capture_screenshot(
    backend: &dyn CaptureBackend,
    source: &CaptureSource,
    output_path: &Path,
) -> AppResult<PathBuf> {
    let format = ScreenshotFormat::from_extension(output_path)?;
    info!("Capturing screenshot to {}", output_path.display());

    let frame = backend.capture_frame(source)?;
    save_frame_to_file(&frame, output_path, format)?;

    debug!(
        "Screenshot saved: {}x{} -> {}",
        frame.width,
        frame.height,
        output_path.display()
    );
    Ok(output_path.to_path_buf())
}

/// Capture a screenshot and return it as a base64-encoded PNG string.
///
/// Useful for copying to clipboard or sending to the frontend.
pub fn capture_screenshot_as_base64_png(
    backend: &dyn CaptureBackend,
    source: &CaptureSource,
) -> AppResult<String> {
    use base64::Engine as _;

    info!("Capturing screenshot as base64 PNG");
    let frame = backend.capture_frame(source)?;

    let img = image::RgbaImage::from_raw(frame.width, frame.height, frame.data).ok_or_else(
        || AppError::Capture("Failed to create image from captured frame data".to_string()),
    )?;

    let mut png_bytes = Vec::new();
    let mut cursor = Cursor::new(&mut png_bytes);
    img.write_to(&mut cursor, ImageFormat::Png).map_err(|e| {
        AppError::Capture(format!("Failed to encode screenshot as PNG: {e}"))
    })?;

    let encoded = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
    debug!(
        "Screenshot base64 encoded: {}x{}, {} bytes PNG, {} chars base64",
        frame.width,
        frame.height,
        png_bytes.len(),
        encoded.len()
    );
    Ok(encoded)
}

/// Save a CapturedFrame to a file in the given format.
fn save_frame_to_file(
    frame: &CapturedFrame,
    path: &Path,
    format: ScreenshotFormat,
) -> AppResult<()> {
    let img = image::RgbaImage::from_raw(frame.width, frame.height, frame.data.clone())
        .ok_or_else(|| {
            AppError::Capture("Failed to create image from captured frame data".to_string())
        })?;

    // Ensure parent directory exists.
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::Capture(format!(
                "Failed to create directory {}: {e}",
                parent.display()
            ))
        })?;
    }

    img.save_with_format(path, format.to_image_format())
        .map_err(|e| {
            AppError::Capture(format!(
                "Failed to save screenshot to {}: {e}",
                path.display()
            ))
        })?;

    Ok(())
}
