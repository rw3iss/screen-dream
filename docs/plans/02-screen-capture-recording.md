# Plan 2: Screen Capture & Recording (Screen Dream)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement screen/window enumeration, screenshot capture, the recording pipeline (continuous frame capture piped to FFmpeg), audio capture via cpal, recording lifecycle management (start/stop/pause/resume), and the transparent overlay window for region selection -- so users can record their screen, capture screenshots, and select regions to capture in Screen Dream.

**Architecture:** A `CaptureBackend` trait in the domain layer defines the interface for enumerating sources and capturing frames. The infrastructure layer implements this using xcap (cross-platform screen/window capture). The recording pipeline captures frames in a loop and pipes raw RGBA bytes to an FFmpeg subprocess via stdin. Audio is captured separately via cpal, written to a temp WAV file, then muxed with video by FFmpeg in a final pass. The frontend provides source selection, recording controls, and a transparent overlay window for region selection.

**Tech Stack:** Rust (`xcap` 0.9+, `cpal` 0.15, `hound` for WAV writing, `image` for frame manipulation), FFmpeg sidecar (from Plan 1's FfmpegProvider), Tauri IPC events, Preact frontend

**Depends on:** Plan 1 (Core Platform & Infrastructure) -- must be completed first.

**Related documents:**
- `PLAN.md` -- high-level architecture and feature overview
- `docs/plans/01-core-platform-infrastructure.md` -- Plan 1 (prerequisite)
- `docs/plans/03-media-editing.md` -- Plan 3 (depends on this plan)
- `docs/plans/04-export-sharing.md` -- Plan 4 (depends on Plans 1-3)

---

## Open Questions and Recommendations

1. **xcap for enumeration + screenshots:** Recommended. xcap 0.9+ provides `Monitor::all()`, `Window::all()`, and `capture_image()` across X11/macOS/Windows. It re-exports the `image` crate, making frame manipulation straightforward. For continuous recording, we capture frames in a loop using `monitor.capture_image()` at the target FPS and pipe raw bytes to FFmpeg.

2. **WAV temp file for audio:** Recommended over dual-pipe muxing. Capturing audio to a temp WAV file via cpal+hound is simpler and more reliable than managing two simultaneous FFmpeg stdin pipes. The final mux step combines the video file and audio WAV into the output file. This adds a small post-processing step but eliminates audio/video sync complexity during capture.

3. **Wayland portal picker limitation:** Accepted. On Wayland, `Window::all()` is not available due to protocol security restrictions. The app will detect Wayland and show a message directing users to select a source via the system portal picker. Screen capture (full monitor) still works via xcap on Wayland through the PipeWire/portal path.

4. **XWayland fallback for window enumeration:** Recommended for hybrid sessions. On Wayland sessions where `DISPLAY` is also set (indicating XWayland is running), we can attempt `Window::all()` to enumerate XWayland windows. This gives partial window listing on hybrid sessions, with a note that native Wayland windows will not appear.

5. **Wayland strategy (CONFIRMED):** The plan uses XWayland detection with X11 fallback for window enumeration (checking `DISPLAY` env var), and directs users to the system portal picker for pure Wayland. See `can_enumerate_windows()` in `XcapCaptureBackend` and the Wayland-specific messaging in `enumerate_sources()`.

6. **Recording format (CONFIRMED):** MP4 (H.264 via `libx264` + AAC for audio mux) is the default output format. WebM can be selected as an alternative via settings. See `RecordingConfig.video_codec` and the mux step in `stop_recording`.

7. **Resolution (CONFIRMED):** Native resolution is used by default. Frames are captured at the source's native resolution via `capture_image()`. The only resize occurs when frame dimensions need to be made even for libx264 compatibility (`enc_width = frame_width & !1`).

8. **Webcam PiP overlay: DEFERRED to v1.1.** Webcam picture-in-picture overlay during recording is out of scope for this plan. See `// TODO(v1.1): Webcam PiP overlay` in Known Limitations.

9. **App name:** The user-facing app name is "Screen Dream". All UI strings reference the `APP_NAME` constant from `src/lib/constants.ts` (exported as `export const APP_NAME = "Screen Dream";`) so the name can be changed in one place.

---

## File Structure

```
src-tauri/
  crates/
    domain/src/
      capture/
        mod.rs                        # Re-exports capture domain types
        source.rs                     # CaptureSource, SourceInfo types
        backend.rs                    # CaptureBackend trait
        recording.rs                  # RecordingState, RecordingConfig types
    infrastructure/src/
      capture/
        mod.rs                        # Re-exports capture infra modules
        xcap_backend.rs               # xcap-based CaptureBackend implementation
        recording_pipeline.rs         # Frame capture loop + FFmpeg encoding
        audio_capture.rs              # cpal microphone capture to WAV
  src/
    commands/
      capture.rs                      # IPC commands for capture/recording
    main.rs                           # (modified) register new commands + events

src/                                  # Frontend (Preact)
  lib/
    constants.ts                      # App-wide constants (APP_NAME, etc.)
    ipc.ts                            # (modified) add capture/recording IPC calls
    types.ts                          # (modified) add capture/recording TS types
  stores/
    recording.ts                      # Recording state store
  components/
    SourcePicker.tsx                   # Source selection dropdown/grid
    SourcePicker.module.scss           # SCSS module for SourcePicker
    RecordingControls.tsx             # Start/stop/pause buttons + timer
    RecordingControls.module.scss     # SCSS module for RecordingControls
    RegionSelector.tsx                # Transparent overlay for region drawing
    RegionSelector.module.scss        # SCSS module for RegionSelector
  pages/
    Home.tsx                          # (modified) integrate recording controls
    Home.module.scss                  # SCSS module for Home page
```

---

## Task 1: Domain Types -- CaptureSource, CaptureBackend Trait, RecordingState

**Files:**
- Create: `src-tauri/crates/domain/src/capture/mod.rs`
- Create: `src-tauri/crates/domain/src/capture/source.rs`
- Create: `src-tauri/crates/domain/src/capture/backend.rs`
- Create: `src-tauri/crates/domain/src/capture/recording.rs`
- Modify: `src-tauri/crates/domain/src/lib.rs`

- [ ] **Step 1: Create the capture module file**

Create `src-tauri/crates/domain/src/capture/mod.rs`:
```rust
pub mod backend;
pub mod recording;
pub mod source;

pub use backend::*;
pub use recording::*;
pub use source::*;
```

- [ ] **Step 2: Define capture source types**

Create `src-tauri/crates/domain/src/capture/source.rs`:
```rust
use serde::{Deserialize, Serialize};

/// Identifies what to capture.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum CaptureSource {
    /// Capture an entire monitor by its ID.
    Screen(ScreenSource),
    /// Capture a specific window by its ID.
    Window(WindowSource),
    /// Capture a rectangular region of a monitor.
    Region(RegionSource),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenSource {
    pub monitor_id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowSource {
    pub window_id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionSource {
    pub monitor_id: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Information about an available monitor for the frontend to display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub id: u32,
    pub name: String,
    pub friendly_name: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub scale_factor: f32,
    pub is_primary: bool,
}

/// Information about an available window for the frontend to display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: u32,
    pub pid: u32,
    pub app_name: String,
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub is_minimized: bool,
    pub is_focused: bool,
}

/// All available sources the user can pick from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableSources {
    pub monitors: Vec<MonitorInfo>,
    pub windows: Vec<WindowInfo>,
    /// If true, window enumeration is not available (e.g., native Wayland).
    pub windows_unavailable: bool,
    /// Human-readable reason why windows are unavailable.
    pub windows_unavailable_reason: Option<String>,
}
```

- [ ] **Step 3: Define the CaptureBackend trait**

Create `src-tauri/crates/domain/src/capture/backend.rs`:
```rust
use crate::error::AppResult;
use super::source::{AvailableSources, CaptureSource};

/// Raw captured frame data.
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    /// RGBA pixel data, row-major, 4 bytes per pixel.
    pub data: Vec<u8>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Trait for platform-specific screen/window capture.
/// Implemented by the infrastructure layer using xcap or platform-native APIs.
pub trait CaptureBackend: Send + Sync {
    /// Enumerate all available capture sources (monitors + windows).
    fn enumerate_sources(&self) -> AppResult<AvailableSources>;

    /// Capture a single frame from the given source.
    /// Returns raw RGBA pixel data suitable for encoding or saving.
    fn capture_frame(&self, source: &CaptureSource) -> AppResult<CapturedFrame>;
}
```

- [ ] **Step 4: Define recording state and config types**

Create `src-tauri/crates/domain/src/capture/recording.rs`:
```rust
use serde::{Deserialize, Serialize};
use super::source::CaptureSource;

/// Recording lifecycle state, emitted to the frontend via Tauri events.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RecordingState {
    /// No recording in progress.
    Idle,
    /// Recording is starting (initializing pipeline).
    Starting,
    /// Actively recording.
    Recording,
    /// Recording is paused (can be resumed).
    Paused,
    /// Recording is stopping (finalizing file).
    Stopping,
    /// Recording completed successfully.
    Completed,
    /// Recording failed with an error message.
    Failed(String),
}

/// Configuration for a recording session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingConfig {
    /// What to capture (screen, window, or region).
    pub source: CaptureSource,
    /// Target frames per second (e.g., 30 or 60).
    pub fps: u32,
    /// Video codec to use (maps to FFmpeg encoder).
    pub video_codec: String,
    /// CRF quality value (0-51, lower = better).
    pub crf: u8,
    /// Encoding speed preset (e.g., "ultrafast", "fast", "medium").
    pub preset: String,
    /// Output file path.
    pub output_path: String,
    /// Whether to capture microphone audio.
    pub capture_microphone: bool,
    /// Microphone device name (if capture_microphone is true).
    pub microphone_device: Option<String>,
}

/// Status update emitted during recording.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingStatus {
    pub state: RecordingState,
    /// Duration of the recording so far, in seconds.
    pub elapsed_seconds: f64,
    /// Number of frames captured so far.
    pub frames_captured: u64,
    /// Output file path (available after completion).
    pub output_path: Option<String>,
}
```

- [ ] **Step 5: Register the capture module in domain lib.rs**

Add to `src-tauri/crates/domain/src/lib.rs`:
```rust
pub mod capture;
```

- [ ] **Step 6: Verify domain compiles**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/src-tauri
cargo check -p domain
```

Expected: Compiles clean.

- [ ] **Step 7: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src-tauri/crates/domain/src/capture/ src-tauri/crates/domain/src/lib.rs
git commit -m "feat: add capture domain types (CaptureSource, CaptureBackend, RecordingState)"
```

---

## Task 2: Screen/Window Enumeration with xcap Backend

**Files:**
- Create: `src-tauri/crates/infrastructure/src/capture/mod.rs`
- Create: `src-tauri/crates/infrastructure/src/capture/xcap_backend.rs`
- Modify: `src-tauri/crates/infrastructure/src/lib.rs`
- Modify: `src-tauri/crates/infrastructure/Cargo.toml`

- [ ] **Step 1: Add xcap and image dependencies**

Add to `src-tauri/crates/infrastructure/Cargo.toml` under `[dependencies]`:
```toml
xcap = "0.9"
image = "0.25"
```

- [ ] **Step 2: Create the capture infrastructure module**

Create `src-tauri/crates/infrastructure/src/capture/mod.rs`:
```rust
pub mod xcap_backend;

pub use xcap_backend::*;
```

- [ ] **Step 3: Implement the xcap-based CaptureBackend**

Create `src-tauri/crates/infrastructure/src/capture/xcap_backend.rs`:
```rust
use domain::capture::{
    AvailableSources, CaptureBackend, CaptureSource, CapturedFrame,
    MonitorInfo, RegionSource, ScreenSource, WindowInfo, WindowSource,
};
use domain::error::{AppError, AppResult};
use domain::platform::{DisplayServer, PlatformInfo};
use image::GenericImageView;
use tracing::{debug, info, warn};
use xcap::{Monitor, Window};

/// CaptureBackend implementation using the xcap crate.
/// Works on X11, macOS, and Windows. Wayland has limited window enumeration.
pub struct XcapCaptureBackend {
    platform: PlatformInfo,
}

impl XcapCaptureBackend {
    pub fn new(platform: PlatformInfo) -> Self {
        info!(
            "Initializing xcap capture backend for {:?}/{:?}",
            platform.os, platform.display_server
        );
        XcapCaptureBackend { platform }
    }

    /// Check if we can enumerate windows on this platform.
    fn can_enumerate_windows(&self) -> bool {
        // xcap Window::all() works on X11, macOS, and Windows.
        // On pure Wayland (no XWayland), it will fail or return empty.
        match self.platform.display_server {
            DisplayServer::X11 => true,
            DisplayServer::Quartz => true,
            DisplayServer::Win32 => true,
            DisplayServer::Wayland => {
                // Check if XWayland is available (DISPLAY env var set)
                std::env::var("DISPLAY").is_ok()
            }
            DisplayServer::Unknown => false,
        }
    }

    fn enumerate_monitors(&self) -> AppResult<Vec<MonitorInfo>> {
        let monitors = Monitor::all().map_err(|e| {
            AppError::Capture(format!("Failed to enumerate monitors: {e}"))
        })?;

        let infos: Vec<MonitorInfo> = monitors
            .iter()
            .map(|m| MonitorInfo {
                id: m.id(),
                name: m.name().to_string(),
                friendly_name: m.friendly_name().to_string(),
                width: m.width(),
                height: m.height(),
                x: m.x(),
                y: m.y(),
                scale_factor: m.scale_factor(),
                is_primary: m.is_primary(),
            })
            .collect();

        debug!("Found {} monitors", infos.len());
        Ok(infos)
    }

    fn enumerate_windows(&self) -> AppResult<Vec<WindowInfo>> {
        let windows = Window::all().map_err(|e| {
            AppError::Capture(format!("Failed to enumerate windows: {e}"))
        })?;

        let infos: Vec<WindowInfo> = windows
            .iter()
            .filter(|w| !w.is_minimized() && w.width() > 0 && w.height() > 0)
            .map(|w| WindowInfo {
                id: w.id(),
                pid: w.pid(),
                app_name: w.app_name().to_string(),
                title: w.title().to_string(),
                width: w.width(),
                height: w.height(),
                is_minimized: w.is_minimized(),
                is_focused: w.is_focused(),
            })
            .collect();

        debug!("Found {} visible windows", infos.len());
        Ok(infos)
    }

    fn capture_monitor(&self, source: &ScreenSource) -> AppResult<CapturedFrame> {
        let monitors = Monitor::all().map_err(|e| {
            AppError::Capture(format!("Failed to enumerate monitors: {e}"))
        })?;

        let monitor = monitors
            .into_iter()
            .find(|m| m.id() == source.monitor_id)
            .ok_or_else(|| {
                AppError::Capture(format!(
                    "Monitor with ID {} not found",
                    source.monitor_id
                ))
            })?;

        let img = monitor.capture_image().map_err(|e| {
            AppError::Capture(format!("Failed to capture monitor {}: {e}", source.monitor_id))
        })?;

        let width = img.width();
        let height = img.height();
        let data = img.into_raw();

        Ok(CapturedFrame {
            data,
            width,
            height,
        })
    }

    fn capture_window(&self, source: &WindowSource) -> AppResult<CapturedFrame> {
        let windows = Window::all().map_err(|e| {
            AppError::Capture(format!("Failed to enumerate windows: {e}"))
        })?;

        let window = windows
            .into_iter()
            .find(|w| w.id() == source.window_id)
            .ok_or_else(|| {
                AppError::Capture(format!(
                    "Window with ID {} not found",
                    source.window_id
                ))
            })?;

        let img = window.capture_image().map_err(|e| {
            AppError::Capture(format!("Failed to capture window {}: {e}", source.window_id))
        })?;

        let width = img.width();
        let height = img.height();
        let data = img.into_raw();

        Ok(CapturedFrame {
            data,
            width,
            height,
        })
    }

    fn capture_region(&self, source: &RegionSource) -> AppResult<CapturedFrame> {
        // Capture the full monitor, then crop to the region.
        let full_frame = self.capture_monitor(&ScreenSource {
            monitor_id: source.monitor_id,
        })?;

        // Use the image crate to crop.
        let full_img = image::RgbaImage::from_raw(
            full_frame.width,
            full_frame.height,
            full_frame.data,
        )
        .ok_or_else(|| {
            AppError::Capture("Failed to reconstruct image from raw data".to_string())
        })?;

        let dynamic = image::DynamicImage::ImageRgba8(full_img);

        // Clamp crop region to image bounds.
        let crop_x = source.x.max(0) as u32;
        let crop_y = source.y.max(0) as u32;
        let crop_w = source.width.min(full_frame.width.saturating_sub(crop_x));
        let crop_h = source.height.min(full_frame.height.saturating_sub(crop_y));

        if crop_w == 0 || crop_h == 0 {
            return Err(AppError::Capture(
                "Region selection has zero width or height after clamping".to_string(),
            ));
        }

        let cropped = dynamic.crop_imm(crop_x, crop_y, crop_w, crop_h);
        let rgba = cropped.to_rgba8();
        let width = rgba.width();
        let height = rgba.height();
        let data = rgba.into_raw();

        Ok(CapturedFrame {
            data,
            width,
            height,
        })
    }
}

impl CaptureBackend for XcapCaptureBackend {
    fn enumerate_sources(&self) -> AppResult<AvailableSources> {
        let monitors = self.enumerate_monitors()?;

        let (windows, windows_unavailable, windows_unavailable_reason) =
            if self.can_enumerate_windows() {
                match self.enumerate_windows() {
                    Ok(wins) => (wins, false, None),
                    Err(e) => {
                        warn!("Window enumeration failed: {e}");
                        (
                            vec![],
                            true,
                            Some(format!("Window enumeration failed: {e}")),
                        )
                    }
                }
            } else {
                let reason = if self.platform.display_server == DisplayServer::Wayland {
                    "Window enumeration is not available on Wayland. \
                     Use the system portal picker or select a full screen to capture."
                        .to_string()
                } else {
                    "Window enumeration is not supported on this display server.".to_string()
                };
                (vec![], true, Some(reason))
            };

        Ok(AvailableSources {
            monitors,
            windows,
            windows_unavailable,
            windows_unavailable_reason,
        })
    }

    fn capture_frame(&self, source: &CaptureSource) -> AppResult<CapturedFrame> {
        match source {
            CaptureSource::Screen(s) => self.capture_monitor(s),
            CaptureSource::Window(w) => self.capture_window(w),
            CaptureSource::Region(r) => self.capture_region(r),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_create_backend() {
        let platform = PlatformInfo::detect();
        let backend = XcapCaptureBackend::new(platform);
        // Just verify it constructs without panic.
        assert!(backend.can_enumerate_windows() || !backend.can_enumerate_windows());
    }

    #[test]
    fn enumerate_sources_returns_at_least_one_monitor() {
        let platform = PlatformInfo::detect();
        let backend = XcapCaptureBackend::new(platform);

        // This test requires a display server (will skip in headless CI).
        match backend.enumerate_sources() {
            Ok(sources) => {
                assert!(
                    !sources.monitors.is_empty(),
                    "Should find at least one monitor"
                );
                println!("Monitors: {:#?}", sources.monitors);
                println!("Windows available: {}", !sources.windows_unavailable);
                println!("Windows count: {}", sources.windows.len());
            }
            Err(e) => {
                eprintln!("Skipping test (no display): {e}");
            }
        }
    }
}
```

- [ ] **Step 4: Register capture module in infrastructure lib.rs**

Add to `src-tauri/crates/infrastructure/src/lib.rs`:
```rust
pub mod capture;
```

- [ ] **Step 5: Verify infrastructure compiles**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/src-tauri
cargo check -p infrastructure
```

Expected: Compiles clean.

- [ ] **Step 6: Run the enumeration tests**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/src-tauri
cargo test -p infrastructure xcap_backend -- --nocapture
```

Expected: Tests pass on a system with a display server. On headless CI, they skip gracefully.

- [ ] **Step 7: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src-tauri/crates/infrastructure/src/capture/ src-tauri/crates/infrastructure/src/lib.rs src-tauri/crates/infrastructure/Cargo.toml
git commit -m "feat: add xcap-based capture backend with monitor/window enumeration"
```

---

## Task 3: Screenshot Capture -- Save to File and Clipboard

**Files:**
- Create: `src-tauri/src/commands/capture.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Add the capture backend to AppState**

Modify `src-tauri/src/state.rs` -- add a `capture` field:
```rust
use std::sync::Arc;

use domain::capture::CaptureBackend;
use domain::ffmpeg::FfmpegProvider;
use domain::platform::PlatformInfo;
use domain::settings::SettingsRepository;

/// Central application state, managed by Tauri.
/// Holds references to all infrastructure services.
pub struct AppState {
    pub ffmpeg: Arc<dyn FfmpegProvider>,
    pub settings: Arc<dyn SettingsRepository>,
    pub platform: PlatformInfo,
    pub capture: Arc<dyn CaptureBackend>,
}
```

- [ ] **Step 2: Create capture IPC commands**

Create `src-tauri/src/commands/capture.rs`:
```rust
use std::path::PathBuf;

use domain::capture::{AvailableSources, CaptureSource};
use tauri::State;
use tracing::{debug, info};

use crate::error::CommandResult;
use crate::state::AppState;

use domain::error::AppError;

/// List all available capture sources (monitors and windows).
#[tauri::command]
pub fn list_capture_sources(state: State<'_, AppState>) -> CommandResult<AvailableSources> {
    debug!("Listing capture sources");
    state.capture.enumerate_sources().map_err(Into::into)
}

/// Capture a screenshot and save it to a file.
/// Returns the path to the saved file.
#[tauri::command]
pub fn take_screenshot(
    state: State<'_, AppState>,
    source: CaptureSource,
    output_path: String,
    format: String,
    quality: u8,
) -> CommandResult<String> {
    info!("Taking screenshot: source={source:?}, path={output_path}, format={format}");

    let frame = state.capture.capture_frame(&source).map_err(Into::into)?;

    let rgba_image = image::RgbaImage::from_raw(frame.width, frame.height, frame.data)
        .ok_or_else(|| AppError::Capture("Failed to create image from frame data".to_string()))?;

    let path = PathBuf::from(&output_path);

    // Ensure parent directory exists.
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::Io(format!("Failed to create output directory: {e}"))
        })?;
    }

    match format.as_str() {
        "png" => {
            rgba_image.save_with_format(&path, image::ImageFormat::Png).map_err(|e| {
                AppError::Capture(format!("Failed to save PNG: {e}"))
            })?;
        }
        "jpeg" | "jpg" => {
            // Convert RGBA to RGB for JPEG (JPEG doesn't support alpha).
            let rgb_image = image::DynamicImage::ImageRgba8(rgba_image).to_rgb8();
            let mut file = std::fs::File::create(&path).map_err(|e| {
                AppError::Io(format!("Failed to create file: {e}"))
            })?;
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut file, quality);
            encoder
                .encode(
                    rgb_image.as_raw(),
                    rgb_image.width(),
                    rgb_image.height(),
                    image::ExtendedColorType::Rgb8,
                )
                .map_err(|e| AppError::Capture(format!("Failed to encode JPEG: {e}")))?;
        }
        "webp" => {
            rgba_image.save_with_format(&path, image::ImageFormat::WebP).map_err(|e| {
                AppError::Capture(format!("Failed to save WebP: {e}"))
            })?;
        }
        _ => {
            return Err(
                AppError::Capture(format!("Unsupported screenshot format: {format}")).into(),
            );
        }
    }

    info!("Screenshot saved to {}", path.display());
    Ok(path.to_string_lossy().to_string())
}

/// Capture a screenshot and return it as base64-encoded PNG for clipboard use.
/// The frontend can then write this to the clipboard.
#[tauri::command]
pub fn capture_to_clipboard(
    state: State<'_, AppState>,
    source: CaptureSource,
) -> CommandResult<String> {
    debug!("Capturing frame for clipboard");

    let frame = state.capture.capture_frame(&source).map_err(Into::into)?;

    let rgba_image = image::RgbaImage::from_raw(frame.width, frame.height, frame.data)
        .ok_or_else(|| AppError::Capture("Failed to create image from frame data".to_string()))?;

    // Encode as PNG to a buffer.
    let mut buf = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut buf);
    encoder
        .encode(
            rgba_image.as_raw(),
            rgba_image.width(),
            rgba_image.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| AppError::Capture(format!("Failed to encode PNG for clipboard: {e}")))?;

    // Return base64-encoded PNG data. The frontend will handle clipboard write.
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&buf);

    Ok(b64)
}
```

- [ ] **Step 3: Add image and base64 dependencies to the app crate**

Add to `src-tauri/Cargo.toml` under `[dependencies]`:
```toml
image = "0.25"
base64 = "0.22"
```

- [ ] **Step 4: Register the capture commands**

Add to `src-tauri/src/commands/mod.rs`:
```rust
pub mod capture;
```

- [ ] **Step 5: Wire the capture backend into main.rs**

In `src-tauri/src/main.rs`, inside the `.setup()` closure, after creating `ffmpeg_resolver`, add:
```rust
use infrastructure::capture::XcapCaptureBackend;

// Initialize capture backend
let capture_backend = Arc::new(XcapCaptureBackend::new(platform.clone()));
```

Update the `AppState` initialization:
```rust
app.manage(AppState {
    ffmpeg: ffmpeg_resolver,
    settings: settings_repo,
    platform,
    capture: capture_backend,
});
```

Register the new commands in the `invoke_handler`:
```rust
.invoke_handler(tauri::generate_handler![
    commands::platform::get_platform_info,
    commands::settings::get_settings,
    commands::settings::save_settings,
    commands::settings::reset_settings,
    commands::ffmpeg::get_ffmpeg_status,
    commands::capture::list_capture_sources,
    commands::capture::take_screenshot,
    commands::capture::capture_to_clipboard,
])
```

- [ ] **Step 6: Verify the app compiles**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/src-tauri
cargo check
```

Expected: Compiles clean.

- [ ] **Step 7: Manual test -- take a screenshot**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
pnpm tauri dev
```

In the browser devtools console (F12), run:
```javascript
// List sources
const sources = await window.__TAURI__.core.invoke('list_capture_sources');
console.log('Sources:', sources);

// Take a screenshot of the primary monitor
const monitorId = sources.monitors[0].id;
await window.__TAURI__.core.invoke('take_screenshot', {
  source: { type: 'Screen', data: { monitor_id: monitorId } },
  outputPath: '/tmp/test-screenshot.png',
  format: 'png',
  quality: 100
});
console.log('Screenshot saved!');
```

Expected: A screenshot file appears at `/tmp/test-screenshot.png`. Verify it opens in an image viewer.

- [ ] **Step 8: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src-tauri/src/commands/capture.rs src-tauri/src/commands/mod.rs src-tauri/src/state.rs src-tauri/src/main.rs src-tauri/Cargo.toml
git commit -m "feat: add screenshot capture commands (file save + clipboard)"
```

---

## Task 4: Recording Pipeline Service (Frame Capture to FFmpeg Encoding)

**Files:**
- Create: `src-tauri/crates/infrastructure/src/capture/recording_pipeline.rs`
- Modify: `src-tauri/crates/infrastructure/src/capture/mod.rs`

- [ ] **Step 1: Add tokio and chrono dependencies**

Ensure `src-tauri/crates/infrastructure/Cargo.toml` has:
```toml
[dependencies]
# ... existing deps ...
chrono = "0.4"
```

(tokio should already be present from Plan 1)

- [ ] **Step 2: Implement the recording pipeline**

Create `src-tauri/crates/infrastructure/src/capture/recording_pipeline.rs`:
```rust
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use domain::capture::{CaptureBackend, CapturedFrame, RecordingConfig, RecordingState, RecordingStatus};
use domain::error::{AppError, AppResult};
use tracing::{debug, error, info, warn};

/// Manages a recording session: captures frames in a loop, pipes to FFmpeg.
///
/// The pipeline runs in a dedicated thread (not async) because frame capture
/// is CPU-bound and we need precise timing control. Communication with the
/// caller happens via atomic flags and a status callback.
pub struct RecordingPipeline {
    /// Shared flag: set to false to stop recording.
    running: Arc<AtomicBool>,
    /// Shared flag: set to true to pause frame capture.
    paused: Arc<AtomicBool>,
    /// Frame counter (readable from outside).
    frame_count: Arc<AtomicU64>,
    /// Recording start time (set when recording begins).
    start_time: Option<Instant>,
    /// Handle to the recording thread.
    thread_handle: Option<std::thread::JoinHandle<AppResult<String>>>,
}

impl RecordingPipeline {
    /// Start a new recording session.
    ///
    /// This spawns a background thread that:
    /// 1. Launches FFmpeg with rawvideo input from stdin
    /// 2. Captures frames from the backend at the target FPS
    /// 3. Writes raw RGBA bytes to FFmpeg's stdin
    /// 4. Stops when `running` is set to false
    ///
    /// Returns immediately. Use `stop()` to end recording and get the output path.
    pub fn start(
        config: RecordingConfig,
        ffmpeg_path: PathBuf,
        capture_backend: Arc<dyn CaptureBackend>,
    ) -> AppResult<Self> {
        info!("Starting recording pipeline: {:?}", config);

        // Validate the source by capturing a test frame to get dimensions.
        let test_frame = capture_backend.capture_frame(&config.source)?;
        let frame_width = test_frame.width;
        let frame_height = test_frame.height;
        info!(
            "Capture dimensions: {}x{} ({} bytes per frame)",
            frame_width,
            frame_height,
            test_frame.data.len()
        );

        let running = Arc::new(AtomicBool::new(true));
        let paused = Arc::new(AtomicBool::new(false));
        let frame_count = Arc::new(AtomicU64::new(0));

        let running_clone = running.clone();
        let paused_clone = paused.clone();
        let frame_count_clone = frame_count.clone();

        let thread_handle = std::thread::Builder::new()
            .name("recording-pipeline".to_string())
            .spawn(move || {
                run_pipeline(
                    config,
                    ffmpeg_path,
                    capture_backend,
                    frame_width,
                    frame_height,
                    running_clone,
                    paused_clone,
                    frame_count_clone,
                )
            })
            .map_err(|e| {
                AppError::Capture(format!("Failed to spawn recording thread: {e}"))
            })?;

        Ok(RecordingPipeline {
            running,
            paused,
            frame_count,
            start_time: Some(Instant::now()),
            thread_handle: Some(thread_handle),
        })
    }

    /// Pause recording (frames stop being captured, FFmpeg stays running).
    pub fn pause(&self) {
        info!("Pausing recording");
        self.paused.store(true, Ordering::SeqCst);
    }

    /// Resume recording after a pause.
    pub fn resume(&self) {
        info!("Resuming recording");
        self.paused.store(false, Ordering::SeqCst);
    }

    /// Check if currently paused.
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// Check if the pipeline is still running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get the current recording status.
    pub fn status(&self) -> RecordingStatus {
        let elapsed = self
            .start_time
            .map(|t| t.elapsed().as_secs_f64())
            .unwrap_or(0.0);

        let state = if !self.is_running() {
            RecordingState::Stopping
        } else if self.is_paused() {
            RecordingState::Paused
        } else {
            RecordingState::Recording
        };

        RecordingStatus {
            state,
            elapsed_seconds: elapsed,
            frames_captured: self.frame_count.load(Ordering::Relaxed),
            output_path: None,
        }
    }

    /// Stop recording and wait for the pipeline to finish.
    /// Returns the path to the output video file.
    pub fn stop(&mut self) -> AppResult<String> {
        info!("Stopping recording pipeline");
        self.running.store(false, Ordering::SeqCst);
        // Also unpause so the loop can exit.
        self.paused.store(false, Ordering::SeqCst);

        if let Some(handle) = self.thread_handle.take() {
            match handle.join() {
                Ok(result) => result,
                Err(_) => Err(AppError::Capture(
                    "Recording thread panicked".to_string(),
                )),
            }
        } else {
            Err(AppError::Capture(
                "Recording thread already joined".to_string(),
            ))
        }
    }
}

/// The actual recording loop, runs in a dedicated thread.
fn run_pipeline(
    config: RecordingConfig,
    ffmpeg_path: PathBuf,
    capture_backend: Arc<dyn CaptureBackend>,
    frame_width: u32,
    frame_height: u32,
    running: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    frame_count: Arc<AtomicU64>,
) -> AppResult<String> {
    // Ensure output width and height are even (required by libx264).
    let enc_width = frame_width & !1;
    let enc_height = frame_height & !1;

    // Build FFmpeg command for raw video input.
    let output_path = &config.output_path;
    let fps_str = config.fps.to_string();
    let size_str = format!("{enc_width}x{enc_height}");

    let mut ffmpeg_args = vec![
        "-hide_banner",
        "-y",
        // Input: raw RGBA video from stdin
        "-f", "rawvideo",
        "-pix_fmt", "rgba",
        "-s", &size_str,
        "-r", &fps_str,
        "-i", "pipe:0",
        // Output encoding
        "-c:v", &config.video_codec,
        "-pix_fmt", "yuv420p",
        "-preset", &config.preset,
        "-crf", &config.crf.to_string(),
        // Ensure even dimensions
        "-vf", &format!("scale={enc_width}:{enc_height}"),
    ];

    ffmpeg_args.push(output_path.as_str());

    info!("FFmpeg command: {} {}", ffmpeg_path.display(), ffmpeg_args.join(" "));

    let mut ffmpeg_child = Command::new(&ffmpeg_path)
        .args(&ffmpeg_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            AppError::FfmpegExecution(format!("Failed to spawn FFmpeg: {e}"))
        })?;

    let mut stdin = ffmpeg_child.stdin.take().ok_or_else(|| {
        AppError::FfmpegExecution("Failed to open FFmpeg stdin".to_string())
    })?;

    let frame_interval = Duration::from_secs_f64(1.0 / config.fps as f64);
    let mut frames_written: u64 = 0;

    info!("Recording loop started at {} FPS", config.fps);

    while running.load(Ordering::SeqCst) {
        let frame_start = Instant::now();

        // Skip frame capture if paused (but keep the loop alive).
        if paused.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }

        // Capture a frame.
        match capture_backend.capture_frame(&config.source) {
            Ok(frame) => {
                // If frame dimensions don't match, resize to expected.
                let data = if frame.width != enc_width || frame.height != enc_height {
                    let img = image::RgbaImage::from_raw(frame.width, frame.height, frame.data)
                        .ok_or_else(|| {
                            AppError::Capture("Invalid frame data".to_string())
                        })?;
                    let resized = image::imageops::resize(
                        &img,
                        enc_width,
                        enc_height,
                        image::imageops::FilterType::Nearest,
                    );
                    resized.into_raw()
                } else {
                    frame.data
                };

                // Write raw RGBA bytes to FFmpeg stdin.
                if let Err(e) = stdin.write_all(&data) {
                    // Broken pipe means FFmpeg exited (probably an error).
                    error!("Failed to write frame to FFmpeg: {e}");
                    break;
                }

                frames_written += 1;
                frame_count.store(frames_written, Ordering::Relaxed);
            }
            Err(e) => {
                warn!("Frame capture failed (skipping): {e}");
                // Continue trying — transient errors are common.
            }
        }

        // Sleep for the remainder of the frame interval.
        let elapsed = frame_start.elapsed();
        if elapsed < frame_interval {
            std::thread::sleep(frame_interval - elapsed);
        }
    }

    info!("Recording loop ended after {frames_written} frames");

    // Close stdin to signal EOF to FFmpeg.
    drop(stdin);

    // Wait for FFmpeg to finish encoding.
    let output = ffmpeg_child.wait_with_output().map_err(|e| {
        AppError::FfmpegExecution(format!("Failed to wait for FFmpeg: {e}"))
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("FFmpeg exited with error:\n{stderr}");
        return Err(AppError::FfmpegExecution(format!(
            "FFmpeg encoding failed (exit code {}): {}",
            output.status.code().unwrap_or(-1),
            stderr.lines().last().unwrap_or("unknown error")
        )));
    }

    info!("Recording saved to {output_path}");
    Ok(output_path.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::capture::{CaptureSource, ScreenSource};

    // NOTE: This test requires a display server and FFmpeg installed.
    // Run manually: cargo test -p infrastructure recording_pipeline -- --nocapture --ignored
    #[test]
    #[ignore]
    fn record_two_seconds() {
        use domain::platform::PlatformInfo;
        use crate::capture::XcapCaptureBackend;

        let platform = PlatformInfo::detect();
        let backend = Arc::new(XcapCaptureBackend::new(platform));

        let sources = backend.enumerate_sources().expect("enumerate sources");
        let monitor = &sources.monitors[0];

        let config = RecordingConfig {
            source: CaptureSource::Screen(ScreenSource {
                monitor_id: monitor.id,
            }),
            fps: 10,
            video_codec: "libx264".to_string(),
            crf: 28,
            preset: "ultrafast".to_string(),
            output_path: "/tmp/test-recording.mp4".to_string(),
            capture_microphone: false,
            microphone_device: None,
        };

        let ffmpeg_path = which::which("ffmpeg").expect("FFmpeg must be installed");

        let mut pipeline = RecordingPipeline::start(config, ffmpeg_path, backend)
            .expect("start pipeline");

        assert!(pipeline.is_running());

        // Record for 2 seconds.
        std::thread::sleep(Duration::from_secs(2));

        let output = pipeline.stop().expect("stop pipeline");
        assert_eq!(output, "/tmp/test-recording.mp4");

        // Verify the file was created and has content.
        let metadata = std::fs::metadata(&output).expect("output file should exist");
        assert!(metadata.len() > 0, "output file should not be empty");

        println!("Recording saved: {} ({} bytes)", output, metadata.len());
    }
}
```

- [ ] **Step 3: Register the recording pipeline in the capture module**

Update `src-tauri/crates/infrastructure/src/capture/mod.rs`:
```rust
pub mod recording_pipeline;
pub mod xcap_backend;

pub use recording_pipeline::*;
pub use xcap_backend::*;
```

- [ ] **Step 4: Add `which` dev-dependency for tests**

Add to `src-tauri/crates/infrastructure/Cargo.toml` if not already present:
```toml
[dev-dependencies]
tempfile = "3"
which = "7"
```

- [ ] **Step 5: Verify infrastructure compiles**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/src-tauri
cargo check -p infrastructure
```

Expected: Compiles clean.

- [ ] **Step 6: Run the recording test (manual, requires display + FFmpeg)**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/src-tauri
cargo test -p infrastructure recording_pipeline -- --nocapture --ignored
```

Expected: A 2-second video file is created at `/tmp/test-recording.mp4`. Verify it plays.

- [ ] **Step 7: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src-tauri/crates/infrastructure/src/capture/recording_pipeline.rs src-tauri/crates/infrastructure/src/capture/mod.rs src-tauri/crates/infrastructure/Cargo.toml
git commit -m "feat: add recording pipeline (frame capture loop + FFmpeg encoding)"
```

---

## Task 5: Audio Capture Module (Microphone via cpal)

**Files:**
- Create: `src-tauri/crates/infrastructure/src/capture/audio_capture.rs`
- Modify: `src-tauri/crates/infrastructure/src/capture/mod.rs`
- Modify: `src-tauri/crates/infrastructure/Cargo.toml`

- [ ] **Step 1: Add cpal and hound dependencies**

Add to `src-tauri/crates/infrastructure/Cargo.toml` under `[dependencies]`:
```toml
cpal = "0.15"
hound = "3.5"
```

- [ ] **Step 2: Implement audio capture to WAV**

Create `src-tauri/crates/infrastructure/src/capture/audio_capture.rs`:
```rust
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, SampleRate, StreamConfig};
use domain::error::{AppError, AppResult};
use tracing::{debug, error, info, warn};

/// Information about an available audio input device.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AudioDeviceInfo {
    pub name: String,
    pub is_default: bool,
    pub sample_rate: u32,
    pub channels: u16,
}

/// Manages microphone audio capture, writing samples to a WAV file.
pub struct AudioCapture {
    /// Shared flag: set to false to stop recording.
    running: Arc<AtomicBool>,
    /// The cpal stream handle (keeps the stream alive while held).
    stream: Option<cpal::Stream>,
    /// Path to the output WAV file.
    output_path: PathBuf,
    /// WAV writer wrapped in Arc<Mutex> for thread-safe access from the audio callback.
    writer: Arc<Mutex<Option<hound::WavWriter<std::io::BufWriter<std::fs::File>>>>>,
    /// Sample rate of the recording.
    pub sample_rate: u32,
    /// Number of channels.
    pub channels: u16,
}

/// List available audio input devices.
pub fn list_audio_devices() -> AppResult<Vec<AudioDeviceInfo>> {
    let host = cpal::default_host();

    let default_device_name = host
        .default_input_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_default();

    let mut devices = Vec::new();

    let input_devices = host.input_devices().map_err(|e| {
        AppError::Capture(format!("Failed to list audio input devices: {e}"))
    })?;

    for device in input_devices {
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        let is_default = name == default_device_name;

        let config = device
            .default_input_config()
            .map_err(|e| {
                AppError::Capture(format!("Failed to get config for device {name}: {e}"))
            })?;

        devices.push(AudioDeviceInfo {
            name,
            is_default,
            sample_rate: config.sample_rate().0,
            channels: config.channels(),
        });
    }

    debug!("Found {} audio input devices", devices.len());
    Ok(devices)
}

impl AudioCapture {
    /// Start capturing audio from the specified device (or default) to a WAV file.
    pub fn start(device_name: Option<&str>, output_path: PathBuf) -> AppResult<Self> {
        let host = cpal::default_host();

        // Find the target device.
        let device: Device = if let Some(name) = device_name {
            let input_devices = host.input_devices().map_err(|e| {
                AppError::Capture(format!("Failed to list input devices: {e}"))
            })?;

            input_devices
                .into_iter()
                .find(|d| d.name().map_or(false, |n| n == name))
                .ok_or_else(|| {
                    AppError::Capture(format!("Audio device not found: {name}"))
                })?
        } else {
            host.default_input_device().ok_or_else(|| {
                AppError::Capture("No default audio input device available".to_string())
            })?
        };

        let device_name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        info!("Using audio device: {device_name}");

        let supported_config = device.default_input_config().map_err(|e| {
            AppError::Capture(format!("Failed to get input config: {e}"))
        })?;

        let sample_rate = supported_config.sample_rate().0;
        let channels = supported_config.channels();
        let sample_format = supported_config.sample_format();

        info!(
            "Audio config: {}Hz, {} channels, {:?}",
            sample_rate, channels, sample_format
        );

        // Create the WAV writer.
        let wav_spec = hound::WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let writer = hound::WavWriter::create(&output_path, wav_spec).map_err(|e| {
            AppError::Capture(format!("Failed to create WAV file at {}: {e}", output_path.display()))
        })?;

        let writer = Arc::new(Mutex::new(Some(writer)));
        let running = Arc::new(AtomicBool::new(true));

        let writer_clone = writer.clone();
        let running_clone = running.clone();

        let stream_config = StreamConfig {
            channels,
            sample_rate: SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let err_callback = |err: cpal::StreamError| {
            error!("Audio stream error: {err}");
        };

        // Build the input stream based on sample format.
        let stream = match sample_format {
            SampleFormat::F32 => {
                device.build_input_stream(
                    &stream_config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        if !running_clone.load(Ordering::Relaxed) {
                            return;
                        }
                        if let Ok(mut guard) = writer_clone.lock() {
                            if let Some(ref mut w) = *guard {
                                for &sample in data {
                                    if w.write_sample(sample).is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    },
                    err_callback,
                    None,
                )
            }
            SampleFormat::I16 => {
                let writer_clone2 = writer.clone();
                let running_clone2 = running.clone();
                device.build_input_stream(
                    &stream_config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        if !running_clone2.load(Ordering::Relaxed) {
                            return;
                        }
                        if let Ok(mut guard) = writer_clone2.lock() {
                            if let Some(ref mut w) = *guard {
                                for &sample in data {
                                    let f = sample as f32 / i16::MAX as f32;
                                    if w.write_sample(f).is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    },
                    err_callback,
                    None,
                )
            }
            SampleFormat::U16 => {
                let writer_clone3 = writer.clone();
                let running_clone3 = running.clone();
                device.build_input_stream(
                    &stream_config,
                    move |data: &[u16], _: &cpal::InputCallbackInfo| {
                        if !running_clone3.load(Ordering::Relaxed) {
                            return;
                        }
                        if let Ok(mut guard) = writer_clone3.lock() {
                            if let Some(ref mut w) = *guard {
                                for &sample in data {
                                    let f = (sample as f32 - 32768.0) / 32768.0;
                                    if w.write_sample(f).is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    },
                    err_callback,
                    None,
                )
            }
            other => {
                return Err(AppError::Capture(format!(
                    "Unsupported audio sample format: {other:?}"
                )));
            }
        }
        .map_err(|e| {
            AppError::Capture(format!("Failed to build audio input stream: {e}"))
        })?;

        // Start the stream.
        stream.play().map_err(|e| {
            AppError::Capture(format!("Failed to start audio stream: {e}"))
        })?;

        info!("Audio capture started -> {}", output_path.display());

        Ok(AudioCapture {
            running,
            stream: Some(stream),
            output_path,
            writer,
            sample_rate,
            channels,
        })
    }

    /// Stop audio capture and finalize the WAV file.
    /// Returns the path to the WAV file.
    pub fn stop(&mut self) -> AppResult<PathBuf> {
        info!("Stopping audio capture");
        self.running.store(false, Ordering::SeqCst);

        // Drop the stream to stop the audio callback.
        self.stream.take();

        // Finalize the WAV file.
        if let Ok(mut guard) = self.writer.lock() {
            if let Some(writer) = guard.take() {
                writer.finalize().map_err(|e| {
                    AppError::Capture(format!("Failed to finalize WAV file: {e}"))
                })?;
            }
        }

        info!("Audio saved to {}", self.output_path.display());
        Ok(self.output_path.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_audio_devices_does_not_panic() {
        // May return empty list if no audio devices, but should not panic.
        match list_audio_devices() {
            Ok(devices) => {
                println!("Audio devices: {:#?}", devices);
            }
            Err(e) => {
                eprintln!("Audio device listing failed (no audio subsystem?): {e}");
            }
        }
    }

    // Manual test: requires a microphone.
    // cargo test -p infrastructure audio_capture_record -- --nocapture --ignored
    #[test]
    #[ignore]
    fn record_two_seconds_audio() {
        let output = PathBuf::from("/tmp/test-audio.wav");
        let mut capture = AudioCapture::start(None, output.clone()).expect("start audio");

        std::thread::sleep(std::time::Duration::from_secs(2));

        let path = capture.stop().expect("stop audio");
        assert_eq!(path, output);

        let metadata = std::fs::metadata(&path).expect("WAV file should exist");
        assert!(metadata.len() > 44, "WAV file should have data beyond header");

        println!("Audio recorded: {} ({} bytes)", path.display(), metadata.len());
    }
}
```

- [ ] **Step 3: Register audio_capture in the module**

Update `src-tauri/crates/infrastructure/src/capture/mod.rs`:
```rust
pub mod audio_capture;
pub mod recording_pipeline;
pub mod xcap_backend;

pub use audio_capture::*;
pub use recording_pipeline::*;
pub use xcap_backend::*;
```

- [ ] **Step 4: Verify it compiles**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/src-tauri
cargo check -p infrastructure
```

Expected: Compiles clean.

- [ ] **Step 5: Run the audio device listing test**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/src-tauri
cargo test -p infrastructure audio_capture::tests::list_audio_devices -- --nocapture
```

Expected: Test passes, lists any available audio devices.

- [ ] **Step 6: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src-tauri/crates/infrastructure/src/capture/audio_capture.rs src-tauri/crates/infrastructure/src/capture/mod.rs src-tauri/crates/infrastructure/Cargo.toml
git commit -m "feat: add microphone audio capture to WAV via cpal"
```

---

## Task 6: Recording Lifecycle Commands (Start, Stop, Pause, Resume)

**Files:**
- Modify: `src-tauri/src/commands/capture.rs`
- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Add recording state management to AppState**

Modify `src-tauri/src/state.rs` to hold the active recording:
```rust
use std::sync::{Arc, Mutex};

use domain::capture::CaptureBackend;
use domain::ffmpeg::FfmpegProvider;
use domain::platform::PlatformInfo;
use domain::settings::SettingsRepository;
use infrastructure::capture::{AudioCapture, RecordingPipeline};

/// Active recording session, if any.
pub struct ActiveRecording {
    pub pipeline: RecordingPipeline,
    pub audio: Option<AudioCapture>,
}

/// Central application state, managed by Tauri.
pub struct AppState {
    pub ffmpeg: Arc<dyn FfmpegProvider>,
    pub settings: Arc<dyn SettingsRepository>,
    pub platform: PlatformInfo,
    pub capture: Arc<dyn CaptureBackend>,
    /// Currently active recording session (if any). Mutex for interior mutability.
    pub active_recording: Mutex<Option<ActiveRecording>>,
}
```

- [ ] **Step 2: Add recording lifecycle commands**

Extend `src-tauri/src/commands/capture.rs` with the following commands (add below the existing screenshot commands):
```rust
use std::sync::Mutex;
use std::path::PathBuf;

use domain::capture::{
    AvailableSources, CaptureSource, RecordingConfig, RecordingState, RecordingStatus,
};
use infrastructure::capture::{
    list_audio_devices, AudioCapture, AudioDeviceInfo, RecordingPipeline,
};
use tauri::{Emitter, State};
use tracing::{debug, error, info};

use crate::error::CommandResult;
use crate::state::{ActiveRecording, AppState};

use domain::error::AppError;

// ... (existing list_capture_sources, take_screenshot, capture_to_clipboard) ...

/// List available audio input devices.
#[tauri::command]
pub fn list_audio_devices_cmd() -> CommandResult<Vec<AudioDeviceInfo>> {
    list_audio_devices().map_err(Into::into)
}

/// Start a new recording session.
#[tauri::command]
pub fn start_recording(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
    config: RecordingConfig,
) -> CommandResult<()> {
    info!("start_recording command: {:?}", config);

    // Check if already recording.
    {
        let guard = state.active_recording.lock().map_err(|_| {
            AppError::Capture("Recording state lock poisoned".to_string())
        })?;
        if guard.is_some() {
            return Err(AppError::Capture(
                "A recording is already in progress. Stop it first.".to_string(),
            ).into());
        }
    }

    // Resolve FFmpeg path.
    let ffmpeg_path = state.ffmpeg.ffmpeg_path().map_err(Into::into)?;

    // Emit starting event.
    let _ = app_handle.emit("recording-state", RecordingState::Starting);

    // Start audio capture if requested.
    let audio = if config.capture_microphone {
        let audio_path = PathBuf::from(&config.output_path)
            .with_extension("wav");
        match AudioCapture::start(
            config.microphone_device.as_deref(),
            audio_path,
        ) {
            Ok(capture) => {
                info!("Audio capture started");
                Some(capture)
            }
            Err(e) => {
                error!("Failed to start audio capture: {e}");
                // Don't fail the whole recording, just skip audio.
                let _ = app_handle.emit(
                    "recording-warning",
                    format!("Audio capture failed: {e}. Recording without audio."),
                );
                None
            }
        }
    } else {
        None
    };

    // Start the video recording pipeline.
    let pipeline = RecordingPipeline::start(
        config,
        ffmpeg_path,
        state.capture.clone(),
    )
    .map_err(|e| -> domain::error::AppError { e })?;

    // Store the active recording.
    {
        let mut guard = state.active_recording.lock().map_err(|_| {
            AppError::Capture("Recording state lock poisoned".to_string())
        })?;
        *guard = Some(ActiveRecording { pipeline, audio });
    }

    // Emit recording state.
    let _ = app_handle.emit("recording-state", RecordingState::Recording);

    info!("Recording started");
    Ok(())
}

/// Stop the current recording.
/// Returns the path to the output video file.
#[tauri::command]
pub fn stop_recording(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<String> {
    info!("stop_recording command");

    let _ = app_handle.emit("recording-state", RecordingState::Stopping);

    let mut guard = state.active_recording.lock().map_err(|_| {
        AppError::Capture("Recording state lock poisoned".to_string())
    })?;

    let mut recording = guard.take().ok_or_else(|| {
        AppError::Capture("No recording in progress".to_string())
    })?;

    // Stop audio capture first.
    let audio_path = if let Some(ref mut audio) = recording.audio {
        match audio.stop() {
            Ok(path) => Some(path),
            Err(e) => {
                error!("Failed to stop audio capture: {e}");
                None
            }
        }
    } else {
        None
    };

    // Stop video recording.
    let video_path = recording.pipeline.stop().map_err(|e| -> domain::error::AppError { e })?;

    // If we have audio, mux it with the video using FFmpeg.
    let final_path = if let Some(audio_path) = audio_path {
        let muxed_path = mux_audio_video(
            &state.ffmpeg.ffmpeg_path().map_err(|e| -> domain::error::AppError { e })?,
            &PathBuf::from(&video_path),
            &audio_path,
        )?;
        // Clean up temp files.
        let _ = std::fs::remove_file(&video_path);
        let _ = std::fs::remove_file(&audio_path);
        muxed_path
    } else {
        video_path
    };

    let _ = app_handle.emit("recording-state", RecordingState::Completed);

    info!("Recording completed: {final_path}");
    Ok(final_path)
}

/// Pause the current recording.
#[tauri::command]
pub fn pause_recording(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let guard = state.active_recording.lock().map_err(|_| {
        AppError::Capture("Recording state lock poisoned".to_string())
    })?;

    let recording = guard.as_ref().ok_or_else(|| {
        AppError::Capture("No recording in progress".to_string())
    })?;

    recording.pipeline.pause();
    let _ = app_handle.emit("recording-state", RecordingState::Paused);

    info!("Recording paused");
    Ok(())
}

/// Resume the current recording.
#[tauri::command]
pub fn resume_recording(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let guard = state.active_recording.lock().map_err(|_| {
        AppError::Capture("Recording state lock poisoned".to_string())
    })?;

    let recording = guard.as_ref().ok_or_else(|| {
        AppError::Capture("No recording in progress".to_string())
    })?;

    recording.pipeline.resume();
    let _ = app_handle.emit("recording-state", RecordingState::Recording);

    info!("Recording resumed");
    Ok(())
}

/// Get the current recording status.
#[tauri::command]
pub fn get_recording_status(state: State<'_, AppState>) -> CommandResult<RecordingStatus> {
    let guard = state.active_recording.lock().map_err(|_| {
        AppError::Capture("Recording state lock poisoned".to_string())
    })?;

    match guard.as_ref() {
        Some(recording) => Ok(recording.pipeline.status()),
        None => Ok(RecordingStatus {
            state: RecordingState::Idle,
            elapsed_seconds: 0.0,
            frames_captured: 0,
            output_path: None,
        }),
    }
}

/// Mux a video file and an audio WAV file into a final output using FFmpeg.
fn mux_audio_video(
    ffmpeg_path: &PathBuf,
    video_path: &Path,
    audio_path: &Path,
) -> AppResult<String> {
    let output_path = video_path.with_file_name(format!(
        "{}_final.{}",
        video_path.file_stem().unwrap_or_default().to_string_lossy(),
        video_path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
    ));

    info!(
        "Muxing video ({}) + audio ({}) -> {}",
        video_path.display(),
        audio_path.display(),
        output_path.display()
    );

    let output = std::process::Command::new(ffmpeg_path)
        .args([
            "-hide_banner",
            "-y",
            "-i",
            &video_path.to_string_lossy(),
            "-i",
            &audio_path.to_string_lossy(),
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-shortest",
            &output_path.to_string_lossy(),
        ])
        .output()
        .map_err(|e| AppError::FfmpegExecution(format!("Failed to run mux: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::FfmpegExecution(format!(
            "Audio/video mux failed: {}",
            stderr.lines().last().unwrap_or("unknown")
        )));
    }

    Ok(output_path.to_string_lossy().to_string())
}
```

- [ ] **Step 3: Register the new commands in main.rs**

Update the `invoke_handler` in `src-tauri/src/main.rs`:
```rust
.invoke_handler(tauri::generate_handler![
    commands::platform::get_platform_info,
    commands::settings::get_settings,
    commands::settings::save_settings,
    commands::settings::reset_settings,
    commands::ffmpeg::get_ffmpeg_status,
    commands::capture::list_capture_sources,
    commands::capture::take_screenshot,
    commands::capture::capture_to_clipboard,
    commands::capture::list_audio_devices_cmd,
    commands::capture::start_recording,
    commands::capture::stop_recording,
    commands::capture::pause_recording,
    commands::capture::resume_recording,
    commands::capture::get_recording_status,
])
```

Update the `AppState` construction in the `.setup()` closure:
```rust
app.manage(AppState {
    ffmpeg: ffmpeg_resolver,
    settings: settings_repo,
    platform,
    capture: capture_backend,
    active_recording: std::sync::Mutex::new(None),
});
```

- [ ] **Step 4: Verify compilation**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/src-tauri
cargo check
```

Expected: Compiles clean.

- [ ] **Step 5: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src-tauri/src/commands/capture.rs src-tauri/src/state.rs src-tauri/src/main.rs
git commit -m "feat: add recording lifecycle commands (start, stop, pause, resume)"
```

---

## Task 7: Recording State Events (Backend to Frontend via Tauri Events)

**Files:**
- Modify: `src-tauri/capabilities/default.json`
- Modify: `src/lib/types.ts`
- Modify: `src/lib/ipc.ts`

- [ ] **Step 1: Add event listener permissions**

In `src-tauri/capabilities/default.json`, add to the `permissions` array:
```json
"core:event:allow-listen",
"core:event:allow-unlisten",
"core:event:allow-emit"
```

- [ ] **Step 2: Add capture/recording types to the frontend**

Add to `src/lib/types.ts`:
```typescript
// --- Capture & Recording types (Plan 2) ---

export interface ScreenSource {
  monitor_id: number;
}

export interface WindowSource {
  window_id: number;
}

export interface RegionSource {
  monitor_id: number;
  x: number;
  y: number;
  width: number;
  height: number;
}

export type CaptureSource =
  | { type: "Screen"; data: ScreenSource }
  | { type: "Window"; data: WindowSource }
  | { type: "Region"; data: RegionSource };

export interface MonitorInfo {
  id: number;
  name: string;
  friendly_name: string;
  width: number;
  height: number;
  x: number;
  y: number;
  scale_factor: number;
  is_primary: boolean;
}

export interface WindowInfo {
  id: number;
  pid: number;
  app_name: string;
  title: string;
  width: number;
  height: number;
  is_minimized: boolean;
  is_focused: boolean;
}

export interface AvailableSources {
  monitors: MonitorInfo[];
  windows: WindowInfo[];
  windows_unavailable: boolean;
  windows_unavailable_reason: string | null;
}

export type RecordingState =
  | "idle"
  | "starting"
  | "recording"
  | "paused"
  | "stopping"
  | "completed"
  | { failed: string };

export interface RecordingConfig {
  source: CaptureSource;
  fps: number;
  video_codec: string;
  crf: number;
  preset: string;
  output_path: string;
  capture_microphone: boolean;
  microphone_device: string | null;
}

export interface RecordingStatus {
  state: RecordingState;
  elapsed_seconds: number;
  frames_captured: number;
  output_path: string | null;
}

export interface AudioDeviceInfo {
  name: string;
  is_default: boolean;
  sample_rate: number;
  channels: number;
}
```

- [ ] **Step 3: Add capture IPC wrappers**

Add to `src/lib/ipc.ts`:
```typescript
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppSettings,
  AudioDeviceInfo,
  AvailableSources,
  CaptureSource,
  FfmpegStatus,
  PlatformInfo,
  RecordingConfig,
  RecordingState,
  RecordingStatus,
} from "./types";

