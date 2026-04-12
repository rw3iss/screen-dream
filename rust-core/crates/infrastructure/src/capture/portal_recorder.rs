//! Portal ScreenCast + PipeWire video recording backend.
//!
//! Uses xdg-desktop-portal's ScreenCast interface (via the `ashpd` crate) to
//! obtain a PipeWire node for screen or window capture, then reads raw video
//! frames from PipeWire and pipes them to FFmpeg for encoding.
//!
//! This backend works natively on Wayland compositors (GNOME, KDE, wlroots)
//! without requiring XWayland or elevated privileges.

use std::io::Write;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use domain::capture::{CaptureSource, RecordingConfig};
use domain::error::{AppError, AppResult};
use domain::ffmpeg::FfmpegCommand;
use tracing::{debug, error, info, warn};

use super::recording_pipeline::PipelineResult;

// ---------------------------------------------------------------------------
// Portal session types (used to pass portal results across thread boundary)
// ---------------------------------------------------------------------------

/// Information obtained from the xdg-desktop-portal ScreenCast session.
struct PortalSession {
    /// The PipeWire node ID for the selected source.
    node_id: u32,
    /// The PipeWire file descriptor for connecting to the stream.
    pipewire_fd: OwnedFd,
    /// Width of the source (from the portal response, if available).
    width: Option<u32>,
    /// Height of the source (from the portal response, if available).
    height: Option<u32>,
    /// Restore token for reusing this session without re-prompting.
    restore_token: Option<String>,
}

// OwnedFd is Send, but the struct as a whole needs to be explicitly Send
// for thread::spawn. All fields are Send-safe.
unsafe impl Send for PortalSession {}

// ---------------------------------------------------------------------------
// PortalRecorder
// ---------------------------------------------------------------------------

/// A video recorder that uses xdg-desktop-portal ScreenCast + PipeWire for
/// hardware-accelerated, Wayland-native screen and window capture.
///
/// # Workflow
///
/// 1. Opens a ScreenCast portal session (shows system picker on first run).
/// 2. Connects to PipeWire using the portal-supplied fd and node ID.
/// 3. Reads raw video frames from the PipeWire stream.
/// 4. Pipes raw BGRA frames to an FFmpeg child process for encoding.
///
/// For region capture, the full monitor is captured via the portal and FFmpeg's
/// `crop` filter is used to extract the selected region.
pub struct PortalRecorder {
    running: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    capture_thread: Option<JoinHandle<AppResult<PipelineResult>>>,
    output_path: PathBuf,
    /// Restore token returned by the portal after a successful session.
    /// Can be reused in future sessions to skip the picker dialog.
    restore_token: Arc<Mutex<Option<String>>>,
}

impl PortalRecorder {
    /// Start a new portal-based recording session.
    ///
    /// This will:
    /// 1. Open a ScreenCast portal session (async, on a temporary tokio runtime).
    /// 2. Spawn a dedicated OS thread that reads PipeWire frames and pipes them
    ///    to FFmpeg.
    ///
    /// # Arguments
    ///
    /// * `ffmpeg_path` - Path to the FFmpeg binary.
    /// * `config` - Recording configuration (source, fps, codec, etc.).
    /// * `restore_token` - Optional restore token from a previous session.
    ///   If provided and still valid, the portal will skip the picker dialog.
    pub fn start(
        ffmpeg_path: PathBuf,
        config: RecordingConfig,
        restore_token: Option<String>,
    ) -> AppResult<Self> {
        let output_path = PathBuf::from(&config.output_path);

        // Run the async portal session setup on a blocking tokio runtime.
        // We create a small current-thread runtime just for the D-Bus calls.
        let session = {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| {
                    AppError::Capture(format!("Failed to create tokio runtime for portal: {e}"))
                })?;

            let source = config.source.clone();
            rt.block_on(open_portal_session(&source, restore_token))?
        };

        info!(
            "Portal session opened: node_id={}, size={:?}, has_restore_token={}",
            session.node_id,
            session.width.zip(session.height),
            session.restore_token.is_some(),
        );

        let new_restore_token = Arc::new(Mutex::new(session.restore_token.clone()));

