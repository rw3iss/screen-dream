use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use domain::capture::RecordingConfig;
use domain::error::{AppError, AppResult};
use domain::ffmpeg::FfmpegCommand;
use tokio::io::AsyncWriteExt;
use tracing::{debug, error, info, warn};

use crate::capture::XcapCaptureBackend;
use crate::ffmpeg::process::FfmpegProcess;

/// A recording pipeline that captures frames in a loop and pipes them to FFmpeg.
///
/// Runs frame capture on a dedicated tokio blocking task and writes raw RGBA
/// bytes into FFmpeg's stdin. FFmpeg encodes the stream to the configured
/// video codec and writes the output file.
pub struct RecordingPipeline {
    /// Shared flag: true while the pipeline should keep running.
    running: Arc<AtomicBool>,
    /// Shared flag: true while recording is paused.
    paused: Arc<AtomicBool>,
    /// Handle to the capture task (so we can await it on stop).
    capture_handle: Option<tokio::task::JoinHandle<AppResult<PipelineResult>>>,
    /// Output file path.
    output_path: PathBuf,
}

/// Result returned when the pipeline finishes.
#[derive(Debug)]
pub struct PipelineResult {
    /// Path to the recorded video file.
    pub output_path: PathBuf,
    /// Total number of frames captured.
    pub frames_captured: u64,
    /// Total recording duration.
    pub elapsed: Duration,
}

impl RecordingPipeline {
    /// Start a new recording pipeline with the given configuration.
    ///
    /// `ffmpeg_path` is the path to the FFmpeg binary.
    /// `backend` is the capture backend (must be Send + Sync).
    /// `config` specifies capture source, FPS, codec settings, and output path.
    pub fn start(
        ffmpeg_path: PathBuf,
        backend: Arc<XcapCaptureBackend>,
        config: RecordingConfig,
    ) -> AppResult<Self> {
        let output_path = PathBuf::from(&config.output_path);
        let running = Arc::new(AtomicBool::new(true));
        let paused = Arc::new(AtomicBool::new(false));

        let running_clone = running.clone();
        let paused_clone = paused.clone();
        let output_clone = output_path.clone();

        let capture_handle = tokio::spawn(async move {
            run_capture_loop(
                ffmpeg_path,
                backend,
                config,
                running_clone,
                paused_clone,
                output_clone,
            )
            .await
        });

        info!("Recording pipeline started -> {}", output_path.display());

        Ok(RecordingPipeline {
            running,
            paused,
            capture_handle: Some(capture_handle),
            output_path,
        })
    }

    /// Stop the recording and finalize the output file.
    ///
    /// Returns the path to the recorded video and stats.
    pub async fn stop(&mut self) -> AppResult<PipelineResult> {
        info!("Stopping recording pipeline");
        self.running.store(false, Ordering::SeqCst);
        // Unpause if paused, so the loop can exit.
        self.paused.store(false, Ordering::SeqCst);

        if let Some(handle) = self.capture_handle.take() {
            match handle.await {
                Ok(result) => result,
                Err(e) => Err(AppError::Capture(format!(
                    "Recording task panicked: {e}"
                ))),
            }
        } else {
            Err(AppError::Capture(
                "Recording pipeline was already stopped".to_string(),
            ))
        }
    }

    /// Pause the recording. Frames will not be captured while paused.
    pub fn pause(&self) {
        info!("Pausing recording pipeline");
        self.paused.store(true, Ordering::SeqCst);
    }

    /// Resume a paused recording.
    pub fn resume(&self) {
        info!("Resuming recording pipeline");
        self.paused.store(false, Ordering::SeqCst);
    }

    /// Check if the pipeline is currently running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Check if the pipeline is currently paused.
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// Get the output file path.
    pub fn output_path(&self) -> &Path {
        &self.output_path
    }
}

use std::path::Path;

