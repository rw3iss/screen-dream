//! DRM/KMS framebuffer capture backend.
//!
//! Reads GPU framebuffers directly via DRM ioctls, bypassing PipeWire/Spectacle
//! entirely. Requires `CAP_SYS_ADMIN` on the binary:
//!
//!     sudo setcap cap_sys_admin+ep ./ScreenDream
//!
//! This gives instant, native-resolution (4K) screen capture with zero portal
//! dialogs and no compositor dependencies.

use std::collections::HashMap;
use std::ffi::CString;
use std::os::unix::io::RawFd;
use std::path::{Path, PathBuf};

use domain::capture::CapturedFrame;
use domain::error::{AppError, AppResult};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// DRM FFI bindings (subset needed for framebuffer capture)
// ---------------------------------------------------------------------------

// DRM format codes (fourcc)
const DRM_FORMAT_XRGB8888: u32 = 0x34325258; // XR24
const DRM_FORMAT_ARGB8888: u32 = 0x34325241; // AR24
const DRM_FORMAT_XBGR8888: u32 = 0x34324258; // XB24
const DRM_FORMAT_ABGR8888: u32 = 0x34324241; // AB24

/// Linear modifier — mmap is safe.
const DRM_FORMAT_MOD_LINEAR: u64 = 0;
/// Invalid modifier — treat as "unknown", try mmap anyway.
const DRM_FORMAT_MOD_INVALID: u64 = 0x00ffffffffffffff;

/// Flag for drmPrimeHandleToFD.
const DRM_CLOEXEC: u32 = 0x02; // O_CLOEXEC equivalent for DRM

// DRM mode resource structures (matching libdrm headers)

#[repr(C)]
#[derive(Debug)]
struct DrmModeRes {
    count_fbs: i32,
    fbs: *mut u32,
    count_crtcs: i32,
    crtcs: *mut u32,
    count_connectors: i32,
    connectors: *mut u32,
    count_encoders: i32,
    encoders: *mut u32,
    min_width: u32,
    max_width: u32,
    min_height: u32,
    max_height: u32,
}

#[repr(C)]
#[derive(Debug)]
struct DrmModeCrtc {
    crtc_id: u32,
    buffer_id: u32,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    mode_valid: i32,
    mode: DrmModeInfo,
    gamma_size: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct DrmModeInfo {
    clock: u32,
    hdisplay: u16,
    hsync_start: u16,
    hsync_end: u16,
    htotal: u16,
    hskew: u16,
    vdisplay: u16,
    vsync_start: u16,
    vsync_end: u16,
    vtotal: u16,
    vscan: u16,
    vrefresh: u32,
    flags: u32,
    r#type: u32,
    name: [u8; 32], // DRM_DISPLAY_MODE_LEN
}

#[repr(C)]
#[derive(Debug)]
struct DrmModeConnector {
    connector_id: u32,
    encoder_id: u32,
    connector_type: u32,
    connector_type_id: u32,
    connection: u32, // 1=connected, 2=disconnected, 3=unknown
    mm_width: u32,
    mm_height: u32,
    subpixel: u32,
    count_modes: i32,
    modes: *mut DrmModeInfo,
    count_props: i32,
    props: *mut u32,
    prop_values: *mut u64,
    count_encoders: i32,
    encoders: *mut u32,
}

#[repr(C)]
#[derive(Debug)]
struct DrmModeEncoder {
    encoder_id: u32,
    encoder_type: u32,
    crtc_id: u32,
    possible_crtcs: u32,
    possible_clones: u32,
}

#[repr(C)]
#[derive(Debug)]
struct DrmModePlaneRes {
    count_planes: u32,
    planes: *mut u32,
}

#[repr(C)]
#[derive(Debug)]
struct DrmModePlane {
    count_formats: u32,
    plane_id: u32,
    crtc_id: u32,
    fb_id: u32,
    crtc_x: u32,
    crtc_y: u32,
    x: u32,
    y: u32,
    possible_crtcs: u32,
    gamma_size: u32,
    formats: *mut u32,
}

#[repr(C)]
#[derive(Debug)]
struct DrmModeFB2 {
    fb_id: u32,
    width: u32,
    height: u32,
    pixel_format: u32,
    modifier: u64,
    flags: u32,
    handles: [u32; 4],
    pitches: [u32; 4],
    offsets: [u32; 4],
}

#[repr(C)]
#[derive(Debug)]
struct DrmModeFB {
    fb_id: u32,
    width: u32,
    height: u32,
    pitch: u32,
    bpp: u32,
    depth: u32,
    handle: u32,
}

// Connector type names for building connector_name
const CONNECTOR_TYPE_NAMES: &[&str] = &[
    "Unknown", "VGA", "DVII", "DVID", "DVIA", "Composite", "SVIDEO", "LVDS",
    "Component", "9PinDIN", "DisplayPort", "HDMIA", "HDMIB", "TV", "eDP",
    "VIRTUAL", "DSI", "DPI", "WRITEBACK", "SPI", "USB",
];

fn connector_type_name(t: u32) -> &'static str {
    CONNECTOR_TYPE_NAMES.get(t as usize).copied().unwrap_or("Unknown")
}