// ... (existing Platform, Settings, FFmpeg wrappers) ...

// Capture & Recording
export const listCaptureSources = () =>
  invoke<AvailableSources>("list_capture_sources");

export const takeScreenshot = (
  source: CaptureSource,
  outputPath: string,
  format: string,
  quality: number,
) =>
  invoke<string>("take_screenshot", {
    source,
    outputPath,
    format,
    quality,
  });

export const captureToClipboard = (source: CaptureSource) =>
  invoke<string>("capture_to_clipboard", { source });

export const listAudioDevices = () =>
  invoke<AudioDeviceInfo[]>("list_audio_devices_cmd");

export const startRecording = (config: RecordingConfig) =>
  invoke<void>("start_recording", { config });

export const stopRecording = () =>
  invoke<string>("stop_recording");

export const pauseRecording = () =>
  invoke<void>("pause_recording");

export const resumeRecording = () =>
  invoke<void>("resume_recording");

export const getRecordingStatus = () =>
  invoke<RecordingStatus>("get_recording_status");

// Event listeners
export const onRecordingState = (
  callback: (state: RecordingState) => void,
): Promise<UnlistenFn> =>
  listen<RecordingState>("recording-state", (event) => {
    callback(event.payload);
  });

export const onRecordingWarning = (
  callback: (message: string) => void,
): Promise<UnlistenFn> =>
  listen<string>("recording-warning", (event) => {
    callback(event.payload);
  });
