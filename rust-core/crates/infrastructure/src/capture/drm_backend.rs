//! DRM/KMS framebuffer capture backend.
//!
//! Uses a setcap helper binary (`drm_capture_helper`) to read GPU framebuffers.
//! The helper must have `CAP_SYS_ADMIN`:
//!
//!     sudo setcap cap_sys_admin+ep ./drm_capture_helper
//!
//! This gives instant, native-resolution (4K) screen capture with zero portal
//! dialogs and no compositor dependencies, without requiring the main binary
//! to hold `CAP_SYS_ADMIN` (which gets stripped by the dynamic linker when
//! RUNPATH points to non-standard directories).

use std::path::{Path, PathBuf};
use std::process::Command;

use domain::capture::CapturedFrame;
use domain::error::{AppError, AppResult};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// DRM format codes (fourcc) — used for pixel conversion
// ---------------------------------------------------------------------------

const DRM_FORMAT_XRGB8888: u32 = 0x34325258; // XR24
const DRM_FORMAT_ARGB8888: u32 = 0x34325241; // AR24
const DRM_FORMAT_XBGR8888: u32 = 0x34324258; // XB24
const DRM_FORMAT_ABGR8888: u32 = 0x34324241; // AB24

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Information about a CRTC (display output) and its associated plane.
#[derive(Debug, Clone)]
pub struct CrtcMonitor {
    pub crtc_id: u32,
    pub plane_id: u32,
    pub fb_id: u32,
    pub width: u32,
    pub height: u32,
    /// Connector name like "DP-1", "HDMI-A-1", etc.
    pub connector_name: String,
    /// CRTC x/y position in the virtual screen space (for multi-monitor).
    pub crtc_x: u32,
    pub crtc_y: u32,
    /// Pixel format fourcc string from the helper (e.g. "AR24", "AB30", "XR24").
    fmt_code: String,
}

/// DRM/KMS framebuffer capture backend.
///
/// Delegates to the `drm_capture_helper` setcap binary for all privileged
/// DRM operations (opening device, reading framebuffers).
pub struct DrmCaptureBackend {
    /// Path to the helper binary.
    helper_path: PathBuf,
    /// Which DRM card (e.g., "/dev/dri/card2").
    card_path: String,
    /// Mapping of CRTC IDs to monitor info.
    crtc_monitors: Vec<CrtcMonitor>,
}

impl DrmCaptureBackend {
    /// Open the DRM device (via the helper) and enumerate active planes/CRTCs.
    ///
    /// Tries all /dev/dri/card* devices, picking the one with active planes.
    pub fn new() -> AppResult<Self> {
        let helper_path = Self::find_helper()?;
        info!("DRM: using helper at {:?}", helper_path);

        let (card_path, crtc_monitors) = Self::find_active_card(&helper_path)?;
        info!("DRM: using card {}", card_path);
        info!("DRM: found {} active CRTCs", crtc_monitors.len());
        for cm in &crtc_monitors {
            info!(
                "  CRTC {} (plane {}, fb {}): {}x{} connector={} fmt={}",
                cm.crtc_id, cm.plane_id, cm.fb_id, cm.width, cm.height,
                cm.connector_name, cm.fmt_code
            );
        }

        Ok(DrmCaptureBackend {
            helper_path,
            card_path,
            crtc_monitors,
        })
    }

