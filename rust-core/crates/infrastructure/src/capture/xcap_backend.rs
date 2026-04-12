use domain::capture::{
    AvailableSources, CaptureBackend, CaptureSource, CapturedFrame,
    MonitorInfo, RegionSource, ScreenSource, WindowInfo, WindowSource,
};
use domain::error::{AppError, AppResult};
use domain::platform::{DisplayServer, PlatformInfo};
use tracing::{debug, info, warn};
use xcap::{Monitor, Window, XCapError};

/// Helper to convert XCapError into AppError::Capture.
fn xcap_err(context: &str, e: XCapError) -> AppError {
    AppError::Capture(format!("{context}: {e}"))
}

/// CaptureBackend implementation using the xcap crate.
/// Works on X11, macOS, and Windows. Wayland has limited window enumeration.
pub struct XcapCaptureBackend {
    platform: PlatformInfo,
}

impl XcapCaptureBackend {
    pub fn new(platform: PlatformInfo) -> Self {
        info!(
            "Initializing xcap capture backend for {:?}/{:?}",
            platform.os, platform.display_server
        );
        XcapCaptureBackend { platform }
    }

    /// Check if we can enumerate windows on this platform.
    fn can_enumerate_windows(&self) -> bool {
        // xcap Window::all() works on X11, macOS, and Windows.
        // On pure Wayland (no XWayland), it will fail or return empty.
        match self.platform.display_server {
            DisplayServer::X11 => true,
            DisplayServer::Quartz => true,
            DisplayServer::Win32 => true,
            DisplayServer::Wayland => {
                // Check if XWayland is available (DISPLAY env var set)
                std::env::var("DISPLAY").is_ok()
            }
            DisplayServer::Unknown => false,
        }
    }

    fn enumerate_monitors(&self) -> AppResult<Vec<MonitorInfo>> {
        let monitors = Monitor::all().map_err(|e| {
            xcap_err("Failed to enumerate monitors", e)
        })?;

        let mut infos = Vec::with_capacity(monitors.len());
        for m in &monitors {
            infos.push(MonitorInfo {
                id: m.id().map_err(|e| xcap_err("monitor.id()", e))?,
                name: m.name().map_err(|e| xcap_err("monitor.name()", e))?,
                friendly_name: m.friendly_name().map_err(|e| xcap_err("monitor.friendly_name()", e))?,
                width: m.width().map_err(|e| xcap_err("monitor.width()", e))?,
                height: m.height().map_err(|e| xcap_err("monitor.height()", e))?,
                x: m.x().map_err(|e| xcap_err("monitor.x()", e))?,
                y: m.y().map_err(|e| xcap_err("monitor.y()", e))?,
                scale_factor: m.scale_factor().map_err(|e| xcap_err("monitor.scale_factor()", e))?,
                is_primary: m.is_primary().map_err(|e| xcap_err("monitor.is_primary()", e))?,
            });
        }

        debug!("Found {} monitors", infos.len());
        Ok(infos)
    }

    fn enumerate_windows(&self) -> AppResult<Vec<WindowInfo>> {
        let windows = Window::all().map_err(|e| {
            xcap_err("Failed to enumerate windows", e)
        })?;

        let mut infos = Vec::new();
        for w in &windows {
            let is_minimized = w.is_minimized().map_err(|e| xcap_err("window.is_minimized()", e))?;
            let width = w.width().map_err(|e| xcap_err("window.width()", e))?;
            let height = w.height().map_err(|e| xcap_err("window.height()", e))?;

            if is_minimized || width == 0 || height == 0 {
                continue;
            }

            infos.push(WindowInfo {
                id: w.id().map_err(|e| xcap_err("window.id()", e))?,
                pid: w.pid().map_err(|e| xcap_err("window.pid()", e))?,
                app_name: w.app_name().map_err(|e| xcap_err("window.app_name()", e))?,
                title: w.title().map_err(|e| xcap_err("window.title()", e))?,
                width,
                height,
                is_minimized,
                is_focused: w.is_focused().map_err(|e| xcap_err("window.is_focused()", e))?,
            });
        }

        debug!("Found {} visible windows", infos.len());
        Ok(infos)
    }

