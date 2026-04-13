//! PipeWire ScreenCast capture backend for persistent, hardware-accelerated
//! frame capture on Wayland.
//!
//! Sets up a portal ScreenCast session once (showing a dialog on first run),
//! connects to PipeWire via the portal fd, and continuously receives frames
//! on a dedicated thread. Callers retrieve the latest frame instantly via
//! `grab_frame()` without any per-frame D-Bus or file I/O overhead.

use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use domain::capture::CapturedFrame;
use domain::error::{AppError, AppResult};
use tracing::{debug, error, info, warn};

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

/// A raw frame received from PipeWire.
struct PwFrame {
    /// RGBA pixel data (already converted from whatever PipeWire delivers).
    data: Vec<u8>,
    width: u32,
    height: u32,
}

/// Negotiated stream parameters.
#[allow(dead_code)]
struct StreamInfo {
    width: u32,
    height: u32,
}

/// Information obtained from the xdg-desktop-portal ScreenCast session.
struct PortalSession {
    node_id: u32,
    pipewire_fd: OwnedFd,
    width: Option<u32>,
    height: Option<u32>,
    /// Position of this stream in the virtual desktop (logical coordinates).
    position: Option<(i32, i32)>,
    restore_token: Option<String>,
}

// OwnedFd is Send but the compiler cannot see through PortalSession.
unsafe impl Send for PortalSession {}

// ---------------------------------------------------------------------------
// PipeWireCapture — public API
// ---------------------------------------------------------------------------

/// Persistent PipeWire ScreenCast capture stream.
///
/// Holds a background thread that continuously receives frames from PipeWire.
/// Callers use `grab_frame()` to read the latest cached frame with zero
/// per-frame overhead.
pub struct PipeWireCapture {
    /// Latest frame received from PipeWire (shared with PW thread).
    latest_frame: Arc<Mutex<Option<PwFrame>>>,
    /// Stream info (width, height) after negotiation.
    #[allow(dead_code)]
    stream_info: Arc<Mutex<Option<StreamInfo>>>,
    /// PipeWire thread handle.
    pw_thread: Option<JoinHandle<()>>,
    /// Signal to stop the PipeWire loop.
    running: Arc<AtomicBool>,
    /// Restore token for skipping dialog on next launch.
    restore_token: Arc<Mutex<Option<String>>>,
    /// Position of the captured region in the virtual desktop (logical coords).
    /// This is what the portal reports as the stream's position.
    pub stream_position: Option<(i32, i32)>,
    /// Tokio runtime for async portal calls.
    _runtime: tokio::runtime::Runtime,
}