    /// Check if DRM capture is available (helper binary exists and can list planes).
    pub fn is_available() -> bool {
        let helper_path = match Self::find_helper() {
            Ok(p) => p,
            Err(_) => return false,
        };

        // Try listing planes on any card to verify the helper works
        // (has CAP_SYS_ADMIN and can access DRM).
        let dri_path = Path::new("/dev/dri");
        if !dri_path.exists() {
            return false;
        }

        let cards: Vec<PathBuf> = match std::fs::read_dir(dri_path) {
            Ok(entries) => entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_name()
                        .to_str()
                        .map(|n| n.starts_with("card"))
                        .unwrap_or(false)
                })
                .map(|e| e.path())
                .collect(),
            Err(_) => return false,
        };

        for card in &cards {
            let card_str = card.to_string_lossy();
            let output = Command::new(&helper_path)
                .arg("--list")
                .arg(card_str.as_ref())
                .output();

            match output {
                Ok(o) if o.status.success() => {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    if stdout.lines().any(|l| l.starts_with("PLANE:")) {
                        return true;
                    }
                }
                _ => continue,
            }
        }

        false
    }

    /// Capture the framebuffer for a specific CRTC/plane.
    /// Returns raw RGBA pixel data at native resolution.
    pub fn capture_crtc(&self, crtc_id: u32) -> AppResult<CapturedFrame> {
        let cm = self
            .crtc_monitors
            .iter()
            .find(|cm| cm.crtc_id == crtc_id)
            .ok_or_else(|| {
                AppError::Capture(format!("CRTC {} not found in active monitors", crtc_id))
            })?;

        self.capture_plane_fb(cm)
    }

    /// Capture ALL screens as separate frames.
    /// Returns a vec of (crtc_id, CapturedFrame) pairs.
    pub fn capture_all_screens(&self) -> AppResult<Vec<(u32, CapturedFrame)>> {
        let mut results = Vec::with_capacity(self.crtc_monitors.len());
        for cm in &self.crtc_monitors {
            let frame = self.capture_plane_fb(cm)?;
            results.push((cm.crtc_id, frame));
        }
        Ok(results)
    }

    /// Get the list of active CRTC monitors.
    pub fn crtc_monitors(&self) -> &[CrtcMonitor] {
        &self.crtc_monitors
    }

    /// Re-enumerate planes/CRTCs (e.g. after hotplug).
    pub fn refresh(&mut self) -> AppResult<()> {
        let monitors = Self::run_list_helper(&self.helper_path, &self.card_path)?;
        self.crtc_monitors = monitors;
        info!("DRM: refreshed, {} active CRTCs", self.crtc_monitors.len());
        Ok(())
    }

    /// Find the best matching CRTC for a given monitor name or index.
    /// Tries connector_name match first, then falls back to index.
    pub fn find_crtc_for_monitor(&self, monitor_name: &str, monitor_index: usize) -> Option<&CrtcMonitor> {
        // Try exact connector name match (e.g., "DP-1" matches connector "DP-1")
        if let Some(cm) = self.crtc_monitors.iter().find(|cm| cm.connector_name == monitor_name) {
            return Some(cm);
        }
        // Try partial match (e.g., "DP-1" in "DP-1 (LG ...)")
        if let Some(cm) = self.crtc_monitors.iter().find(|cm| {
            cm.connector_name.contains(monitor_name) || monitor_name.contains(&cm.connector_name)
        }) {
            return Some(cm);
        }
        // Fall back to index
        self.crtc_monitors.get(monitor_index)
    }

    // -----------------------------------------------------------------------
    // Internal: find the helper binary
    // -----------------------------------------------------------------------

    fn find_helper() -> AppResult<PathBuf> {
        // 1. Next to the current executable
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let candidate = dir.join("drm_capture_helper");
                if candidate.is_file() {
                    return Ok(candidate);
                }
            }
        }

        // 2. Current working directory
        let cwd_candidate = PathBuf::from("./drm_capture_helper");
        if cwd_candidate.is_file() {
            return Ok(std::fs::canonicalize(&cwd_candidate).unwrap_or(cwd_candidate));
        }

        // 3. In PATH
        if let Ok(path) = which::which("drm_capture_helper") {
            return Ok(path);
        }

        Err(AppError::Capture(
            "DRM: drm_capture_helper not found. Place it next to the executable, \
             in the current directory, or in PATH."
                .to_string(),
        ))
    }

    // -----------------------------------------------------------------------
    // Internal: find the right DRM card
    // -----------------------------------------------------------------------

    fn find_active_card(helper_path: &Path) -> AppResult<(String, Vec<CrtcMonitor>)> {
        let dri_path = Path::new("/dev/dri");
        if !dri_path.exists() {
            return Err(AppError::Capture(
                "DRM: /dev/dri does not exist".to_string(),
            ));
        }

        let mut cards: Vec<PathBuf> = std::fs::read_dir(dri_path)
            .map_err(|e| AppError::Capture(format!("Failed to read /dev/dri: {e}")))?
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .map(|n| n.starts_with("card"))
                    .unwrap_or(false)
            })
            .map(|entry| entry.path())
            .collect();

        cards.sort();

        let mut best: Option<(String, Vec<CrtcMonitor>)> = None;

        for card in &cards {
            let card_str = card.to_string_lossy().to_string();
            match Self::run_list_helper(helper_path, &card_str) {
                Ok(monitors) if !monitors.is_empty() => {
                    let count = monitors.len();
                    debug!("DRM: {} has {} active planes", card_str, count);
                    match &best {
                        Some((_, best_monitors)) if count <= best_monitors.len() => {}
                        _ => {
                            best = Some((card_str, monitors));
                        }
                    }
                }
                Ok(_) => {
                    debug!("DRM: {} has 0 active planes", card_str);
                }
                Err(e) => {
                    debug!("DRM: {} failed: {}", card_str, e);
                }
            }
        }

        best.ok_or_else(|| {
            AppError::Capture(
                "DRM: no cards with active framebuffers found. \
                 Check that drm_capture_helper has CAP_SYS_ADMIN."
                    .to_string(),
            )
        })
    }

    // -----------------------------------------------------------------------
    // Internal: run helper --list and parse output
    // -----------------------------------------------------------------------

    /// Run `drm_capture_helper --list <card>` and parse the output into CrtcMonitor entries.
    ///
    /// Expected output format (one line per active plane):
    ///   PLANE:51:CRTC:62:FB:153:SIZE:3840x2160:POS:0,0:FMT:AB4H
    fn run_list_helper(helper_path: &Path, card_path: &str) -> AppResult<Vec<CrtcMonitor>> {
        let output = Command::new(helper_path)
            .arg("--list")
            .arg(card_path)
            .output()
            .map_err(|e| {
                AppError::Capture(format!(
                    "DRM: failed to run helper {:?} --list {}: {}",
                    helper_path, card_path, e
                ))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::Capture(format!(
                "DRM: helper --list failed (status={}): {}",
                output.status, stderr.trim()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut monitors = Vec::new();
        let mut seen_crtcs = std::collections::HashSet::new();

        for line in stdout.lines() {
            let line = line.trim();
            if !line.starts_with("PLANE:") {
                continue;
            }

            match Self::parse_list_line(line) {
                Ok(cm) => {
                    // Skip duplicate CRTCs (multiple planes per CRTC)
                    if seen_crtcs.insert(cm.crtc_id) {
                        monitors.push(cm);
                    }
                }
                Err(e) => {
                    warn!("DRM: failed to parse helper output line {:?}: {}", line, e);
                }
            }
        }

        Ok(monitors)
    }

    /// Parse a single line from the helper's --list output.
    ///
    /// Format: `PLANE:<id>:CRTC:<id>:FB:<id>:SIZE:<W>x<H>:POS:<X>,<Y>:FMT:<fourcc>`
    fn parse_list_line(line: &str) -> AppResult<CrtcMonitor> {
        // Split by ':' — fields come in key:value pairs.
        let parts: Vec<&str> = line.split(':').collect();

        // We need at least: PLANE, <id>, CRTC, <id>, FB, <id>, SIZE, <WxH>, POS, <X,Y>, FMT, <fmt>
        // That's 12 fields.
        if parts.len() < 12 {
            return Err(AppError::Capture(format!(
                "DRM: malformed list line (expected 12+ fields): {:?}",
                line
            )));
        }

        // Build a map of key -> value from consecutive pairs.
        let mut map = std::collections::HashMap::new();
        let mut i = 0;
        while i + 1 < parts.len() {
            map.insert(parts[i], parts[i + 1]);
            i += 2;
        }

        let plane_id: u32 = map
            .get("PLANE")
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| AppError::Capture(format!("DRM: missing PLANE in {:?}", line)))?;

        let crtc_id: u32 = map
            .get("CRTC")
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| AppError::Capture(format!("DRM: missing CRTC in {:?}", line)))?;

        let fb_id: u32 = map
            .get("FB")
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| AppError::Capture(format!("DRM: missing FB in {:?}", line)))?;

        let size_str = map
            .get("SIZE")
            .ok_or_else(|| AppError::Capture(format!("DRM: missing SIZE in {:?}", line)))?;
        let (width, height) = {
            let mut split = size_str.split('x');
            let w: u32 = split
                .next()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| AppError::Capture(format!("DRM: bad SIZE width in {:?}", line)))?;
            let h: u32 = split
                .next()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| AppError::Capture(format!("DRM: bad SIZE height in {:?}", line)))?;
            (w, h)
        };

        let pos_str = map
            .get("POS")
            .ok_or_else(|| AppError::Capture(format!("DRM: missing POS in {:?}", line)))?;
        let (crtc_x, crtc_y) = {
            let mut split = pos_str.split(',');
            let x: u32 = split.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let y: u32 = split.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            (x, y)
        };

        let fmt_code = map
            .get("FMT")
            .map(|s| s.to_string())
            .unwrap_or_else(|| "XR24".to_string());

        // Build a connector name from the CRTC id for now.
        // The helper doesn't provide connector names, so use CRTC-based naming.
        let connector_name = format!("CRTC-{}", crtc_id);

        Ok(CrtcMonitor {
            crtc_id,
            plane_id,
            fb_id,
            width,
            height,
            connector_name,
            crtc_x,
            crtc_y,
            fmt_code,
        })
    }

    // -----------------------------------------------------------------------
    // Internal: capture a framebuffer from a plane via the helper
    // -----------------------------------------------------------------------

    fn capture_plane_fb(&self, cm: &CrtcMonitor) -> AppResult<CapturedFrame> {
        debug!(
            "DRM: capturing plane {} on card {} (CRTC {} fb {} fmt {})",
            cm.plane_id, self.card_path, cm.crtc_id, cm.fb_id, cm.fmt_code
        );

        let output = Command::new(&self.helper_path)
            .arg(&self.card_path)
            .arg(cm.plane_id.to_string())
            .output()
            .map_err(|e| {
                AppError::Capture(format!(
                    "DRM: failed to run helper for plane {}: {}",
                    cm.plane_id, e
                ))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::Capture(format!(
                "DRM: helper capture failed for plane {} (status={}): {}",
                cm.plane_id, output.status, stderr.trim()
            )));
        }

        let raw = &output.stdout;

        // Header: 12 bytes = width(u32 LE) + height(u32 LE) + pitch(u32 LE)
        if raw.len() < 12 {
            return Err(AppError::Capture(format!(
                "DRM: helper output too short ({} bytes, expected at least 12)",
                raw.len()
            )));
        }

        let width = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
        let height = u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]);
        let pitch = u32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]);

        let pixel_data = &raw[12..];
        let expected_size = (pitch as usize) * (height as usize);

        if pixel_data.len() < expected_size {
            return Err(AppError::Capture(format!(
                "DRM: helper output has {} pixel bytes, expected {} ({}x{} pitch={})",
                pixel_data.len(),
                expected_size,
                width,
                height,
                pitch
            )));
        }

        // Determine pixel format from the fourcc string the helper reported.
        let pixel_format = fourcc_str_to_format(&cm.fmt_code);

        let rgba = convert_drm_to_rgba(pixel_data, width, height, pitch, pixel_format);

        Ok(CapturedFrame {
            data: rgba,
            width,
            height,
        })
    }
}

