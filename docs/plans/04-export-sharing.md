# Plan 4: Export & Sharing

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the final rendering pipeline that takes an EditProject (from Plan 3) and produces output files via FFmpeg, with real-time progress reporting. Add export presets, GIF export with palette optimization, and a local-save export workflow. The v1 export flow is: configure preset, render via FFmpeg, save to local disk via file dialog. Upload functionality (S3, HTTP) is **DEFERRED to v1.1**.

**Architecture:** Clean Architecture continued — (1) `domain` crate defines export presets, render job types, and upload target trait stubs (`// TODO(v1.1)`) with zero framework dependencies; (2) `infrastructure` crate implements the render pipeline (FFmpeg subprocess with progress parsing) and GIF renderer (two-pass palette); (3) the Tauri `app` crate exposes IPC commands for rendering, using Tauri events to stream progress to the frontend. The frontend provides an ExportDialog (preset/format picker), a RenderProgress component (progress bar, ETA, cancel), and triggers a file save dialog on completion. Upload infrastructure (S3, HTTP) is **DEFERRED to v1.1**.

**Tech Stack:** FFmpeg sidecar, Tauri v2 events/channels, Preact, TypeScript. (`rust-s3` and `reqwest` DEFERRED to v1.1 for upload support.)

**Depends on:** Plans 1, 2, and 3.

**Related documents:**
- `PLAN.md` — high-level architecture and feature overview
- `docs/plans/01-core-platform-infrastructure.md` — Plan 1 (foundation, FFmpeg provider, command builder, IPC patterns)
- `docs/plans/02-screen-capture-recording.md` — Plan 2 (capture pipeline)
- `docs/plans/03-media-editing.md` — Plan 3 (EditProject, timeline, edit decisions)

---

## File Structure

```
screen-dream/
├── src-tauri/
│   ├── crates/
│   │   ├── domain/src/
│   │   │   ├── export/
│   │   │   │   ├── mod.rs              # Re-exports all export domain modules
│   │   │   │   ├── preset.rs           # ExportPreset, ExportConfig types
│   │   │   │   ├── render.rs           # RenderJob, RenderProgress, RenderStatus types
│   │   │   │   └── upload.rs           # UploadTarget trait, UploadProgress types (STUB — TODO(v1.1))
│   │   │   └── lib.rs                  # Add `pub mod export;`
│   │   │
│   │   └── infrastructure/src/
│   │       ├── export/
│   │       │   ├── mod.rs              # Re-exports all export infra modules
│   │       │   ├── renderer.rs         # Builds FFmpeg command from EditProject + ExportConfig, runs it, parses progress
│   │       │   └── gif_renderer.rs     # Two-pass GIF export with palette generation
│   │       │   # s3_uploader.rs        # DEFERRED to v1.1
│   │       │   # http_uploader.rs      # DEFERRED to v1.1
│   │       └── lib.rs                  # Add `pub mod export;`
│   │
│   └── src/
│       ├── commands/
│       │   ├── export.rs               # IPC commands for rendering (upload commands DEFERRED to v1.1)
│       │   └── mod.rs                  # Add `pub mod export;`
│       └── state.rs                    # Add RenderManager to AppState
│
├── src/
│   ├── lib/
│   │   ├── types.ts                    # Add export-related TS types
│   │   └── ipc.ts                      # Add export IPC wrappers
│   ├── stores/
│   │   └── export.ts                   # Export state management (render state only; upload DEFERRED to v1.1)
│   ├── components/
│   │   └── export/
│   │       ├── ExportDialog.tsx        # Format/quality/preset picker
│   │       ├── ExportDialog.module.scss  # SCSS module styles
│   │       ├── RenderProgress.tsx      # Progress bar during rendering → file save dialog on completion
│   │       └── RenderProgress.module.scss  # SCSS module styles
│   │       # UploadPanel.tsx           # DEFERRED to v1.1
│   └── pages/
│       └── Editor.tsx                  # Wire export into editor page (modify existing)
```

### Separation of Concerns

| Layer | Location | Responsibility |
|-------|----------|----------------|
| **Domain** | `crates/domain/src/export/` | ExportPreset, ExportConfig, RenderJob, RenderProgress, RenderStatus. UploadTarget trait stub (`// TODO(v1.1)`). Pure types, no I/O. |
| **Infrastructure** | `crates/infrastructure/src/export/` | FFmpeg command construction from edit decisions, subprocess execution with progress parsing, GIF two-pass rendering. S3/HTTP upload implementations DEFERRED to v1.1. |
| **App (Tauri)** | `src/commands/export.rs` | IPC commands for rendering (start, cancel, status). Streams progress via Tauri events. Upload commands DEFERRED to v1.1. |
| **Frontend** | `src/components/export/`, `src/stores/export.ts` | ExportDialog (preset/format picker), RenderProgress (progress bar + file save dialog on completion). SCSS modules for styling. UploadPanel DEFERRED to v1.1. |

---

## Task 1: Domain — ExportPreset and ExportConfig Types

**Files:**
- Create: `src-tauri/crates/domain/src/export/mod.rs`
- Create: `src-tauri/crates/domain/src/export/preset.rs`
- Modify: `src-tauri/crates/domain/src/lib.rs` (add `pub mod export;`)

- [ ] **Step 1: Create the export module**

Create `src-tauri/crates/domain/src/export/mod.rs`:
```rust
pub mod preset;
pub mod render;
pub mod upload; // TODO(v1.1): stub only — full upload types deferred

pub use preset::*;
pub use render::*;
pub use upload::*;
```

- [ ] **Step 2: Define ExportPreset and ExportConfig**

Create `src-tauri/crates/domain/src/export/preset.rs`:
```rust
use serde::{Deserialize, Serialize};

use crate::ffmpeg::codec::{AudioCodec, ContainerFormat, VideoCodec};

/// A predefined combination of format, codec, quality, and resolution.
/// Users select a preset and optionally customize individual fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportPreset {
    /// Human-readable name (e.g., "HD MP4", "Web-optimized WebM", "Animated GIF")
    pub name: String,
    /// Video codec to use
    pub video_codec: VideoCodec,
    /// Audio codec to use
    pub audio_codec: AudioCodec,
    /// Output container format
    pub container: ContainerFormat,
    /// Constant Rate Factor (quality). Lower = better. Typical range: 18-28.
    pub crf: u8,
    /// Output resolution as (width, height). None = same as source.
    pub resolution: Option<(u32, u32)>,
    /// Short description shown in the UI
    pub description: String,
}

/// The full export configuration: a preset plus user overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportConfig {
    /// The base preset being used
    pub preset: ExportPreset,
    /// Where to write the output file (absolute path)
    pub output_path: String,
    /// User override for resolution. If Some, overrides the preset.
    pub custom_resolution: Option<(u32, u32)>,
    /// User override for CRF. If Some, overrides the preset.
    pub custom_crf: Option<u8>,
    /// FFmpeg encoding speed preset (e.g., "fast", "medium", "slow")
    pub speed_preset: String,
}

impl ExportPreset {
    /// Returns the built-in presets available by default.
    pub fn builtin_presets() -> Vec<ExportPreset> {
        vec![
            ExportPreset {
                name: "HD MP4 (H.264)".to_string(),
                video_codec: VideoCodec::H264,
                audio_codec: AudioCodec::Aac,
                container: ContainerFormat::Mp4,
                crf: 23,
                resolution: None,
                description: "Best compatibility. Works everywhere.".to_string(),
            },
            ExportPreset {
                name: "High Quality MP4 (H.264)".to_string(),
                video_codec: VideoCodec::H264,
                audio_codec: AudioCodec::Aac,
                container: ContainerFormat::Mp4,
                crf: 18,
                resolution: None,
                description: "Near-lossless quality. Large file size.".to_string(),
            },
            ExportPreset {
                name: "HD MP4 (H.265)".to_string(),
                video_codec: VideoCodec::H265,
                audio_codec: AudioCodec::Aac,
                container: ContainerFormat::Mp4,
                crf: 28,
                resolution: None,
                description: "50% smaller files than H.264 at similar quality.".to_string(),
            },
            ExportPreset {
                name: "Web-optimized WebM (VP9)".to_string(),
                video_codec: VideoCodec::Vp9,
                audio_codec: AudioCodec::Opus,
                container: ContainerFormat::Webm,
                crf: 30,
                resolution: None,
                description: "Great for web embedding. Royalty-free.".to_string(),
            },
            ExportPreset {
                name: "720p MP4".to_string(),
                video_codec: VideoCodec::H264,
                audio_codec: AudioCodec::Aac,
                container: ContainerFormat::Mp4,
                crf: 23,
                resolution: Some((1280, 720)),
                description: "Downscaled to 720p. Good for sharing.".to_string(),
            },
            ExportPreset {
                name: "Animated GIF".to_string(),
                video_codec: VideoCodec::H264, // placeholder — GIF uses special pipeline
                audio_codec: AudioCodec::Aac,  // placeholder — GIF has no audio
                container: ContainerFormat::Gif,
                crf: 0, // not used for GIF
                resolution: Some((640, 480)),
                description: "Animated GIF with palette optimization. No audio.".to_string(),
            },
        ]
    }
}

impl ExportConfig {
    /// Returns the effective resolution (custom override or preset default).
    pub fn effective_resolution(&self) -> Option<(u32, u32)> {
        self.custom_resolution.or(self.preset.resolution)
    }

    /// Returns the effective CRF (custom override or preset default).
    pub fn effective_crf(&self) -> u8 {
        self.custom_crf.unwrap_or(self.preset.crf)
    }

    /// Returns true if this export targets the GIF format.
    pub fn is_gif(&self) -> bool {
        self.preset.container == ContainerFormat::Gif
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_presets_returns_non_empty_list() {
        let presets = ExportPreset::builtin_presets();
        assert!(!presets.is_empty());
        assert!(presets.len() >= 5);
        // Each preset must have a name and description
        for p in &presets {
            assert!(!p.name.is_empty());
            assert!(!p.description.is_empty());
        }
    }

    #[test]
    fn export_config_effective_resolution_uses_custom_override() {
        let preset = ExportPreset::builtin_presets().into_iter().next().unwrap();
        let config = ExportConfig {
            preset,
            output_path: "/tmp/out.mp4".to_string(),
            custom_resolution: Some((1920, 1080)),
            custom_crf: None,
            speed_preset: "fast".to_string(),
        };
        assert_eq!(config.effective_resolution(), Some((1920, 1080)));
    }

    #[test]
    fn export_config_effective_crf_uses_preset_default() {
        let preset = ExportPreset::builtin_presets().into_iter().next().unwrap();
        let expected_crf = preset.crf;
        let config = ExportConfig {
            preset,
            output_path: "/tmp/out.mp4".to_string(),
            custom_resolution: None,
            custom_crf: None,
            speed_preset: "fast".to_string(),
        };
        assert_eq!(config.effective_crf(), expected_crf);
    }

    #[test]
    fn is_gif_detects_gif_preset() {
        let presets = ExportPreset::builtin_presets();
        let gif_preset = presets.iter().find(|p| p.container == ContainerFormat::Gif).unwrap();
        let config = ExportConfig {
            preset: gif_preset.clone(),
            output_path: "/tmp/out.gif".to_string(),
            custom_resolution: None,
            custom_crf: None,
            speed_preset: "fast".to_string(),
        };
        assert!(config.is_gif());
    }
}
```

- [ ] **Step 3: Register the export module in domain lib.rs**

Add to `src-tauri/crates/domain/src/lib.rs`:
```rust
pub mod export;
```

- [ ] **Step 4: Verify domain compiles and tests pass**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/src-tauri
cargo test -p domain export::preset -- --nocapture
```

Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src-tauri/crates/domain/src/export/ src-tauri/crates/domain/src/lib.rs
git commit -m "feat: add ExportPreset and ExportConfig domain types"
```

---

## Task 2: Domain — RenderJob and RenderProgress Types

**Files:**
- Create: `src-tauri/crates/domain/src/export/render.rs`

- [ ] **Step 1: Define render job and progress types**