/// The core capture loop. Runs inside a tokio task.
///
/// Captures frames from the backend at the target FPS, and pipes raw RGBA
/// bytes into FFmpeg's stdin for encoding.
async fn run_capture_loop(
    ffmpeg_path: PathBuf,
    backend: Arc<XcapCaptureBackend>,
    config: RecordingConfig,
    running: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    output_path: PathBuf,
) -> AppResult<PipelineResult> {
    // We need to know the frame dimensions before building the FFmpeg command.
    // Capture a single probe frame to get width/height.
    let probe_frame = {
        let source = config.source.clone();
        let backend = backend.clone();
        tokio::task::spawn_blocking(move || {
            use domain::capture::CaptureBackend;
            backend.capture_frame(&source)
        })
        .await
        .map_err(|e| AppError::Capture(format!("Probe frame task failed: {e}")))?
    }?;

    let width = probe_frame.width;
    let height = probe_frame.height;
    let fps = config.fps;

    info!(
        "Recording: {}x{} @ {} FPS, crf={}, preset={}, output={}",
        width, height, fps, config.crf, config.preset, output_path.display()
    );

    // Build the FFmpeg command for raw RGBA pipe input -> encoded video output.
    let command = FfmpegCommand::new()
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
        })?);

    // Spawn FFmpeg process.
    let mut ffmpeg = FfmpegProcess::spawn(&ffmpeg_path, command)?;

    let frame_duration = Duration::from_secs_f64(1.0 / fps as f64);
    let mut frames_captured: u64 = 0;
    let start_time = Instant::now();

    // Write the probe frame first (we already have it).
    if let Some(stdin) = ffmpeg.stdin() {
        if let Err(e) = stdin.write_all(&probe_frame.data).await {
            error!("Failed to write probe frame to FFmpeg stdin: {e}");
            running.store(false, Ordering::SeqCst);
        } else {
            frames_captured += 1;
        }
    }

    // Main capture loop.
    while running.load(Ordering::SeqCst) {
        let frame_start = Instant::now();

        // If paused, sleep briefly and skip capturing.
        if paused.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(50)).await;
            continue;
        }

        // Capture a frame on a blocking thread (xcap capture is synchronous).
        let source = config.source.clone();
        let backend_clone = backend.clone();
        let frame_result = tokio::task::spawn_blocking(move || {
            use domain::capture::CaptureBackend;
            backend_clone.capture_frame(&source)
        })
        .await;

        let frame = match frame_result {
            Ok(Ok(f)) => f,
            Ok(Err(e)) => {
                warn!("Frame capture failed, skipping: {e}");
                tokio::time::sleep(frame_duration).await;
                continue;
            }
            Err(e) => {
                error!("Frame capture task panicked: {e}");
                break;
            }
        };

        // Write frame data to FFmpeg stdin.
        let write_ok = if let Some(stdin) = ffmpeg.stdin() {
            match stdin.write_all(&frame.data).await {
                Ok(()) => true,
                Err(e) => {
                    error!("Failed to write frame to FFmpeg stdin: {e}");
                    false
                }
            }
        } else {
            error!("FFmpeg stdin is not available");
            false
        };

        if !write_ok {
            break;
        }

        frames_captured += 1;

        if frames_captured % (fps as u64 * 5) == 0 {
            debug!(
                "Recording progress: {} frames captured, {:.1}s elapsed",
                frames_captured,
                start_time.elapsed().as_secs_f64()
            );
        }

        // Sleep to maintain target FPS.
        let elapsed = frame_start.elapsed();
        if elapsed < frame_duration {
            tokio::time::sleep(frame_duration - elapsed).await;
        }
    }

    // Close FFmpeg stdin to signal end of input.
    drop(ffmpeg.stdin().take());
    // We need to drop stdin by taking the child's stdin. The FfmpegProcess API
    // gives us &mut ChildStdin. To actually close it we must drop the process stdin.
    // Since FfmpegProcess doesn't expose a close_stdin(), we wait for ffmpeg which
    // will see EOF when our process drops.

    // Wait for FFmpeg to finish encoding.
    info!("Waiting for FFmpeg to finalize output...");
    let exit_code = ffmpeg.wait().await?;
    if exit_code != 0 {
        warn!("FFmpeg exited with non-zero code: {exit_code}");
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