// ---------------------------------------------------------------------------
// Fourcc string to DRM format constant
// ---------------------------------------------------------------------------

/// Convert a 4-char fourcc string (e.g. "AR24", "XR24", "AB4H", "AB30") to
/// a DRM_FORMAT_* constant. Falls back to XRGB8888 for unknown formats.
fn fourcc_str_to_format(s: &str) -> u32 {
    match s {
        "XR24" => DRM_FORMAT_XRGB8888,
        "AR24" => DRM_FORMAT_ARGB8888,
        "XB24" => DRM_FORMAT_XBGR8888,
        "AB24" => DRM_FORMAT_ABGR8888,
        // AB4H and AB30 are 10-bit formats. The helper outputs raw pixel data
        // as-is. For these, the conversion function will treat them as XRGB8888
        // (best-effort). A proper 10-bit path would need more work.
        _ => {
            // For unknown formats, treat as XRGB8888 (BGRA in memory).
            // This is the most common framebuffer format.
            DRM_FORMAT_XRGB8888
        }
    }
}

// ---------------------------------------------------------------------------
// Pixel format conversion
// ---------------------------------------------------------------------------

/// Convert DRM pixel data to RGBA8888.
///
/// DRM pixel formats on little-endian:
///   - XRGB8888 (0x34325258): memory layout [B, G, R, X] per pixel
///   - ARGB8888 (0x34325241): memory layout [B, G, R, A]
///   - XBGR8888 (0x34324258): memory layout [R, G, B, X]
///   - ABGR8888 (0x34324241): memory layout [R, G, B, A]
fn convert_drm_to_rgba(
    data: &[u8],
    width: u32,
    height: u32,
    pitch: u32,
    pixel_format: u32,
) -> Vec<u8> {
    let pixel_count = (width as usize) * (height as usize);
    let mut rgba = Vec::with_capacity(pixel_count * 4);

    for y in 0..height {
        let row_start = (y * pitch) as usize;
        for x in 0..width {
            let offset = row_start + (x as usize) * 4;
            if offset + 3 >= data.len() {
                rgba.extend_from_slice(&[0, 0, 0, 255]);
                continue;
            }

            let (r, g, b, a) = match pixel_format {
                DRM_FORMAT_XRGB8888 => {
                    // Memory: [B, G, R, X]
                    (data[offset + 2], data[offset + 1], data[offset], 255u8)
                }
                DRM_FORMAT_ARGB8888 => {
                    // Memory: [B, G, R, A]
                    (data[offset + 2], data[offset + 1], data[offset], data[offset + 3])
                }
                DRM_FORMAT_XBGR8888 => {
                    // Memory: [R, G, B, X]
                    (data[offset], data[offset + 1], data[offset + 2], 255u8)
                }
                DRM_FORMAT_ABGR8888 => {
                    // Memory: [R, G, B, A]
                    (data[offset], data[offset + 1], data[offset + 2], data[offset + 3])
                }
                _ => {
                    // Unknown format — assume XRGB8888 layout
                    (data[offset + 2], data[offset + 1], data[offset], 255u8)
                }
            };

            rgba.push(r);
            rgba.push(g);
            rgba.push(b);
            rgba.push(a);
        }
    }

    rgba
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_xrgb8888() {
        // XRGB8888 LE: [B, G, R, X]
        let data = vec![0x10, 0x20, 0x30, 0x00]; // B=16, G=32, R=48, X=0
        let rgba = convert_drm_to_rgba(&data, 1, 1, 4, DRM_FORMAT_XRGB8888);
        assert_eq!(rgba, vec![0x30, 0x20, 0x10, 0xFF]); // R=48, G=32, B=16, A=255
    }

    #[test]
    fn test_convert_argb8888() {
        // ARGB8888 LE: [B, G, R, A]
        let data = vec![0x00, 0x80, 0xFF, 0xCC];
        let rgba = convert_drm_to_rgba(&data, 1, 1, 4, DRM_FORMAT_ARGB8888);
        assert_eq!(rgba, vec![0xFF, 0x80, 0x00, 0xCC]);
    }

    #[test]
    fn test_convert_xbgr8888() {
        // XBGR8888 LE: [R, G, B, X]
        let data = vec![0xFF, 0x80, 0x00, 0x00];
        let rgba = convert_drm_to_rgba(&data, 1, 1, 4, DRM_FORMAT_XBGR8888);
        assert_eq!(rgba, vec![0xFF, 0x80, 0x00, 0xFF]);
    }

    #[test]
    fn test_convert_with_pitch() {
        // 2 pixels wide, pitch=12 (extra 4 bytes padding per row)
        let data = vec![
            0x10, 0x20, 0x30, 0x00, // pixel (0,0): B=16, G=32, R=48
            0x40, 0x50, 0x60, 0x00, // pixel (1,0): B=64, G=80, R=96
            0x00, 0x00, 0x00, 0x00, // padding
        ];
        let rgba = convert_drm_to_rgba(&data, 2, 1, 12, DRM_FORMAT_XRGB8888);
        assert_eq!(
            rgba,
            vec![
                0x30, 0x20, 0x10, 0xFF, // pixel (0,0)
                0x60, 0x50, 0x40, 0xFF, // pixel (1,0)
            ]
        );
    }

    #[test]
    fn test_parse_list_line() {
        let line = "PLANE:51:CRTC:62:FB:153:SIZE:3840x2160:POS:0,0:FMT:AB4H";
        let cm = DrmCaptureBackend::parse_list_line(line).unwrap();
        assert_eq!(cm.plane_id, 51);
        assert_eq!(cm.crtc_id, 62);
        assert_eq!(cm.fb_id, 153);
        assert_eq!(cm.width, 3840);
        assert_eq!(cm.height, 2160);
        assert_eq!(cm.crtc_x, 0);
        assert_eq!(cm.crtc_y, 0);
        assert_eq!(cm.fmt_code, "AB4H");
    }

    #[test]
    fn test_parse_list_line_with_position() {
        let line = "PLANE:70:CRTC:81:FB:162:SIZE:1920x1080:POS:3840,0:FMT:XR24";
        let cm = DrmCaptureBackend::parse_list_line(line).unwrap();
        assert_eq!(cm.plane_id, 70);
        assert_eq!(cm.crtc_id, 81);
        assert_eq!(cm.fb_id, 162);
        assert_eq!(cm.width, 1920);
        assert_eq!(cm.height, 1080);
        assert_eq!(cm.crtc_x, 3840);
        assert_eq!(cm.crtc_y, 0);
        assert_eq!(cm.fmt_code, "XR24");
    }

    #[test]
    fn test_fourcc_str_to_format() {
        assert_eq!(fourcc_str_to_format("XR24"), DRM_FORMAT_XRGB8888);
        assert_eq!(fourcc_str_to_format("AR24"), DRM_FORMAT_ARGB8888);
        assert_eq!(fourcc_str_to_format("XB24"), DRM_FORMAT_XBGR8888);
        assert_eq!(fourcc_str_to_format("AB24"), DRM_FORMAT_ABGR8888);
        // Unknown formats fall back to XRGB8888
        assert_eq!(fourcc_str_to_format("AB30"), DRM_FORMAT_XRGB8888);
    }

    #[test]
    fn test_is_available() {
        // Just verify it doesn't panic. On CI this will likely return false.
        let _avail = DrmCaptureBackend::is_available();
    }
}
