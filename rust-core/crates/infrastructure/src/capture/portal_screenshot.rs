//! Silent screenshot capture via xdg-desktop-portal Screenshot API.
//!
//! Uses `org.freedesktop.portal.Screenshot.Screenshot` with `interactive: false`
//! to capture a full-screen screenshot without any dialog on KDE Plasma.
//! The portal saves a PNG to `~/Pictures/Screenshot_*.png` and returns its URI.

use std::path::PathBuf;

use domain::error::{AppError, AppResult};
use tracing::{debug, warn};
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

/// Capture a screenshot via xdg-desktop-portal Screenshot with interactive=false.
/// Returns the path to the captured PNG file (saved by the portal).
/// NO dialog is shown -- works silently on KDE.
pub async fn portal_screenshot_silent_async() -> AppResult<PathBuf> {
    let conn = zbus::Connection::session()
        .await
        .map_err(|e| AppError::Capture(format!("D-Bus session connection failed: {e}")))?;

    // Build a unique token for our request so we can match the Response signal.
    let token = format!("sd_screenshot_{}", std::process::id());

    // Build options dict: interactive=false, handle_token=<token>
    let mut options: std::collections::HashMap<&str, Value<'_>> = std::collections::HashMap::new();
    options.insert("interactive", Value::Bool(false));
    options.insert("handle_token", Value::Str(token.as_str().into()));

    // The portal will emit a Response signal on this object path.
    let sender_name = conn
        .unique_name()
        .ok_or_else(|| AppError::Capture("No unique D-Bus name".to_string()))?
        .as_str()
        .trim_start_matches(':')
        .replace('.', "_");
    let request_path_str = format!(
        "/org/freedesktop/portal/desktop/request/{}/{}",
        sender_name, token
    );
    let request_path = ObjectPath::try_from(request_path_str.as_str()).map_err(|e| {
        AppError::Capture(format!("Invalid request object path: {e}"))
    })?;

    debug!("Portal screenshot request path: {}", request_path);

    // Subscribe to the Response signal BEFORE making the call to avoid races.
    let signal_rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("org.freedesktop.portal.Request")
        .map_err(|e| AppError::Capture(format!("MatchRule interface error: {e}")))?
        .member("Response")
        .map_err(|e| AppError::Capture(format!("MatchRule member error: {e}")))?
        .path(request_path.clone())
        .map_err(|e| AppError::Capture(format!("MatchRule path error: {e}")))?
        .build();

    let proxy = zbus::MessageStream::for_match_rule(signal_rule, &conn, Some(128))
        .await
        .map_err(|e| AppError::Capture(format!("Failed to subscribe to Response signal: {e}")))?;

    // Call Screenshot on the portal.
    let portal = zbus::Proxy::new(
        &conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Screenshot",
    )
    .await
    .map_err(|e| AppError::Capture(format!("Failed to create portal proxy: {e}")))?;

    let _reply_path: zbus::zvariant::OwnedObjectPath = portal
        .call("Screenshot", &("", options))
        .await
        .map_err(|e| AppError::Capture(format!("Portal Screenshot call failed: {e}")))?;

    debug!("Portal Screenshot call sent, waiting for Response signal...");

    // Wait for the Response signal with a timeout.
    use futures_util::StreamExt as _;
    let timeout_duration = std::time::Duration::from_secs(10);

    let response_msg = tokio::time::timeout(timeout_duration, {
        let mut stream = proxy;
        async move { stream.next().await }
    })
    .await
    .map_err(|_| {
        AppError::Capture("Timed out waiting for portal Screenshot response".to_string())
    })?
    .ok_or_else(|| {
        AppError::Capture("Response signal stream ended without a message".to_string())
    })?;

    let response_msg = response_msg.map_err(|e| {
        AppError::Capture(format!("Error receiving Response signal: {e}"))
    })?;

    // The Response signal body is (response: u32, results: a{sv}).
    let (response_code, results): (
        u32,
        std::collections::HashMap<String, OwnedValue>,
    ) = response_msg.body().deserialize().map_err(|e| {
        AppError::Capture(format!("Failed to deserialize Response signal body: {e}"))
    })?;

    if response_code != 0 {
        return Err(AppError::Capture(format!(
            "Portal Screenshot failed with response code {response_code} (0=success, 1=cancelled, 2=other)"
        )));
    }

    // Extract the URI from results.
    let uri_value = results.get("uri").ok_or_else(|| {
        AppError::Capture("No 'uri' field in portal Screenshot response".to_string())
    })?;

    let uri_str: String = <String>::try_from(uri_value.clone()).map_err(|e| {
        AppError::Capture(format!("Failed to extract URI string: {e}"))
    })?;

    debug!("Portal screenshot URI: {}", uri_str);

    // Convert file:// URI to PathBuf.
    let path = uri_to_path(&uri_str)?;

    if !path.exists() {
        return Err(AppError::Capture(format!(
            "Portal screenshot file does not exist: {}",
            path.display()
        )));
    }

    Ok(path)
}

