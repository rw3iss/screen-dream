use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use domain::capture::{CaptureBackend, RecordingConfig};
use domain::error::{AppError, AppResult};
use domain::ffmpeg::FfmpegCommand;
use tracing::{debug, error, info, warn};

/// A recording pipeline that captures frames in a loop and pipes them to FFmpeg.
///
/// Uses a dedicated OS thread (not tokio) since frame capture via xcap is
/// synchronous and we need a real thread to avoid blocking the async runtime.
pub struct RecordingPipeline {
    running: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    capture_thread: Option<thread::JoinHandle<AppResult<PipelineResult>>>,
    output_path: PathBuf,
}

/// Result returned when the pipeline finishes.
#[derive(Debug)]
pub struct PipelineResult {
    pub output_path: PathBuf,
    pub frames_captured: u64,
    pub elapsed: Duration,
}

impl RecordingPipeline {
    /// Start a new recording pipeline.
    ///
    /// Spawns an OS thread that captures frames and pipes raw RGBA to FFmpeg.
    pub fn start(
        ffmpeg_path: PathBuf,
        backend: Arc<dyn CaptureBackend>,
        config: RecordingConfig,
    ) -> AppResult<Self> {
        let output_path = PathBuf::from(&config.output_path);
        let running = Arc::new(AtomicBool::new(true));
        let paused = Arc::new(AtomicBool::new(false));

        let running_clone = running.clone();
        let paused_clone = paused.clone();
        let output_clone = output_path.clone();

        let capture_thread = thread::spawn(move || {
            run_capture_loop(
                ffmpeg_path,
                backend,
                config,
                running_clone,
                paused_clone,
                output_clone,
            )
        });

        info!("Recording pipeline started -> {}", output_path.display());

        Ok(RecordingPipeline {
            running,
            paused,
            capture_thread: Some(capture_thread),
            output_path,
        })
    }

    /// Stop the recording and wait for the capture thread to finish.
    pub fn stop(&mut self) -> AppResult<PipelineResult> {
        info!("Stopping recording pipeline");
        self.running.store(false, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);

        if let Some(handle) = self.capture_thread.take() {
            match handle.join() {
                Ok(result) => result,
                Err(_) => Err(AppError::Capture(
                    "Recording thread panicked".to_string(),
                )),
            }
        } else {
            Err(AppError::Capture(
                "Recording pipeline was already stopped".to_string(),
            ))
        }
    }

    pub fn pause(&self) {
        info!("Pausing recording pipeline");
        self.paused.store(true, Ordering::SeqCst);
    }

    pub fn resume(&self) {
        info!("Resuming recording pipeline");
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
}

/// The core capture loop. Runs on a dedicated OS thread.
///
/// Captures frames synchronously from xcap, writes raw RGBA bytes to
/// FFmpeg's stdin via a standard process pipe.
fn run_capture_loop(
    ffmpeg_path: PathBuf,
    backend: Arc<dyn CaptureBackend>,
    config: RecordingConfig,
    running: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    output_path: PathBuf,
) -> AppResult<PipelineResult> {
    // Capture a probe frame to get dimensions.
    let probe_frame = backend.capture_frame(&config.source)?;
    let width = probe_frame.width;
    let height = probe_frame.height;
    let fps = config.fps;

    // Ensure dimensions are even (required by libx264).
    let enc_width = width & !1;
    let enc_height = height & !1;

    info!(
        "Recording: {}x{} (enc: {}x{}) @ {} FPS, codec={}, crf={}, preset={}, output={}",
        width, height, enc_width, enc_height, fps,
        config.video_codec, config.crf, config.preset, output_path.display()
    );

    // Build FFmpeg arguments.
    let args = FfmpegCommand::new()
        .overwrite()
        .arg("-f")
        .arg("rawvideo")
        .pixel_format("rgba")
        .resolution(width, height)
        .framerate(fps)
        .input_pipe()
        .arg("-c:v")
        .arg(&config.video_codec)
        .pixel_format("yuv420p")
        .crf(config.crf)
        .preset(&config.preset)
        .output(output_path.to_str().ok_or_else(|| {
            AppError::Capture("Output path is not valid UTF-8".to_string())
        })?)
        .build();

    // Spawn FFmpeg as a standard process (not tokio).
    debug!("Spawning FFmpeg: {} {}", ffmpeg_path.display(), args.join(" "));
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

    // Write the probe frame first.
    if let Err(e) = stdin.write_all(&probe_frame.data) {
        error!("Failed to write probe frame: {e}");
        running.store(false, Ordering::SeqCst);
    } else {
        frames_captured += 1;
    }

    // Main capture loop.
    while running.load(Ordering::SeqCst) {
        let frame_start = Instant::now();

        if paused.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(50));
            continue;
        }

        // Capture a frame (synchronous).
        let frame = match backend.capture_frame(&config.source) {
            Ok(f) => f,
            Err(e) => {
                warn!("Frame capture failed, skipping: {e}");
                thread::sleep(frame_duration);
                continue;
            }
        };

        // Write raw RGBA to FFmpeg stdin.
        if let Err(e) = stdin.write_all(&frame.data) {
            error!("Failed to write frame to FFmpeg stdin: {e}");
            break;
        }

        frames_captured += 1;

        if frames_captured % (fps as u64 * 5) == 0 {
            debug!(
                "Recording progress: {} frames, {:.1}s elapsed",
                frames_captured,
                start_time.elapsed().as_secs_f64()
            );
        }

        // Sleep to maintain target FPS.
        let elapsed = frame_start.elapsed();
        if elapsed < frame_duration {
            thread::sleep(frame_duration - elapsed);
        }
    }

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
        "Recording complete: {} frames in {:.1}s -> {}",
        frames_captured,
        elapsed.as_secs_f64(),
        output_path.display()
    );

    Ok(PipelineResult {
        output_path,
        frames_captured,
        elapsed,
    })
}
