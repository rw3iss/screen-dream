//! KWin D-Bus capture backend for KDE Plasma on Wayland.
//!
//! Uses KWin's scripting D-Bus API for window enumeration and
//! `org.kde.KWin.ScreenShot2` for frame capture.

use std::collections::HashMap;
use std::io::Read as _;
use std::os::unix::io::FromRawFd;
use std::sync::Mutex;

use domain::capture::{
    AvailableSources, CaptureBackend, CaptureSource, CapturedFrame, MonitorInfo, RegionSource,
    ScreenSource, WindowInfo, WindowSource,
};
use domain::error::{AppError, AppResult};
use domain::platform::PlatformInfo;
use tracing::{debug, info, warn};
use xcap::Monitor;
use zbus::zvariant::{Fd, OwnedValue, Value};

// ---------------------------------------------------------------------------
// Compositor detection
// ---------------------------------------------------------------------------

/// Detected compositor type.
#[derive(Debug, Clone, PartialEq)]
pub enum Compositor {
    KWin,
    GnomeMutter,
    Unknown,
}

/// Detect which Wayland compositor is running.
pub fn detect_compositor() -> Compositor {
    // Fast path: check environment variable.
    if let Ok(desktop) = std::env::var("XDG_CURRENT_DESKTOP") {
        let lower = desktop.to_lowercase();
        if lower.contains("kde") {
            return Compositor::KWin;
        }
        if lower.contains("gnome") {
            return Compositor::GnomeMutter;
        }
    }

    // Slow path: probe D-Bus for well-known services.
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return Compositor::Unknown,
    };

    rt.block_on(async {
        let conn = match zbus::Connection::session().await {
            Ok(c) => c,
            Err(_) => return Compositor::Unknown,
        };
        let dbus = zbus::fdo::DBusProxy::new(&conn).await.ok();
        if let Some(proxy) = &dbus {
            if let Ok(true) = proxy.name_has_owner("org.kde.KWin".try_into().unwrap()).await {
                return Compositor::KWin;
            }
            if let Ok(true) = proxy
                .name_has_owner("org.gnome.Shell".try_into().unwrap())
                .await
            {
                return Compositor::GnomeMutter;
            }
        }
        Compositor::Unknown
    })
}

// ---------------------------------------------------------------------------
// KWin capture backend
// ---------------------------------------------------------------------------

/// CaptureBackend implementation using KWin D-Bus APIs for window enumeration
/// and a persistent PipeWire ScreenCast stream for frame capture.
///
/// Window enumeration: KWin scripting API (`org.kde.kwin.Scripting`) — sees all
/// native Wayland windows, not just XWayland.
/// Frame capture: PipeWire ScreenCast stream provides hardware-accelerated,
/// continuous frames. Grabbed instantly from a cached buffer.
pub struct KwinCaptureBackend {
    platform: PlatformInfo,
    runtime: tokio::runtime::Runtime,
    /// xcap backend used for screen/region capture (portal-based).
    xcap_fallback: super::XcapCaptureBackend,
    /// Maps synthetic numeric window IDs to KWin UUID strings.
    uuid_map: Mutex<HashMap<u32, String>>,
    /// Maps synthetic numeric window IDs to geometry (x, y, w, h).
    geometry_map: Mutex<HashMap<u32, (i32, i32, u32, u32)>>,
    /// Persistent PipeWire capture stream (lazily initialised).
    pw_capture: Mutex<Option<super::pipewire_capture::PipeWireCapture>>,
}