```

- [ ] **Step 4: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src-tauri/capabilities/default.json src/lib/types.ts src/lib/ipc.ts
git commit -m "feat: add frontend types and IPC wrappers for capture and recording"
```

---

## Task 8: Transparent Overlay Window for Region Selection

**Files:**
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/src/commands/capture.rs`
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/capabilities/default.json`

- [ ] **Step 1: Add the overlay window configuration to tauri.conf.json**

In `src-tauri/tauri.conf.json`, add a second window to the `windows` array:
```json
{
  "windows": [
    {
      "title": "Screen Dream",
      "label": "main",
      "width": 900,
      "height": 670,
      "resizable": true,
      "fullscreen": false
    },
    {
      "title": "Region Selector",
      "label": "region-selector",
      "url": "#/region-selector",
      "width": 800,
      "height": 600,
      "resizable": false,
      "fullscreen": true,
      "decorations": false,
      "transparent": true,
      "alwaysOnTop": true,
      "visible": false,
      "skipTaskbar": true
    }
  ]
}
```

- [ ] **Step 2: Add overlay window permissions**

Add to `src-tauri/capabilities/default.json` -- add the region-selector window and permissions:
```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default capabilities for the main window",
  "windows": ["main", "region-selector"],
  "permissions": [
    "core:default",
    "core:window:allow-close",
    "core:window:allow-hide",
    "core:window:allow-show",
    "core:window:allow-set-fullscreen",
    "core:window:allow-set-focus",
    "core:event:allow-listen",
    "core:event:allow-unlisten",
    "core:event:allow-emit",
    "core:event:allow-emit-to",
    "global-shortcut:allow-is-registered",
    "global-shortcut:allow-register",
    "global-shortcut:allow-register-all",
    "global-shortcut:allow-unregister",
    "global-shortcut:allow-unregister-all",
    "shell:allow-execute",
    "shell:allow-spawn",
    "shell:allow-stdin-write",
    "shell:allow-kill",
    "dialog:allow-open",
    "dialog:allow-save",
    "dialog:allow-message",
    "dialog:allow-ask",
    "dialog:allow-confirm",
    "notification:default",
    "clipboard-manager:allow-read-text",
    "clipboard-manager:allow-write-text"
  ]
}
```