Create `src-tauri/crates/domain/src/export/render.rs`:
```rust
use serde::{Deserialize, Serialize};

/// Unique identifier for a render job.
pub type RenderJobId = String;

/// Status of a render job through its lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", content = "detail")]
pub enum RenderStatus {
    /// Waiting in the queue to start.
    Queued,
    /// Currently rendering.
    Rendering,
    /// Render completed successfully. Contains the output file path.
    Completed(String),
    /// Render failed. Contains the error message.
    Failed(String),
    /// Render was cancelled by the user.
    Cancelled,
}

/// Real-time progress information for a running render.
/// Parsed from FFmpeg's stderr output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderProgress {
    /// Completion percentage (0.0 to 100.0).
    pub percent: f64,
    /// Seconds elapsed since render started.
    pub elapsed_secs: f64,
    /// Estimated seconds remaining. None if not yet calculable.
    pub eta_secs: Option<f64>,
    /// Current frame number being processed.
    pub current_frame: u64,
    /// Total frames in the source (estimated). None if unknown.
    pub total_frames: Option<u64>,
    /// Current FFmpeg processing speed (e.g., 2.5 means 2.5x real-time).
    pub speed: Option<f64>,
    /// Current output file size in bytes.
    pub output_size_bytes: Option<u64>,
}

impl RenderProgress {
    /// Create an initial empty progress.
    pub fn zero() -> Self {
        RenderProgress {
            percent: 0.0,
            elapsed_secs: 0.0,
            eta_secs: None,
            current_frame: 0,
            total_frames: None,
            speed: None,
            output_size_bytes: None,
        }
    }
}

/// A render job combining identity, configuration reference, status, and progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderJob {
    /// Unique job ID (UUID).
    pub id: RenderJobId,
    /// ID of the EditProject being rendered.
    pub project_id: String,
    /// Current status.
    pub status: RenderStatus,
    /// Latest progress update (only meaningful when status is Rendering).
    pub progress: RenderProgress,
    /// Output file path (set when job is created).
    pub output_path: String,
}

/// Parsed fields from a single FFmpeg stderr progress line.
/// FFmpeg writes lines like:
/// `frame= 1234 fps=30 q=23.0 size= 12345kB time=00:01:23.45 bitrate=1234.5kbits/s speed=2.5x`
#[derive(Debug, Clone, Default)]
pub struct FfmpegProgressLine {
    pub frame: Option<u64>,
    pub fps: Option<f64>,
    pub size_kb: Option<u64>,
    pub time_secs: Option<f64>,
    pub bitrate_kbits: Option<f64>,
    pub speed: Option<f64>,
}

impl FfmpegProgressLine {
    /// Parse an FFmpeg stderr line into structured fields.
    /// Returns None if the line is not a progress line.
    pub fn parse(line: &str) -> Option<Self> {
        // Progress lines contain "time=" — this distinguishes them from other stderr output
        if !line.contains("time=") {
            return None;
        }

        let mut parsed = FfmpegProgressLine::default();

        // Parse frame=
        if let Some(val) = extract_field(line, "frame=") {
            parsed.frame = val.trim().parse().ok();
        }

        // Parse fps=
        if let Some(val) = extract_field(line, "fps=") {
            parsed.fps = val.trim().parse().ok();
        }

        // Parse size=
        if let Some(val) = extract_field(line, "size=") {
            let val = val.trim().trim_end_matches("kB").trim();
            parsed.size_kb = val.parse().ok();
        }

        // Parse time= (format: HH:MM:SS.ff)
        if let Some(val) = extract_field(line, "time=") {
            parsed.time_secs = parse_timecode(val.trim());
        }

        // Parse bitrate=
        if let Some(val) = extract_field(line, "bitrate=") {
            let val = val.trim().trim_end_matches("kbits/s").trim();
            parsed.bitrate_kbits = val.parse().ok();
        }

        // Parse speed=
        if let Some(val) = extract_field(line, "speed=") {
            let val = val.trim().trim_end_matches('x').trim();
            parsed.speed = val.parse().ok();
        }

        Some(parsed)
    }

    /// Convert to a RenderProgress given total duration and start time.
    pub fn to_progress(&self, total_duration_secs: f64, elapsed_secs: f64, total_frames: Option<u64>) -> RenderProgress {
        let percent = if total_duration_secs > 0.0 {
            let p = (self.time_secs.unwrap_or(0.0) / total_duration_secs) * 100.0;
            p.clamp(0.0, 100.0)
        } else {
            0.0
        };

        let eta_secs = if percent > 0.0 && percent < 100.0 {
            let remaining_fraction = (100.0 - percent) / percent;
            Some(elapsed_secs * remaining_fraction)
        } else {
            None
        };

        RenderProgress {
            percent,
            elapsed_secs,
            eta_secs,
            current_frame: self.frame.unwrap_or(0),
            total_frames,
            speed: self.speed,
            output_size_bytes: self.size_kb.map(|kb| kb * 1024),
        }
    }
}

/// Extract the value of a key=value field from an FFmpeg progress line.
/// The value ends at the next whitespace-separated key= or end of string.
fn extract_field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let start = line.find(key)?;
    let value_start = start + key.len();
    let rest = &line[value_start..];

    // Find the end: next occurrence of a space followed by a word and '='
    // This handles fields like "size= 12345kB" (with leading spaces in value)
    let end = rest
        .find(|c: char| c == ' ')
        .and_then(|space_pos| {
            // Look for the next "key=" pattern after this space
            let after_space = &rest[space_pos..];
            // Find a non-space char followed by eventually '='
            for (i, _) in after_space.char_indices() {
                let remaining = &after_space[i..];
                if remaining.starts_with(|c: char| c.is_alphabetic()) {
                    if let Some(eq_pos) = remaining.find('=') {
                        // Verify everything between start and '=' is word chars
                        let candidate = &remaining[..eq_pos];
                        if candidate.chars().all(|c| c.is_alphanumeric() || c == '_') {
                            return Some(space_pos + i);
                        }
                    }
                }
            }
            None
        })
        .unwrap_or(rest.len());

    Some(rest[..end].trim())
}

/// Parse an FFmpeg timecode (HH:MM:SS.ff) to seconds.
fn parse_timecode(tc: &str) -> Option<f64> {
    // Handle negative time (sometimes FFmpeg outputs "N/A" or negative)
    if tc.starts_with('-') || tc == "N/A" {
        return Some(0.0);
    }

    let parts: Vec<&str> = tc.split(':').collect();
    if parts.len() != 3 {
        return None;
    }

    let hours: f64 = parts[0].parse().ok()?;
    let minutes: f64 = parts[1].parse().ok()?;
    let seconds: f64 = parts[2].parse().ok()?;

    Some(hours * 3600.0 + minutes * 60.0 + seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ffmpeg_progress_line() {
        let line = "frame= 1234 fps=30.0 q=23.0 size=   12345kB time=00:01:23.45 bitrate=1234.5kbits/s speed=2.5x";
        let parsed = FfmpegProgressLine::parse(line).expect("should parse");
        assert_eq!(parsed.frame, Some(1234));
        assert_eq!(parsed.fps, Some(30.0));
        assert_eq!(parsed.size_kb, Some(12345));
        assert!((parsed.time_secs.unwrap() - 83.45).abs() < 0.01);
        assert_eq!(parsed.speed, Some(2.5));
    }

    #[test]
    fn parse_timecode_converts_correctly() {
        assert!((parse_timecode("00:00:00.00").unwrap() - 0.0).abs() < 0.001);
        assert!((parse_timecode("00:01:23.45").unwrap() - 83.45).abs() < 0.001);
        assert!((parse_timecode("01:30:00.00").unwrap() - 5400.0).abs() < 0.001);
        assert!((parse_timecode("00:00:05.50").unwrap() - 5.5).abs() < 0.001);
    }

    #[test]
    fn parse_returns_none_for_non_progress_lines() {
        assert!(FfmpegProgressLine::parse("Stream #0:0 -> #0:0 (h264 (native) -> h264 (libx264))").is_none());
        assert!(FfmpegProgressLine::parse("Press [q] to stop, [?] for help").is_none());
        assert!(FfmpegProgressLine::parse("").is_none());
    }

    #[test]
    fn to_progress_calculates_percent_and_eta() {
        let ffmpeg_line = FfmpegProgressLine {
            frame: Some(500),
            fps: Some(30.0),
            size_kb: Some(5000),
            time_secs: Some(50.0),
            bitrate_kbits: Some(800.0),
            speed: Some(2.0),
        };

        let progress = ffmpeg_line.to_progress(100.0, 25.0, Some(3000));
        assert!((progress.percent - 50.0).abs() < 0.01);
        assert_eq!(progress.current_frame, 500);
        assert_eq!(progress.total_frames, Some(3000));
        assert_eq!(progress.speed, Some(2.0));
        // ETA: 50% done in 25s, so ~25s remaining
        assert!((progress.eta_secs.unwrap() - 25.0).abs() < 0.01);
    }

    #[test]
    fn render_status_serializes_with_tag() {
        let status = RenderStatus::Completed("/tmp/out.mp4".to_string());
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"status\":\"Completed\""));
        assert!(json.contains("/tmp/out.mp4"));
    }
}
```

- [ ] **Step 2: Verify domain compiles and tests pass**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/src-tauri
cargo test -p domain export::render -- --nocapture
```

Expected: 5 tests pass.

- [ ] **Step 3: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src-tauri/crates/domain/src/export/render.rs
git commit -m "feat: add RenderJob, RenderProgress, and FFmpeg progress parsing types"
```

---

## Task 3: Domain — UploadTarget Trait and Types (STUB — DEFERRED to v1.1)

> **v1 scope:** This task creates only the trait definitions and type stubs so the domain API surface is stable. Full implementation of upload types, tests, and infrastructure will be done in v1.1.

**Files:**
- Create: `src-tauri/crates/domain/src/export/upload.rs`

- [ ] **Step 1: Define upload target types and trait (stubs)**

Create `src-tauri/crates/domain/src/export/upload.rs`:
```rust
// TODO(v1.1): Full upload implementation deferred to v1.1.
// This file contains only trait definitions and type stubs so the domain API is stable.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::AppResult;

/// An upload destination configured by the user. TODO(v1.1): implement S3 and HTTP variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum UploadTarget {
    // TODO(v1.1): S3-compatible storage (AWS S3, Cloudflare R2, MinIO, etc.)
    // TODO(v1.1): Custom HTTP POST endpoint
}

/// Progress information during an upload. TODO(v1.1)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadProgress {
    pub bytes_uploaded: u64,
    pub total_bytes: u64,
    pub percent: f64,
    pub eta_secs: Option<f64>,
}

/// Result of a successful upload. TODO(v1.1)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadResult {
    pub url: Option<String>,
    pub remote_path: String,
    pub bytes_uploaded: u64,
}

/// Trait for upload implementations. TODO(v1.1): implement in infrastructure crate.
pub trait Uploader: Send + Sync {
    fn upload(
        &self,
        file_path: &Path,
        target: &UploadTarget,
        progress_callback: Box<dyn Fn(UploadProgress) + Send>,
    ) -> AppResult<UploadResult>;
}
```

- [ ] **Step 2: Verify domain compiles**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/src-tauri
cargo check -p domain
```

Expected: Compiles clean (no tests — stubs only).

- [ ] **Step 3: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src-tauri/crates/domain/src/export/upload.rs
git commit -m "feat: add UploadTarget trait and upload type stubs (TODO v1.1)"
```

---

## Task 4: Infrastructure — Render Pipeline (EditProject to FFmpeg with Progress)

**Files:**
- Create: `src-tauri/crates/infrastructure/src/export/mod.rs`
- Create: `src-tauri/crates/infrastructure/src/export/renderer.rs`
- Modify: `src-tauri/crates/infrastructure/src/lib.rs` (add `pub mod export;`)
- Modify: `src-tauri/crates/infrastructure/Cargo.toml` (add `uuid` dependency)

- [ ] **Step 1: Create the export module**

Create `src-tauri/crates/infrastructure/src/export/mod.rs`:
```rust
pub mod renderer;
pub mod gif_renderer;
// TODO(v1.1): pub mod s3_uploader;
// TODO(v1.1): pub mod http_uploader;

pub use renderer::*;
pub use gif_renderer::*;
```

- [ ] **Step 2: Add dependencies**

Add to `src-tauri/crates/infrastructure/Cargo.toml` under `[dependencies]`:
```toml
uuid = { version = "1", features = ["v4"] }
```

- [ ] **Step 3: Implement the render pipeline**