impl KwinCaptureBackend {
    pub fn new(platform: PlatformInfo) -> AppResult<Self> {
        info!(
            "Initializing KWin capture backend for {:?}/{:?}",
            platform.os, platform.display_server
        );
        let xcap_fallback = super::XcapCaptureBackend::new(platform.clone());
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| AppError::Capture(format!("Failed to create tokio runtime: {e}")))?;
        Ok(KwinCaptureBackend {
            platform,
            runtime,
            xcap_fallback,
            uuid_map: Mutex::new(HashMap::new()),
            geometry_map: Mutex::new(HashMap::new()),
            pw_capture: Mutex::new(None),
        })
    }

    // -----------------------------------------------------------------------
    // Monitor enumeration (delegates to xcap)
    // -----------------------------------------------------------------------

    fn enumerate_monitors(&self) -> AppResult<Vec<MonitorInfo>> {
        let monitors = Monitor::all().map_err(|e| {
            AppError::Capture(format!("Failed to enumerate monitors via xcap: {e}"))
        })?;

        let mut infos = Vec::with_capacity(monitors.len());
        for m in &monitors {
            infos.push(MonitorInfo {
                id: m.id().map_err(|e| AppError::Capture(format!("monitor.id(): {e}")))?,
                name: m.name().map_err(|e| AppError::Capture(format!("monitor.name(): {e}")))?,
                friendly_name: m
                    .friendly_name()
                    .map_err(|e| AppError::Capture(format!("monitor.friendly_name(): {e}")))?,
                width: m.width().map_err(|e| AppError::Capture(format!("monitor.width(): {e}")))?,
                height: m
                    .height()
                    .map_err(|e| AppError::Capture(format!("monitor.height(): {e}")))?,
                x: m.x().map_err(|e| AppError::Capture(format!("monitor.x(): {e}")))?,
                y: m.y().map_err(|e| AppError::Capture(format!("monitor.y(): {e}")))?,
                scale_factor: m
                    .scale_factor()
                    .map_err(|e| AppError::Capture(format!("monitor.scale_factor(): {e}")))?,
                is_primary: m
                    .is_primary()
                    .map_err(|e| AppError::Capture(format!("monitor.is_primary(): {e}")))?,
            });
        }
        debug!("Found {} monitors via xcap", infos.len());
        Ok(infos)
    }

    // -----------------------------------------------------------------------
    // Window enumeration via KWin scripting D-Bus
    // -----------------------------------------------------------------------

    fn enumerate_windows(&self) -> AppResult<Vec<WindowInfo>> {
        self.runtime.block_on(self.enumerate_windows_async())
    }

    async fn enumerate_windows_async(&self) -> AppResult<Vec<WindowInfo>> {
        let conn = zbus::Connection::session()
            .await
            .map_err(|e| AppError::Capture(format!("D-Bus session connection failed: {e}")))?;

        // Write the enumeration script to a temp file.
        let script_content = r#"
const clients = workspace.windowList();
for (let i = 0; i < clients.length; i++) {
    const c = clients[i];
    if (c.normalWindow) {
        console.info("SD_WIN|" + c.internalId + "|" + c.caption + "|" + c.resourceClass + "|" + c.desktopFileName + "|" + c.frameGeometry.x + "," + c.frameGeometry.y + "," + c.frameGeometry.width + "," + c.frameGeometry.height + "|" + (c.minimized ? "1" : "0") + "|" + (c.active ? "1" : "0"));
    }
}
"#;

        let tmp_dir = std::env::temp_dir();
        let script_path = tmp_dir.join("sd_kwin_enum.js");
        std::fs::write(&script_path, script_content).map_err(|e| {
            AppError::Capture(format!("Failed to write KWin enum script: {e}"))
        })?;

        let script_path_str = script_path.to_string_lossy().to_string();

        // Call org.kde.kwin.Scripting to load and run the script.
        let scripting_proxy = zbus::Proxy::new(
            &conn,
            "org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting",
        )
        .await
        .map_err(|e| AppError::Capture(format!("Failed to create KWin Scripting proxy: {e}")))?;

        // loadScript(path: String, name: String) -> i32 (script ID)
        let script_id: i32 = scripting_proxy
            .call("loadScript", &(script_path_str.as_str(), "sd_enum"))
            .await
            .map_err(|e| AppError::Capture(format!("KWin loadScript failed: {e}")))?;

        debug!("KWin script loaded with ID {}", script_id);

        // Call start() on the Scripting interface itself — this runs all loaded scripts.
        let _: () = scripting_proxy
            .call("start", &())
            .await
            .map_err(|e| AppError::Capture(format!("KWin script start() failed: {e}")))?;

        // Give KWin a moment to execute the script and flush to journal.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Unload the script.
        let _unload_result: Result<(), _> = scripting_proxy
            .call("unloadScript", &"sd_enum")
            .await;

        // Read output from journalctl.
        let output = tokio::process::Command::new("journalctl")
            .args([
                "--user",
                "-t",
                "kwin_wayland",
                "--since",
                "5 seconds ago",
                "--no-pager",
                "-o",
                "cat",
            ])
            .output()
            .await
            .map_err(|e| AppError::Capture(format!("Failed to run journalctl: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let windows = self.parse_kwin_window_output(&stdout)?;

        // Clean up temp file (best effort).
        let _ = std::fs::remove_file(&script_path);

        debug!("KWin enumeration found {} windows", windows.len());
        Ok(windows)
    }

    /// Parse SD_WIN lines from KWin script output.
    ///
    /// Format: `SD_WIN|uuid|caption|resourceClass|desktopFile|x,y,w,h|minimized|active`
    fn parse_kwin_window_output(&self, output: &str) -> AppResult<Vec<WindowInfo>> {
        let mut windows = Vec::new();
        let mut uuid_map = self.uuid_map.lock().map_err(|e| {
            AppError::Capture(format!("UUID map lock poisoned: {e}"))
        })?;
        uuid_map.clear();
        let mut geom_map = self.geometry_map.lock().map_err(|e| {
            AppError::Capture(format!("Geometry map lock poisoned: {e}"))
        })?;
        geom_map.clear();

        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        for line in output.lines() {
            let line = line.trim();
            if !line.starts_with("SD_WIN|") {
                continue;
            }

            let parts: Vec<&str> = line.splitn(9, '|').collect();
            if parts.len() < 8 {
                warn!("Malformed SD_WIN line (expected 8+ fields): {line}");
                continue;
            }

            let uuid = parts[1].to_string();
            let caption = parts[2].to_string();
            let resource_class = parts[3].to_string();
            let _desktop_file = parts[4];
            let geometry_str = parts[5];
            let minimized_str = parts[6];
            let active_str = parts[7];

            // Parse geometry: x,y,w,h
            let geom_parts: Vec<&str> = geometry_str.split(',').collect();
            if geom_parts.len() < 4 {
                warn!("Malformed geometry in SD_WIN line: {geometry_str}");
                continue;
            }

            let x: i32 = geom_parts[0].parse().unwrap_or(0);
            let y: i32 = geom_parts[1].parse().unwrap_or(0);
            let width: u32 = geom_parts[2].parse().unwrap_or(0);
            let height: u32 = geom_parts[3].parse().unwrap_or(0);

            let is_minimized = minimized_str == "1";
            let is_focused = active_str == "1";

            // Derive a stable numeric ID from the UUID hash so IDs don't shift
            // when windows open/close between enumerations.
            let mut hasher = DefaultHasher::new();
            uuid.hash(&mut hasher);
            let id = (hasher.finish() & 0x7FFFFFFF) as u32; // positive u32
            uuid_map.insert(id, uuid.clone());
            geom_map.insert(id, (x, y, width, height));

            windows.push(WindowInfo {
                id,
                pid: 0, // KWin scripting API does not expose PID directly.
                app_name: resource_class,
                title: caption,
                width,
                height,
                is_minimized,
                is_focused,
                uuid: Some(uuid),
            });
        }

        Ok(windows)
    }

    // -----------------------------------------------------------------------
    // PipeWire capture (lazy init)
    // -----------------------------------------------------------------------

    /// Get or lazily initialise the PipeWire capture stream.
    fn ensure_pw_capture(
        &self,
    ) -> AppResult<std::sync::MutexGuard<'_, Option<super::pipewire_capture::PipeWireCapture>>>
    {
        let mut guard = self.pw_capture.lock().map_err(|e| {
            AppError::Capture(format!("PipeWire capture lock poisoned: {e}"))
        })?;
        if guard.is_none() {
            // Use XDG config dir, falling back to ~/.config/screen-dream.
            let config_dir = dirs::config_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("~/.config"))
                .join("screen-dream");
            info!("Lazily initialising PipeWire capture (config_dir={})", config_dir.display());
            let capture = super::pipewire_capture::PipeWireCapture::start(&config_dir)?;
            *guard = Some(capture);
        }
        Ok(guard)
    }

    // -----------------------------------------------------------------------
    // Frame capture via org.kde.KWin.ScreenShot2
    // -----------------------------------------------------------------------

    fn capture_screen(&self, source: &ScreenSource) -> AppResult<CapturedFrame> {
        self.runtime.block_on(self.capture_screen_async(source))
    }

    async fn capture_screen_async(&self, source: &ScreenSource) -> AppResult<CapturedFrame> {
        // Find the monitor name by ID.
        let monitors = Monitor::all().map_err(|e| {
            AppError::Capture(format!("Failed to enumerate monitors: {e}"))
        })?;

        let monitor = monitors
            .iter()
            .find(|m| m.id().ok() == Some(source.monitor_id))
            .ok_or_else(|| {
                AppError::Capture(format!("Monitor with ID {} not found", source.monitor_id))
            })?;

        let screen_name = monitor
            .name()
            .map_err(|e| AppError::Capture(format!("monitor.name(): {e}")))?;

        let conn = zbus::Connection::session()
            .await
            .map_err(|e| AppError::Capture(format!("D-Bus session connection failed: {e}")))?;

        let options: HashMap<String, Value<'_>> = HashMap::new();
        self.capture_via_screenshot2(&conn, "CaptureScreen", &screen_name, options)
            .await
    }

    fn capture_window(&self, source: &WindowSource) -> AppResult<CapturedFrame> {
        self.runtime.block_on(self.capture_window_async(source))
    }

    async fn capture_window_async(&self, source: &WindowSource) -> AppResult<CapturedFrame> {
        // Resolve the UUID: prefer the uuid field on the source, else look up from map.
        let uuid = if let Some(ref uuid) = source.uuid {
            uuid.clone()
        } else {
            let map = self.uuid_map.lock().map_err(|e| {
                AppError::Capture(format!("UUID map lock poisoned: {e}"))
            })?;
            map.get(&source.window_id)
                .cloned()
                .ok_or_else(|| {
                    AppError::Capture(format!(
                        "No KWin UUID found for window ID {}. Call enumerate_sources first.",
                        source.window_id
                    ))
                })?
        };

        let conn = zbus::Connection::session()
            .await
            .map_err(|e| AppError::Capture(format!("D-Bus session connection failed: {e}")))?;

        let options: HashMap<String, Value<'_>> = HashMap::new();
        self.capture_via_screenshot2(&conn, "CaptureWindow", &uuid, options)
            .await
    }

    fn capture_region(&self, source: &RegionSource) -> AppResult<CapturedFrame> {
        self.runtime.block_on(self.capture_region_async(source))
    }

    async fn capture_region_async(&self, source: &RegionSource) -> AppResult<CapturedFrame> {
        let conn = zbus::Connection::session()
            .await
            .map_err(|e| AppError::Capture(format!("D-Bus session connection failed: {e}")))?;

        let proxy = zbus::Proxy::new(
            &conn,
            "org.kde.KWin",
            "/org/kde/KWin/ScreenShot2",
            "org.kde.KWin.ScreenShot2",
        )
        .await
        .map_err(|e| {
            AppError::Capture(format!("Failed to create ScreenShot2 proxy: {e}"))
        })?;

        let (read_raw, write_raw) = nix::unistd::pipe()
            .map_err(|e| AppError::Capture(format!("Failed to create pipe: {e}")))?;

        let write_fd = Fd::from(write_raw);

        let options: HashMap<String, Value<'_>> = HashMap::new();

        let reply: HashMap<String, OwnedValue> = proxy
            .call(
                "CaptureArea",
                &(
                    source.x,
                    source.y,
                    source.width,
                    source.height,
                    options,
                    write_fd,
                ),
            )
            .await
            .map_err(|e| {
                AppError::Capture(format!("KWin CaptureArea failed: {e}"))
            })?;

        // The write end has been passed to KWin; drop our reference.
        // (Fd::from(OwnedFd) takes ownership, so write_raw is already consumed.)

        read_screenshot_from_fd(read_raw, &reply)
    }

    /// Common helper for CaptureScreen and CaptureWindow.
    /// Both take (handle_or_name: String, options: a{sv}, fd: h).
    async fn capture_via_screenshot2(
        &self,
        conn: &zbus::Connection,
        method: &str,
        handle: &str,
        options: HashMap<String, Value<'_>>,
    ) -> AppResult<CapturedFrame> {
        let proxy = zbus::Proxy::new(
            conn,
            "org.kde.KWin",
            "/org/kde/KWin/ScreenShot2",
            "org.kde.KWin.ScreenShot2",
        )
        .await
        .map_err(|e| {
            AppError::Capture(format!("Failed to create ScreenShot2 proxy: {e}"))
        })?;

        let (read_raw, write_raw) = nix::unistd::pipe()
            .map_err(|e| AppError::Capture(format!("Failed to create pipe: {e}")))?;

        let write_fd = Fd::from(write_raw);

        let reply: HashMap<String, OwnedValue> = proxy
            .call(method, &(handle, options, write_fd))
            .await
            .map_err(|e| {
                AppError::Capture(format!("KWin {method} failed: {e}"))
            })?;

        // The write end has been passed to KWin via D-Bus fd passing.
        // read_raw is our read end of the pipe.

        read_screenshot_from_fd(read_raw, &reply)
    }
}

// ---------------------------------------------------------------------------
// Pipe reading helper
// ---------------------------------------------------------------------------

/// Read pixel data from a pipe fd and interpret using the D-Bus response metadata.
fn read_screenshot_from_fd(
    read_fd: std::os::unix::io::OwnedFd,
    reply: &HashMap<String, OwnedValue>,
) -> AppResult<CapturedFrame> {
    let width = extract_u32(reply, "width")?;
    let height = extract_u32(reply, "height")?;
    let stride = extract_u32(reply, "stride")?;
    let format = extract_u32(reply, "format").unwrap_or(0);

    debug!(
        "KWin screenshot response: {}x{}, stride={}, format={}",
        width, height, stride, format
    );

    let expected_size = (stride * height) as usize;

    // Convert OwnedFd to File for reading.
    let raw_fd = std::os::unix::io::IntoRawFd::into_raw_fd(read_fd);
    let mut file = unsafe { std::fs::File::from_raw_fd(raw_fd) };
    let mut raw_data = Vec::with_capacity(expected_size);
    file.read_to_end(&mut raw_data).map_err(|e| {
        AppError::Capture(format!("Failed to read screenshot data from pipe: {e}"))
    })?;

    if raw_data.len() < expected_size {
        return Err(AppError::Capture(format!(
            "Incomplete screenshot data: got {} bytes, expected {}",
            raw_data.len(),
            expected_size
        )));
    }

    // KWin ScreenShot2 format values (from wl_shm):
    //   0 = ARGB8888 (or BGRA in memory on little-endian)
    //   1 = XRGB8888 (BGRX)
    // We need to convert to RGBA.
    let rgba_data = convert_to_rgba(&raw_data, width, height, stride, format);

    Ok(CapturedFrame {
        data: rgba_data,
        width,
        height,
    })
}

/// Extract a u32 value from the D-Bus response map.
fn extract_u32(map: &HashMap<String, OwnedValue>, key: &str) -> AppResult<u32> {
    let val = map
        .get(key)
        .ok_or_else(|| AppError::Capture(format!("Missing '{key}' in ScreenShot2 response")))?;

    // The value might be u32, i32, or u64 depending on the KWin version.
    if let Ok(v) = <u32>::try_from(val) {
        return Ok(v);
    }
    if let Ok(v) = <i32>::try_from(val) {
        return Ok(v as u32);
    }
    if let Ok(v) = <u64>::try_from(val) {
        return Ok(v as u32);
    }

    Err(AppError::Capture(format!(
        "Cannot convert '{key}' value to u32: {:?}",
        val
    )))
}

/// Convert ARGB8888/XRGB8888 pixel data to RGBA8888.
///
/// KWin uses wl_shm format ARGB8888 (format=0) which on little-endian is
/// stored as B, G, R, A bytes in memory. We reorder to R, G, B, A.
fn convert_to_rgba(data: &[u8], width: u32, height: u32, stride: u32, format: u32) -> Vec<u8> {
    let pixel_count = (width * height) as usize;
    let mut rgba = Vec::with_capacity(pixel_count * 4);

    for y in 0..height {
        let row_start = (y * stride) as usize;
        for x in 0..width {
            let offset = row_start + (x as usize) * 4;
            if offset + 3 >= data.len() {
                // Pad with transparent black if data is short.
                rgba.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            // Memory layout for ARGB8888 on little-endian: [B, G, R, A]
            let b = data[offset];
            let g = data[offset + 1];
            let r = data[offset + 2];
            let a = if format == 1 { 255 } else { data[offset + 3] }; // XRGB has no alpha

            rgba.push(r);
            rgba.push(g);
            rgba.push(b);
            rgba.push(a);
        }
    }

    rgba
}

// ---------------------------------------------------------------------------
// CaptureBackend trait implementation
// ---------------------------------------------------------------------------

impl CaptureBackend for KwinCaptureBackend {
    fn enumerate_sources(&self) -> AppResult<AvailableSources> {
        let monitors = self.enumerate_monitors()?;

        let (windows, windows_unavailable, windows_unavailable_reason) =
            match self.enumerate_windows() {
                Ok(wins) => (wins, false, None),
                Err(e) => {
                    warn!("KWin window enumeration failed: {e}");
                    (
                        vec![],
                        true,
                        Some(format!("KWin window enumeration failed: {e}")),
                    )
                }
            };

        Ok(AvailableSources {
            monitors,
            windows,
            windows_unavailable,
            windows_unavailable_reason,
        })
    }

    fn capture_frame(&self, source: &CaptureSource) -> AppResult<CapturedFrame> {
        // Ensure the PipeWire capture stream is running.
        let pw_guard = self.ensure_pw_capture()?;
        let pw = pw_guard.as_ref().ok_or_else(|| {
            AppError::Capture("PipeWire capture not initialised".to_string())
        })?;

        match source {
            CaptureSource::Screen(_) => {
                debug!("Capturing screen via PipeWire stream");
                pw.grab_frame()
            }
            CaptureSource::Window(w) => {
                let geom_map = self.geometry_map.lock().map_err(|e| {
                    AppError::Capture(format!("Geometry map lock poisoned: {e}"))
                })?;
                info!(
                    "Window capture: looking up window_id={} in geometry_map ({} entries: {:?})",
                    w.window_id, geom_map.len(), geom_map
                );
                if let Some(&(x, y, width, height)) = geom_map.get(&w.window_id) {
                    if width == 0 || height == 0 {
                        return Err(AppError::Capture(format!(
                            "Window {} has zero-size geometry ({x},{y} {width}x{height}). \
                             The window may be minimized or not yet enumerated.",
                            w.window_id
                        )));
                    }
                    debug!(
                        "Capturing window {} via PipeWire + crop ({x},{y} {width}x{height})",
                        w.window_id
                    );
                    pw.grab_frame_cropped(x, y, width, height)
                } else {
                    Err(AppError::Capture(format!(
                        "Window ID {} not found in geometry map. Try refreshing sources first.",
                        w.window_id
                    )))
                }
            }
            CaptureSource::Region(r) => {
                debug!(
                    "Capturing region via PipeWire + crop ({},{} {}x{})",
                    r.x, r.y, r.width, r.height
                );
                pw.grab_frame_cropped(r.x, r.y, r.width, r.height)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_compositor() {
        // Just verify it doesn't panic.
        let _compositor = detect_compositor();
    }

    #[test]
    fn test_parse_kwin_output() {
        let platform = PlatformInfo::detect();
        let backend = KwinCaptureBackend::new(platform).unwrap();

        let output = r#"
some random log line
SD_WIN|{abc-123}|Firefox|firefox|org.mozilla.firefox|100,200,1920,1080|0|1
SD_WIN|{def-456}|Terminal|org.kde.konsole|org.kde.konsole|0,0,800,600|1|0
another random line
"#;

        let windows = backend.parse_kwin_window_output(output).unwrap();
        assert_eq!(windows.len(), 2);

        assert_eq!(windows[0].title, "Firefox");
        assert_eq!(windows[0].app_name, "firefox");
        assert_eq!(windows[0].width, 1920);
        assert_eq!(windows[0].height, 1080);
        assert!(!windows[0].is_minimized);
        assert!(windows[0].is_focused);
        assert_eq!(windows[0].uuid.as_deref(), Some("{abc-123}"));

        assert_eq!(windows[1].title, "Terminal");
        assert!(windows[1].is_minimized);
        assert!(!windows[1].is_focused);
    }

    #[test]
    fn test_convert_to_rgba() {
        // ARGB8888 little-endian: [B, G, R, A]
        let data = vec![
            0x00, 0x80, 0xFF, 0xCC, // pixel: B=0, G=128, R=255, A=204
        ];
        let rgba = convert_to_rgba(&data, 1, 1, 4, 0);
        assert_eq!(rgba, vec![0xFF, 0x80, 0x00, 0xCC]); // R=255, G=128, B=0, A=204
    }

    #[test]
    fn test_convert_xrgb_to_rgba() {
        // XRGB8888 (format=1): alpha should be forced to 255.
        let data = vec![0x10, 0x20, 0x30, 0x00]; // B=16, G=32, R=48, X=0
        let rgba = convert_to_rgba(&data, 1, 1, 4, 1);
        assert_eq!(rgba, vec![0x30, 0x20, 0x10, 0xFF]); // R=48, G=32, B=16, A=255
    }
}