        let running = Arc::new(AtomicBool::new(true));
        let paused = Arc::new(AtomicBool::new(false));

        let running_clone = running.clone();
        let paused_clone = paused.clone();
        let output_clone = output_path.clone();

        let capture_thread = thread::spawn(move || {
            run_pipewire_capture_loop(
                ffmpeg_path,
                config,
                session,
                running_clone,
                paused_clone,
                output_clone,
            )
        });

        info!(
            "Portal recording pipeline started -> {}",
            output_path.display()
        );

        Ok(PortalRecorder {
            running,
            paused,
            capture_thread: Some(capture_thread),
            output_path,
            restore_token: new_restore_token,
        })
    }

    /// Stop the recording and wait for the capture thread to finish.
    ///
    /// Returns the pipeline result (output path, frame count, duration) and
    /// the restore token for reuse in future sessions.
    pub fn stop(&mut self) -> AppResult<(PipelineResult, Option<String>)> {
        info!("Stopping portal recording pipeline");
        self.running.store(false, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);

        let result = if let Some(handle) = self.capture_thread.take() {
            match handle.join() {
                Ok(result) => result,
                Err(_) => Err(AppError::Capture(
                    "Portal recording thread panicked".to_string(),
                )),
            }
        } else {
            Err(AppError::Capture(
                "Portal recording pipeline was already stopped".to_string(),
            ))
        }?;

        let token = self.restore_token.lock().unwrap().clone();
        Ok((result, token))
    }

    /// Pause frame piping. The PipeWire stream keeps running but frames are
    /// discarded until `resume()` is called.
    pub fn pause(&self) {
        info!("Pausing portal recording pipeline");
        self.paused.store(true, Ordering::SeqCst);
    }

    /// Resume frame piping after a pause.
    pub fn resume(&self) {
        info!("Resuming portal recording pipeline");
        self.paused.store(false, Ordering::SeqCst);
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    pub fn output_path(&self) -> &Path {
        &self.output_path
    }

    /// Get the current restore token (may be updated after the session starts).
    pub fn restore_token(&self) -> Option<String> {
        self.restore_token.lock().unwrap().clone()
    }
}

// ---------------------------------------------------------------------------
// Portal session setup (async, uses ashpd)
// ---------------------------------------------------------------------------

/// Open an xdg-desktop-portal ScreenCast session.
///
/// This performs the D-Bus calls to:
/// 1. Create a session
/// 2. Select sources (monitor or window)
/// 3. Start the session (shows picker if no restore token)
/// 4. Get the PipeWire fd
async fn open_portal_session(
    source: &CaptureSource,
    restore_token: Option<String>,
) -> AppResult<PortalSession> {
    use ashpd::desktop::screencast::{CursorMode, Screencast, SourceType};
    use ashpd::desktop::PersistMode;

    let proxy = Screencast::new().await.map_err(|e| {
        AppError::Capture(format!("Failed to create Screencast portal proxy: {e}"))
    })?;

    // 1. Create session
    let session = proxy.create_session().await.map_err(|e| {
        AppError::Capture(format!("Failed to create ScreenCast session: {e}"))
    })?;

    // 2. Select sources
    let source_type = match source {
        CaptureSource::Screen(_) | CaptureSource::Region(_) => SourceType::Monitor.into(),
        CaptureSource::Window(_) => SourceType::Window.into(),
    };

    proxy
        .select_sources(
            &session,
            CursorMode::Embedded,
            source_type,
            false, // multiple sources
            restore_token.as_deref(),
            PersistMode::ExplicitlyRevoked,
        )
        .await
        .map_err(|e| {
            AppError::Capture(format!("Failed to select sources: {e}"))
        })?;

    // 3. Start (shows picker on first use, uses restore_token thereafter)
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

    // Extract stream info from the response.
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

    let new_restore_token = response.restore_token().map(|s| s.to_string());

    // 4. Get PipeWire fd
    let pw_fd = proxy
        .open_pipe_wire_remote(&session)
        .await
        .map_err(|e| {
            AppError::Capture(format!("Failed to open PipeWire remote: {e}"))
        })?;

    Ok(PortalSession {
        node_id,
        pipewire_fd: pw_fd,
        width,
        height,
        restore_token: new_restore_token,
    })
}