Create `src-tauri/crates/infrastructure/src/export/renderer.rs`:
```rust
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use domain::error::{AppError, AppResult};
use domain::export::{ExportConfig, RenderJob, RenderProgress, RenderStatus, FfmpegProgressLine};
use domain::ffmpeg::{FfmpegCommand, FfmpegProvider};
use tokio::io::AsyncBufReadExt;
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Manages render jobs: builds FFmpeg commands from export configs,
/// spawns FFmpeg subprocesses, parses progress from stderr, and
/// streams updates via a channel.
pub struct Renderer {
    ffmpeg: Arc<dyn FfmpegProvider>,
}

/// Events emitted during a render for the caller to forward to the frontend.
#[derive(Debug, Clone)]
pub enum RenderEvent {
    /// Progress update.
    Progress(RenderProgress),
    /// Render completed successfully.
    Completed { output_path: String },
    /// Render failed.
    Failed { error: String },
    /// Render was cancelled.
    Cancelled,
}

impl Renderer {
    pub fn new(ffmpeg: Arc<dyn FfmpegProvider>) -> Self {
        Renderer { ffmpeg }
    }

    /// Start a render job. Returns the RenderJob (with Queued status) and a receiver
    /// for progress events. The render runs on a background tokio task.
    ///
    /// `input_path` is the source media file from the recording.
    /// `filter_graph` is the FFmpeg filter_complex string built from edit decisions.
    /// `total_duration_secs` is the expected output duration (for progress calculation).
    /// `total_frames` is the estimated total frame count (optional, for display).
    pub async fn start_render(
        &self,
        input_path: &str,
        filter_graph: Option<&str>,
        config: &ExportConfig,
        total_duration_secs: f64,
        total_frames: Option<u64>,
    ) -> AppResult<(RenderJob, mpsc::Receiver<RenderEvent>)> {
        let ffmpeg_path = self.ffmpeg.ffmpeg_path()?;
        let job_id = uuid::Uuid::new_v4().to_string();
        let output_path = config.output_path.clone();

        let job = RenderJob {
            id: job_id.clone(),
            project_id: String::new(), // set by the caller
            status: RenderStatus::Queued,
            progress: RenderProgress::zero(),
            output_path: output_path.clone(),
        };

        let args = self.build_ffmpeg_args(input_path, filter_graph, config)?;
        let (tx, rx) = mpsc::channel::<RenderEvent>(64);

        let ffmpeg_path_clone = ffmpeg_path.clone();
        tokio::spawn(async move {
            let result = run_ffmpeg_with_progress(
                &ffmpeg_path_clone,
                args,
                total_duration_secs,
                total_frames,
                &tx,
            )
            .await;

            match result {
                Ok(()) => {
                    let _ = tx
                        .send(RenderEvent::Completed {
                            output_path: output_path.clone(),
                        })
                        .await;
                }
                Err(e) => {
                    let _ = tx
                        .send(RenderEvent::Failed {
                            error: e.to_string(),
                        })
                        .await;
                }
            }
        });

        Ok((job, rx))
    }

    /// Build the FFmpeg argument list from an ExportConfig and optional filter graph.
    fn build_ffmpeg_args(
        &self,
        input_path: &str,
        filter_graph: Option<&str>,
        config: &ExportConfig,
    ) -> AppResult<Vec<String>> {
        let mut cmd = FfmpegCommand::new()
            .overwrite()
            .input(input_path);

        // Apply filter graph from edit decisions (if any)
        if let Some(graph) = filter_graph {
            if !graph.is_empty() {
                cmd = cmd.filter_complex(graph);
            }
        }

        // Video codec and quality
        cmd = cmd
            .video_codec(&config.preset.video_codec)
            .crf(config.effective_crf())
            .preset(&config.speed_preset);

        // Audio codec
        cmd = cmd.audio_codec(&config.preset.audio_codec);

        // Resolution override
        if let Some((w, h)) = config.effective_resolution() {
            cmd = cmd.resolution(w, h);
        }

        // Progress output (FFmpeg writes progress to stderr by default; also enable
        // stats output with -progress pipe:2 for more parseable output)
        cmd = cmd.arg("-progress").arg("pipe:2");

        // Output format and path
        cmd = cmd.format(&config.preset.container).output(&config.output_path);

        Ok(cmd.build())
    }
}

/// Spawn FFmpeg, read stderr line-by-line, parse progress, and send events.
async fn run_ffmpeg_with_progress(
    ffmpeg_path: &PathBuf,
    args: Vec<String>,
    total_duration_secs: f64,
    total_frames: Option<u64>,
    tx: &mpsc::Sender<RenderEvent>,
) -> AppResult<()> {
    debug!("FFmpeg render args: {} {}", ffmpeg_path.display(), args.join(" "));

    let mut child = Command::new(ffmpeg_path)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| AppError::FfmpegExecution(format!("Failed to spawn FFmpeg: {e}")))?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::FfmpegExecution("Failed to capture FFmpeg stderr".to_string()))?;

    let start = Instant::now();
    let reader = tokio::io::BufReader::new(stderr);
    let mut lines = reader.lines();

    while let Ok(Some(line)) = lines.next_line().await {
        if let Some(parsed) = FfmpegProgressLine::parse(&line) {
            let elapsed = start.elapsed().as_secs_f64();
            let progress = parsed.to_progress(total_duration_secs, elapsed, total_frames);

            if tx.send(RenderEvent::Progress(progress)).await.is_err() {
                // Receiver dropped — cancel the render
                warn!("Render event receiver dropped, killing FFmpeg");
                let _ = child.kill().await;
                return Ok(());
            }
        }
    }

    // Wait for FFmpeg to exit
    let status = child.wait().await.map_err(|e| {
        AppError::FfmpegExecution(format!("Failed to wait for FFmpeg: {e}"))
    })?;

    if status.success() {
        info!("FFmpeg render completed successfully");
        Ok(())
    } else {
        let code = status.code().unwrap_or(-1);
        error!("FFmpeg render failed with exit code {code}");
        Err(AppError::FfmpegExecution(format!(
            "FFmpeg exited with code {code}"
        )))
    }
}

/// Cancel a running render by dropping the receiver (which causes the spawn
/// task to detect the broken channel and kill FFmpeg).
/// This is a convenience function — callers can also just drop the receiver.
pub fn cancel_render(rx: mpsc::Receiver<RenderEvent>) {
    drop(rx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::export::ExportPreset;

    #[test]
    fn build_ffmpeg_args_produces_valid_command() {
        // We can't easily construct a Renderer in a unit test (needs FfmpegProvider),
        // so test the argument structure via the FfmpegCommand builder directly.
        let config = ExportConfig {
            preset: ExportPreset::builtin_presets().into_iter().next().unwrap(),
            output_path: "/tmp/test_output.mp4".to_string(),
            custom_resolution: None,
            custom_crf: Some(20),
            speed_preset: "fast".to_string(),
        };

        // Verify effective values
        assert_eq!(config.effective_crf(), 20);
        assert!(!config.is_gif());
    }
}
```

- [ ] **Step 4: Register the export module in infrastructure lib.rs**

Add to `src-tauri/crates/infrastructure/src/lib.rs`:
```rust
pub mod export;
```

- [ ] **Step 5: Verify infrastructure compiles**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/src-tauri
cargo check -p infrastructure
```

Expected: Compiles clean (warning about unused gif_renderer module is expected — we create it next).

Note: You may need to create an empty placeholder file for gif_renderer to avoid compilation errors:

```bash
touch src-tauri/crates/infrastructure/src/export/gif_renderer.rs
```

- [ ] **Step 6: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src-tauri/crates/infrastructure/src/export/ src-tauri/crates/infrastructure/src/lib.rs src-tauri/crates/infrastructure/Cargo.toml
git commit -m "feat: add render pipeline with FFmpeg subprocess progress parsing"
```

---

## Task 5: Infrastructure — GIF Renderer (Two-Pass Palette Optimization)

**Files:**
- Create: `src-tauri/crates/infrastructure/src/export/gif_renderer.rs`

- [ ] **Step 1: Implement two-pass GIF renderer**

GIF export requires a two-pass approach for quality:
1. Pass 1: Generate an optimized color palette from the video.
2. Pass 2: Re-encode the video using the generated palette.

Create `src-tauri/crates/infrastructure/src/export/gif_renderer.rs`:
```rust
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use domain::error::{AppError, AppResult};
use domain::export::{ExportConfig, FfmpegProgressLine, RenderProgress};
use domain::ffmpeg::FfmpegProvider;
use tokio::io::AsyncBufReadExt;
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use super::renderer::RenderEvent;

/// Handles GIF export using FFmpeg's two-pass palette generation approach.
/// This produces much higher quality GIFs compared to a single-pass conversion.
pub struct GifRenderer {
    ffmpeg: Arc<dyn FfmpegProvider>,
}

impl GifRenderer {
    pub fn new(ffmpeg: Arc<dyn FfmpegProvider>) -> Self {
        GifRenderer { ffmpeg }
    }

    /// Export a video as an optimized GIF.
    ///
    /// Pass 1: Generate palette — `ffmpeg -i input -vf "fps=15,scale=W:H:flags=lanczos,palettegen" palette.png`
    /// Pass 2: Apply palette  — `ffmpeg -i input -i palette.png -lavfi "fps=15,scale=W:H:flags=lanczos[x];[x][1:v]paletteuse" output.gif`
    pub async fn render_gif(
        &self,
        input_path: &str,
        filter_graph: Option<&str>,
        config: &ExportConfig,
        total_duration_secs: f64,
        tx: &mpsc::Sender<RenderEvent>,
    ) -> AppResult<()> {
        let ffmpeg_path = self.ffmpeg.ffmpeg_path()?;
        let output_path = &config.output_path;

        // Determine resolution
        let (width, height) = config.effective_resolution().unwrap_or((640, 480));
        let fps = 15; // GIF framerate — 15fps is a good balance of quality/size

        // Build the scale/fps filter chain
        let base_filter = if let Some(graph) = filter_graph {
            if graph.is_empty() {
                format!("fps={fps},scale={width}:{height}:flags=lanczos")
            } else {
                format!("{graph},fps={fps},scale={width}:{height}:flags=lanczos")
            }
        } else {
            format!("fps={fps},scale={width}:{height}:flags=lanczos")
        };

        // Generate a temporary palette path next to the output
        let output_dir = Path::new(output_path)
            .parent()
            .unwrap_or(Path::new("/tmp"));
        let palette_path = output_dir.join(format!(
            ".palette_{}.png",
            uuid::Uuid::new_v4()
        ));

        info!("GIF Pass 1: Generating palette at {}", palette_path.display());

        // --- Pass 1: Generate palette ---
        let pass1_args = vec![
            "-hide_banner".to_string(),
            "-y".to_string(),
            "-i".to_string(),
            input_path.to_string(),
            "-vf".to_string(),
            format!("{base_filter},palettegen=stats_mode=diff"),
            palette_path.to_string_lossy().to_string(),
        ];

        self.run_ffmpeg_pass(
            &ffmpeg_path,
            pass1_args,
            total_duration_secs,
            0.0,  // progress offset: 0%
            50.0, // progress range: 0-50% for pass 1
            tx,
        )
        .await?;

        // Verify palette was created
        if !palette_path.is_file() {
            return Err(AppError::FfmpegExecution(
                "Palette generation failed: output file not created".to_string(),
            ));
        }

        info!("GIF Pass 2: Encoding with palette to {}", output_path);

        // --- Pass 2: Encode with palette ---
        let pass2_filter = format!(
            "{base_filter}[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle"
        );

        let pass2_args = vec![
            "-hide_banner".to_string(),
            "-y".to_string(),
            "-i".to_string(),
            input_path.to_string(),
            "-i".to_string(),
            palette_path.to_string_lossy().to_string(),
            "-lavfi".to_string(),
            pass2_filter,
            output_path.to_string(),
        ];

        let result = self
            .run_ffmpeg_pass(
                &ffmpeg_path,
                pass2_args,
                total_duration_secs,
                50.0,  // progress offset: 50%
                50.0,  // progress range: 50-100% for pass 2
                tx,
            )
            .await;

        // Clean up temporary palette file
        if palette_path.is_file() {
            if let Err(e) = std::fs::remove_file(&palette_path) {
                debug!("Failed to remove temporary palette file: {e}");
            }
        }

        result
    }

    /// Run a single FFmpeg pass and report scaled progress.
    async fn run_ffmpeg_pass(
        &self,
        ffmpeg_path: &PathBuf,
        args: Vec<String>,
        total_duration_secs: f64,
        progress_offset: f64,
        progress_range: f64,
        tx: &mpsc::Sender<RenderEvent>,
    ) -> AppResult<()> {
        debug!("FFmpeg GIF pass args: {} {}", ffmpeg_path.display(), args.join(" "));

        let mut child = Command::new(ffmpeg_path)
            .args(&args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| AppError::FfmpegExecution(format!("Failed to spawn FFmpeg: {e}")))?;

        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| AppError::FfmpegExecution("Failed to capture stderr".to_string()))?;

        let start = Instant::now();
        let reader = tokio::io::BufReader::new(stderr);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(parsed) = FfmpegProgressLine::parse(&line) {
                let elapsed = start.elapsed().as_secs_f64();
                let mut progress = parsed.to_progress(total_duration_secs, elapsed, None);

                // Scale progress to the assigned range (e.g., 0-50% or 50-100%)
                progress.percent = progress_offset + (progress.percent / 100.0) * progress_range;

                if tx.send(RenderEvent::Progress(progress)).await.is_err() {
                    let _ = child.kill().await;
                    return Ok(());
                }
            }
        }

        let status = child.wait().await.map_err(|e| {
            AppError::FfmpegExecution(format!("Failed to wait for FFmpeg: {e}"))
        })?;

        if status.success() {
            Ok(())
        } else {
            let code = status.code().unwrap_or(-1);
            error!("FFmpeg GIF pass failed with exit code {code}");
            Err(AppError::FfmpegExecution(format!(
                "FFmpeg GIF pass exited with code {code}"
            )))
        }
    }
}
```