- [ ] **Step 3: Add commands to show/hide the overlay window**

Add to `src-tauri/src/commands/capture.rs`:
```rust
use tauri::Manager;

/// Show the transparent overlay window for region selection.
#[tauri::command]
pub fn show_region_selector(app_handle: tauri::AppHandle) -> CommandResult<()> {
    let window = app_handle
        .get_webview_window("region-selector")
        .ok_or_else(|| AppError::Capture("Region selector window not found".to_string()))?;

    window.show().map_err(|e| {
        AppError::Capture(format!("Failed to show region selector: {e}"))
    })?;

    window.set_focus().map_err(|e| {
        AppError::Capture(format!("Failed to focus region selector: {e}"))
    })?;

    info!("Region selector overlay shown");
    Ok(())
}

/// Hide the overlay window (called after selection is made or cancelled).
#[tauri::command]
pub fn hide_region_selector(app_handle: tauri::AppHandle) -> CommandResult<()> {
    let window = app_handle
        .get_webview_window("region-selector")
        .ok_or_else(|| AppError::Capture("Region selector window not found".to_string()))?;

    window.hide().map_err(|e| {
        AppError::Capture(format!("Failed to hide region selector: {e}"))
    })?;

    info!("Region selector overlay hidden");
    Ok(())
}
```