// ---------------------------------------------------------------------------
// PipeWire capture loop (runs on a dedicated OS thread)
// ---------------------------------------------------------------------------

/// Frame data received from PipeWire.
struct PwFrame {
    data: Vec<u8>,
    width: u32,
    height: u32,
}

/// The core capture loop. Runs on a dedicated OS thread.
///
/// Connects to PipeWire using the portal-supplied fd, reads raw video frames,
/// and writes them as raw BGRA to FFmpeg's stdin.
fn run_pipewire_capture_loop(
    ffmpeg_path: PathBuf,
    config: RecordingConfig,
    session: PortalSession,
    running: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    output_path: PathBuf,
) -> AppResult<PipelineResult> {
    use pipewire as pw;
    use pw::spa::utils::Direction;
    use pw::stream::{StreamBox, StreamFlags, StreamState};

    // Initialise PipeWire on this thread.
    pw::init();

    let mainloop = pw::main_loop::MainLoopBox::new(None).map_err(|e| {
        AppError::Capture(format!("Failed to create PipeWire main loop: {e}"))
    })?;

    let pw_loop = mainloop.loop_();

    let context =
        pw::context::ContextBox::new(pw_loop, None).map_err(|e| {
            AppError::Capture(format!("Failed to create PipeWire context: {e}"))
        })?;

    // Connect to PipeWire using the fd from the portal.
    let core = context
        .connect_fd(session.pipewire_fd, None)
        .map_err(|e| {
            AppError::Capture(format!("Failed to connect to PipeWire via fd: {e}"))
        })?;

    // Shared state for frame exchange between PipeWire callback and our loop.
    let frame_data: Arc<Mutex<Option<PwFrame>>> = Arc::new(Mutex::new(None));
    let frame_ready = Arc::new(AtomicBool::new(false));
    let negotiated_width: Arc<Mutex<Option<u32>>> = Arc::new(Mutex::new(session.width));
    let negotiated_height: Arc<Mutex<Option<u32>>> = Arc::new(Mutex::new(session.height));
    let stream_started = Arc::new(AtomicBool::new(false));
    let stream_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let frame_data_cb = frame_data.clone();
    let frame_ready_cb = frame_ready.clone();
    let width_for_process = negotiated_width.clone();
    let height_for_process = negotiated_height.clone();
    let stream_started_cb = stream_started.clone();
    let stream_error_cb = stream_error.clone();
    let running_cb = running.clone();

    // Create the PipeWire stream.
    let props = pw::properties::properties! {
        *pw::keys::MEDIA_TYPE => "Video",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Screen",
    };

    let stream = StreamBox::new(&core, "screen-dream-portal-capture", props).map_err(|e| {
        AppError::Capture(format!("Failed to create PipeWire stream: {e}"))
    })?;

    // Set up stream event listeners.
    // The default user data type is () since we use add_local_listener::<()>().
    let _listener = stream
        .add_local_listener::<()>()
        .state_changed(move |_stream, _data, _old, new| {
            debug!("PipeWire stream state: {:?}", new);
            match new {
                StreamState::Streaming => {
                    stream_started_cb.store(true, Ordering::SeqCst);
                }
                StreamState::Error(ref msg) => {
                    error!("PipeWire stream error: {}", msg);
                    *stream_error_cb.lock().unwrap() = Some(msg.to_string());
                    running_cb.store(false, Ordering::SeqCst);
                }
                _ => {}
            }
        })
        .param_changed(move |_stream, _data, id, pod| {
            // We only care about the Format parameter.
            if id != pw::spa::param::ParamType::Format.as_raw() {
                return;
            }
            if let Some(_pod) = pod {
                // Try to extract width/height from the SPA format pod.
                // The pod encodes an Object with video format properties.
                // Full SPA pod parsing is complex; we rely on the portal-reported
                // dimensions and first-frame detection as fallback.
                //
                // TODO: Parse SPA_FORMAT_VIDEO_size from the pod for precise
                // negotiated dimensions.
                debug!("PipeWire format negotiated (param_changed)");
            }
        })
        .process(move |stream, _data| {
            if let Some(mut buffer) = stream.dequeue_buffer() {
                let datas = buffer.datas_mut();
                if datas.is_empty() {
                    return;
                }

                let spa_data = &mut datas[0];

                // Read chunk metadata first (immutable borrow).
                let chunk_size = spa_data.chunk().size();
                let chunk_stride = spa_data.chunk().stride();

                // Only process frames with valid data.
                if chunk_size == 0 {
                    return;
                }

                if let Some(slice) = spa_data.data() {
                    let w = width_for_process.lock().unwrap().unwrap_or(0);
                    let h = height_for_process.lock().unwrap().unwrap_or(0);
                    if w > 0 && h > 0 {
                        let stride = chunk_stride;
                        let expected_stride = (w * 4) as i32;

                        let frame_bytes = if stride == expected_stride || stride == 0 {
                            // No padding, copy directly.
                            let expected_size = (w * h * 4) as usize;
                            let copy_size = expected_size.min(slice.len());
                            slice[..copy_size].to_vec()
                        } else {
                            // Handle stride padding: copy row by row,
                            // discarding padding bytes at the end of each row.
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

                        *frame_data_cb.lock().unwrap() = Some(PwFrame {
                            data: frame_bytes,
                            width: w,
                            height: h,
                        });
                        frame_ready_cb.store(true, Ordering::SeqCst);
                    }
                }
            }
        })
        .register()
        .map_err(|e| {
            AppError::Capture(format!("Failed to register PipeWire stream listener: {e}"))
        })?;

    // Connect the stream to the portal's PipeWire node.
    // Pass empty params to let PipeWire auto-negotiate the format.
    stream
        .connect(
            Direction::Input,
            Some(session.node_id),
            StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS,
            &mut [],
        )
        .map_err(|e| {
            AppError::Capture(format!(
                "Failed to connect PipeWire stream to node {}: {e}",
                session.node_id
            ))
        })?;

    // Wait for the stream to enter Streaming state.
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

    // Wait for the first frame to determine actual dimensions.
    let mut first_frame: Option<PwFrame> = None;
    let probe_start = Instant::now();
    while first_frame.is_none() && running.load(Ordering::SeqCst) {
        pw_loop.iterate(Duration::from_millis(50));
        if frame_ready.load(Ordering::SeqCst) {
            first_frame = frame_data.lock().unwrap().take();
            frame_ready.store(false, Ordering::SeqCst);
        }
        if probe_start.elapsed() > Duration::from_secs(5) {
            return Err(AppError::Capture(
                "Timed out waiting for first PipeWire frame".to_string(),
            ));
        }
    }

    let first_frame = first_frame.ok_or_else(|| {
        AppError::Capture("Failed to receive first frame from PipeWire".to_string())
    })?;

    let capture_width = first_frame.width;
    let capture_height = first_frame.height;
    let fps = config.fps;

    info!(
        "Portal capture: {}x{} @ {} FPS, codec={}, crf={}, preset={}, output={}",
        capture_width,
        capture_height,
        fps,
        config.video_codec,
        config.crf,
        config.preset,
        output_path.display()
    );

    // Build FFmpeg command.
    // PipeWire portal streams typically deliver BGRx (or BGRA) format.
    let mut ffmpeg_builder = FfmpegCommand::new()
        .overwrite()
        .arg("-f")
        .arg("rawvideo")
        .pixel_format("bgra")
        .resolution(capture_width, capture_height)
        .framerate(fps)
        .input_pipe();

    // For region capture, add a crop filter.
    if let CaptureSource::Region(ref region) = config.source {
        let crop_w = region.width & !1; // ensure even
        let crop_h = region.height & !1;
        let crop_x = region.x.max(0) as u32;
        let crop_y = region.y.max(0) as u32;
        ffmpeg_builder = ffmpeg_builder.video_filter(&format!(
            "crop={}:{}:{}:{}",
            crop_w, crop_h, crop_x, crop_y
        ));
    }

    let args = ffmpeg_builder
        .arg("-c:v")
        .arg(&config.video_codec)
        .pixel_format("yuv420p")
        .crf(config.crf)
        .preset(&config.preset)
        .output(output_path.to_str().ok_or_else(|| {
            AppError::Capture("Output path is not valid UTF-8".to_string())
        })?)
        .build();

    // Spawn FFmpeg.
    debug!(
        "Spawning FFmpeg: {} {}",
        ffmpeg_path.display(),
        args.join(" ")
    );
    let mut child = Command::new(&ffmpeg_path)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            AppError::FfmpegExecution(format!(
                "Failed to spawn FFmpeg at {}: {e}",
                ffmpeg_path.display()
            ))
        })?;

    let mut stdin = child.stdin.take().ok_or_else(|| {
        AppError::FfmpegExecution("Failed to get FFmpeg stdin".to_string())
    })?;

    let frame_duration = Duration::from_secs_f64(1.0 / fps as f64);
    let mut frames_captured: u64 = 0;
    let start_time = Instant::now();

    // Write the first frame.
    if let Err(e) = stdin.write_all(&first_frame.data) {
        error!("Failed to write first frame: {e}");
        running.store(false, Ordering::SeqCst);
    } else {
        frames_captured += 1;
    }

    // Main capture loop: iterate PipeWire and pipe frames to FFmpeg.
    while running.load(Ordering::SeqCst) {
        let frame_start = Instant::now();

        // Iterate PipeWire to process events and receive frames.
        pw_loop.iterate(Duration::from_millis(1));

        if paused.load(Ordering::SeqCst) {
            // Discard any frame that arrived.
            if frame_ready.load(Ordering::SeqCst) {
                let _ = frame_data.lock().unwrap().take();
                frame_ready.store(false, Ordering::SeqCst);
            }
            thread::sleep(Duration::from_millis(50));
            continue;
        }

        // Check if a new frame is available.
        if frame_ready.load(Ordering::SeqCst) {
            if let Some(frame) = frame_data.lock().unwrap().take() {
                frame_ready.store(false, Ordering::SeqCst);

                if let Err(e) = stdin.write_all(&frame.data) {
                    error!("Failed to write frame to FFmpeg stdin: {e}");
                    break;
                }

                frames_captured += 1;

                if frames_captured % (fps as u64 * 5) == 0 {
                    debug!(
                        "Portal recording progress: {} frames, {:.1}s elapsed",
                        frames_captured,
                        start_time.elapsed().as_secs_f64()
                    );
                }
            }
        }

        // Maintain target frame rate by sleeping if we're ahead.
        let elapsed = frame_start.elapsed();
        if elapsed < frame_duration {
            thread::sleep(frame_duration - elapsed);
        }
    }

    // Disconnect the PipeWire stream.
    let _ = stream.disconnect();

    // Close stdin to signal EOF to FFmpeg.
    drop(stdin);

    // Wait for FFmpeg to finish.
    info!("Waiting for FFmpeg to finalize output...");
    let status = child.wait().map_err(|e| {
        AppError::FfmpegExecution(format!("Failed to wait for FFmpeg: {e}"))
    })?;

    if !status.success() {
        warn!("FFmpeg exited with status: {status}");
    }

    let elapsed = start_time.elapsed();
    info!(
        "Portal recording complete: {} frames in {:.1}s -> {}",
        frames_captured,
        elapsed.as_secs_f64(),
        output_path.display()
    );

    // Clean up PipeWire.
    unsafe { pw::deinit() };

    Ok(PipelineResult {
        output_path,
        frames_captured,
        elapsed,
    })
}

// ---------------------------------------------------------------------------
// Utility: check if portal screencast is available
// ---------------------------------------------------------------------------

/// Check whether the xdg-desktop-portal ScreenCast interface is available.
///
/// This can be called before attempting to use PortalRecorder to determine
/// if the portal backend is a viable option on this system.
pub async fn is_portal_available() -> bool {
    use ashpd::desktop::screencast::Screencast;

    Screencast::new().await.is_ok()
}

/// Synchronous wrapper around `is_portal_available()`.
pub fn is_portal_available_sync() -> bool {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();

    match rt {
        Ok(rt) => rt.block_on(is_portal_available()),
        Err(_) => false,
    }
}