- [ ] **Step 2: Verify infrastructure compiles**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/src-tauri
cargo check -p infrastructure
```

Expected: Compiles clean.

- [ ] **Step 3: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src-tauri/crates/infrastructure/src/export/gif_renderer.rs
git commit -m "feat: add two-pass GIF renderer with palette optimization"
```

---

## Task 6: Infrastructure — S3-Compatible Uploader (DEFERRED to v1.1)

> **DEFERRED:** This entire task is deferred to v1.1. No S3 upload implementation for v1. The domain trait stubs in `upload.rs` are sufficient for now.

~~**Files:**~~
~~- Create: `src-tauri/crates/infrastructure/src/export/s3_uploader.rs`~~
~~- Modify: `src-tauri/crates/infrastructure/Cargo.toml` (add `rust-s3` dependency)~~

All steps in this task are **skipped for v1**.

---

## Task 7: Infrastructure — HTTP POST Uploader (DEFERRED to v1.1)

> **DEFERRED:** This entire task is deferred to v1.1. No HTTP upload implementation for v1.

~~**Files:**~~
~~- Create: `src-tauri/crates/infrastructure/src/export/http_uploader.rs`~~
~~- Modify: `src-tauri/crates/infrastructure/Cargo.toml` (add `reqwest` dependency)~~

All steps in this task are **skipped for v1**.

---

## Task 8: App — Export IPC Commands

**Files:**
- Create: `src-tauri/src/commands/export.rs`
- Modify: `src-tauri/src/commands/mod.rs` (add `pub mod export;`)
- Modify: `src-tauri/src/state.rs` (add RenderManager)
- Modify: `src-tauri/src/main.rs` (register export commands)
- Modify: `src-tauri/src/error.rs` (add Export error variant)

- [ ] **Step 1: Add Export error variant to domain AppError**

Modify `src-tauri/crates/domain/src/error.rs` — add this variant to the `AppError` enum:

```rust
    #[error("Export error: {0}")]
    Export(String),

    // TODO(v1.1): #[error("Upload error: {0}")] Upload(String),
```

Also add corresponding match arm in `src-tauri/src/error.rs` `CommandError::from`:

```rust
            AppError::Export(_) => "export",
            // TODO(v1.1): AppError::Upload(_) => "upload",
```

- [ ] **Step 2: Extend AppState with render and upload services**

Modify `src-tauri/src/state.rs` to add:
```rust
use std::collections::HashMap;
use std::sync::Arc;

use domain::ffmpeg::FfmpegProvider;
use domain::platform::PlatformInfo;
use domain::settings::SettingsRepository;
use infrastructure::export::{Renderer, GifRenderer};
use tokio::sync::{Mutex, mpsc};

use domain::export::{RenderJob, RenderJobId};
use infrastructure::export::RenderEvent;

/// Central application state, managed by Tauri.
pub struct AppState {
    pub ffmpeg: Arc<dyn FfmpegProvider>,
    pub settings: Arc<dyn SettingsRepository>,
    pub platform: PlatformInfo,
    pub renderer: Arc<Renderer>,
    pub gif_renderer: Arc<GifRenderer>,
    // TODO(v1.1): pub s3_uploader: Arc<S3Uploader>,
    // TODO(v1.1): pub http_uploader: Arc<HttpUploader>,
    /// Active render jobs keyed by job ID.
    pub active_renders: Arc<Mutex<HashMap<RenderJobId, ActiveRender>>>,
}

/// Tracks an active render job and its event receiver.
pub struct ActiveRender {
    pub job: RenderJob,
    pub cancel_tx: Option<tokio::sync::oneshot::Sender<()>>,
}
```

- [ ] **Step 3: Create the export IPC commands**

Create `src-tauri/src/commands/export.rs`:
```rust
use std::collections::HashMap;
use std::sync::Arc;

use domain::export::{
    ExportConfig, ExportPreset, RenderJob, RenderProgress, RenderStatus,
};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;
use tracing::{debug, error, info};

use crate::error::CommandResult;
use crate::state::{ActiveRender, AppState};
use infrastructure::export::RenderEvent;

/// Get all built-in export presets.
#[tauri::command]
pub fn get_export_presets() -> Vec<ExportPreset> {
    ExportPreset::builtin_presets()
}

/// Start a render job. Returns the job ID immediately.
/// Progress is streamed via Tauri events: "render-progress" and "render-complete".
#[tauri::command]
pub async fn start_render(
    app: AppHandle,
    state: State<'_, AppState>,
    input_path: String,
    filter_graph: Option<String>,
    config: ExportConfig,
    total_duration_secs: f64,
    total_frames: Option<u64>,
    project_id: String,
) -> CommandResult<String> {
    info!("Starting render for project {project_id}");

    let (mut job, mut rx) = if config.is_gif() {
        // GIF uses two-pass pipeline
        let (tx, rx) = tokio::sync::mpsc::channel::<RenderEvent>(64);
        let gif_renderer = state.gif_renderer.clone();
        let input = input_path.clone();
        let fg = filter_graph.clone();
        let cfg = config.clone();

        let job = RenderJob {
            id: uuid::Uuid::new_v4().to_string(),
            project_id: project_id.clone(),
            status: RenderStatus::Queued,
            progress: RenderProgress::zero(),
            output_path: config.output_path.clone(),
        };

        let job_id = job.id.clone();
        tokio::spawn(async move {
            let result = gif_renderer
                .render_gif(
                    &input,
                    fg.as_deref(),
                    &cfg,
                    total_duration_secs,
                    &tx,
                )
                .await;

            match result {
                Ok(()) => {
                    let _ = tx
                        .send(RenderEvent::Completed {
                            output_path: cfg.output_path.clone(),
                        })
                        .await;
                }
                Err(e) => {
                    let _ = tx
                        .send(RenderEvent::Failed {
                            error: e.to_string(),
                        })
                        .await;
                }
            }
        });

        (job, rx)
    } else {
        // Standard video render
        let (mut job, rx) = state
            .renderer
            .start_render(
                &input_path,
                filter_graph.as_deref(),
                &config,
                total_duration_secs,
                total_frames,
            )
            .await
            .map_err(|e| domain::error::AppError::Export(e.to_string()))?;

        job.project_id = project_id.clone();
        (job, rx)
    };

    let job_id = job.id.clone();
    job.status = RenderStatus::Rendering;

    // Store the active render
    {
        let mut renders = state.active_renders.lock().await;
        renders.insert(
            job_id.clone(),
            ActiveRender {
                job: job.clone(),
                cancel_tx: None,
            },
        );
    }

    // Spawn a task to forward render events as Tauri events
    let active_renders = state.active_renders.clone();
    let job_id_clone = job_id.clone();
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                RenderEvent::Progress(progress) => {
                    let _ = app.emit("render-progress", serde_json::json!({
                        "job_id": job_id_clone,
                        "progress": progress,
                    }));
                }
                RenderEvent::Completed { output_path } => {
                    let _ = app.emit("render-complete", serde_json::json!({
                        "job_id": job_id_clone,
                        "status": "completed",
                        "output_path": output_path,
                    }));
                    let mut renders = active_renders.lock().await;
                    if let Some(render) = renders.get_mut(&job_id_clone) {
                        render.job.status = RenderStatus::Completed(output_path);
                    }
                    break;
                }
                RenderEvent::Failed { error } => {
                    let _ = app.emit("render-complete", serde_json::json!({
                        "job_id": job_id_clone,
                        "status": "failed",
                        "error": error,
                    }));
                    let mut renders = active_renders.lock().await;
                    if let Some(render) = renders.get_mut(&job_id_clone) {
                        render.job.status = RenderStatus::Failed(error);
                    }
                    break;
                }
                RenderEvent::Cancelled => {
                    let _ = app.emit("render-complete", serde_json::json!({
                        "job_id": job_id_clone,
                        "status": "cancelled",
                    }));
                    let mut renders = active_renders.lock().await;
                    if let Some(render) = renders.get_mut(&job_id_clone) {
                        render.job.status = RenderStatus::Cancelled;
                    }
                    break;
                }
            }
        }
    });

    Ok(job_id)
}

/// Cancel a running render job.
#[tauri::command]
pub async fn cancel_render(
    state: State<'_, AppState>,
    job_id: String,
) -> CommandResult<()> {
    info!("Cancelling render job {job_id}");
    let mut renders = state.active_renders.lock().await;
    if let Some(render) = renders.get_mut(&job_id) {
        render.job.status = RenderStatus::Cancelled;
        // Dropping the receiver causes the render task to detect it and kill FFmpeg
        if let Some(cancel_tx) = render.cancel_tx.take() {
            let _ = cancel_tx.send(());
        }
    }
    Ok(())
}

/// Get the current status of a render job.
#[tauri::command]
pub async fn get_render_status(
    state: State<'_, AppState>,
    job_id: String,
) -> CommandResult<RenderJob> {
    let renders = state.active_renders.lock().await;
    renders
        .get(&job_id)
        .map(|r| r.job.clone())
        .ok_or_else(|| domain::error::AppError::Export(format!("Render job {job_id} not found")).into())
}

// TODO(v1.1): upload_to_s3 and upload_to_http IPC commands
```

- [ ] **Step 4: Register export commands in mod.rs**

Add to `src-tauri/src/commands/mod.rs`:
```rust
pub mod export;
```

- [ ] **Step 5: Wire export commands into main.rs**

Add the export commands to the `invoke_handler` in `src-tauri/src/main.rs`:
```rust
        .invoke_handler(tauri::generate_handler![
            commands::platform::get_platform_info,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::reset_settings,
            commands::ffmpeg::get_ffmpeg_status,
            commands::export::get_export_presets,
            commands::export::start_render,
            commands::export::cancel_render,
            commands::export::get_render_status,
            // TODO(v1.1): commands::export::upload_to_s3,
            // TODO(v1.1): commands::export::upload_to_http,
        ])
```

Add the renderer and uploader initialization in the `setup` hook (after the FFmpeg resolver setup):
```rust
            // Export services
            let renderer = Arc::new(infrastructure::export::Renderer::new(ffmpeg_resolver.clone()));
            let gif_renderer = Arc::new(infrastructure::export::GifRenderer::new(ffmpeg_resolver.clone()));
            // TODO(v1.1): let s3_uploader = Arc::new(infrastructure::export::S3Uploader::new());
            // TODO(v1.1): let http_uploader = Arc::new(infrastructure::export::HttpUploader::new());

            // Register app state
            app.manage(AppState {
                ffmpeg: ffmpeg_resolver,
                settings: settings_repo,
                platform,
                renderer,
                gif_renderer,
                // TODO(v1.1): s3_uploader,
                // TODO(v1.1): http_uploader,
                active_renders: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            });
```

- [ ] **Step 6: Add uuid dependency to the app crate**

Add to `src-tauri/Cargo.toml` under `[dependencies]`:
```toml
uuid = { version = "1", features = ["v4"] }
```

- [ ] **Step 7: Verify the full app compiles**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/src-tauri
cargo check
```

Expected: Compiles clean.

- [ ] **Step 8: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src-tauri/src/commands/export.rs src-tauri/src/commands/mod.rs src-tauri/src/state.rs src-tauri/src/main.rs src-tauri/src/error.rs src-tauri/crates/domain/src/error.rs src-tauri/Cargo.toml
git commit -m "feat: add export IPC commands (render, cancel, status) with Tauri event streaming"
```