- [ ] **Step 4: Register the new commands in main.rs**

Add to the `invoke_handler`:
```rust
commands::capture::show_region_selector,
commands::capture::hide_region_selector,
```

- [ ] **Step 5: Verify compilation**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/src-tauri
cargo check
```

Expected: Compiles clean.

- [ ] **Step 6: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src-tauri/tauri.conf.json src-tauri/capabilities/default.json src-tauri/src/commands/capture.rs src-tauri/src/main.rs
git commit -m "feat: add transparent overlay window for region selection"
```

---

## Task 9: Frontend -- SourcePicker Component

**Files:**
- Create: `src/components/SourcePicker.tsx`
- Create: `src/components/SourcePicker.module.scss` (SCSS module with semantic class names; styles TBD)
- Create: `src/lib/constants.ts` (exports `APP_NAME = "Screen Dream"` and other app-wide constants)

- [ ] **Step 1: Create the SourcePicker component**

Create `src/components/SourcePicker.tsx`:
```tsx
import { useState, useEffect, useCallback } from "preact/hooks";
import { listCaptureSources } from "../lib/ipc";
import type {
  AvailableSources,
  CaptureSource,
  MonitorInfo,
  WindowInfo,
} from "../lib/types";
import { formatError } from "../lib/errors";
import styles from "./SourcePicker.module.scss";

interface SourcePickerProps {
  onSourceSelected: (source: CaptureSource) => void;
  /** Currently selected source, if any. */
  selectedSource: CaptureSource | null;
}

type SourceTab = "screens" | "windows";

export default function SourcePicker({
  onSourceSelected,
  selectedSource,
}: SourcePickerProps) {
  const [sources, setSources] = useState<AvailableSources | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [activeTab, setActiveTab] = useState<SourceTab>("screens");

  const loadSources = useCallback(async () => {
    try {
      setError(null);
      setLoading(true);
      const s = await listCaptureSources();
      setSources(s);
    } catch (err) {
      setError(formatError(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadSources();
  }, [loadSources]);

  const isScreenSelected = (monitor: MonitorInfo): boolean => {
    if (!selectedSource) return false;
    return (
      selectedSource.type === "Screen" &&
      selectedSource.data.monitor_id === monitor.id
    );
  };

  const isWindowSelected = (window: WindowInfo): boolean => {
    if (!selectedSource) return false;
    return (
      selectedSource.type === "Window" &&
      selectedSource.data.window_id === window.id
    );
  };

  const selectScreen = (monitor: MonitorInfo) => {
    onSourceSelected({
      type: "Screen",
      data: { monitor_id: monitor.id },
    });
  };

  const selectWindow = (window: WindowInfo) => {
    onSourceSelected({
      type: "Window",
      data: { window_id: window.id },
    });
  };

  if (loading) {
    return <div class={styles.sourcePicker}><p>Loading sources...</p></div>;
  }

  if (error) {
    return (
      <div class={styles.sourcePicker}>
        <p class={styles.error}>Failed to load sources: {error}</p>
        <button onClick={loadSources}>Retry</button>
      </div>
    );
  }

  if (!sources) return null;

  return (
    <div class={styles.sourcePicker}>
      <div class={styles.sourceTabs}>
        <button
          class={`${styles.tab} ${activeTab === "screens" ? styles.active : ""}`}
          onClick={() => setActiveTab("screens")}
        >
          Screens ({sources.monitors.length})
        </button>
        <button
          class={`${styles.tab} ${activeTab === "windows" ? styles.active : ""}`}
          onClick={() => setActiveTab("windows")}
          disabled={sources.windows_unavailable}
          title={sources.windows_unavailable_reason || undefined}
        >
          Windows ({sources.windows_unavailable ? "N/A" : sources.windows.length})
        </button>
        <button class={styles.tabRefresh} onClick={loadSources} title="Refresh sources">
          Refresh
        </button>
      </div>

      {activeTab === "screens" && (
        <div class={styles.sourceList}>
          {sources.monitors.map((monitor) => (
            <div
              key={monitor.id}
              class={`${styles.sourceItem} ${isScreenSelected(monitor) ? styles.selected : ""}`}
              onClick={() => selectScreen(monitor)}
            >
              <div class={styles.sourceName}>
                {monitor.friendly_name || monitor.name}
                {monitor.is_primary && <span class={styles.badge}>Primary</span>}
              </div>
              <div class={styles.sourceDetails}>
                {monitor.width}x{monitor.height}
                {monitor.scale_factor !== 1.0 && ` (${monitor.scale_factor}x scale)`}
              </div>
            </div>
          ))}
        </div>
      )}

      {activeTab === "windows" && (
        <div class={styles.sourceList}>
          {sources.windows_unavailable && (
            <p class={styles.notice}>{sources.windows_unavailable_reason}</p>
          )}
          {sources.windows.map((window) => (
            <div
              key={window.id}
              class={`${styles.sourceItem} ${isWindowSelected(window) ? styles.selected : ""}`}
              onClick={() => selectWindow(window)}
            >
              <div class={styles.sourceName}>
                {window.title || window.app_name || `Window ${window.id}`}
              </div>
              <div class={styles.sourceDetails}>
                {window.app_name} -- {window.width}x{window.height}
                {window.is_focused && <span class={styles.badge}>Focused</span>}
              </div>
            </div>
          ))}
          {!sources.windows_unavailable && sources.windows.length === 0 && (
            <p class={styles.notice}>No visible windows found.</p>
          )}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src/components/SourcePicker.tsx
git commit -m "feat: add SourcePicker component for screen/window selection"
```