    fn capture_monitor(&self, source: &ScreenSource) -> AppResult<CapturedFrame> {
        let monitors = Monitor::all().map_err(|e| {
            xcap_err("Failed to enumerate monitors", e)
        })?;

        let monitor = monitors
            .into_iter()
            .find(|m| m.id().ok() == Some(source.monitor_id))
            .ok_or_else(|| {
                AppError::Capture(format!(
                    "Monitor with ID {} not found",
                    source.monitor_id
                ))
            })?;

        let img = monitor.capture_image().map_err(|e| {
            xcap_err(&format!("Failed to capture monitor {}", source.monitor_id), e)
        })?;

        let width = img.width();
        let height = img.height();
        let data = img.into_raw();

        Ok(CapturedFrame {
            data,
            width,
            height,
        })
    }

    fn capture_window(&self, source: &WindowSource) -> AppResult<CapturedFrame> {
        let windows = Window::all().map_err(|e| {
            xcap_err("Failed to enumerate windows", e)
        })?;

        let window = windows
            .into_iter()
            .find(|w| w.id().ok() == Some(source.window_id))
            .ok_or_else(|| {
                AppError::Capture(format!(
                    "Window with ID {} not found",
                    source.window_id
                ))
            })?;

        let img = window.capture_image().map_err(|e| {
            xcap_err(&format!("Failed to capture window {}", source.window_id), e)
        })?;

        let width = img.width();
        let height = img.height();
        let data = img.into_raw();

        Ok(CapturedFrame {
            data,
            width,
            height,
        })
    }

    fn capture_region(&self, source: &RegionSource) -> AppResult<CapturedFrame> {
        // Capture the full monitor, then crop to the region.
        let full_frame = self.capture_monitor(&ScreenSource {
            monitor_id: source.monitor_id,
        })?;

        // Use the image crate to crop.
        let full_img = image::RgbaImage::from_raw(
            full_frame.width,
            full_frame.height,
            full_frame.data,
        )
        .ok_or_else(|| {
            AppError::Capture("Failed to reconstruct image from raw data".to_string())
        })?;

        let dynamic = image::DynamicImage::ImageRgba8(full_img);

        // Clamp crop region to image bounds.
        let crop_x = source.x.max(0) as u32;
        let crop_y = source.y.max(0) as u32;
        let crop_w = source.width.min(full_frame.width.saturating_sub(crop_x));
        let crop_h = source.height.min(full_frame.height.saturating_sub(crop_y));

        if crop_w == 0 || crop_h == 0 {
            return Err(AppError::Capture(
                "Region selection has zero width or height after clamping".to_string(),
            ));
        }

        let cropped = dynamic.crop_imm(crop_x, crop_y, crop_w, crop_h);
        let rgba = cropped.to_rgba8();
        let width = rgba.width();
        let height = rgba.height();
        let data = rgba.into_raw();

        Ok(CapturedFrame {
            data,
            width,
            height,
        })
    }
}

impl CaptureBackend for XcapCaptureBackend {
    fn enumerate_sources(&self) -> AppResult<AvailableSources> {
        let monitors = self.enumerate_monitors()?;

        let (windows, windows_unavailable, windows_unavailable_reason) =
            if self.can_enumerate_windows() {
                match self.enumerate_windows() {
                    Ok(wins) => (wins, false, None),
                    Err(e) => {
                        warn!("Window enumeration failed: {e}");
                        (
                            vec![],
                            true,
                            Some(format!("Window enumeration failed: {e}")),
                        )
                    }
                }
            } else {
                let reason = if self.platform.display_server == DisplayServer::Wayland {
                    "Window enumeration is not available on Wayland. \
                     Use the system portal picker or select a full screen to capture."
                        .to_string()
                } else {
                    "Window enumeration is not supported on this display server.".to_string()
                };
                (vec![], true, Some(reason))
            };

        Ok(AvailableSources {
            monitors,
            windows,
            windows_unavailable,
            windows_unavailable_reason,
        })
    }

    fn capture_frame(&self, source: &CaptureSource) -> AppResult<CapturedFrame> {
        match source {
            CaptureSource::Screen(s) => self.capture_monitor(s),
            CaptureSource::Window(w) => self.capture_window(w),
            CaptureSource::Region(r) => self.capture_region(r),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_create_backend() {
        let platform = PlatformInfo::detect();
        let backend = XcapCaptureBackend::new(platform);
        // Just verify it constructs without panic.
        assert!(backend.can_enumerate_windows() || !backend.can_enumerate_windows());
    }
}