impl PipeWireCapture {
    /// Start the capture stream.
    ///
    /// Shows the portal picker dialog on first run. On subsequent runs the
    /// saved restore token (from `config_dir/pipewire_token.txt`) is used to
    /// skip the dialog.
    pub fn start(config_dir: &Path) -> AppResult<Self> {
        info!("Starting PipeWire ScreenCast capture");

        // Build a small tokio runtime for the async portal D-Bus calls.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| {
                AppError::Capture(format!("Failed to create tokio runtime: {e}"))
            })?;

        // Load saved restore token.
        let saved_token = load_restore_token(config_dir);
        if saved_token.is_some() {
            debug!("Loaded PipeWire restore token from disk");
        }

        // Open portal session (may show picker).
        let session = runtime.block_on(open_portal_session(saved_token))?;

        info!(
            "Portal session opened: node_id={}, size={:?}, has_token={}",
            session.node_id,
            session.width.zip(session.height),
            session.restore_token.is_some(),
        );

        // Persist the new restore token.
        if let Some(ref token) = session.restore_token {
            save_restore_token(config_dir, token);
        }

        let restore_token = Arc::new(Mutex::new(session.restore_token.clone()));
        let latest_frame: Arc<Mutex<Option<PwFrame>>> = Arc::new(Mutex::new(None));
        let stream_info: Arc<Mutex<Option<StreamInfo>>> = Arc::new(Mutex::new(None));
        let running = Arc::new(AtomicBool::new(true));

        let lf = latest_frame.clone();
        let si = stream_info.clone();
        let r = running.clone();
        let node_id = session.node_id;
        let pw_fd = session.pipewire_fd;
        let init_width = session.width;
        let init_height = session.height;

        // Spawn the PipeWire thread.
        let pw_thread = thread::Builder::new()
            .name("pw-capture".into())
            .spawn(move || {
                if let Err(e) = run_pw_thread(pw_fd, node_id, init_width, init_height, lf, si, r) {
                    error!("PipeWire capture thread error: {e}");
                }
            })
            .map_err(|e| {
                AppError::Capture(format!("Failed to spawn PipeWire thread: {e}"))
            })?;

        Ok(PipeWireCapture {
            latest_frame,
            stream_info,
            pw_thread: Some(pw_thread),
            running,
            restore_token,
            stream_position: session.position,
            _runtime: runtime,
        })
    }

    /// Grab the latest frame (instant -- reads from cached buffer).
    ///
    /// If no frame has arrived yet, retries briefly (up to ~2 seconds) before
    /// returning an error.
    pub fn grab_frame(&self) -> AppResult<CapturedFrame> {
        // Quick path: check if a frame is already available.
        {
            let guard = self.latest_frame.lock().map_err(|e| {
                AppError::Capture(format!("Frame lock poisoned: {e}"))
            })?;
            if let Some(ref f) = *guard {
                return Ok(CapturedFrame {
                    data: f.data.clone(),
                    width: f.width,
                    height: f.height,
                });
            }
        }

        // Slow path: wait for the first frame.
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            thread::sleep(Duration::from_millis(50));
            let guard = self.latest_frame.lock().map_err(|e| {
                AppError::Capture(format!("Frame lock poisoned: {e}"))
            })?;
            if let Some(ref f) = *guard {
                return Ok(CapturedFrame {
                    data: f.data.clone(),
                    width: f.width,
                    height: f.height,
                });
            }
        }

        Err(AppError::Capture(
            "No frame available from PipeWire stream (timed out after 2s)".to_string(),
        ))
    }

    /// Grab a cropped region of the latest frame.
    ///
    /// Coordinates are clamped to the frame bounds.
    pub fn grab_frame_cropped(
        &self,
        x: i32,
        y: i32,
        w: u32,
        h: u32,
    ) -> AppResult<CapturedFrame> {
        let full = self.grab_frame()?;
        crop_frame(&full, x, y, w, h)
    }

    /// Get the current restore token (for persisting).
    pub fn restore_token(&self) -> Option<String> {
        self.restore_token.lock().ok()?.clone()
    }

    /// Check if the stream is active and receiving frames.
    pub fn is_active(&self) -> bool {
        if !self.running.load(Ordering::SeqCst) {
            return false;
        }
        self.latest_frame.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    /// Stop the capture stream.
    pub fn stop(&mut self) {
        info!("Stopping PipeWire capture stream");
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.pw_thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for PipeWireCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

// ---------------------------------------------------------------------------
// Portal session setup (async, uses ashpd)
// ---------------------------------------------------------------------------

async fn open_portal_session(
    restore_token: Option<String>,
) -> AppResult<PortalSession> {
    use ashpd::desktop::screencast::{CursorMode, Screencast, SourceType};
    use ashpd::desktop::PersistMode;

    let proxy = Screencast::new().await.map_err(|e| {
        AppError::Capture(format!("Failed to create Screencast portal proxy: {e}"))
    })?;

    let session = proxy.create_session().await.map_err(|e| {
        AppError::Capture(format!("Failed to create ScreenCast session: {e}"))
    })?;

    // Select sources: monitor + window, allow multiple (for multi-monitor).
    // When the user selects multiple monitors in the picker, we get a combined
    // frame spanning the full virtual desktop.
    proxy
        .select_sources(
            &session,
            CursorMode::Embedded,
            SourceType::Monitor | SourceType::Window,
            true,  // multiple — allows selecting all monitors
            restore_token.as_deref(),
            PersistMode::ExplicitlyRevoked,
        )
        .await
        .map_err(|e| {
            AppError::Capture(format!("Failed to select sources: {e}"))
        })?;

    // Start (shows picker on first use, uses restore token thereafter).
    let response = proxy
        .start(&session, None)
        .await
        .map_err(|e| {
            AppError::Capture(format!(
                "Portal Start failed (user may have cancelled): {e}"
            ))
        })?
        .response()
        .map_err(|e| {
            AppError::Capture(format!("Portal Start response error: {e}"))
        })?;

    let streams = response.streams();
    if streams.is_empty() {
        return Err(AppError::Capture(
            "Portal returned no streams -- user may have cancelled the picker".to_string(),
        ));
    }

    let stream = &streams[0];
    let node_id = stream.pipe_wire_node_id();
    let (width, height) = match stream.size() {
        Some((w, h)) => (Some(w as u32), Some(h as u32)),
        None => (None, None),
    };

    // Log the stream's position in the virtual desktop (crucial for multi-monitor)
    let position = stream.position();
    info!(
        "Portal stream: node_id={node_id}, size={:?}, position={:?}",
        (width, height), position
    );

    let new_token = response.restore_token().map(|s| s.to_string());

    let pw_fd = proxy
        .open_pipe_wire_remote(&session)
        .await
        .map_err(|e| {
            AppError::Capture(format!("Failed to open PipeWire remote: {e}"))
        })?;

    let stream_position = position.map(|(x, y)| (x as i32, y as i32));

    Ok(PortalSession {
        node_id,
        pipewire_fd: pw_fd,
        width,
        height,
        position: stream_position,
        restore_token: new_token,
    })
}

// ---------------------------------------------------------------------------
// PipeWire thread
// ---------------------------------------------------------------------------

fn run_pw_thread(
    pw_fd: OwnedFd,
    node_id: u32,
    init_width: Option<u32>,
    init_height: Option<u32>,
    latest_frame: Arc<Mutex<Option<PwFrame>>>,
    stream_info: Arc<Mutex<Option<StreamInfo>>>,
    running: Arc<AtomicBool>,
) -> AppResult<()> {
    use pipewire as pw;
    use pw::spa::utils::Direction;
    use pw::stream::{StreamBox, StreamFlags, StreamState};

    pw::init();

    let mainloop = pw::main_loop::MainLoopBox::new(None).map_err(|e| {
        AppError::Capture(format!("Failed to create PipeWire main loop: {e}"))
    })?;

    let pw_loop = mainloop.loop_();

    let context = pw::context::ContextBox::new(pw_loop, None).map_err(|e| {
        AppError::Capture(format!("Failed to create PipeWire context: {e}"))
    })?;

    let core = context.connect_fd(pw_fd, None).map_err(|e| {
        AppError::Capture(format!("Failed to connect to PipeWire via fd: {e}"))
    })?;

    // Shared negotiated dimensions (updated by param_changed or first-frame).
    let neg_width: Arc<Mutex<Option<u32>>> = Arc::new(Mutex::new(init_width));
    let neg_height: Arc<Mutex<Option<u32>>> = Arc::new(Mutex::new(init_height));
    let stream_started = Arc::new(AtomicBool::new(false));
    let stream_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let frame_ref = latest_frame.clone();
    let si_ref = stream_info.clone();
    let nw = neg_width.clone();
    let nh = neg_height.clone();
    let started_cb = stream_started.clone();
    let error_cb = stream_error.clone();
    let running_cb = running.clone();

    let props = pw::properties::properties! {
        *pw::keys::MEDIA_TYPE => "Video",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Screen",
    };

    let stream = StreamBox::new(&core, "screen-dream-pw-capture", props).map_err(|e| {
        AppError::Capture(format!("Failed to create PipeWire stream: {e}"))
    })?;

    let _listener = stream
        .add_local_listener::<()>()
        .state_changed({
            let started = started_cb.clone();
            let err_store = error_cb.clone();
            let run = running_cb.clone();
            move |_stream, _data, _old, new| {
                debug!("PipeWire capture stream state: {:?}", new);
                match new {
                    StreamState::Streaming => {
                        started.store(true, Ordering::SeqCst);
                    }
                    StreamState::Error(ref msg) => {
                        error!("PipeWire capture stream error: {}", msg);
                        *err_store.lock().unwrap() = Some(msg.to_string());
                        run.store(false, Ordering::SeqCst);
                    }
                    _ => {}
                }
            }
        })
        .param_changed({
            let si = si_ref.clone();
            let nw2 = nw.clone();
            let nh2 = nh.clone();
            move |_stream, _data, id, _pod| {
                if id != pw::spa::param::ParamType::Format.as_raw() {
                    return;
                }
                // We rely on portal-reported dimensions + first-frame detection.
                // If we already have dimensions, update StreamInfo.
                let w = *nw2.lock().unwrap();
                let h = *nh2.lock().unwrap();
                if let (Some(w), Some(h)) = (w, h) {
                    *si.lock().unwrap() = Some(StreamInfo { width: w, height: h });
                }
                debug!("PipeWire format negotiated (param_changed)");
            }
        })
        .process({
            let nw3 = nw.clone();
            let nh3 = nh.clone();
            let frame_store = frame_ref;
            let si2 = si_ref;
            move |stream, _data| {
                if let Some(mut buffer) = stream.dequeue_buffer() {
                    let datas = buffer.datas_mut();
                    if datas.is_empty() {
                        return;
                    }

                    let spa_data = &mut datas[0];
                    let chunk_size = spa_data.chunk().size();
                    let chunk_stride = spa_data.chunk().stride();

                    if chunk_size == 0 {
                        return;
                    }

                    if let Some(slice) = spa_data.data() {
                        let mut w = nw3.lock().unwrap().unwrap_or(0);
                        let mut h = nh3.lock().unwrap().unwrap_or(0);

                        // If portal didn't give us dimensions, try to derive from stride + size.
                        if (w == 0 || h == 0) && chunk_stride > 0 {
                            w = (chunk_stride / 4) as u32;
                            h = chunk_size / chunk_stride as u32;
                            if w > 0 && h > 0 {
                                info!("Derived frame dimensions from buffer: {w}x{h}");
                                *nw3.lock().unwrap() = Some(w);
                                *nh3.lock().unwrap() = Some(h);
                            }
                        }

                        if w == 0 || h == 0 {
                            debug!("Skipping frame: dimensions unknown (stride={chunk_stride}, size={chunk_size})");
                            return;
                        }

                        let stride = chunk_stride;
                        let expected_stride = (w * 4) as i32;

                        // Copy pixel data, handling stride padding.
                        let raw_bytes = if stride == expected_stride || stride == 0 {
                            let expected_size = (w * h * 4) as usize;
                            let copy_size = expected_size.min(slice.len());
                            slice[..copy_size].to_vec()
                        } else {
                            let row_bytes = (w * 4) as usize;
                            let stride_bytes = stride as usize;
                            let mut out = Vec::with_capacity((w * h * 4) as usize);
                            for row in 0..h as usize {
                                let start = row * stride_bytes;
                                let end = start + row_bytes;
                                if end <= slice.len() {
                                    out.extend_from_slice(&slice[start..end]);
                                }
                            }
                            out
                        };

                        // PipeWire portal streams deliver BGRA/BGRx — convert to RGBA.
                        let rgba = bgra_to_rgba(&raw_bytes);

                        // Update stream info if not yet set.
                        {
                            let mut si_guard = si2.lock().unwrap();
                            if si_guard.is_none() {
                                *si_guard = Some(StreamInfo { width: w, height: h });
                            }
                        }

                        *frame_store.lock().unwrap() = Some(PwFrame {
                            data: rgba,
                            width: w,
                            height: h,
                        });
                    }
                }
            }
        })
        .register()
        .map_err(|e| {
            AppError::Capture(format!("Failed to register PipeWire stream listener: {e}"))
        })?;

    // Build format params telling PipeWire what we can accept.
    // Without this, PipeWire may not negotiate and the stream won't produce frames.
    use pw::spa;
    let obj = spa::pod::object!(
        spa::utils::SpaTypes::ObjectParamFormat,
        spa::param::ParamType::EnumFormat,
        spa::pod::property!(
            spa::param::format::FormatProperties::MediaType, Id,
            spa::param::format::MediaType::Video
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::MediaSubtype, Id,
            spa::param::format::MediaSubtype::Raw
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFormat, Choice, Enum, Id,
            spa::param::video::VideoFormat::BGRx,
            spa::param::video::VideoFormat::BGRx,
            spa::param::video::VideoFormat::BGRA,
            spa::param::video::VideoFormat::RGBx,
            spa::param::video::VideoFormat::RGBA,
            spa::param::video::VideoFormat::RGB
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoSize, Choice, Range, Rectangle,
            spa::utils::Rectangle { width: 3840, height: 2160 },  // prefer native 4K
            spa::utils::Rectangle { width: 1, height: 1 },
            spa::utils::Rectangle { width: 8192, height: 8192 }
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFramerate, Choice, Range, Fraction,
            spa::utils::Fraction { num: 30, denom: 1 },
            spa::utils::Fraction { num: 0, denom: 1 },
            spa::utils::Fraction { num: 144, denom: 1 }
        )
    );
    let values: Vec<u8> = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(obj),
    )
    .map_err(|e| AppError::Capture(format!("Failed to serialize PipeWire format params: {e}")))?
    .0
    .into_inner();
    let mut params = [spa::pod::Pod::from_bytes(&values).unwrap()];

    info!("Connecting PipeWire stream to node {node_id}");

    // Connect to the portal node.
    stream
        .connect(
            Direction::Input,
            Some(node_id),
            StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS,
            &mut params,
        )
        .map_err(|e| {
            AppError::Capture(format!(
                "Failed to connect PipeWire stream to node {node_id}: {e}"
            ))
        })?;

    // Wait for streaming state.
    let wait_start = Instant::now();
    while !stream_started.load(Ordering::SeqCst) && running.load(Ordering::SeqCst) {
        pw_loop.iterate(Duration::from_millis(50));
        if wait_start.elapsed() > Duration::from_secs(10) {
            return Err(AppError::Capture(
                "Timed out waiting for PipeWire stream to start".to_string(),
            ));
        }
        if let Some(err) = stream_error.lock().unwrap().take() {
            return Err(AppError::Capture(format!(
                "PipeWire stream error during startup: {err}"
            )));
        }
    }

    info!("PipeWire capture stream active — entering main loop");

    // Main loop: keep iterating to receive frames until `running` is false.
    while running.load(Ordering::SeqCst) {
        pw_loop.iterate(Duration::from_millis(16)); // ~60 iterations/sec
    }

    debug!("PipeWire capture thread shutting down");
    let _ = stream.disconnect();

    // Safety: deinit must only be called when no other PipeWire objects are alive.
    // The stream and core will be dropped when this function returns, so we defer
    // deinit. In practice this is best-effort cleanup.
    unsafe { pw::deinit() };

    Ok(())
}

// ---------------------------------------------------------------------------
// Pixel format conversion
// ---------------------------------------------------------------------------

/// Convert BGRA pixel data to RGBA by swapping bytes 0 and 2 (B <-> R) for
/// each pixel. Also sets alpha to 255 for fully opaque (handles BGRx).
fn bgra_to_rgba(bgra: &[u8]) -> Vec<u8> {
    let mut rgba = bgra.to_vec();
    let len = rgba.len();
    let mut i = 0;
    while i + 3 < len {
        // Swap B and R.
        rgba.swap(i, i + 2);
        // Ensure alpha is opaque (BGRx has undefined alpha byte).
        if rgba[i + 3] == 0 {
            rgba[i + 3] = 255;
        }
        i += 4;
    }
    rgba
}

// ---------------------------------------------------------------------------
// Crop helper
// ---------------------------------------------------------------------------

/// Crop a CapturedFrame to the given rectangle. Coordinates are clamped.
pub fn crop_frame(
    frame: &CapturedFrame,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
) -> AppResult<CapturedFrame> {
    let src_w = frame.width;
    let src_h = frame.height;

    let cx = x.max(0) as u32;
    let cy = y.max(0) as u32;

    if cx >= src_w || cy >= src_h {
        return Err(AppError::Capture(format!(
            "Crop origin ({cx}, {cy}) outside frame bounds ({src_w}x{src_h})"
        )));
    }

    let cw = w.min(src_w.saturating_sub(cx));
    let ch = h.min(src_h.saturating_sub(cy));

    if cw == 0 || ch == 0 {
        return Err(AppError::Capture(format!(
            "Crop region empty after clamping: {cw}x{ch} at ({cx},{cy}) in {src_w}x{src_h}"
        )));
    }

    let src_stride = (src_w * 4) as usize;
    let dst_row_bytes = (cw * 4) as usize;
    let mut data = Vec::with_capacity((cw * ch * 4) as usize);

    for row in 0..ch {
        let src_row = (cy + row) as usize;
        let src_offset = src_row * src_stride + (cx as usize) * 4;
        let end = src_offset + dst_row_bytes;
        if end <= frame.data.len() {
            data.extend_from_slice(&frame.data[src_offset..end]);
        }
    }

    Ok(CapturedFrame {
        data,
        width: cw,
        height: ch,
    })
}

// ---------------------------------------------------------------------------
// Token persistence
// ---------------------------------------------------------------------------

fn token_path(config_dir: &Path) -> PathBuf {
    config_dir.join("pipewire_token.txt")
}

fn load_restore_token(config_dir: &Path) -> Option<String> {
    let path = token_path(config_dir);
    match std::fs::read_to_string(&path) {
        Ok(s) => {
            let trimmed = s.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }
        Err(_) => None,
    }
}

fn save_restore_token(config_dir: &Path, token: &str) {
    // Ensure the directory exists.
    if let Err(e) = std::fs::create_dir_all(config_dir) {
        warn!("Failed to create config dir {}: {e}", config_dir.display());
        return;
    }
    let path = token_path(config_dir);
    if let Err(e) = std::fs::write(&path, token) {
        warn!(
            "Failed to save PipeWire restore token to {}: {e}",
            path.display()
        );
    } else {
        debug!("Saved PipeWire restore token to {}", path.display());
    }
}