---

## Task 10: Frontend -- RecordingControls Component

**Files:**
- Create: `src/components/RecordingControls.tsx`
- Create: `src/components/RecordingControls.module.scss` (SCSS module with semantic class names; styles TBD)
- Create: `src/stores/recording.ts`

- [ ] **Step 1: Create the recording state store**

Create `src/stores/recording.ts`:
```typescript
import { useState, useEffect, useCallback, useRef } from "preact/hooks";
import {
  startRecording,
  stopRecording,
  pauseRecording,
  resumeRecording,
  getRecordingStatus,
  onRecordingState,
  onRecordingWarning,
} from "../lib/ipc";
import type {
  CaptureSource,
  RecordingConfig,
  RecordingState,
  RecordingStatus,
} from "../lib/types";
import { formatError } from "../lib/errors";

export interface RecordingStore {
  state: RecordingState;
  elapsed: number;
  framesCapt: number;
  error: string | null;
  warning: string | null;
  outputPath: string | null;
  start: (source: CaptureSource, outputDir: string) => Promise<void>;
  stop: () => Promise<void>;
  pause: () => Promise<void>;
  resume: () => Promise<void>;
}

export function useRecording(): RecordingStore {
  const [state, setState] = useState<RecordingState>("idle");
  const [elapsed, setElapsed] = useState(0);
  const [framesCapt, setFramesCapt] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [warning, setWarning] = useState<string | null>(null);
  const [outputPath, setOutputPath] = useState<string | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Listen for recording state events from the backend.
  useEffect(() => {
    let unlisten1: (() => void) | null = null;
    let unlisten2: (() => void) | null = null;

    onRecordingState((s) => {
      setState(s);
    }).then((fn) => {
      unlisten1 = fn;
    });

    onRecordingWarning((msg) => {
      setWarning(msg);
    }).then((fn) => {
      unlisten2 = fn;
    });

    return () => {
      unlisten1?.();
      unlisten2?.();
    };
  }, []);

  // Poll recording status while recording is active.
  useEffect(() => {
    if (state === "recording" || state === "paused") {
      pollRef.current = setInterval(async () => {
        try {
          const status = await getRecordingStatus();
          setElapsed(status.elapsed_seconds);
          setFramesCapt(status.frames_captured);
        } catch {
          // Ignore polling errors.
        }
      }, 500);
    } else {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
    }

    return () => {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
    };
  }, [state]);

  const start = useCallback(
    async (source: CaptureSource, outputDir: string) => {
      setError(null);
      setWarning(null);
      setOutputPath(null);

      const timestamp = new Date()
        .toISOString()
        .replace(/[:.]/g, "-")
        .replace("T", "_")
        .slice(0, 19);
      const outputPath = `${outputDir}/recording_${timestamp}.mp4`;

      const config: RecordingConfig = {
        source,
        fps: 30,
        video_codec: "libx264",
        crf: 23,
        preset: "ultrafast",
        output_path: outputPath,
        capture_microphone: false,
        microphone_device: null,
      };

      try {
        await startRecording(config);
      } catch (err) {
        setError(formatError(err));
        setState("idle");
      }
    },
    [],
  );

  const stop = useCallback(async () => {
    try {
      const path = await stopRecording();
      setOutputPath(path);
      setElapsed(0);
      setFramesCapt(0);
    } catch (err) {
      setError(formatError(err));
    }
  }, []);

  const pause = useCallback(async () => {
    try {
      await pauseRecording();
    } catch (err) {
      setError(formatError(err));
    }
  }, []);

  const resume = useCallback(async () => {
    try {
      await resumeRecording();
    } catch (err) {
      setError(formatError(err));
    }
  }, []);

  return {
    state,
    elapsed,
    framesCapt,
    error,
    warning,
    outputPath,
    start,
    stop,
    pause,
    resume,
  };
}
```