#[link(name = "drm")]
extern "C" {
    fn drmClose(fd: libc::c_int) -> libc::c_int;
    fn drmSetClientCap(fd: libc::c_int, capability: u64, value: u64) -> libc::c_int;

    fn drmModeGetResources(fd: libc::c_int) -> *mut DrmModeRes;
    fn drmModeFreeResources(ptr: *mut DrmModeRes);

    fn drmModeGetCrtc(fd: libc::c_int, crtc_id: u32) -> *mut DrmModeCrtc;
    fn drmModeFreeCrtc(ptr: *mut DrmModeCrtc);

    fn drmModeGetConnector(fd: libc::c_int, connector_id: u32) -> *mut DrmModeConnector;
    fn drmModeFreeConnector(ptr: *mut DrmModeConnector);

    fn drmModeGetEncoder(fd: libc::c_int, encoder_id: u32) -> *mut DrmModeEncoder;
    fn drmModeFreeEncoder(ptr: *mut DrmModeEncoder);

    fn drmModeGetPlaneResources(fd: libc::c_int) -> *mut DrmModePlaneRes;
    fn drmModeFreePlaneResources(ptr: *mut DrmModePlaneRes);

    fn drmModeGetPlane(fd: libc::c_int, plane_id: u32) -> *mut DrmModePlane;
    fn drmModeFreePlane(ptr: *mut DrmModePlane);

    fn drmModeGetFB2(fd: libc::c_int, fb_id: u32) -> *mut DrmModeFB2;
    fn drmModeFreeFB2(ptr: *mut DrmModeFB2);

    fn drmModeGetFB(fd: libc::c_int, fb_id: u32) -> *mut DrmModeFB;
    fn drmModeFreeFB(ptr: *mut DrmModeFB);

    fn drmPrimeHandleToFD(
        fd: libc::c_int,
        handle: u32,
        flags: u32,
        prime_fd: *mut libc::c_int,
    ) -> libc::c_int;
}

// DRM client capabilities
const DRM_CLIENT_CAP_UNIVERSAL_PLANES: u64 = 2;

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
}

/// DRM/KMS framebuffer capture backend.
///
/// Opens a DRM device and reads GPU framebuffers directly for instant,
/// native-resolution screen capture without PipeWire or Spectacle.
pub struct DrmCaptureBackend {
    /// Open DRM device fd.
    drm_fd: RawFd,
    /// Which DRM card (e.g., "/dev/dri/card2").
    card_path: String,
    /// Mapping of CRTC IDs to monitor info.
    crtc_monitors: Vec<CrtcMonitor>,
}

impl Drop for DrmCaptureBackend {
    fn drop(&mut self) {
        if self.drm_fd >= 0 {
            unsafe { drmClose(self.drm_fd) };
        }
    }
}