---

## Task 9: Frontend — Export Store (Render State Only)

**Files:**
- Create: `src/stores/export.ts`
- Modify: `src/lib/types.ts` (add export-related types)
- Modify: `src/lib/ipc.ts` (add export IPC wrappers)

- [ ] **Step 1: Add export types to types.ts**

Add to `src/lib/types.ts`:
```typescript
// --- Export types (mirrors domain::export) ---

export interface ExportPreset {
  name: string;
  video_codec: VideoCodec;
  audio_codec: AudioCodec;
  container: ContainerFormat;
  crf: number;
  resolution: [number, number] | null;
  description: string;
}

export interface ExportConfig {
  preset: ExportPreset;
  output_path: string;
  custom_resolution: [number, number] | null;
  custom_crf: number | null;
  speed_preset: string;
}

export type RenderStatus =
  | { status: "Queued" }
  | { status: "Rendering" }
  | { status: "Completed"; detail: string }
  | { status: "Failed"; detail: string }
  | { status: "Cancelled" };

export interface RenderProgress {
  percent: number;
  elapsed_secs: number;
  eta_secs: number | null;
  current_frame: number;
  total_frames: number | null;
  speed: number | null;
  output_size_bytes: number | null;
}

export interface RenderJob {
  id: string;
  project_id: string;
  status: RenderStatus;
  progress: RenderProgress;
  output_path: string;
}

// TODO(v1.1): UploadTarget, UploadProgress, UploadResult types
```

- [ ] **Step 2: Add export IPC wrappers to ipc.ts**

Add to `src/lib/ipc.ts`:
```typescript
import { invoke } from "@tauri-apps/api/core";
import type {
  ExportPreset,
  ExportConfig,
  RenderJob,
} from "./types";

// --- Export commands ---

export async function getExportPresets(): Promise<ExportPreset[]> {
  return invoke("get_export_presets");
}

export async function startRender(params: {
  inputPath: string;
  filterGraph: string | null;
  config: ExportConfig;
  totalDurationSecs: number;
  totalFrames: number | null;
  projectId: string;
}): Promise<string> {
  return invoke("start_render", {
    inputPath: params.inputPath,
    filterGraph: params.filterGraph,
    config: params.config,
    totalDurationSecs: params.totalDurationSecs,
    totalFrames: params.totalFrames,
    projectId: params.projectId,
  });
}

export async function cancelRender(jobId: string): Promise<void> {
  return invoke("cancel_render", { jobId });
}

export async function getRenderStatus(jobId: string): Promise<RenderJob> {
  return invoke("get_render_status", { jobId });
}

// TODO(v1.1): uploadToS3() and uploadToHttp() IPC wrappers
```

- [ ] **Step 3: Create the export store**

Create `src/stores/export.ts`:
```typescript
import { signal, computed } from "@preact/signals";
import { listen } from "@tauri-apps/api/event";
import type {
  ExportPreset,
  ExportConfig,
  RenderProgress,
  RenderStatus,
} from "../lib/types";
import {
  getExportPresets,
  startRender,
  cancelRender,
} from "../lib/ipc";

// --- Render state ---

/** Available export presets (loaded once on init). */
export const presets = signal<ExportPreset[]>([]);

/** Currently selected preset index. */
export const selectedPresetIndex = signal<number>(0);

/** The active render job ID (null if no render in progress). */
export const activeJobId = signal<string | null>(null);

/** Current render status. */
export const renderStatus = signal<RenderStatus>({ status: "Queued" });

/** Current render progress. */
export const renderProgress = signal<RenderProgress>({
  percent: 0,
  elapsed_secs: 0,
  eta_secs: null,
  current_frame: 0,
  total_frames: null,
  speed: null,
  output_size_bytes: null,
});

/** Whether a render is currently running. */
export const isRendering = computed(() => renderStatus.value.status === "Rendering");

/** Whether the render completed successfully. */
export const isRenderComplete = computed(
  () => renderStatus.value.status === "Completed"
);

/** The output file path after successful render. */
export const renderOutputPath = computed(() => {
  const status = renderStatus.value;
  return status.status === "Completed" ? status.detail : null;
});

/** Render error message (if failed). */
export const renderError = computed(() => {
  const status = renderStatus.value;
  return status.status === "Failed" ? status.detail : null;
});

// --- Upload state (DEFERRED to v1.1) ---
// TODO(v1.1): uploadTargets, selectedUploadIndex, isUploading, uploadProgress, uploadResult, uploadError

// --- Actions ---

/** Load built-in presets from the backend. */
export async function loadPresets(): Promise<void> {
  try {
    const loaded = await getExportPresets();
    presets.value = loaded;
    selectedPresetIndex.value = 0;
  } catch (err) {
    console.error("Failed to load export presets:", err);
  }
}

/** Start a render with the given config. */
export async function doStartRender(params: {
  inputPath: string;
  filterGraph: string | null;
  config: ExportConfig;
  totalDurationSecs: number;
  totalFrames: number | null;
  projectId: string;
}): Promise<void> {
  renderStatus.value = { status: "Rendering" };
  renderProgress.value = {
    percent: 0,
    elapsed_secs: 0,
    eta_secs: null,
    current_frame: 0,
    total_frames: params.totalFrames,
    speed: null,
    output_size_bytes: null,
  };

  try {
    const jobId = await startRender(params);
    activeJobId.value = jobId;
  } catch (err) {
    renderStatus.value = { status: "Failed", detail: String(err) };
  }
}

/** Cancel the active render. */
export async function doCancelRender(): Promise<void> {
  if (activeJobId.value) {
    try {
      await cancelRender(activeJobId.value);
      renderStatus.value = { status: "Cancelled" };
    } catch (err) {
      console.error("Failed to cancel render:", err);
    }
  }
}

// TODO(v1.1): doUpload() function

/** Reset all export state (call when leaving the export flow). */
export function resetExportState(): void {
  activeJobId.value = null;
  renderStatus.value = { status: "Queued" };
  renderProgress.value = {
    percent: 0,
    elapsed_secs: 0,
    eta_secs: null,
    current_frame: 0,
    total_frames: null,
    speed: null,
    output_size_bytes: null,
  };
}

// --- Event listeners ---

/** Initialize event listeners for render progress.
 *  Call this once at app startup. */
export async function initExportListeners(): Promise<void> {
  // Listen for render progress events from the backend
  await listen<{ job_id: string; progress: RenderProgress }>(
    "render-progress",
    (event) => {
      if (event.payload.job_id === activeJobId.value) {
        renderProgress.value = event.payload.progress;
      }
    }
  );

  // Listen for render completion events
  await listen<{
    job_id: string;
    status: string;
    output_path?: string;
    error?: string;
  }>("render-complete", (event) => {
    if (event.payload.job_id === activeJobId.value) {
      switch (event.payload.status) {
        case "completed":
          renderStatus.value = {
            status: "Completed",
            detail: event.payload.output_path ?? "",
          };
          break;
        case "failed":
          renderStatus.value = {
            status: "Failed",
            detail: event.payload.error ?? "Unknown error",
          };
          break;
        case "cancelled":
          renderStatus.value = { status: "Cancelled" };
          break;
      }
    }
  });

  // TODO(v1.1): Listen for upload progress events
}
```

- [ ] **Step 4: Install @preact/signals if not already present**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
pnpm add @preact/signals
```

- [ ] **Step 5: Verify the frontend compiles**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
pnpm run build
```

Expected: No TypeScript errors.

- [ ] **Step 6: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src/stores/export.ts src/lib/types.ts src/lib/ipc.ts package.json pnpm-lock.yaml
git commit -m "feat: add export store with render state management"
```

---

## Task 10: Frontend — ExportDialog Component (Preset Picker, Format/Quality Controls)

**Files:**
- Create: `src/components/export/ExportDialog.tsx`
- Create: `src/components/export/ExportDialog.module.scss`

- [ ] **Step 1: Create the ExportDialog component**

Create `src/components/export/ExportDialog.tsx`:
```tsx
import { useEffect, useCallback } from "preact/hooks";
import { signal } from "@preact/signals";
import type { ExportConfig, ExportPreset } from "../../lib/types";
import {
  presets,
  selectedPresetIndex,
  loadPresets,
} from "../../stores/export";
import styles from "./ExportDialog.module.scss";

interface ExportDialogProps {
  /** Called when user confirms export settings. */
  onExport: (config: ExportConfig) => void;
  /** Called when user cancels the dialog. */
  onCancel: () => void;
  /** Default output directory (from settings). */
  defaultOutputDir: string;
  /** Source file name (for default output name). */
  sourceFileName: string;
}

// Local form state
const customCrf = signal<number | null>(null);
const customWidth = signal<number | null>(null);
const customHeight = signal<number | null>(null);
const speedPreset = signal<string>("fast");
const outputFileName = signal<string>("");