/// Synchronous wrapper around `portal_screenshot_silent_async` for use within
/// a tokio runtime context (called via `runtime.block_on`).
pub fn portal_screenshot_silent(runtime: &tokio::runtime::Runtime) -> AppResult<PathBuf> {
    runtime.block_on(portal_screenshot_silent_async())
}

/// Load a PNG file from disk into a `CapturedFrame` (RGBA pixel data).
pub fn load_png_as_frame(
    path: &std::path::Path,
) -> AppResult<domain::capture::CapturedFrame> {
    let img = image::open(path).map_err(|e| {
        AppError::Capture(format!(
            "Failed to open screenshot PNG '{}': {e}",
            path.display()
        ))
    })?;

    let rgba_img = img.to_rgba8();
    let (width, height) = rgba_img.dimensions();
    let data = rgba_img.into_raw();

    Ok(domain::capture::CapturedFrame {
        data,
        width,
        height,
    })
}

/// Load a PNG and crop it to the given rectangle, returning a `CapturedFrame`.
/// Coordinates are clamped to the image bounds.
pub fn load_png_and_crop(
    path: &std::path::Path,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> AppResult<domain::capture::CapturedFrame> {
    let img = image::open(path).map_err(|e| {
        AppError::Capture(format!(
            "Failed to open screenshot PNG '{}': {e}",
            path.display()
        ))
    })?;

    let (img_w, img_h) = (img.width(), img.height());

    // Clamp crop coordinates to image bounds.
    let crop_x = x.max(0) as u32;
    let crop_y = y.max(0) as u32;

    if crop_x >= img_w || crop_y >= img_h {
        return Err(AppError::Capture(format!(
            "Crop origin ({crop_x}, {crop_y}) is outside image bounds ({img_w}x{img_h})"
        )));
    }

    let crop_w = width.min(img_w.saturating_sub(crop_x));
    let crop_h = height.min(img_h.saturating_sub(crop_y));

    if crop_w == 0 || crop_h == 0 {
        return Err(AppError::Capture(format!(
            "Crop region is empty after clamping: {}x{} at ({crop_x}, {crop_y}) in {img_w}x{img_h} image",
            crop_w, crop_h
        )));
    }

    debug!(
        "Cropping screenshot: ({crop_x}, {crop_y}) {}x{} from {img_w}x{img_h}",
        crop_w, crop_h
    );

    let cropped = img.crop_imm(crop_x, crop_y, crop_w, crop_h).to_rgba8();
    let (final_w, final_h) = cropped.dimensions();
    let data = cropped.into_raw();

    Ok(domain::capture::CapturedFrame {
        data,
        width: final_w,
        height: final_h,
    })
}

/// Convert a `file://` URI to a `PathBuf`.
fn uri_to_path(uri: &str) -> AppResult<PathBuf> {
    if let Some(path_str) = uri.strip_prefix("file://") {
        // URL-decode percent-encoded characters.
        let decoded = percent_decode(path_str);
        Ok(PathBuf::from(decoded))
    } else {
        Err(AppError::Capture(format!(
            "Unexpected URI scheme (expected file://): {uri}"
        )))
    }
}

/// Simple percent-decoding for file paths (handles %20, %23, etc.).
fn percent_decode(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let hi = chars.next();
            let lo = chars.next();
            if let (Some(h), Some(l)) = (hi, lo) {
                let hex = [h, l];
                if let Ok(s) = std::str::from_utf8(&hex) {
                    if let Ok(byte_val) = u8::from_str_radix(s, 16) {
                        result.push(byte_val as char);
                        continue;
                    }
                }
                // If decoding fails, pass through literally.
                result.push('%');
                result.push(h as char);
                result.push(l as char);
            } else {
                result.push('%');
            }
        } else {
            result.push(b as char);
        }
    }
    result
}

/// Helper: capture screenshot, load as frame, and clean up the temp file.
pub fn capture_full_frame(
    runtime: &tokio::runtime::Runtime,
) -> AppResult<domain::capture::CapturedFrame> {
    let path = portal_screenshot_silent(runtime)?;
    let frame = load_png_as_frame(&path)?;
    // Clean up the temp screenshot file (best effort).
    if let Err(e) = std::fs::remove_file(&path) {
        warn!(
            "Failed to remove portal screenshot temp file '{}': {e}",
            path.display()
        );
    }
    Ok(frame)
}

/// Helper: capture screenshot, crop to region, load as frame, and clean up.
pub fn capture_cropped_frame(
    runtime: &tokio::runtime::Runtime,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> AppResult<domain::capture::CapturedFrame> {
    let path = portal_screenshot_silent(runtime)?;
    let frame = load_png_and_crop(&path, x, y, width, height)?;
    // Clean up the temp screenshot file (best effort).
    if let Err(e) = std::fs::remove_file(&path) {
        warn!(
            "Failed to remove portal screenshot temp file '{}': {e}",
            path.display()
        );
    }
    Ok(frame)
}