impl DrmCaptureBackend {
    /// Open the DRM device and enumerate active planes/CRTCs.
    ///
    /// Tries all /dev/dri/card* devices, picking the one with active planes.
    pub fn new() -> AppResult<Self> {
        let card_path = Self::find_active_card()?;
        info!("DRM: using card {}", card_path);

        let c_path = CString::new(card_path.as_str()).map_err(|e| {
            AppError::Capture(format!("Invalid card path: {e}"))
        })?;

        let fd = unsafe {
            libc::open(c_path.as_ptr(), libc::O_RDWR | libc::O_CLOEXEC)
        };
        if fd < 0 {
            return Err(AppError::Capture(format!(
                "Failed to open DRM device {}: {}",
                card_path,
                std::io::Error::last_os_error()
            )));
        }

        // Enable universal planes so we can see all planes, not just overlay planes.
        let ret = unsafe { drmSetClientCap(fd, DRM_CLIENT_CAP_UNIVERSAL_PLANES, 1) };
        if ret != 0 {
            warn!("DRM: drmSetClientCap(UNIVERSAL_PLANES) failed (ret={}), continuing anyway", ret);
        }

        let crtc_monitors = Self::enumerate_crtcs(fd, &card_path)?;
        info!("DRM: found {} active CRTCs", crtc_monitors.len());
        for cm in &crtc_monitors {
            info!(
                "  CRTC {} (plane {}, fb {}): {}x{} connector={}",
                cm.crtc_id, cm.plane_id, cm.fb_id, cm.width, cm.height, cm.connector_name
            );
        }

        Ok(DrmCaptureBackend {
            drm_fd: fd,
            card_path,
            crtc_monitors,
        })
    }

    /// Check if DRM capture is available (CAP_SYS_ADMIN and at least one card).
    pub fn is_available() -> bool {
        // Check if we have CAP_SYS_ADMIN by trying to open a DRM device.
        // A quick heuristic: try to find and open any card.
        match Self::find_active_card() {
            Ok(card) => {
                let c_path = match CString::new(card.as_str()) {
                    Ok(p) => p,
                    Err(_) => return false,
                };
                let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };
                if fd < 0 {
                    return false;
                }
                // Try setting universal planes — requires master or CAP_SYS_ADMIN
                let ret = unsafe { drmSetClientCap(fd, DRM_CLIENT_CAP_UNIVERSAL_PLANES, 1) };
                unsafe { libc::close(fd) };
                ret == 0
            }
            Err(_) => false,
        }
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
        self.crtc_monitors = Self::enumerate_crtcs(self.drm_fd, &self.card_path)?;
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
    // Internal: find the right DRM card
    // -----------------------------------------------------------------------

    fn find_active_card() -> AppResult<String> {
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

        // Sort by card number (card0, card1, card2...) so we try them in order.
        cards.sort();

        // Try each card, pick the one with active planes (has a framebuffer).
        let mut best_card: Option<(String, usize)> = None;

        for card in &cards {
            let card_str = card.to_string_lossy().to_string();
            let c_path = match CString::new(card_str.as_str()) {
                Ok(p) => p,
                Err(_) => continue,
            };

            let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };
            if fd < 0 {
                debug!("DRM: cannot open {} ({})", card_str, std::io::Error::last_os_error());
                continue;
            }

            // Enable universal planes
            let _ = unsafe { drmSetClientCap(fd, DRM_CLIENT_CAP_UNIVERSAL_PLANES, 1) };

            // Count active planes with framebuffers
            let active_count = Self::count_active_planes(fd);
            debug!("DRM: {} has {} active planes", card_str, active_count);

            unsafe { libc::close(fd) };

            if active_count > 0 {
                match &best_card {
                    Some((_, best_count)) if active_count <= *best_count => {}
                    _ => {
                        best_card = Some((card_str, active_count));
                    }
                }
            }
        }