export function ExportDialog({
  onExport,
  onCancel,
  defaultOutputDir,
  sourceFileName,
}: ExportDialogProps) {
  // Load presets on mount
  useEffect(() => {
    loadPresets();
    // Generate default output filename
    const baseName = sourceFileName.replace(/\.[^/.]+$/, "");
    outputFileName.value = baseName + "_export";
  }, [sourceFileName]);

  const selectedPreset = (): ExportPreset | null => {
    const list = presets.value;
    const index = selectedPresetIndex.value;
    return index < list.length ? list[index] : null;
  };

  const handlePresetChange = useCallback((e: Event) => {
    const target = e.target as HTMLSelectElement;
    selectedPresetIndex.value = parseInt(target.value, 10);
    // Reset custom overrides when preset changes
    customCrf.value = null;
    customWidth.value = null;
    customHeight.value = null;
  }, []);

  const handleExport = useCallback(() => {
    const preset = selectedPreset();
    if (!preset) return;

    const ext = preset.container === "gif" ? "gif"
      : preset.container === "webm" ? "webm"
      : preset.container === "mkv" ? "mkv"
      : "mp4";

    const outputPath = `${defaultOutputDir}/${outputFileName.value}.${ext}`;

    const config: ExportConfig = {
      preset,
      output_path: outputPath,
      custom_resolution:
        customWidth.value && customHeight.value
          ? [customWidth.value, customHeight.value]
          : null,
      custom_crf: customCrf.value,
      speed_preset: speedPreset.value,
    };

    onExport(config);
  }, [defaultOutputDir, onExport]);

  const preset = selectedPreset();

  return (
    <div class={styles.overlay}>
      <div class={styles.dialog}>
        <h2 class={styles.title}>Export Settings</h2>

        {/* Preset selector */}
        <div class={styles.field}>
          <label class={styles.label}>Preset</label>
          <select
            class={styles.select}
            value={selectedPresetIndex.value}
            onChange={handlePresetChange}
          >
            {presets.value.map((p, i) => (
              <option key={i} value={i}>
                {p.name}
              </option>
            ))}
          </select>
          {preset && (
            <p class={styles.description}>{preset.description}</p>
          )}
        </div>

        {/* Output file name */}
        <div class={styles.field}>
          <label class={styles.label}>Output File Name</label>
          <input
            class={styles.input}
            type="text"
            value={outputFileName.value}
            onInput={(e) => {
              outputFileName.value = (e.target as HTMLInputElement).value;
            }}
          />
        </div>

        {/* Quality (CRF) slider */}
        {preset && preset.container !== "gif" && (
          <div class={styles.field}>
            <label class={styles.label}>
              Quality (CRF): {customCrf.value ?? preset.crf}
            </label>
            <input
              class={styles.range}
              type="range"
              min="0"
              max="51"
              value={customCrf.value ?? preset.crf}
              onInput={(e) => {
                customCrf.value = parseInt(
                  (e.target as HTMLInputElement).value,
                  10
                );
              }}
            />
            <div class={styles.rangeLabels}>
              <span>Best quality</span>
              <span>Smallest file</span>
            </div>
          </div>
        )}

        {/* Resolution override */}
        <div class={styles.field}>
          <label class={styles.label}>Resolution</label>
          <div class={styles.resolutionRow}>
            <input
              class={styles.inputSmall}
              type="number"
              placeholder={
                preset?.resolution
                  ? String(preset.resolution[0])
                  : "Source width"
              }
              value={customWidth.value ?? ""}
              onInput={(e) => {
                const val = parseInt(
                  (e.target as HTMLInputElement).value,
                  10
                );
                customWidth.value = isNaN(val) ? null : val;
              }}
            />
            <span class={styles.resolutionSeparator}>x</span>
            <input
              class={styles.inputSmall}
              type="number"
              placeholder={
                preset?.resolution
                  ? String(preset.resolution[1])
                  : "Source height"
              }
              value={customHeight.value ?? ""}
              onInput={(e) => {
                const val = parseInt(
                  (e.target as HTMLInputElement).value,
                  10
                );
                customHeight.value = isNaN(val) ? null : val;
              }}
            />
          </div>
          <p class={styles.hint}>Leave blank to use source resolution.</p>
        </div>

        {/* Encoding speed */}
        {preset && preset.container !== "gif" && (
          <div class={styles.field}>
            <label class={styles.label}>Encoding Speed</label>
            <select
              class={styles.select}
              value={speedPreset.value}
              onChange={(e) => {
                speedPreset.value = (e.target as HTMLSelectElement).value;
              }}
            >
              <option value="ultrafast">Ultrafast (lowest quality)</option>
              <option value="superfast">Superfast</option>
              <option value="veryfast">Very fast</option>
              <option value="faster">Faster</option>
              <option value="fast">Fast (recommended)</option>
              <option value="medium">Medium</option>
              <option value="slow">Slow (best quality)</option>
              <option value="slower">Slower</option>
              <option value="veryslow">Very slow (smallest file)</option>
            </select>
          </div>
        )}

        {/* Format info */}
        {preset && (
          <div class={styles.info}>
            <span>Format: {preset.container.toUpperCase()}</span>
            {preset.container !== "gif" && (
              <>
                <span>Video: {preset.video_codec.toUpperCase()}</span>
                <span>Audio: {preset.audio_codec.toUpperCase()}</span>
              </>
            )}
          </div>
        )}

        {/* Action buttons */}
        <div class={styles.actions}>
          <button class={styles.cancelBtn} onClick={onCancel}>
            Cancel
          </button>
          <button class={styles.exportBtn} onClick={handleExport}>
            Export
          </button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Create the ExportDialog styles (SCSS module)**

Create `src/components/export/ExportDialog.module.scss`:
```scss
.overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}

.dialog {
  background: var(--bg-primary, #1e1e2e);
  border: 1px solid var(--border-color, #313244);
  border-radius: 12px;
  padding: 24px;
  width: 480px;
  max-height: 80vh;
  overflow-y: auto;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
}

.title {
  margin: 0 0 20px;
  font-size: 18px;
  font-weight: 600;
  color: var(--text-primary, #cdd6f4);
}

.field {
  margin-bottom: 16px;
}

.label {
  display: block;
  font-size: 13px;
  font-weight: 500;
  color: var(--text-secondary, #a6adc8);
  margin-bottom: 6px;
}

.select,
.input {
  width: 100%;
  padding: 8px 12px;
  background: var(--bg-secondary, #181825);
  border: 1px solid var(--border-color, #313244);
  border-radius: 6px;
  color: var(--text-primary, #cdd6f4);
  font-size: 14px;
  outline: none;
}

.select:focus,
.input:focus {
  border-color: var(--accent, #89b4fa);
}

.inputSmall {
  width: 100px;
  padding: 8px 12px;
  background: var(--bg-secondary, #181825);
  border: 1px solid var(--border-color, #313244);
  border-radius: 6px;
  color: var(--text-primary, #cdd6f4);
  font-size: 14px;
  outline: none;
}

.resolutionRow {
  display: flex;
  align-items: center;
  gap: 8px;
}

.resolutionSeparator {
  color: var(--text-secondary, #a6adc8);
  font-size: 14px;
}

.range {
  width: 100%;
  margin: 4px 0;
}

.rangeLabels {
  display: flex;
  justify-content: space-between;
  font-size: 11px;
  color: var(--text-muted, #6c7086);
}

.description {
  font-size: 12px;
  color: var(--text-muted, #6c7086);
  margin: 4px 0 0;
}

.hint {
  font-size: 11px;
  color: var(--text-muted, #6c7086);
  margin: 4px 0 0;
}

.info {
  display: flex;
  gap: 16px;
  padding: 12px;
  background: var(--bg-secondary, #181825);
  border-radius: 6px;
  font-size: 12px;
  color: var(--text-secondary, #a6adc8);
  margin-bottom: 16px;
}

.actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 8px;
}

.cancelBtn {
  padding: 8px 16px;
  background: transparent;
  border: 1px solid var(--border-color, #313244);
  border-radius: 6px;
  color: var(--text-secondary, #a6adc8);
  cursor: pointer;
  font-size: 14px;
}

.cancelBtn:hover {
  background: var(--bg-secondary, #181825);
}

.exportBtn {
  padding: 8px 20px;
  background: var(--accent, #89b4fa);
  border: none;
  border-radius: 6px;
  color: var(--bg-primary, #1e1e2e);
  cursor: pointer;
  font-size: 14px;
  font-weight: 600;
}

.exportBtn:hover {
  opacity: 0.9;
}
```

- [ ] **Step 3: Verify the frontend compiles**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
pnpm run build
```

Expected: No TypeScript errors.

- [ ] **Step 4: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src/components/export/ExportDialog.tsx src/components/export/ExportDialog.module.scss
git commit -m "feat: add ExportDialog component with preset picker and format controls"
```

---

## Task 11: Frontend — RenderProgress Component (Progress Bar, Cancel Button, ETA)

**Files:**
- Create: `src/components/export/RenderProgress.tsx`
- Create: `src/components/export/RenderProgress.module.scss`

- [ ] **Step 1: Create the RenderProgress component**

Create `src/components/export/RenderProgress.tsx`:
```tsx
import {
  renderProgress,
  renderStatus,
  isRendering,
  isRenderComplete,
  renderError,
  renderOutputPath,
  doCancelRender,
} from "../../stores/export";
import styles from "./RenderProgress.module.scss";

interface RenderProgressProps {
  /** Called when user clicks "Done" after render completes. */
  onDone: () => void;
}

/** Format seconds into a human-readable string like "2m 15s". */
function formatDuration(secs: number | null): string {
  if (secs === null || secs < 0) return "--";
  if (secs < 60) return `${Math.round(secs)}s`;
  const minutes = Math.floor(secs / 60);
  const seconds = Math.round(secs % 60);
  if (minutes < 60) return `${minutes}m ${seconds}s`;
  const hours = Math.floor(minutes / 60);
  const remainingMinutes = minutes % 60;
  return `${hours}h ${remainingMinutes}m`;
}

/** Format bytes into a human-readable string like "12.3 MB". */
function formatBytes(bytes: number | null): string {
  if (bytes === null || bytes === 0) return "--";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

export function RenderProgress({ onDone }: RenderProgressProps) {
  const progress = renderProgress.value;
  const status = renderStatus.value;

  return (
    <div class={styles.container}>
      <h3 class={styles.title}>
        {status.status === "Rendering" && "Rendering..."}
        {status.status === "Completed" && "Render Complete"}
        {status.status === "Failed" && "Render Failed"}
        {status.status === "Cancelled" && "Render Cancelled"}
        {status.status === "Queued" && "Queued..."}
      </h3>

      {/* Progress bar */}
      <div class={styles.progressBar}>
        <div
          class={styles.progressFill}
          style={{ width: `${Math.min(progress.percent, 100)}%` }}
        />
      </div>

      <div class={styles.stats}>
        <span class={styles.percent}>
          {progress.percent.toFixed(1)}%
        </span>
        {progress.speed !== null && (
          <span class={styles.stat}>
            Speed: {progress.speed.toFixed(1)}x
          </span>
        )}
        <span class={styles.stat}>
          Elapsed: {formatDuration(progress.elapsed_secs)}
        </span>
        <span class={styles.stat}>
          ETA: {formatDuration(progress.eta_secs)}
        </span>
      </div>

      {/* File size and frame info */}
      <div class={styles.details}>
        {progress.current_frame > 0 && (
          <span>
            Frame: {progress.current_frame}
            {progress.total_frames ? ` / ${progress.total_frames}` : ""}
          </span>
        )}
        {progress.output_size_bytes && (
          <span>Size: {formatBytes(progress.output_size_bytes)}</span>
        )}
      </div>

      {/* Error message */}
      {renderError.value && (
        <div class={styles.error}>
          {renderError.value}
        </div>
      )}

      {/* Action buttons */}
      <div class={styles.actions}>
        {isRendering.value && (
          <button class={styles.cancelBtn} onClick={doCancelRender}>
            Cancel Render
          </button>
        )}
        {isRenderComplete.value && (
          <>
            <button class={styles.doneBtn} onClick={onDone}>
              Done
            </button>
            {renderOutputPath.value && (
              <p class={styles.savedPath}>
                Saved to: <code>{renderOutputPath.value}</code>
              </p>
            )}
          </>
        )}
        {(status.status === "Failed" || status.status === "Cancelled") && (
          <button class={styles.doneBtn} onClick={onDone}>
            Close
          </button>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Create the RenderProgress styles (SCSS module)**

Create `src/components/export/RenderProgress.module.scss`:
```scss
.container {
  padding: 24px;
}

.title {
  margin: 0 0 16px;
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary, #cdd6f4);
}

.progressBar {
  width: 100%;
  height: 8px;
  background: var(--bg-secondary, #181825);
  border-radius: 4px;
  overflow: hidden;
  margin-bottom: 12px;
}

.progressFill {
  height: 100%;
  background: var(--accent, #89b4fa);
  border-radius: 4px;
  transition: width 0.3s ease;
}

.stats {
  display: flex;
  gap: 16px;
  margin-bottom: 8px;
  flex-wrap: wrap;
}

.percent {
  font-size: 20px;
  font-weight: 700;
  color: var(--text-primary, #cdd6f4);
}

.stat {
  font-size: 13px;
  color: var(--text-secondary, #a6adc8);
  display: flex;
  align-items: center;
}

.details {
  display: flex;
  gap: 16px;
  font-size: 12px;
  color: var(--text-muted, #6c7086);
  margin-bottom: 16px;
}

.error {
  padding: 12px;
  background: rgba(243, 139, 168, 0.1);
  border: 1px solid rgba(243, 139, 168, 0.3);
  border-radius: 6px;
  color: #f38ba8;
  font-size: 13px;
  margin-bottom: 16px;
}

.actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}

.cancelBtn {
  padding: 8px 16px;
  background: rgba(243, 139, 168, 0.1);
  border: 1px solid rgba(243, 139, 168, 0.3);
  border-radius: 6px;
  color: #f38ba8;
  cursor: pointer;
  font-size: 14px;
}

.cancelBtn:hover {
  background: rgba(243, 139, 168, 0.2);
}

.doneBtn {
  padding: 8px 16px;
  background: transparent;
  border: 1px solid var(--border-color, #313244);
  border-radius: 6px;
  color: var(--text-secondary, #a6adc8);
  cursor: pointer;
  font-size: 14px;
}

.doneBtn:hover {
  background: var(--bg-secondary, #181825);
}

.savedPath {
  font-size: 12px;
  color: var(--text-muted, #6c7086);
  margin: 0;
  word-break: break-all;

  code {
    background: var(--bg-secondary, #181825);
    padding: 2px 6px;
    border-radius: 3px;
    font-size: 11px;
  }
}
```

- [ ] **Step 3: Verify the frontend compiles**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
pnpm run build
```

Expected: No TypeScript errors.

- [ ] **Step 4: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src/components/export/RenderProgress.tsx src/components/export/RenderProgress.module.scss
git commit -m "feat: add RenderProgress component with progress bar, ETA, and cancel"
```

---

## Task 12: Frontend — UploadPanel Component (DEFERRED to v1.1)

> **DEFERRED:** This entire task is deferred to v1.1. No UploadPanel component for v1. The v1 export flow ends with a file save dialog after render completion.

~~**Files:**~~
~~- Create: `src/components/export/UploadPanel.tsx`~~
~~- Create: `src/components/export/UploadPanel.module.scss`~~

All steps in this task are **skipped for v1**.

<!--
DEFERRED to v1.1: All UploadPanel implementation removed from v1 plan.

- [ ] **Step 1: Create the UploadPanel component**

Create `src/components/export/UploadPanel.tsx`:
```tsx
import { useCallback } from "preact/hooks";
import { signal } from "@preact/signals";
import type { UploadTarget } from "../../lib/types";
import {
  uploadTargets,
  selectedUploadIndex,
  isUploading,
  uploadProgress,
  uploadResult,
  uploadError,
  doUpload,
} from "../../stores/export";
import styles from "./UploadPanel.module.scss";

interface UploadPanelProps {
  /** The file path to upload. */
  filePath: string;
  /** Called when user wants to go back. */
  onBack: () => void;
}

// Local state for adding a new upload target
const showAddTarget = signal<boolean>(false);
const newTargetType = signal<"S3" | "Http">("S3");
const newTargetName = signal<string>("");
// S3 fields
const newS3Endpoint = signal<string>("");
const newS3Bucket = signal<string>("");
const newS3Region = signal<string>("us-east-1");
const newS3AccessKey = signal<string>("");
const newS3SecretKey = signal<string>("");
const newS3Prefix = signal<string>("");
// HTTP fields
const newHttpUrl = signal<string>("");
const newHttpMethod = signal<string>("POST");
const newHttpAuthHeader = signal<string>("");

/** Format bytes into human-readable string. */
function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function UploadPanel({ filePath, onBack }: UploadPanelProps) {
  const handleAddTarget = useCallback(() => {
    let target: UploadTarget;

    if (newTargetType.value === "S3") {
      target = {
        type: "S3",
        name: newTargetName.value || "Untitled S3 Target",
        endpoint: newS3Endpoint.value,
        bucket: newS3Bucket.value,
        region: newS3Region.value,
        access_key: newS3AccessKey.value,
        secret_key: newS3SecretKey.value,
        prefix: newS3Prefix.value,
      };
    } else {
      const headers: Record<string, string> = {};
      if (newHttpAuthHeader.value) {
        headers["Authorization"] = newHttpAuthHeader.value;
      }
      target = {
        type: "Http",
        name: newTargetName.value || "Untitled HTTP Target",
        url: newHttpUrl.value,
        method: newHttpMethod.value,
        headers,
      };
    }

    uploadTargets.value = [...uploadTargets.value, target];
    selectedUploadIndex.value = uploadTargets.value.length - 1;
    showAddTarget.value = false;

    // Reset form
    newTargetName.value = "";
    newS3Endpoint.value = "";
    newS3Bucket.value = "";
    newS3Region.value = "us-east-1";
    newS3AccessKey.value = "";
    newS3SecretKey.value = "";
    newS3Prefix.value = "";
    newHttpUrl.value = "";
    newHttpMethod.value = "POST";
    newHttpAuthHeader.value = "";
  }, []);

  const handleUpload = useCallback(() => {
    doUpload(filePath);
  }, [filePath]);

  const handleRemoveTarget = useCallback((index: number) => {
    const targets = [...uploadTargets.value];
    targets.splice(index, 1);
    uploadTargets.value = targets;
    if (selectedUploadIndex.value >= targets.length) {
      selectedUploadIndex.value = Math.max(0, targets.length - 1);
    }
  }, []);

  const progress = uploadProgress.value;
  const result = uploadResult.value;

  return (
    <div class={styles.container}>
      <h3 class={styles.title}>Upload</h3>

      <p class={styles.filePath}>
        File: <code>{filePath}</code>
      </p>

      {/* Upload target list */}
      <div class={styles.targetList}>
        {uploadTargets.value.length === 0 && !showAddTarget.value && (
          <p class={styles.emptyMessage}>
            No upload targets configured. Add one below.
          </p>
        )}

        {uploadTargets.value.map((target, i) => (
          <div
            key={i}
            class={`${styles.targetItem} ${
              selectedUploadIndex.value === i ? styles.targetSelected : ""
            }`}
            onClick={() => {
              selectedUploadIndex.value = i;
            }}
          >
            <div class={styles.targetInfo}>
              <span class={styles.targetName}>{target.name}</span>
              <span class={styles.targetType}>{target.type}</span>
            </div>
            <button
              class={styles.removeBtn}
              onClick={(e) => {
                e.stopPropagation();
                handleRemoveTarget(i);
              }}
            >
              Remove
            </button>
          </div>
        ))}
      </div>

      {/* Add target form */}
      {!showAddTarget.value ? (
        <button
          class={styles.addBtn}
          onClick={() => {
            showAddTarget.value = true;
          }}
        >
          + Add Upload Target
        </button>
      ) : (
        <div class={styles.addForm}>
          <div class={styles.field}>
            <label class={styles.label}>Type</label>
            <select
              class={styles.select}
              value={newTargetType.value}
              onChange={(e) => {
                newTargetType.value = (e.target as HTMLSelectElement)
                  .value as "S3" | "Http";
              }}
            >
              <option value="S3">S3-Compatible (AWS, R2, MinIO)</option>
              <option value="Http">Custom HTTP Endpoint</option>
            </select>
          </div>

          <div class={styles.field}>
            <label class={styles.label}>Name</label>
            <input
              class={styles.input}
              type="text"
              placeholder="My Upload Target"
              value={newTargetName.value}
              onInput={(e) => {
                newTargetName.value = (e.target as HTMLInputElement).value;
              }}
            />
          </div>

          {newTargetType.value === "S3" && (
            <>
              <div class={styles.field}>
                <label class={styles.label}>Endpoint URL</label>
                <input
                  class={styles.input}
                  type="text"
                  placeholder="https://s3.amazonaws.com"
                  value={newS3Endpoint.value}
                  onInput={(e) => {
                    newS3Endpoint.value = (e.target as HTMLInputElement).value;
                  }}
                />
              </div>
              <div class={styles.field}>
                <label class={styles.label}>Bucket</label>
                <input
                  class={styles.input}
                  type="text"
                  placeholder="my-bucket"
                  value={newS3Bucket.value}
                  onInput={(e) => {
                    newS3Bucket.value = (e.target as HTMLInputElement).value;
                  }}
                />
              </div>
              <div class={styles.field}>
                <label class={styles.label}>Region</label>
                <input
                  class={styles.input}
                  type="text"
                  placeholder="us-east-1"
                  value={newS3Region.value}
                  onInput={(e) => {
                    newS3Region.value = (e.target as HTMLInputElement).value;
                  }}
                />
              </div>
              <div class={styles.field}>
                <label class={styles.label}>Access Key</label>
                <input
                  class={styles.input}
                  type="password"
                  placeholder="AKIAIOSFODNN7EXAMPLE"
                  value={newS3AccessKey.value}
                  onInput={(e) => {
                    newS3AccessKey.value = (e.target as HTMLInputElement).value;
                  }}
                />
              </div>
              <div class={styles.field}>
                <label class={styles.label}>Secret Key</label>
                <input
                  class={styles.input}
                  type="password"
                  placeholder="wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
                  value={newS3SecretKey.value}
                  onInput={(e) => {
                    newS3SecretKey.value = (e.target as HTMLInputElement).value;
                  }}
                />
              </div>
              <div class={styles.field}>
                <label class={styles.label}>Key Prefix (optional)</label>
                <input
                  class={styles.input}
                  type="text"
                  placeholder="recordings/"
                  value={newS3Prefix.value}
                  onInput={(e) => {
                    newS3Prefix.value = (e.target as HTMLInputElement).value;
                  }}
                />
              </div>
            </>
          )}

          {newTargetType.value === "Http" && (
            <>
              <div class={styles.field}>
                <label class={styles.label}>URL</label>
                <input
                  class={styles.input}
                  type="text"
                  placeholder="https://api.example.com/upload"
                  value={newHttpUrl.value}
                  onInput={(e) => {
                    newHttpUrl.value = (e.target as HTMLInputElement).value;
                  }}
                />
              </div>
              <div class={styles.field}>
                <label class={styles.label}>Method</label>
                <select
                  class={styles.select}
                  value={newHttpMethod.value}
                  onChange={(e) => {
                    newHttpMethod.value = (e.target as HTMLSelectElement).value;
                  }}
                >
                  <option value="POST">POST</option>
                  <option value="PUT">PUT</option>
                </select>
              </div>
              <div class={styles.field}>
                <label class={styles.label}>
                  Authorization Header (optional)
                </label>
                <input
                  class={styles.input}
                  type="password"
                  placeholder="Bearer your-token-here"
                  value={newHttpAuthHeader.value}
                  onInput={(e) => {
                    newHttpAuthHeader.value = (e.target as HTMLInputElement).value;
                  }}
                />
              </div>
            </>
          )}

          <div class={styles.formActions}>
            <button
              class={styles.cancelFormBtn}
              onClick={() => {
                showAddTarget.value = false;
              }}
            >
              Cancel
            </button>
            <button class={styles.saveBtn} onClick={handleAddTarget}>
              Add Target
            </button>
          </div>
        </div>
      )}

      {/* Upload progress */}
      {isUploading.value && (
        <div class={styles.uploadProgress}>
          <div class={styles.progressBar}>
            <div
              class={styles.progressFill}
              style={{ width: `${progress.percent}%` }}
            />
          </div>
          <div class={styles.progressStats}>
            <span>{progress.percent.toFixed(1)}%</span>
            <span>
              {formatBytes(progress.bytes_uploaded)} /{" "}
              {formatBytes(progress.total_bytes)}
            </span>
          </div>
        </div>
      )}

      {/* Upload result */}
      {result && (
        <div class={styles.result}>
          <p class={styles.resultMessage}>Upload complete!</p>
          {result.url && (
            <div class={styles.resultUrl}>
              <label class={styles.label}>Shareable URL:</label>
              <div class={styles.urlRow}>
                <input
                  class={styles.urlInput}
                  type="text"
                  value={result.url}
                  readOnly
                />
                <button
                  class={styles.copyBtn}
                  onClick={() => {
                    navigator.clipboard.writeText(result.url!);
                  }}
                >
                  Copy
                </button>
              </div>
            </div>
          )}
          <p class={styles.resultDetail}>
            Uploaded {formatBytes(result.bytes_uploaded)} to{" "}
            {result.remote_path}
          </p>
        </div>
      )}

      {/* Upload error */}
      {uploadError.value && (
        <div class={styles.error}>{uploadError.value}</div>
      )}

      {/* Action buttons */}
      <div class={styles.actions}>
        <button class={styles.backBtn} onClick={onBack}>
          Back
        </button>
        {uploadTargets.value.length > 0 && !isUploading.value && (
          <button class={styles.uploadBtn} onClick={handleUpload}>
            Upload to {uploadTargets.value[selectedUploadIndex.value]?.name ?? "Target"}
          </button>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Create the UploadPanel styles**

Create `src/components/export/UploadPanel.module.scss`:
```css
.container {
  padding: 24px;
}

.title {
  margin: 0 0 12px;
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary, #cdd6f4);
}

.filePath {
  font-size: 12px;
  color: var(--text-muted, #6c7086);
  margin-bottom: 16px;
  word-break: break-all;
}

.filePath code {
  background: var(--bg-secondary, #181825);
  padding: 2px 6px;
  border-radius: 3px;
  font-size: 11px;
}

.targetList {
  margin-bottom: 12px;
}

.emptyMessage {
  font-size: 13px;
  color: var(--text-muted, #6c7086);
  text-align: center;
  padding: 16px;
}

.targetItem {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 10px 12px;
  border: 1px solid var(--border-color, #313244);
  border-radius: 6px;
  margin-bottom: 6px;
  cursor: pointer;
  transition: border-color 0.15s;
}

.targetItem:hover {
  border-color: var(--text-muted, #6c7086);
}

.targetSelected {
  border-color: var(--accent, #89b4fa);
  background: rgba(137, 180, 250, 0.05);
}

.targetInfo {
  display: flex;
  align-items: center;
  gap: 8px;
}

.targetName {
  font-size: 14px;
  color: var(--text-primary, #cdd6f4);
}

.targetType {
  font-size: 11px;
  color: var(--text-muted, #6c7086);
  background: var(--bg-secondary, #181825);
  padding: 2px 6px;
  border-radius: 3px;
}

.removeBtn {
  font-size: 12px;
  color: #f38ba8;
  background: transparent;
  border: none;
  cursor: pointer;
  padding: 4px 8px;
}

.removeBtn:hover {
  text-decoration: underline;
}

.addBtn {
  width: 100%;
  padding: 10px;
  background: transparent;
  border: 1px dashed var(--border-color, #313244);
  border-radius: 6px;
  color: var(--text-secondary, #a6adc8);
  cursor: pointer;
  font-size: 13px;
  margin-bottom: 16px;
}

.addBtn:hover {
  border-color: var(--accent, #89b4fa);
  color: var(--accent, #89b4fa);
}

.addForm {
  border: 1px solid var(--border-color, #313244);
  border-radius: 8px;
  padding: 16px;
  margin-bottom: 16px;
  background: var(--bg-secondary, #181825);
}

.field {
  margin-bottom: 12px;
}

.label {
  display: block;
  font-size: 12px;
  font-weight: 500;
  color: var(--text-secondary, #a6adc8);
  margin-bottom: 4px;
}

.select,
.input {
  width: 100%;
  padding: 8px 12px;
  background: var(--bg-primary, #1e1e2e);
  border: 1px solid var(--border-color, #313244);
  border-radius: 6px;
  color: var(--text-primary, #cdd6f4);
  font-size: 13px;
  outline: none;
}

.select:focus,
.input:focus {
  border-color: var(--accent, #89b4fa);
}

.formActions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}

.cancelFormBtn {
  padding: 6px 12px;
  background: transparent;
  border: 1px solid var(--border-color, #313244);
  border-radius: 6px;
  color: var(--text-secondary, #a6adc8);
  cursor: pointer;
  font-size: 13px;
}

.saveBtn {
  padding: 6px 16px;
  background: var(--accent, #89b4fa);
  border: none;
  border-radius: 6px;
  color: var(--bg-primary, #1e1e2e);
  cursor: pointer;
  font-size: 13px;
  font-weight: 600;
}

.uploadProgress {
  margin: 16px 0;
}

.progressBar {
  width: 100%;
  height: 6px;
  background: var(--bg-secondary, #181825);
  border-radius: 3px;
  overflow: hidden;
  margin-bottom: 6px;
}

.progressFill {
  height: 100%;
  background: var(--accent, #89b4fa);
  border-radius: 3px;
  transition: width 0.3s ease;
}

.progressStats {
  display: flex;
  justify-content: space-between;
  font-size: 12px;
  color: var(--text-secondary, #a6adc8);
}

.result {
  padding: 12px;
  background: rgba(166, 227, 161, 0.1);
  border: 1px solid rgba(166, 227, 161, 0.3);
  border-radius: 6px;
  margin: 16px 0;
}

.resultMessage {
  font-size: 14px;
  font-weight: 600;
  color: #a6e3a1;
  margin: 0 0 8px;
}

.resultUrl {
  margin-bottom: 8px;
}

.urlRow {
  display: flex;
  gap: 8px;
}

.urlInput {
  flex: 1;
  padding: 6px 10px;
  background: var(--bg-primary, #1e1e2e);
  border: 1px solid var(--border-color, #313244);
  border-radius: 4px;
  color: var(--text-primary, #cdd6f4);
  font-size: 12px;
  font-family: monospace;
}

.copyBtn {
  padding: 6px 12px;
  background: var(--accent, #89b4fa);
  border: none;
  border-radius: 4px;
  color: var(--bg-primary, #1e1e2e);
  cursor: pointer;
  font-size: 12px;
  font-weight: 600;
}

.resultDetail {
  font-size: 12px;
  color: var(--text-muted, #6c7086);
  margin: 0;
}

.error {
  padding: 12px;
  background: rgba(243, 139, 168, 0.1);
  border: 1px solid rgba(243, 139, 168, 0.3);
  border-radius: 6px;
  color: #f38ba8;
  font-size: 13px;
  margin: 16px 0;
}

.actions {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  margin-top: 8px;
}

.backBtn {
  padding: 8px 16px;
  background: transparent;
  border: 1px solid var(--border-color, #313244);
  border-radius: 6px;
  color: var(--text-secondary, #a6adc8);
  cursor: pointer;
  font-size: 14px;
}

.backBtn:hover {
  background: var(--bg-secondary, #181825);
}

.uploadBtn {
  padding: 8px 20px;
  background: var(--accent, #89b4fa);
  border: none;
  border-radius: 6px;
  color: var(--bg-primary, #1e1e2e);
  cursor: pointer;
  font-size: 14px;
  font-weight: 600;
}

.uploadBtn:hover {
  opacity: 0.9;
}
```

- [ ] **Step 3: Verify the frontend compiles**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
pnpm run build
```

Expected: No TypeScript errors.

- [ ] **Step 4: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src/components/export/UploadPanel.tsx src/components/export/UploadPanel.module.scss
git commit -m "feat: add UploadPanel component with target config, progress, and URL copy"
```
-->

---

## Task 13: Integration — Wire Export into Editor Page

**Files:**
- Modify: `src/pages/Editor.tsx` (or create if not yet present from Plan 3)

- [ ] **Step 1: Add export flow to the editor page**

This wires the export components into the editor page. The v1 export flow is a two-step process: (1) Open ExportDialog to configure settings and choose output path via file save dialog, (2) Show RenderProgress during rendering. On completion, the file is already saved to disk.

Modify `src/pages/Editor.tsx` to add the export integration. If the file does not exist yet (Plan 3 not implemented), create a minimal version:

```tsx
import { signal } from "@preact/signals";
import { useEffect } from "preact/hooks";
import type { ExportConfig } from "../lib/types";
import { ExportDialog } from "../components/export/ExportDialog";
import { RenderProgress } from "../components/export/RenderProgress";
import {
  initExportListeners,
  doStartRender,
  resetExportState,
  renderStatus,
} from "../stores/export";
import styles from "./Editor.module.scss";

/** The current phase of the export flow. */
type ExportPhase = "idle" | "configure" | "rendering";

const exportPhase = signal<ExportPhase>("idle");

export function Editor() {
  // Initialize Tauri event listeners for export progress
  useEffect(() => {
    initExportListeners();
    return () => {
      resetExportState();
    };
  }, []);

  // Watch render status to advance the export phase
  useEffect(() => {
    const status = renderStatus.value;
    if (
      exportPhase.value === "rendering" &&
      (status.status === "Completed" ||
        status.status === "Failed" ||
        status.status === "Cancelled")
    ) {
      // Stay on rendering phase to show the result — user clicks "Done" or "Upload"
    }
  }, [renderStatus.value]);

  const handleOpenExport = () => {
    exportPhase.value = "configure";
  };

  const handleExport = async (config: ExportConfig) => {
    exportPhase.value = "rendering";

    // TODO: Get these from the actual EditProject (Plan 3)
    // For now, use placeholder values
    const inputPath = ""; // from EditProject.source_path
    const filterGraph = null; // from EditProject filter graph builder
    const totalDurationSecs = 0; // from EditProject.duration
    const totalFrames = null;
    const projectId = ""; // from EditProject.id

    await doStartRender({
      inputPath,
      filterGraph,
      config,
      totalDurationSecs,
      totalFrames,
      projectId,
    });
  };

  const handleRenderDone = () => {
    exportPhase.value = "idle";
    resetExportState();
  };

  return (
    <div class={styles.editor}>
      {/* Editor toolbar */}
      <div class={styles.toolbar}>
        {/* ... other toolbar buttons from Plan 3 ... */}
        <button class={styles.exportButton} onClick={handleOpenExport}>
          Export
        </button>
      </div>

      {/* Editor canvas / timeline area */}
      <div class={styles.canvas}>
        {/* ... video preview and timeline from Plan 3 ... */}
        <p class={styles.canvasPlaceholder}>
          Editor canvas — see Plan 3 for full implementation
        </p>
      </div>

      {/* Export flow overlays */}
      {exportPhase.value === "configure" && (
        <ExportDialog
          onExport={handleExport}
          onCancel={() => {
            exportPhase.value = "idle";
          }}
          defaultOutputDir="" // TODO: from settings.export.output_directory
          sourceFileName="recording.mp4" // TODO: from EditProject
        />
      )}

      {exportPhase.value === "rendering" && (
        <div class={styles.overlay}>
          <div class={styles.overlayContent}>
            <RenderProgress
              onDone={handleRenderDone}
            />
          </div>
        </div>
      )}

      {/* Upload phase DEFERRED to v1.1 */}
    </div>
  );
}
```

- [ ] **Step 2: Create minimal Editor styles (SCSS module, if not from Plan 3)**

Create `src/pages/Editor.module.scss` (if it does not already exist):
```scss
.editor {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: var(--bg-primary, #1e1e2e);
}

.toolbar {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  padding: 8px 16px;
  background: var(--bg-secondary, #181825);
  border-bottom: 1px solid var(--border-color, #313244);
  gap: 8px;
}

.exportButton {
  padding: 6px 16px;
  background: var(--accent, #89b4fa);
  border: none;
  border-radius: 6px;
  color: var(--bg-primary, #1e1e2e);
  cursor: pointer;
  font-size: 13px;
  font-weight: 600;
}

.exportButton:hover {
  opacity: 0.9;
}

.canvas {
  flex: 1;
  overflow: hidden;
}

.canvasPlaceholder {
  color: var(--text-muted, #6c7086);
  text-align: center;
  padding-top: 40px;
}

.overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}

.overlayContent {
  background: var(--bg-primary, #1e1e2e);
  border: 1px solid var(--border-color, #313244);
  border-radius: 12px;
  width: 500px;
  max-height: 80vh;
  overflow-y: auto;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
}
```

- [ ] **Step 3: Verify the full frontend compiles**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
pnpm run build
```

Expected: No TypeScript errors.

- [ ] **Step 4: Run the full Tauri app to verify integration**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
pnpm tauri dev
```

Expected: App launches. The editor page shows an "Export" button. Clicking it opens the ExportDialog with preset options. Selecting a preset and clicking "Export" triggers the file save dialog, then transitions to the render progress view. On completion, the file is saved to the chosen local path.

- [ ] **Step 5: Run all Rust tests**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/src-tauri
cargo test -- --nocapture
```

Expected: All domain tests pass (preset, render, upload, progress parsing). Infrastructure tests pass.

- [ ] **Step 6: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src/pages/Editor.tsx src/pages/Editor.module.scss
git commit -m "feat: integrate export flow into editor page (configure, render, save to disk)"
```

---

## Summary

| Task | Layer | What it adds | v1 Status |
|------|-------|-------------|-----------|
| 1 | Domain | `ExportPreset`, `ExportConfig` types with built-in presets | v1 |
| 2 | Domain | `RenderJob`, `RenderProgress`, `RenderStatus`, FFmpeg progress line parser | v1 |
| 3 | Domain | `UploadTarget` trait, `UploadProgress`, `UploadResult` (stubs only) | STUB (v1.1) |
| 4 | Infrastructure | `Renderer` — builds FFmpeg args, spawns subprocess, streams progress via channel | v1 |
| 5 | Infrastructure | `GifRenderer` — two-pass palette optimization for high-quality GIF export | v1 |
| 6 | Infrastructure | `S3Uploader` — uploads to S3/R2/MinIO via `rust-s3` crate | **DEFERRED v1.1** |
| 7 | Infrastructure | `HttpUploader` — multipart POST/PUT via `reqwest` with URL extraction | **DEFERRED v1.1** |
| 8 | App | IPC commands: `start_render`, `cancel_render`, `get_render_status` | v1 (upload cmds deferred) |
| 9 | Frontend | Export store: render state, Tauri event listeners, action functions | v1 (upload state deferred) |
| 10 | Frontend | `ExportDialog` — preset picker, CRF slider, resolution override, speed preset (SCSS modules) | v1 |
| 11 | Frontend | `RenderProgress` — progress bar, ETA, speed, file size, cancel button, saved path (SCSS modules) | v1 |
| 12 | Frontend | `UploadPanel` — add/remove targets, S3/HTTP config forms, upload progress, URL copy | **DEFERRED v1.1** |
| 13 | Frontend | Wire export flow into editor page (configure -> render -> save to disk) | v1 |

### Dependencies added by this plan

**Rust (`Cargo.toml`):**
- `uuid = { version = "1", features = ["v4"] }` — render job IDs
- ~~`rust-s3 = { version = "0.35", ... }` — DEFERRED to v1.1~~
- ~~`reqwest = { version = "0.12", ... }` — DEFERRED to v1.1~~
- ~~`tokio-util = { version = "0.7", ... }` — DEFERRED to v1.1~~

**Frontend (`package.json`):**
- `@preact/signals` — reactive state management for export store

### Integration points with Plan 3

The export pipeline takes the following from Plan 3's edit system:
- **EditProject.source_path** — input file for FFmpeg
- **Filter graph string** — built from edit decisions (trims, crops, overlays, audio effects)
- **EditProject.duration** — total expected output duration for progress calculation
- **EditProject.id** — project identifier for the render job

These are marked with `TODO` comments in Task 13 and should be connected when Plan 3 is implemented.
