use std::path::{Path, PathBuf};

use domain::error::{AppError, AppResult};
use domain::ffmpeg::{FfmpegCapabilities, FfmpegProvider};
use tracing::info;

use super::probe;

/// Finds FFmpeg by checking (in order):
/// 1. Bundled sidecar at a known relative path
/// 2. User-configured custom path
/// 3. System PATH
pub struct FfmpegResolver {
    /// Directory where bundled sidecars live (e.g., `<app_dir>/sidecars/`)
    sidecar_dir: Option<PathBuf>,
    /// User-configured override path
    custom_path: Option<PathBuf>,
    /// Cached resolved path (populated on first resolution)
    resolved: std::sync::OnceLock<ResolvedFfmpeg>,
}

#[derive(Debug, Clone)]
struct ResolvedFfmpeg {
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
    source: String,
}

impl FfmpegResolver {
    pub fn new(sidecar_dir: Option<PathBuf>, custom_path: Option<PathBuf>) -> Self {
        FfmpegResolver {
            sidecar_dir,
            custom_path,
            resolved: std::sync::OnceLock::new(),
        }
    }

    fn resolve(&self) -> AppResult<&ResolvedFfmpeg> {
        if let Some(resolved) = self.resolved.get() {
            return Ok(resolved);
        }

        let resolved = self.do_resolve()?;
        // If another thread beat us, that's fine — just use whichever was set first.
        let _ = self.resolved.set(resolved);
        Ok(self.resolved.get().unwrap())
    }

    fn do_resolve(&self) -> AppResult<ResolvedFfmpeg> {
        // 1. Try bundled sidecar
        if let Some(dir) = &self.sidecar_dir {
            let ffmpeg = ffmpeg_binary_name(dir);
            let ffprobe = ffprobe_binary_name(dir);
            if ffmpeg.is_file() {
                info!("Using bundled FFmpeg at {}", ffmpeg.display());
                return Ok(ResolvedFfmpeg {
                    ffmpeg,
                    ffprobe,
                    source: "bundled sidecar".to_string(),
                });
            }
        }

        // 2. Try user-configured path
        if let Some(custom) = &self.custom_path {
            if custom.is_file() {
                let dir = custom.parent().unwrap_or(Path::new("."));
                info!("Using custom FFmpeg at {}", custom.display());
                return Ok(ResolvedFfmpeg {
                    ffmpeg: custom.clone(),
                    ffprobe: ffprobe_binary_name(dir),
                    source: format!("custom path: {}", custom.display()),
                });
            }
        }

        // 3. Try system PATH
        if let Ok(ffmpeg) = which::which("ffmpeg") {
            let ffprobe = which::which("ffprobe").unwrap_or_else(|_| {
                ffmpeg.parent().unwrap_or(Path::new(".")).join("ffprobe")
            });
            info!("Using system FFmpeg at {}", ffmpeg.display());
            return Ok(ResolvedFfmpeg {
                ffmpeg,
                ffprobe,
                source: "system PATH".to_string(),
            });
        }

        Err(AppError::FfmpegNotFound(
            "FFmpeg not found. Install it or configure a custom path in settings."
                .to_string(),
        ))
    }
}

impl FfmpegProvider for FfmpegResolver {
    fn ffmpeg_path(&self) -> AppResult<PathBuf> {
        Ok(self.resolve()?.ffmpeg.clone())
    }

    fn ffprobe_path(&self) -> AppResult<PathBuf> {
        Ok(self.resolve()?.ffprobe.clone())
    }

    fn capabilities(&self) -> AppResult<FfmpegCapabilities> {
        let resolved = self.resolve()?;
        probe::query_capabilities(&resolved.ffmpeg)
    }

    fn source_description(&self) -> String {
        match self.resolve() {
            Ok(r) => r.source.clone(),
            Err(e) => format!("not found: {e}"),
        }
    }
}

fn ffmpeg_binary_name(dir: &Path) -> PathBuf {
    if cfg!(target_os = "windows") {
        dir.join("ffmpeg.exe")
    } else {
        dir.join("ffmpeg")
    }
}

fn ffprobe_binary_name(dir: &Path) -> PathBuf {
    if cfg!(target_os = "windows") {
        dir.join("ffprobe.exe")
    } else {
        dir.join("ffprobe")
    }
}