        best_card
            .map(|(path, _)| path)
            .ok_or_else(|| {
                AppError::Capture(
                    "DRM: no cards with active framebuffers found. Check CAP_SYS_ADMIN.".to_string(),
                )
            })
    }

    fn count_active_planes(fd: RawFd) -> usize {
        unsafe {
            let plane_res = drmModeGetPlaneResources(fd);
            if plane_res.is_null() {
                return 0;
            }

            let count = (*plane_res).count_planes as usize;
            let planes_ptr = (*plane_res).planes;
            let mut active = 0;

            for i in 0..count {
                let plane_id = *planes_ptr.add(i);
                let plane = drmModeGetPlane(fd, plane_id);
                if !plane.is_null() {
                    if (*plane).fb_id != 0 && (*plane).crtc_id != 0 {
                        active += 1;
                    }
                    drmModeFreePlane(plane);
                }
            }

            drmModeFreePlaneResources(plane_res);
            active
        }
    }

    // -----------------------------------------------------------------------
    // Internal: enumerate active CRTCs and map to connectors
    // -----------------------------------------------------------------------

    fn enumerate_crtcs(fd: RawFd, _card_path: &str) -> AppResult<Vec<CrtcMonitor>> {
        unsafe {
            let res = drmModeGetResources(fd);
            if res.is_null() {
                return Err(AppError::Capture(
                    "DRM: drmModeGetResources returned null. Missing CAP_SYS_ADMIN?".to_string(),
                ));
            }

            // Build a map: crtc_id -> connector_name
            let mut crtc_connector: HashMap<u32, String> = HashMap::new();
            let connector_count = (*res).count_connectors as usize;
            for i in 0..connector_count {
                let conn_id = *(*res).connectors.add(i);
                let conn = drmModeGetConnector(fd, conn_id);
                if conn.is_null() {
                    continue;
                }

                // Only care about connected connectors
                if (*conn).connection == 1 {
                    let type_name = connector_type_name((*conn).connector_type);
                    let name = format!("{}-{}", type_name, (*conn).connector_type_id);

                    // Find the CRTC through the encoder
                    if (*conn).encoder_id != 0 {
                        let enc = drmModeGetEncoder(fd, (*conn).encoder_id);
                        if !enc.is_null() {
                            if (*enc).crtc_id != 0 {
                                crtc_connector.insert((*enc).crtc_id, name);
                            }
                            drmModeFreeEncoder(enc);
                        }
                    }
                }

                drmModeFreeConnector(conn);
            }

            // Enumerate planes to find active ones with framebuffers
            let plane_res = drmModeGetPlaneResources(fd);
            if plane_res.is_null() {
                drmModeFreeResources(res);
                return Err(AppError::Capture(
                    "DRM: drmModeGetPlaneResources returned null".to_string(),
                ));
            }

            let mut monitors = Vec::new();
            let plane_count = (*plane_res).count_planes as usize;

            // Track which CRTCs we've already matched to avoid duplicates
            // (multiple planes can be on the same CRTC — cursor, overlay, etc.)
            let mut seen_crtcs = std::collections::HashSet::new();

            for i in 0..plane_count {
                let plane_id = *(*plane_res).planes.add(i);
                let plane = drmModeGetPlane(fd, plane_id);
                if plane.is_null() {
                    continue;
                }

                let crtc_id = (*plane).crtc_id;
                let fb_id = (*plane).fb_id;

                if crtc_id == 0 || fb_id == 0 {
                    drmModeFreePlane(plane);
                    continue;
                }

                // Skip if we already have a plane for this CRTC
                if seen_crtcs.contains(&crtc_id) {
                    drmModeFreePlane(plane);
                    continue;
                }

                // Get FB info to determine dimensions
                let fb2 = drmModeGetFB2(fd, fb_id);
                let (width, height) = if !fb2.is_null() {
                    let w = (*fb2).width;
                    let h = (*fb2).height;
                    drmModeFreeFB2(fb2);
                    (w, h)
                } else {
                    // Fall back to legacy GetFB
                    let fb = drmModeGetFB(fd, fb_id);
                    if !fb.is_null() {
                        let w = (*fb).width;
                        let h = (*fb).height;
                        drmModeFreeFB(fb);
                        (w, h)
                    } else {
                        drmModeFreePlane(plane);
                        continue;
                    }
                };

                // Get CRTC position
                let (crtc_x, crtc_y) = {
                    let crtc = drmModeGetCrtc(fd, crtc_id);
                    if !crtc.is_null() {
                        let x = (*crtc).x;
                        let y = (*crtc).y;
                        drmModeFreeCrtc(crtc);
                        (x, y)
                    } else {
                        (0, 0)
                    }
                };

                let connector_name = crtc_connector
                    .get(&crtc_id)
                    .cloned()
                    .unwrap_or_else(|| format!("CRTC-{}", crtc_id));

                seen_crtcs.insert(crtc_id);
                monitors.push(CrtcMonitor {
                    crtc_id,
                    plane_id,
                    fb_id,
                    width,
                    height,
                    connector_name,
                    crtc_x,
                    crtc_y,
                });

                drmModeFreePlane(plane);
            }

            drmModeFreePlaneResources(plane_res);
            drmModeFreeResources(res);

            if monitors.is_empty() {
                return Err(AppError::Capture(
                    "DRM: no active CRTCs with framebuffers found".to_string(),
                ));
            }

            Ok(monitors)
        }
    }

    // -----------------------------------------------------------------------
    // Internal: capture a framebuffer from a plane
    // -----------------------------------------------------------------------

    fn capture_plane_fb(&self, cm: &CrtcMonitor) -> AppResult<CapturedFrame> {
        // Re-read the plane to get the current fb_id (it may change every frame).
        let (current_fb_id, _plane_crtc) = unsafe {
            let plane = drmModeGetPlane(self.drm_fd, cm.plane_id);
            if plane.is_null() {
                return Err(AppError::Capture(format!(
                    "DRM: drmModeGetPlane({}) returned null",
                    cm.plane_id
                )));
            }
            let fb_id = (*plane).fb_id;
            let crtc_id = (*plane).crtc_id;
            drmModeFreePlane(plane);
            (fb_id, crtc_id)
        };

        if current_fb_id == 0 {
            return Err(AppError::Capture(format!(
                "DRM: plane {} has no framebuffer (fb_id=0)",
                cm.plane_id
            )));
        }

        // Try drmModeGetFB2 first (gives us format/modifier info).
        let fb2_result = unsafe {
            let fb2 = drmModeGetFB2(self.drm_fd, current_fb_id);
            if fb2.is_null() {
                None
            } else {
                let info = Fb2Info {
                    fb_id: (*fb2).fb_id,
                    width: (*fb2).width,
                    height: (*fb2).height,
                    pixel_format: (*fb2).pixel_format,
                    modifier: (*fb2).modifier,
                    handles: (*fb2).handles,
                    pitches: (*fb2).pitches,
                    offsets: (*fb2).offsets,
                };
                drmModeFreeFB2(fb2);
                Some(info)
            }
        };

        if let Some(fb2) = fb2_result {
            debug!(
                "DRM: FB2 fb={} {}x{} fmt=0x{:08x} modifier=0x{:x} handle={} pitch={}",
                fb2.fb_id, fb2.width, fb2.height, fb2.pixel_format, fb2.modifier,
                fb2.handles[0], fb2.pitches[0]
            );

            if fb2.handles[0] == 0 {
                return Err(AppError::Capture(format!(
                    "DRM: FB {} has handle=0 — likely insufficient permissions. \
                     Run: sudo setcap cap_sys_admin+ep <binary>",
                    fb2.fb_id
                )));
            }

            // Check modifier — we can only mmap linear buffers directly.
            let is_linear = fb2.modifier == DRM_FORMAT_MOD_LINEAR
                || fb2.modifier == DRM_FORMAT_MOD_INVALID;

            if !is_linear {
                warn!(
                    "DRM: FB {} uses tiled modifier 0x{:x}. Will attempt mmap but may get garbled output.",
                    fb2.fb_id, fb2.modifier
                );
            }

            self.read_fb_via_prime(
                fb2.handles[0],
                fb2.width,
                fb2.height,
                fb2.pitches[0],
                fb2.pixel_format,
            )
        } else {
            // Fall back to legacy drmModeGetFB
            let fb_result = unsafe {
                let fb = drmModeGetFB(self.drm_fd, current_fb_id);
                if fb.is_null() {
                    None
                } else {
                    let info = FbInfo {
                        fb_id: (*fb).fb_id,
                        width: (*fb).width,
                        height: (*fb).height,
                        pitch: (*fb).pitch,
                        bpp: (*fb).bpp,
                        handle: (*fb).handle,
                    };
                    drmModeFreeFB(fb);
                    Some(info)
                }
            };

            let fb = fb_result.ok_or_else(|| {
                AppError::Capture(format!(
                    "DRM: both drmModeGetFB2 and drmModeGetFB failed for fb_id={}",
                    current_fb_id
                ))
            })?;

            debug!(
                "DRM: legacy FB fb={} {}x{} bpp={} pitch={} handle={}",
                fb.fb_id, fb.width, fb.height, fb.bpp, fb.pitch, fb.handle
            );

            if fb.handle == 0 {
                return Err(AppError::Capture(format!(
                    "DRM: FB {} has handle=0 — likely insufficient permissions. \
                     Run: sudo setcap cap_sys_admin+ep <binary>",
                    fb.fb_id
                )));
            }

            // Legacy FB doesn't tell us the format; assume XRGB8888 for 32bpp.
            let pixel_format = if fb.bpp == 32 {
                DRM_FORMAT_XRGB8888
            } else {
                return Err(AppError::Capture(format!(
                    "DRM: unsupported bpp={} for legacy FB",
                    fb.bpp
                )));
            };

            self.read_fb_via_prime(fb.handle, fb.width, fb.height, fb.pitch, pixel_format)
        }
    }

    /// Export a GEM handle to a DMA-BUF fd, mmap it, and read pixel data.
    fn read_fb_via_prime(
        &self,
        handle: u32,
        width: u32,
        height: u32,
        pitch: u32,
        pixel_format: u32,
    ) -> AppResult<CapturedFrame> {
        // Export handle to DMA-BUF fd
        let mut prime_fd: libc::c_int = -1;
        let ret = unsafe {
            drmPrimeHandleToFD(self.drm_fd, handle, DRM_CLOEXEC, &mut prime_fd)
        };
        if ret != 0 || prime_fd < 0 {
            return Err(AppError::Capture(format!(
                "DRM: drmPrimeHandleToFD failed (ret={}, errno={})",
                ret,
                std::io::Error::last_os_error()
            )));
        }

        let size = (pitch as usize) * (height as usize);

        // mmap the DMA-BUF
        let mapped = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ,
                libc::MAP_SHARED,
                prime_fd,
                0,
            )
        };

        if mapped == libc::MAP_FAILED {
            // If mmap fails, try reading via read() syscall instead.
            // read_fb_via_read takes ownership of prime_fd and closes it.
            return self.read_fb_via_read(prime_fd, width, height, pitch, pixel_format, size);
        }

        // Copy pixel data out and convert format
        let raw_data = unsafe {
            std::slice::from_raw_parts(mapped as *const u8, size)
        };

        let rgba = convert_drm_to_rgba(raw_data, width, height, pitch, pixel_format);

        // Cleanup
        unsafe {
            libc::munmap(mapped, size);
            libc::close(prime_fd);
        }

        Ok(CapturedFrame {
            data: rgba,
            width,
            height,
        })
    }

    /// Fallback: read the DMA-BUF fd via read() if mmap fails.
    fn read_fb_via_read(
        &self,
        prime_fd: RawFd,
        width: u32,
        height: u32,
        pitch: u32,
        pixel_format: u32,
        size: usize,
    ) -> AppResult<CapturedFrame> {
        use std::io::Read;

        info!("DRM: mmap failed, trying read() on DMA-BUF fd");

        let mut file = unsafe { std::fs::File::from_raw_fd(prime_fd) };
        let mut raw_data = vec![0u8; size];
        file.read_exact(&mut raw_data).map_err(|e| {
            AppError::Capture(format!("DRM: failed to read DMA-BUF: {e}"))
        })?;
        // File is dropped here, which closes the fd.

        let rgba = convert_drm_to_rgba(&raw_data, width, height, pitch, pixel_format);

        Ok(CapturedFrame {
            data: rgba,
            width,
            height,
        })
    }
}

// We need FromRawFd for the fallback read path.
use std::os::unix::io::FromRawFd;

// ---------------------------------------------------------------------------
// Helper structs for FB info
// ---------------------------------------------------------------------------

#[derive(Debug)]
#[allow(dead_code)]
struct Fb2Info {
    fb_id: u32,
    width: u32,
    height: u32,
    pixel_format: u32,
    modifier: u64,
    handles: [u32; 4],
    pitches: [u32; 4],
    offsets: [u32; 4],
}

#[derive(Debug)]
struct FbInfo {
    fb_id: u32,
    width: u32,
    height: u32,
    pitch: u32,
    bpp: u32,
    handle: u32,
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
    fn test_connector_type_name() {
        assert_eq!(connector_type_name(0), "Unknown");
        assert_eq!(connector_type_name(1), "VGA");
        assert_eq!(connector_type_name(10), "DisplayPort");
        assert_eq!(connector_type_name(11), "HDMIA");
        assert_eq!(connector_type_name(14), "eDP");
        assert_eq!(connector_type_name(99), "Unknown");
    }

    #[test]
    fn test_is_available() {
        // Just verify it doesn't panic. On CI this will likely return false.
        let _avail = DrmCaptureBackend::is_available();
    }
}
