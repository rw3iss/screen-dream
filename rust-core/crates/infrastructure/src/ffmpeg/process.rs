use std::path::PathBuf;
use std::process::Stdio;

use domain::error::{AppError, AppResult};
use domain::ffmpeg::FfmpegCommand;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

/// Represents a running or completed FFmpeg process.
pub struct FfmpegProcess {
    child: Child,
}

/// Events emitted by a running FFmpeg process.
#[derive(Debug, Clone)]
pub enum FfmpegEvent {
    /// A line of stderr output (FFmpeg writes progress to stderr).
    StderrLine(String),
    /// The process exited with a code.
    Exited(i32),
    /// The process was killed or crashed.
    Failed(String),
}

impl FfmpegProcess {
    /// Spawn an FFmpeg process with the given command args.
    /// stdin is piped (for feeding raw frames). stderr is captured for progress.
    pub fn spawn(ffmpeg_path: &PathBuf, command: FfmpegCommand) -> AppResult<Self> {
        let args = command.build();
        debug!(
            "Spawning FFmpeg: {} {}",
            ffmpeg_path.display(),
            args.join(" ")
        );

        let child = Command::new(ffmpeg_path)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                AppError::FfmpegExecution(format!(
                    "Failed to spawn FFmpeg at {}: {e}",
                    ffmpeg_path.display()
                ))
            })?;

        Ok(FfmpegProcess { child })
    }

    /// Spawn and run to completion (for batch operations like transcode/export).
    /// Returns a channel that streams stderr lines and a final exit event.
    pub fn spawn_with_progress(
        ffmpeg_path: &PathBuf,
        command: FfmpegCommand,
    ) -> AppResult<(Self, mpsc::Receiver<FfmpegEvent>)> {
        let mut process = Self::spawn(ffmpeg_path, command)?;
        let (tx, rx) = mpsc::channel(64);

        let stderr = process
            .child
            .stderr
            .take()
            .ok_or_else(|| AppError::FfmpegExecution("Failed to capture stderr".to_string()))?;

        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                if tx.send(FfmpegEvent::StderrLine(line)).await.is_err() {
                    break; // Receiver dropped
                }
            }
        });

        Ok((process, rx))
    }

    /// Get a mutable reference to the child's stdin (for piping raw frames).
    pub fn stdin(&mut self) -> Option<&mut tokio::process::ChildStdin> {
        self.child.stdin.as_mut()
    }

    /// Wait for the process to finish and return exit code.
    pub async fn wait(&mut self) -> AppResult<i32> {
        let status =
            self.child.wait().await.map_err(|e| {
                AppError::FfmpegExecution(format!("Failed to wait for FFmpeg: {e}"))
            })?;

        let code = status.code().unwrap_or(-1);
        if code != 0 {
            warn!("FFmpeg exited with code {code}");
        }
        Ok(code)
    }

    /// Kill the FFmpeg process.
    pub async fn kill(&mut self) -> AppResult<()> {
        self.child.kill().await.map_err(|e| {
            error!("Failed to kill FFmpeg process: {e}");
            AppError::FfmpegExecution(format!("Failed to kill FFmpeg: {e}"))
        })
    }
}