- [ ] **Step 2: Create the RecordingControls component**

Create `src/components/RecordingControls.tsx`:
```tsx
import { useRecording } from "../stores/recording";
import type { CaptureSource } from "../lib/types";
import styles from "./RecordingControls.module.scss";

interface RecordingControlsProps {
  selectedSource: CaptureSource | null;
  outputDirectory: string;
}

function formatTime(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.floor(seconds % 60);
  if (h > 0) {
    return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  }
  return `${m}:${String(s).padStart(2, "0")}`;
}

export default function RecordingControls({
  selectedSource,
  outputDirectory,
}: RecordingControlsProps) {
  const {
    state,
    elapsed,
    framesCapt,
    error,
    warning,
    outputPath,
    start,
    stop,
    pause,
    resume,
  } = useRecording();

  const isIdle = state === "idle" || state === "completed";
  const isRecording = state === "recording";
  const isPaused = state === "paused";
  const isBusy = state === "starting" || state === "stopping";

  const handleStart = async () => {
    if (!selectedSource) return;
    await start(selectedSource, outputDirectory);
  };

  return (
    <div class={styles.recordingControls}>
      {/* Timer display */}
      {(isRecording || isPaused) && (
        <div class={styles.recordingTimer}>
          <span class={`${styles.recordingDot} ${isPaused ? styles.paused : styles.active}`} />
          <span class={styles.time}>{formatTime(elapsed)}</span>
          <span class={styles.frames}>{framesCapt} frames</span>
        </div>
      )}

      {/* Control buttons */}
      <div class={styles.recordingButtons}>
        {isIdle && (
          <button
            class={`${styles.btn} ${styles.btnRecord}`}
            onClick={handleStart}
            disabled={!selectedSource || isBusy}
            title={!selectedSource ? "Select a source first" : "Start recording"}
          >
            Record
          </button>
        )}

        {isRecording && (
          <>
            <button class={`${styles.btn} ${styles.btnPause}`} onClick={pause}>
              Pause
            </button>
            <button class={`${styles.btn} ${styles.btnStop}`} onClick={stop}>
              Stop
            </button>
          </>
        )}

        {isPaused && (
          <>
            <button class={`${styles.btn} ${styles.btnResume}`} onClick={resume}>
              Resume
            </button>
            <button class={`${styles.btn} ${styles.btnStop}`} onClick={stop}>
              Stop
            </button>
          </>
        )}

        {isBusy && (
          <button class={styles.btn} disabled>
            {state === "starting" ? "Starting..." : "Stopping..."}
          </button>
        )}
      </div>

      {/* Status messages */}
      {error && <p class={styles.error}>{error}</p>}
      {warning && <p class={styles.warning}>{warning}</p>}
      {outputPath && state === "completed" && (
        <p class={styles.success}>Recording saved: {outputPath}</p>
      )}
    </div>
  );
}
```

- [ ] **Step 3: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src/stores/recording.ts src/components/RecordingControls.tsx
git commit -m "feat: add RecordingControls component and recording state store"
```

---

## Task 11: Frontend -- RegionSelector Overlay

**Files:**
- Create: `src/components/RegionSelector.tsx`
- Create: `src/components/RegionSelector.module.scss` (SCSS module; handles overlay positioning, selection rect, dimension label)
- Modify: `src/App.tsx`

- [ ] **Step 1: Create the RegionSelector overlay component**

This component renders in the transparent overlay window. It draws a semi-transparent backdrop and lets the user drag to select a rectangle. When the selection is confirmed, it emits the region coordinates back to the main window via Tauri events.

Create `src/components/RegionSelector.tsx`:
```tsx
import { useState, useRef, useCallback, useEffect } from "preact/hooks";
import { emit } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import styles from "./RegionSelector.module.scss";

interface Point {
  x: number;
  y: number;
}

interface Region {
  x: number;
  y: number;
  width: number;
  height: number;
}

export default function RegionSelector() {
  const [isDragging, setIsDragging] = useState(false);
  const [startPoint, setStartPoint] = useState<Point | null>(null);
  const [currentPoint, setCurrentPoint] = useState<Point | null>(null);
  const overlayRef = useRef<HTMLDivElement>(null);

  const getRegion = (): Region | null => {
    if (!startPoint || !currentPoint) return null;
    const x = Math.min(startPoint.x, currentPoint.x);
    const y = Math.min(startPoint.y, currentPoint.y);
    const width = Math.abs(currentPoint.x - startPoint.x);
    const height = Math.abs(currentPoint.y - startPoint.y);
    if (width < 10 || height < 10) return null;
    return { x, y, width, height };
  };

  const handleMouseDown = useCallback((e: MouseEvent) => {
    e.preventDefault();
    setStartPoint({ x: e.clientX, y: e.clientY });
    setCurrentPoint({ x: e.clientX, y: e.clientY });
    setIsDragging(true);
  }, []);

  const handleMouseMove = useCallback(
    (e: MouseEvent) => {
      if (!isDragging) return;
      setCurrentPoint({ x: e.clientX, y: e.clientY });
    },
    [isDragging],
  );

  const handleMouseUp = useCallback(async () => {
    if (!isDragging) return;
    setIsDragging(false);

    const region = getRegion();
    if (region) {
      // Emit the selected region to the main window.
      await emit("region-selected", region);
    }

    // Hide the overlay window.
    try {
      await invoke("hide_region_selector");
    } catch {
      // Fallback: try closing via window API.
    }

    // Reset state.
    setStartPoint(null);
    setCurrentPoint(null);
  }, [isDragging, startPoint, currentPoint]);

  const handleKeyDown = useCallback(async (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      // Cancel selection.
      await emit("region-cancelled", null);
      try {
        await invoke("hide_region_selector");
      } catch {
        // ignore
      }
      setIsDragging(false);
      setStartPoint(null);
      setCurrentPoint(null);
    }
  }, []);

  useEffect(() => {
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  const region = getRegion();

  return (
    <div
      ref={overlayRef}
      class={styles.regionOverlay}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
    >
      {/* Instructions */}
      {!isDragging && !region && (
        <div class={styles.instructions}>
          Click and drag to select a region. Press Escape to cancel.
        </div>
      )}

      {/* Selection rectangle */}
      {region && (
        <div
          class={styles.selectionRect}
          style={{
            left: `${region.x}px`,
            top: `${region.y}px`,
            width: `${region.width}px`,
            height: `${region.height}px`,
          }}
        >
          {/* Dimension label */}
          <div class={styles.dimensionLabel}>
            {region.width} x {region.height}
          </div>
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Add region selector route to App.tsx**

Update `src/App.tsx` to handle the region-selector route:
```tsx
import { useState, useEffect } from "preact/hooks";
import Layout from "./components/Layout";
import Home from "./pages/Home";
import SettingsPage from "./pages/Settings";
import RegionSelector from "./components/RegionSelector";

function useRoute() {
  const [route, setRoute] = useState(window.location.hash || "#/");

  useEffect(() => {
    const handler = () => setRoute(window.location.hash || "#/");
    window.addEventListener("hashchange", handler);
    return () => window.removeEventListener("hashchange", handler);
  }, []);

  return route;
}

export default function App() {
  const route = useRoute();

  // The region selector runs in its own transparent window -- no Layout wrapper.
  if (route === "#/region-selector") {
    return <RegionSelector />;
  }

  return (
    <Layout>
      {route === "#/settings" ? <SettingsPage /> : <Home />}
    </Layout>
  );
}
```

- [ ] **Step 3: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src/components/RegionSelector.tsx src/App.tsx
git commit -m "feat: add RegionSelector overlay component for region capture"
```

---

## Task 12: Integrate Recording Controls into Home Page

**Files:**
- Modify: `src/pages/Home.tsx`
- Create: `src/pages/Home.module.scss` (SCSS module with semantic class names; styles TBD)
- Modify: `src/lib/ipc.ts`

- [ ] **Step 1: Update Home.tsx to integrate SourcePicker, RecordingControls, and region selection**

Replace `src/pages/Home.tsx`:
```tsx
import { useState, useEffect, useCallback } from "preact/hooks";
import { listen } from "@tauri-apps/api/event";
import {
  getPlatformInfo,
  takeScreenshot,
  showRegionSelector,
} from "../lib/ipc";
import { useFfmpegStatus, useSettings } from "../stores/settings";
import type { CaptureSource, PlatformInfo, RegionSource } from "../lib/types";
import SourcePicker from "../components/SourcePicker";
import RecordingControls from "../components/RecordingControls";
import { APP_NAME } from "../lib/constants";
import styles from "./Home.module.scss";

export default function Home() {
  const [platform, setPlatform] = useState<PlatformInfo | null>(null);
  const { status } = useFfmpegStatus();
  const { settings } = useSettings();
  const [selectedSource, setSelectedSource] = useState<CaptureSource | null>(
    null,
  );
  const [screenshotMessage, setScreenshotMessage] = useState<string | null>(
    null,
  );

  useEffect(() => {
    getPlatformInfo().then(setPlatform);
  }, []);

  // Listen for region selection events from the overlay window.
  useEffect(() => {
    let unlisten: (() => void) | null = null;

    listen<RegionSource>("region-selected", (event) => {
      const region = event.payload;
      // Use the first monitor as default for region captures.
      // In a full implementation, detect which monitor the region is on.
      setSelectedSource({
        type: "Region",
        data: {
          monitor_id: region.monitor_id || 0,
          x: region.x,
          y: region.y,
          width: region.width,
          height: region.height,
        },
      });
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      unlisten?.();
    };
  }, []);

  const handleScreenshot = useCallback(async () => {
    if (!selectedSource) return;
    try {
      setScreenshotMessage(null);
      const outputDir =
        settings?.export?.output_directory || "/tmp";
      const timestamp = new Date()
        .toISOString()
        .replace(/[:.]/g, "-")
        .slice(0, 19);
      const path = `${outputDir}/screenshot_${timestamp}.png`;
      const savedPath = await takeScreenshot(selectedSource, path, "png", 100);
      setScreenshotMessage(`Screenshot saved: ${savedPath}`);
    } catch (err) {
      setScreenshotMessage(`Screenshot failed: ${err}`);
    }
  }, [selectedSource, settings]);

  const handleSelectRegion = useCallback(async () => {
    try {
      await showRegionSelector();
    } catch (err) {
      console.error("Failed to open region selector:", err);
    }
  }, []);

  const outputDirectory =
    settings?.export?.output_directory || "/tmp";

  return (
    <div>
      <h1>{APP_NAME}</h1>

      {platform && (
        <section>
          <h3>Platform</h3>
          <p>
            {platform.os} / {platform.display_server} / {platform.arch}
          </p>
        </section>
      )}

      {status && (
        <section>
          <h3>FFmpeg</h3>
          <p>
            {status.available
              ? `v${status.capabilities?.version} (${status.source})`
              : `Not available: ${status.error}`}
          </p>
        </section>
      )}

      <section>
        <h3>Capture Source</h3>
        <SourcePicker
          onSourceSelected={setSelectedSource}
          selectedSource={selectedSource}
        />
        <button
          class={`${styles.btn} ${styles.btnRegion}`}
          onClick={handleSelectRegion}
        >
          Select Region
        </button>
        {selectedSource && (
          <p class={styles.selectedSource}>
            Selected: {selectedSource.type}
            {selectedSource.type === "Region" &&
              ` (${selectedSource.data.width}x${selectedSource.data.height})`}
          </p>
        )}
      </section>

      <section>
        <h3>Recording</h3>
        <RecordingControls
          selectedSource={selectedSource}
          outputDirectory={outputDirectory}
        />
      </section>

      <section>
        <h3>Screenshot</h3>
        <button
          class={styles.btn}
          onClick={handleScreenshot}
          disabled={!selectedSource}
        >
          Take Screenshot
        </button>
        {screenshotMessage && <p>{screenshotMessage}</p>}
      </section>
    </div>
  );
}
```

- [ ] **Step 2: Add the showRegionSelector IPC wrapper**

Add to `src/lib/ipc.ts`:
```typescript
export const showRegionSelector = () =>
  invoke<void>("show_region_selector");

export const hideRegionSelector = () =>
  invoke<void>("hide_region_selector");
```

- [ ] **Step 3: Verify the frontend builds**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
pnpm build
```

Expected: Builds with no TypeScript errors.

- [ ] **Step 4: Run the full app end-to-end**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
pnpm tauri dev
```

Expected:
- The Home page shows a SourcePicker with screens and windows listed
- Selecting a screen and clicking "Record" starts a recording
- The timer counts up while recording
- Clicking "Stop" saves the recording to the output directory
- Clicking "Take Screenshot" saves a PNG screenshot
- Clicking "Select Region" opens the transparent overlay (on supported window managers)
- No console errors

- [ ] **Step 5: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src/pages/Home.tsx src/lib/ipc.ts
git commit -m "feat: integrate SourcePicker and RecordingControls into Home page"
```

---

## Task 13: Integration Test on Linux

**Files:**
- Create: `src-tauri/crates/infrastructure/tests/capture_integration_test.rs`

- [ ] **Step 1: Write the integration test**

This test exercises the full capture pipeline: enumerate sources, take a screenshot, record a short clip, verify outputs. Requires a display server and FFmpeg.

Create `src-tauri/crates/infrastructure/tests/capture_integration_test.rs`:
```rust
//! Integration test for the capture pipeline on Linux.
//! Requires:
//! - A running display server (X11 or Wayland with XWayland)
//! - FFmpeg installed on the system
//!
//! Run with: cargo test -p infrastructure --test capture_integration_test -- --nocapture --ignored

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use domain::capture::{CaptureBackend, CaptureSource, ScreenSource};
use domain::platform::PlatformInfo;
use infrastructure::capture::{RecordingPipeline, XcapCaptureBackend};

#[test]
#[ignore]
fn enumerate_sources_and_capture_screenshot() {
    let platform = PlatformInfo::detect();
    let backend = XcapCaptureBackend::new(platform);

    // 1. Enumerate sources.
    let sources = backend
        .enumerate_sources()
        .expect("Should enumerate sources");

    assert!(
        !sources.monitors.is_empty(),
        "Should find at least one monitor"
    );
    println!("Monitors:");
    for m in &sources.monitors {
        println!(
            "  [{}] {} ({}x{}) primary={}",
            m.id, m.friendly_name, m.width, m.height, m.is_primary
        );
    }

    if !sources.windows_unavailable {
        println!("Windows:");
        for w in &sources.windows {
            println!(
                "  [{}] {} - {} ({}x{})",
                w.id, w.app_name, w.title, w.width, w.height
            );
        }
    } else {
        println!(
            "Windows unavailable: {}",
            sources.windows_unavailable_reason.as_deref().unwrap_or("unknown")
        );
    }

    // 2. Capture a screenshot of the primary monitor.
    let primary = sources
        .monitors
        .iter()
        .find(|m| m.is_primary)
        .unwrap_or(&sources.monitors[0]);

    let frame = backend
        .capture_frame(&CaptureSource::Screen(ScreenSource {
            monitor_id: primary.id,
        }))
        .expect("Should capture frame");

    assert_eq!(frame.width, primary.width);
    assert_eq!(frame.height, primary.height);
    assert_eq!(
        frame.data.len() as u32,
        frame.width * frame.height * 4,
        "Frame should be RGBA (4 bytes per pixel)"
    );

    // Save to a file to verify visually.
    let img = image::RgbaImage::from_raw(frame.width, frame.height, frame.data)
        .expect("Valid image data");
    let screenshot_path = "/tmp/integration-test-screenshot.png";
    img.save(screenshot_path).expect("Save screenshot");
    println!("Screenshot saved to {screenshot_path}");

    let metadata = std::fs::metadata(screenshot_path).expect("File exists");
    assert!(metadata.len() > 0, "Screenshot should not be empty");
}

#[test]
#[ignore]
fn record_three_second_clip() {
    let platform = PlatformInfo::detect();
    let backend = Arc::new(XcapCaptureBackend::new(platform));

    let sources = backend
        .enumerate_sources()
        .expect("Should enumerate sources");

    let primary = sources
        .monitors
        .iter()
        .find(|m| m.is_primary)
        .unwrap_or(&sources.monitors[0]);

    let ffmpeg_path = which::which("ffmpeg").expect("FFmpeg must be installed for this test");

    let output_path = "/tmp/integration-test-recording.mp4";

    let config = domain::capture::RecordingConfig {
        source: CaptureSource::Screen(ScreenSource {
            monitor_id: primary.id,
        }),
        fps: 10,
        video_codec: "libx264".to_string(),
        crf: 28,
        preset: "ultrafast".to_string(),
        output_path: output_path.to_string(),
        capture_microphone: false,
        microphone_device: None,
    };

    let mut pipeline =
        RecordingPipeline::start(config, ffmpeg_path, backend).expect("Start pipeline");

    assert!(pipeline.is_running());

    // Test pause/resume.
    std::thread::sleep(Duration::from_secs(1));
    pipeline.pause();
    assert!(pipeline.is_paused());
    std::thread::sleep(Duration::from_millis(500));
    pipeline.resume();
    assert!(!pipeline.is_paused());
    std::thread::sleep(Duration::from_secs(1));

    // Get status before stopping.
    let status = pipeline.status();
    println!("Status: {status:?}");
    assert!(status.frames_captured > 0, "Should have captured frames");

    // Stop.
    let result = pipeline.stop().expect("Stop pipeline");
    assert_eq!(result, output_path);

    let metadata = std::fs::metadata(output_path).expect("Output file exists");
    assert!(metadata.len() > 1000, "Recording should have substantial content");
    println!(
        "Recording saved: {output_path} ({} bytes, {} frames captured)",
        metadata.len(),
        status.frames_captured
    );
}

#[test]
#[ignore]
fn region_capture_crops_correctly() {
    let platform = PlatformInfo::detect();
    let backend = XcapCaptureBackend::new(platform);

    let sources = backend
        .enumerate_sources()
        .expect("Should enumerate sources");

    let primary = sources
        .monitors
        .iter()
        .find(|m| m.is_primary)
        .unwrap_or(&sources.monitors[0]);

    // Capture a 200x200 region from the top-left corner.
    let region = domain::capture::RegionSource {
        monitor_id: primary.id,
        x: 0,
        y: 0,
        width: 200,
        height: 200,
    };

    let frame = backend
        .capture_frame(&CaptureSource::Region(region))
        .expect("Should capture region");

    assert_eq!(frame.width, 200);
    assert_eq!(frame.height, 200);
    assert_eq!(frame.data.len(), (200 * 200 * 4) as usize);

    let img = image::RgbaImage::from_raw(frame.width, frame.height, frame.data)
        .expect("Valid image data");
    let path = "/tmp/integration-test-region.png";
    img.save(path).expect("Save region screenshot");
    println!("Region capture saved to {path}");
}
```

- [ ] **Step 2: Add test dependencies**

Ensure `src-tauri/crates/infrastructure/Cargo.toml` has:
```toml
[dev-dependencies]
tempfile = "3"
which = "7"
image = "0.25"
```

(`image` is already in `[dependencies]`, so the dev-dep may not be needed -- but `which` is only used in tests.)

- [ ] **Step 3: Run the integration tests**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/src-tauri
cargo test -p infrastructure --test capture_integration_test -- --nocapture --ignored
```

Expected:
- `enumerate_sources_and_capture_screenshot` -- lists monitors and windows, saves a screenshot to `/tmp/integration-test-screenshot.png`
- `record_three_second_clip` -- records a 3-second clip to `/tmp/integration-test-recording.mp4`, tests pause/resume
- `region_capture_crops_correctly` -- captures a 200x200 region, verifies dimensions

Verify the output files:
```bash
ls -la /tmp/integration-test-*
ffprobe /tmp/integration-test-recording.mp4 2>&1 | head -20
xdg-open /tmp/integration-test-screenshot.png
```

- [ ] **Step 4: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src-tauri/crates/infrastructure/tests/capture_integration_test.rs src-tauri/crates/infrastructure/Cargo.toml
git commit -m "test: add capture pipeline integration tests (screenshot, recording, region)"
```

---

## Summary of Deliverables

After completing all 13 tasks, the application will have:

1. **Domain layer** (`crates/domain/src/capture/`):
   - `CaptureSource` enum (Screen, Window, Region) with serializable source info types
   - `CaptureBackend` trait with `enumerate_sources()` and `capture_frame()` methods
   - `RecordingState` enum, `RecordingConfig`, and `RecordingStatus` types

2. **Infrastructure layer** (`crates/infrastructure/src/capture/`):
   - `XcapCaptureBackend` -- implements `CaptureBackend` using xcap for cross-platform capture
   - `RecordingPipeline` -- continuous frame capture loop piping raw RGBA to FFmpeg stdin
   - `AudioCapture` -- microphone recording to WAV via cpal

3. **Tauri commands** (`src/commands/capture.rs`):
   - `list_capture_sources` -- enumerate monitors and windows
   - `take_screenshot` -- capture and save to file (PNG/JPEG/WebP)
   - `capture_to_clipboard` -- capture and return base64 PNG
   - `list_audio_devices_cmd` -- list microphones
   - `start_recording`, `stop_recording`, `pause_recording`, `resume_recording` -- lifecycle
   - `get_recording_status` -- poll status
   - `show_region_selector`, `hide_region_selector` -- overlay window control

4. **Frontend components** (`src/components/`):
   - `SourcePicker` -- screen/window selection with tabs
   - `RecordingControls` -- start/stop/pause/resume with timer display
   - `RegionSelector` -- transparent overlay with click-drag rectangle selection

5. **Integration tests** verifying the full pipeline on Linux

### Known Limitations

- **Wayland window enumeration:** Not available on pure Wayland. The app detects this and shows a message. XWayland windows are enumerable on hybrid sessions.
- **System audio capture:** Not implemented in this plan. cpal captures microphone input only. System audio (loopback) requires WASAPI on Windows or PulseAudio monitor sources on Linux -- deferred to a future enhancement.
- **Audio/video sync during pause:** When recording is paused and resumed, the audio WAV continues to record silence. The mux step uses `-shortest` to handle length mismatches, but pause/resume produces a continuous audio track with silent gaps rather than trimmed segments. For precise sync, a more sophisticated multi-segment approach would be needed.
- **Frame rate stability:** The recording loop uses `thread::sleep` for timing, which is sufficient for 30 FPS but may drift under load. A vsync-aligned capture approach would be more precise for 60 FPS.
- **Overlay window on Wayland:** The transparent always-on-top overlay depends on compositor support. Some Wayland compositors may not allow setting `alwaysOnTop` from a regular window. Testing on GNOME (Mutter) and KDE (KWin) is recommended.
- **Webcam PiP overlay (DEFERRED to v1.1):** Webcam picture-in-picture overlay composited onto the recording output is not included in this plan. It requires additional work for camera enumeration, a PiP positioning UI, and real-time frame compositing in the recording pipeline.
  // TODO(v1.1): Webcam PiP overlay -- add camera source enumeration, PiP position/size config, and frame compositing in RecordingPipeline.
