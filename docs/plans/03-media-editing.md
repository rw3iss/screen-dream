# Plan 3: Media Editing

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the video, image, and audio editing capabilities with an interactive Canvas-based preview in the webview, frame decoding via FFmpeg subprocess in Rust, a JSON-based non-destructive edit decision model, and undo/redo via the command pattern — so that Screen Dream users can trim, annotate, overlay, and mix media captured by Plan 2 before exporting in Plan 4.

**Architecture:** Three-layer editing pipeline: (1) Domain layer defines the `EditProject` model, `EditOperation` enum, and FFmpeg filter graph builder as pure Rust types with no I/O. (2) Infrastructure layer implements frame extraction via FFmpeg subprocess and project JSON persistence. (3) Frontend renders an interactive editor with Canvas API (Fabric.js for image annotations), a single-video-track + single-audio-track timeline (v1), video frame preview with scrubbing, audio waveform visualization (wavesurfer.js), and tool panels. All edits are non-destructive JSON operations translated to FFmpeg filter graphs for final render. Undo/redo uses a command pattern with a history stack.
<!-- // TODO(v2): Multi-track timeline — upgrade to arbitrary video/audio/overlay track count -->

**Tech Stack:** Canvas API, Fabric.js v6, wavesurfer.js v7, FFmpeg subprocess (frame extraction), Preact, TypeScript, Rust (2021 edition), serde/serde_json, Tauri v2 IPC

**Depends on:** Plan 1 (Core Platform) and Plan 2 (Screen Capture) — recordings must exist to edit.

**Related documents:**
- `PLAN.md` — high-level architecture and feature overview
- `docs/plans/01-core-platform-infrastructure.md` — Plan 1 (foundation this builds on)
- `docs/plans/02-screen-capture-recording.md` — Plan 2 (capture pipeline that produces source media)
- `docs/plans/04-export-sharing.md` — Plan 4 (consumes edit decisions from this plan)

---

## Key Architecture Decisions

1. **Interactive preview:** Canvas API in the Preact webview for image editing (**Fabric.js** for annotations, shapes, text — confirmed). For video preview, decode frames in Rust via FFmpeg subprocess and send to frontend as base64 PNG data URLs.

2. **Frame decoding:** Use FFmpeg subprocess (`ffmpeg -ss <time> -i input.mp4 -frames:v 1 -f image2pipe -vcodec png pipe:1`) to extract individual frames for preview/scrubbing. This avoids linking libav directly (complex build dependency across platforms). Can upgrade to `ffmpeg-next` later if subprocess latency becomes a bottleneck.

3. **Edit decision model:** JSON-based project format. All edits are non-destructive — stored as a list of `EditOperation` values that get translated to FFmpeg `filter_complex` strings for final render. Source media is never modified.

4. **No MLT dependency (confirmed):** Build a simpler timeline. MLT is powerful but adds a heavy C dependency and complex build chain. Screen Dream uses a JSON-based edit list rendered by FFmpeg — no MLT at any layer.

5. **Undo/redo:** Command pattern — each edit operation is a reversible command pushed onto a history stack. Undo pops the last operation; redo pushes it back.

---

## File Structure

```
screen-recorder/
├── src-tauri/
│   ├── crates/
│   │   ├── domain/
│   │   │   └── src/
│   │   │       ├── lib.rs                          # Add: pub mod editing;
│   │   │       └── editing/
│   │   │           ├── mod.rs                      # Re-exports
│   │   │           ├── project.rs                  # EditProject, Track, Clip, TimeRange
│   │   │           ├── operations.rs               # EditOperation enum
│   │   │           └── filter_graph.rs             # Translates operations to FFmpeg filter_complex
│   │   │
│   │   └── infrastructure/
│   │       └── src/
│   │           ├── lib.rs                          # Add: pub mod editing;
│   │           └── editing/
│   │               ├── mod.rs                      # Re-exports
│   │               ├── frame_extractor.rs          # FFmpeg subprocess frame extraction
│   │               └── project_repository.rs       # Save/load project JSON files
│   │
│   └── src/
│       └── commands/
│           ├── mod.rs                              # Add: pub mod editing;
│           └── editing.rs                          # IPC commands for editing operations
│
└── src/                                            # Frontend (Preact)
    ├── lib/
    │   ├── types.ts                                # Add editing types
    │   └── ipc.ts                                  # Add editing IPC wrappers
    ├── stores/
    │   └── editor.ts                               # Editor state, undo/redo, selection
    ├── pages/
    │   └── Editor.tsx                              # Main editor page layout
    └── components/
        └── editor/
            ├── VideoPreview.tsx                     # Canvas-based video frame preview
            ├── Timeline.tsx                         # Single video + audio track timeline (v1) // TODO(v2): Multi-track timeline
            ├── ImageEditor.tsx                      # Fabric.js image annotation editor
            ├── AudioPanel.tsx                       # Waveform display and audio effects
            ├── ToolPanel.tsx                        # Text, image, shape overlay tools
            └── PropertiesPanel.tsx                  # Properties for selected element
```

---

## Task 1: Domain — EditProject Model (Tracks, Clips, Time Ranges, Operations)

**Files:**
- Create: `src-tauri/crates/domain/src/editing/mod.rs`
- Create: `src-tauri/crates/domain/src/editing/project.rs`
- Modify: `src-tauri/crates/domain/src/lib.rs` (add `pub mod editing;`)
- Modify: `src-tauri/crates/domain/src/error.rs` (add editing error variant)

- [ ] **Step 1: Add the editing error variant**

Add to `src-tauri/crates/domain/src/error.rs`, inside the `AppError` enum:

```rust
    #[error("Editing error: {0}")]
    Editing(String),

    #[error("Project not found: {0}")]
    ProjectNotFound(String),
```

- [ ] **Step 2: Create the project model**

Create `src-tauri/crates/domain/src/editing/project.rs`:
```rust
use serde::{Deserialize, Serialize};

/// A unique identifier for projects, tracks, and clips.
pub type EntityId = String;

/// Generates a new unique entity ID using a simple UUID v4 approach.
pub fn new_entity_id() -> EntityId {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let random: u64 = (nanos as u64).wrapping_mul(6364136223846793005).wrapping_add(1);
    format!("{:016x}{:016x}", nanos as u64, random)
}

/// A time range within a media clip, in milliseconds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeRange {
    /// Start time in milliseconds from the beginning of the source.
    pub start_ms: u64,
    /// End time in milliseconds from the beginning of the source.
    pub end_ms: u64,
}

impl TimeRange {
    pub fn new(start_ms: u64, end_ms: u64) -> Self {
        assert!(end_ms >= start_ms, "end_ms must be >= start_ms");
        TimeRange { start_ms, end_ms }
    }

    /// Duration in milliseconds.
    pub fn duration_ms(&self) -> u64 {
        self.end_ms - self.start_ms
    }

    /// Convert start time to seconds as a float (for FFmpeg -ss).
    pub fn start_seconds(&self) -> f64 {
        self.start_ms as f64 / 1000.0
    }

    /// Convert end time to seconds as a float.
    pub fn end_seconds(&self) -> f64 {
        self.end_ms as f64 / 1000.0
    }

    /// Convert duration to seconds as a float (for FFmpeg -t).
    pub fn duration_seconds(&self) -> f64 {
        self.duration_ms() as f64 / 1000.0
    }
}

/// The kind of track in the timeline.
/// v1: Single video track + single audio track only.
/// // TODO(v2): Multi-track timeline — support multiple video, audio, and overlay tracks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TrackKind {
    /// Primary video track (screen recording, imported video).
    Video,
    /// Audio-only track (microphone, music, sound effects).
    Audio,
    /// Overlay track (webcam PiP, text, images, shapes).
    /// // TODO(v2): Multi-track timeline — overlay tracks not exposed in v1 UI.
    Overlay,
}

/// A clip placed on a track in the timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clip {
    /// Unique clip identifier.
    pub id: EntityId,
    /// Path to the source media file on disk.
    pub source_path: String,
    /// The portion of the source file this clip uses.
    pub source_range: TimeRange,
    /// Where this clip sits on the timeline (absolute position).
    pub timeline_start_ms: u64,
    /// Per-clip operations (e.g., crop applied to just this clip).
    pub operations: Vec<super::operations::EditOperation>,
}

impl Clip {
    pub fn new(source_path: String, source_range: TimeRange, timeline_start_ms: u64) -> Self {
        Clip {
            id: new_entity_id(),
            source_path,
            source_range,
            timeline_start_ms,
            operations: Vec::new(),
        }
    }

    /// The duration of this clip on the timeline.
    pub fn duration_ms(&self) -> u64 {
        self.source_range.duration_ms()
    }

    /// The end position of this clip on the timeline.
    pub fn timeline_end_ms(&self) -> u64 {
        self.timeline_start_ms + self.duration_ms()
    }
}

/// A track in the timeline containing ordered clips.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    /// Unique track identifier.
    pub id: EntityId,
    /// What kind of media this track holds.
    pub kind: TrackKind,
    /// Human-readable name (e.g., "Screen", "Webcam", "Audio 1").
    pub name: String,
    /// Ordered list of clips on this track.
    pub clips: Vec<Clip>,
    /// Whether the track is muted (audio) or hidden (video/overlay).
    pub muted: bool,
    /// Whether the track is locked (prevents edits).
    pub locked: bool,
}

impl Track {
    pub fn new(kind: TrackKind, name: String) -> Self {
        Track {
            id: new_entity_id(),
            kind,
            name,
            clips: Vec::new(),
            muted: false,
            locked: false,
        }
    }

    /// Get the total duration of this track (end of last clip).
    pub fn duration_ms(&self) -> u64 {
        self.clips
            .iter()
            .map(|c| c.timeline_end_ms())
            .max()
            .unwrap_or(0)
    }
}

/// The top-level editing project. Contains all tracks and global operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditProject {
    /// Unique project identifier.
    pub id: EntityId,
    /// Human-readable project name.
    pub name: String,
    /// Path to the primary source media file (the recording being edited).
    pub source_path: String,
    /// Width of the project canvas in pixels.
    pub width: u32,
    /// Height of the project canvas in pixels.
    pub height: u32,
    /// Frame rate for the project.
    pub fps: f64,
    /// Timeline tracks.
    pub tracks: Vec<Track>,
    /// Global operations applied to the entire project output.
    pub global_operations: Vec<super::operations::EditOperation>,
    /// ISO 8601 timestamp of when the project was created.
    pub created_at: String,
    /// ISO 8601 timestamp of the last modification.
    pub modified_at: String,
}

impl EditProject {
    /// Create a new project from a source media file.
    pub fn new(name: String, source_path: String, width: u32, height: u32, fps: f64) -> Self {
        let now = chrono_now();
        EditProject {
            id: new_entity_id(),
            name,
            source_path,
            width,
            height,
            fps,
            tracks: Vec::new(),
            global_operations: Vec::new(),
            created_at: now.clone(),
            modified_at: now,
        }
    }

    /// Get the total duration of the project (longest track).
    pub fn duration_ms(&self) -> u64 {
        self.tracks.iter().map(|t| t.duration_ms()).max().unwrap_or(0)
    }

    /// Add a track and return its ID.
    pub fn add_track(&mut self, track: Track) -> EntityId {
        let id = track.id.clone();
        self.tracks.push(track);
        self.touch();
        id
    }

    /// Find a track by ID.
    pub fn track(&self, track_id: &str) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == track_id)
    }

    /// Find a track by ID (mutable).
    pub fn track_mut(&mut self, track_id: &str) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|t| t.id == track_id)
    }

    /// Update the modified_at timestamp.
    pub fn touch(&mut self) {
        self.modified_at = chrono_now();
    }
}

/// Returns the current time as an ISO 8601 string without external dependencies.
fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Simple UTC timestamp: seconds since epoch formatted manually
    // For a proper ISO string we'd use chrono, but keeping dependencies minimal.
    format!("{}Z", secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_range_duration() {
        let range = TimeRange::new(1000, 5000);
        assert_eq!(range.duration_ms(), 4000);
        assert!((range.start_seconds() - 1.0).abs() < f64::EPSILON);
        assert!((range.end_seconds() - 5.0).abs() < f64::EPSILON);
        assert!((range.duration_seconds() - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn clip_timeline_position() {
        let clip = Clip::new(
            "/tmp/video.mp4".to_string(),
            TimeRange::new(0, 10000),
            5000,
        );
        assert_eq!(clip.duration_ms(), 10000);
        assert_eq!(clip.timeline_end_ms(), 15000);
    }

    #[test]
    fn track_duration_from_clips() {
        let mut track = Track::new(TrackKind::Video, "Screen".to_string());
        track.clips.push(Clip::new(
            "/tmp/a.mp4".to_string(),
            TimeRange::new(0, 5000),
            0,
        ));
        track.clips.push(Clip::new(
            "/tmp/b.mp4".to_string(),
            TimeRange::new(0, 3000),
            5000,
        ));
        assert_eq!(track.duration_ms(), 8000);
    }

    #[test]
    fn project_duration_from_tracks() {
        let mut project = EditProject::new(
            "Test".to_string(),
            "/tmp/source.mp4".to_string(),
            1920,
            1080,
            30.0,
        );
        let mut video_track = Track::new(TrackKind::Video, "Screen".to_string());
        video_track.clips.push(Clip::new(
            "/tmp/source.mp4".to_string(),
            TimeRange::new(0, 10000),
            0,
        ));
        let mut audio_track = Track::new(TrackKind::Audio, "Audio".to_string());
        audio_track.clips.push(Clip::new(
            "/tmp/source.mp4".to_string(),
            TimeRange::new(0, 15000),
            0,
        ));
        project.add_track(video_track);
        project.add_track(audio_track);
        assert_eq!(project.duration_ms(), 15000);
    }

    #[test]
    fn new_entity_ids_are_unique() {
        let a = new_entity_id();
        // Introduce a tiny delay so nanos differ
        std::thread::sleep(std::time::Duration::from_nanos(1));
        let b = new_entity_id();
        assert_ne!(a, b);
    }
}
```

- [ ] **Step 3: Create the editing module file**

Create `src-tauri/crates/domain/src/editing/mod.rs`:
```rust
pub mod filter_graph;
pub mod operations;
pub mod project;

pub use filter_graph::*;
pub use operations::*;
pub use project::*;
```

- [ ] **Step 4: Register the editing module in the domain crate**

Add to `src-tauri/crates/domain/src/lib.rs`:
```rust
pub mod editing;
```

- [ ] **Step 5: Verify it compiles and tests pass**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/src-tauri
cargo test -p domain -- editing --nocapture
```

Expected: 5 tests pass (time_range_duration, clip_timeline_position, track_duration_from_clips, project_duration_from_tracks, new_entity_ids_are_unique).

- [ ] **Step 6: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src-tauri/crates/domain/src/editing/ src-tauri/crates/domain/src/lib.rs src-tauri/crates/domain/src/error.rs
git commit -m "feat: add EditProject domain model with tracks, clips, and time ranges"
```

---

## Task 2: Domain — EditOperation Enum and FFmpeg Filter Graph Builder

**Files:**
- Create: `src-tauri/crates/domain/src/editing/operations.rs`
- Create: `src-tauri/crates/domain/src/editing/filter_graph.rs`

- [ ] **Step 1: Define the EditOperation enum**

Create `src-tauri/crates/domain/src/editing/operations.rs`:
```rust
use serde::{Deserialize, Serialize};

use super::project::TimeRange;

/// A text overlay configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TextOverlayConfig {
    /// The text content to display.
    pub text: String,
    /// X position in pixels from the left edge.
    pub x: i32,
    /// Y position in pixels from the top edge.
    pub y: i32,
    /// Font size in points.
    pub font_size: u32,
    /// Font family name (must be available on the system for FFmpeg drawtext).
    pub font_family: String,
    /// Hex color string (e.g., "#FFFFFF").
    pub color: String,
    /// Background color (e.g., "#00000080" for semi-transparent black). Empty = no background.
    pub background_color: String,
    /// When the text appears on the timeline (ms from project start).
    pub start_ms: u64,
    /// When the text disappears (ms from project start).
    pub end_ms: u64,
}

/// An image overlay configuration (watermark, logo, etc.).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImageOverlayConfig {
    /// Path to the overlay image file.
    pub image_path: String,
    /// X position in pixels from the left edge.
    pub x: i32,
    /// Y position in pixels from the top edge.
    pub y: i32,
    /// Width to scale the overlay image to (0 = original size).
    pub width: u32,
    /// Height to scale the overlay image to (0 = original size).
    pub height: u32,
    /// Opacity from 0.0 (transparent) to 1.0 (opaque).
    pub opacity: f64,
    /// When the overlay appears (ms from project start).
    pub start_ms: u64,
    /// When the overlay disappears (ms from project start).
    pub end_ms: u64,
}

/// Audio compressor settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioCompressorConfig {
    /// Threshold in dB (e.g., -20.0).
    pub threshold_db: f64,
    /// Compression ratio (e.g., 4.0 means 4:1).
    pub ratio: f64,
    /// Attack time in milliseconds.
    pub attack_ms: f64,
    /// Release time in milliseconds.
    pub release_ms: f64,
}

/// Audio equalizer band.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioEqBand {
    /// Center frequency in Hz.
    pub frequency: f64,
    /// Width type: bandwidth in Hz.
    pub width: f64,
    /// Gain in dB (positive = boost, negative = cut).
    pub gain: f64,
}

/// Audio equalizer settings (multiple bands).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioEqConfig {
    pub bands: Vec<AudioEqBand>,
}

/// A blur region configuration (for redacting sensitive areas).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlurRegionConfig {
    /// X position of the blur region.
    pub x: i32,
    /// Y position of the blur region.
    pub y: i32,
    /// Width of the blur region.
    pub width: u32,
    /// Height of the blur region.
    pub height: u32,
    /// Blur strength (box blur size, e.g., 20).
    pub strength: u32,
    /// When the blur starts (ms from project start).
    pub start_ms: u64,
    /// When the blur ends (ms from project start).
    pub end_ms: u64,
}

/// All possible edit operations. Each variant is a non-destructive instruction
/// that gets translated into FFmpeg filter arguments for final render.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "config")]
pub enum EditOperation {
    /// Trim the clip to a sub-range.
    Trim {
        range: TimeRange,
    },
    /// Crop the video to a rectangle.
    Crop {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },
    /// Scale the video to a new resolution.
    Scale {
        width: u32,
        height: u32,
    },
    /// Rotate the video by degrees (90, 180, 270).
    Rotate {
        degrees: u32,
    },
    /// Add a text overlay.
    AddTextOverlay(TextOverlayConfig),
    /// Add an image overlay (watermark, logo).
    AddImageOverlay(ImageOverlayConfig),
    /// Add a blur region (for redacting content).
    AddBlurRegion(BlurRegionConfig),
    /// Normalize audio loudness (EBU R128, FFmpeg loudnorm filter).
    AudioNormalize,
    /// Apply dynamic range compression to audio.
    /// // TODO(v2): Compress UI not exposed in v1; domain type kept for filter graph builder.
    AudioCompress(AudioCompressorConfig),
    /// Apply parametric EQ to audio.
    /// // TODO(v2): EQ UI not exposed in v1; domain type kept for filter graph builder.
    AudioEq(AudioEqConfig),
    /// Apply RNN-based noise reduction (FFmpeg arnndn filter).
    /// // TODO(v2): Noise reduction UI not exposed in v1; domain type kept for filter graph builder.
    AudioNoiseReduce,
    /// Adjust audio volume (multiplier: 1.0 = no change, 0.5 = half, 2.0 = double).
    AudioVolume {
        multiplier: f64,
    },
    /// Speed change (0.5 = half speed, 2.0 = double speed).
    Speed {
        factor: f64,
    },
}

impl EditOperation {
    /// Returns a human-readable label for the operation (for undo/redo UI).
    pub fn label(&self) -> &'static str {
        match self {
            EditOperation::Trim { .. } => "Trim",
            EditOperation::Crop { .. } => "Crop",
            EditOperation::Scale { .. } => "Scale",
            EditOperation::Rotate { .. } => "Rotate",
            EditOperation::AddTextOverlay(_) => "Add Text",
            EditOperation::AddImageOverlay(_) => "Add Image",
            EditOperation::AddBlurRegion(_) => "Add Blur",
            EditOperation::AudioNormalize => "Normalize Audio",
            EditOperation::AudioCompress(_) => "Compress Audio",
            EditOperation::AudioEq(_) => "Equalize Audio",
            EditOperation::AudioNoiseReduce => "Reduce Noise",
            EditOperation::AudioVolume { .. } => "Adjust Volume",
            EditOperation::Speed { .. } => "Change Speed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_serializes_to_tagged_json() {
        let op = EditOperation::Crop {
            x: 10,
            y: 20,
            width: 1280,
            height: 720,
        };
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("\"type\":\"Crop\""));
        assert!(json.contains("\"width\":1280"));
    }

    #[test]
    fn operation_deserializes_from_json() {
        let json = r#"{"type":"Trim","config":{"range":{"start_ms":1000,"end_ms":5000}}}"#;
        let op: EditOperation = serde_json::from_str(json).unwrap();
        match op {
            EditOperation::Trim { range } => {
                assert_eq!(range.start_ms, 1000);
                assert_eq!(range.end_ms, 5000);
            }
            _ => panic!("Expected Trim"),
        }
    }

    #[test]
    fn text_overlay_serializes() {
        let op = EditOperation::AddTextOverlay(TextOverlayConfig {
            text: "Hello World".to_string(),
            x: 100,
            y: 50,
            font_size: 24,
            font_family: "Sans".to_string(),
            color: "#FFFFFF".to_string(),
            background_color: "#00000080".to_string(),
            start_ms: 0,
            end_ms: 5000,
        });
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("\"type\":\"AddTextOverlay\""));
        assert!(json.contains("Hello World"));
    }

    #[test]
    fn operation_labels_are_correct() {
        assert_eq!(EditOperation::AudioNormalize.label(), "Normalize Audio");
        assert_eq!(
            EditOperation::Scale {
                width: 1920,
                height: 1080,
            }
            .label(),
            "Scale"
        );
    }
}
```

- [ ] **Step 2: Define the FFmpeg filter graph builder**

Create `src-tauri/crates/domain/src/editing/filter_graph.rs`:
```rust
use super::operations::{
    AudioCompressorConfig, AudioEqConfig, BlurRegionConfig, EditOperation, ImageOverlayConfig,
    TextOverlayConfig,
};
use super::project::{EditProject, Track, TrackKind};

/// Builds an FFmpeg filter_complex string from an EditProject.
///
/// The builder translates the project's non-destructive operations into
/// a single FFmpeg filter graph that can be passed to `-filter_complex`.
/// Video and audio filter chains are built separately and then combined.
pub struct FilterGraphBuilder<'a> {
    project: &'a EditProject,
}

impl<'a> FilterGraphBuilder<'a> {
    pub fn new(project: &'a EditProject) -> Self {
        FilterGraphBuilder { project }
    }

    /// Build the complete filter_complex string and return it along with
    /// the list of input files needed and the output map labels.
    pub fn build(&self) -> FilterGraphOutput {
        let mut inputs: Vec<String> = Vec::new();
        let mut filters: Vec<String> = Vec::new();
        let mut video_out_label = String::new();
        let mut audio_out_label = String::new();
        let mut input_index: usize = 0;

        // Collect video operations from the primary video track
        let video_track = self.project.tracks.iter().find(|t| t.kind == TrackKind::Video);
        let audio_track = self.project.tracks.iter().find(|t| t.kind == TrackKind::Audio);

        // Process video track
        if let Some(track) = video_track {
            if let Some(clip) = track.clips.first() {
                inputs.push(clip.source_path.clone());
                let base_label = format!("[{input_index}:v]");
                let mut current_label = base_label;
                let mut filter_count = 0;

                // Apply clip-level operations
                for op in &clip.operations {
                    let (filter_str, out_label) =
                        self.build_video_filter(op, &current_label, filter_count);
                    if !filter_str.is_empty() {
                        filters.push(filter_str);
                        current_label = out_label;
                        filter_count += 1;
                    }
                }

                // Apply global operations
                for op in &self.project.global_operations {
                    let (filter_str, out_label) =
                        self.build_video_filter(op, &current_label, filter_count);
                    if !filter_str.is_empty() {
                        filters.push(filter_str);
                        current_label = out_label;
                        filter_count += 1;
                    }
                }

                video_out_label = if filter_count == 0 {
                    // No filters applied — use the raw input
                    format!("0:v")
                } else {
                    current_label.trim_start_matches('[').trim_end_matches(']').to_string()
                };

                input_index += 1;
            }
        }

        // Process audio track
        if let Some(track) = audio_track {
            if let Some(clip) = track.clips.first() {
                // If audio is from same file as video, reuse the input index
                let audio_input_idx = if inputs.first() == Some(&clip.source_path) {
                    0
                } else {
                    inputs.push(clip.source_path.clone());
                    input_index += 1;
                    input_index - 1
                };

                let base_label = format!("[{audio_input_idx}:a]");
                let mut current_label = base_label;
                let mut filter_count = 0;

                for op in &clip.operations {
                    let (filter_str, out_label) =
                        self.build_audio_filter(op, &current_label, filter_count);
                    if !filter_str.is_empty() {
                        filters.push(filter_str);
                        current_label = out_label;
                        filter_count += 1;
                    }
                }

                for op in &self.project.global_operations {
                    let (filter_str, out_label) =
                        self.build_audio_filter(op, &current_label, filter_count);
                    if !filter_str.is_empty() {
                        filters.push(filter_str);
                        current_label = out_label;
                        filter_count += 1;
                    }
                }

                audio_out_label = if filter_count == 0 {
                    format!("{}:a", audio_input_idx)
                } else {
                    current_label.trim_start_matches('[').trim_end_matches(']').to_string()
                };
            }
        }

        // Process overlay tracks (images, text, blur regions from global operations)
        for op in &self.project.global_operations {
            if let EditOperation::AddImageOverlay(config) = op {
                inputs.push(config.image_path.clone());
                let overlay_idx = inputs.len() - 1;
                let in_label = if video_out_label.is_empty() {
                    "0:v".to_string()
                } else {
                    format!("[{}]", video_out_label)
                };
                let out = format!("[ovl{}]", overlay_idx);
                let enable = format!(
                    "enable='between(t,{},{})'",
                    config.start_ms as f64 / 1000.0,
                    config.end_ms as f64 / 1000.0
                );
                let scale_part = if config.width > 0 && config.height > 0 {
                    format!("[{overlay_idx}:v]scale={}:{}[ovs{overlay_idx}];", config.width, config.height)
                } else {
                    String::new()
                };
                let overlay_input = if config.width > 0 && config.height > 0 {
                    format!("[ovs{overlay_idx}]")
                } else {
                    format!("[{overlay_idx}:v]")
                };
                let filter = format!(
                    "{scale_part}{in_label}{overlay_input}overlay={}:{}:{enable}{out}",
                    config.x, config.y
                );
                filters.push(filter);
                video_out_label = format!("ovl{}", overlay_idx);
            }
        }

        FilterGraphOutput {
            inputs,
            filter_complex: if filters.is_empty() {
                None
            } else {
                Some(filters.join(";"))
            },
            video_map: if video_out_label.is_empty() {
                None
            } else {
                Some(video_out_label)
            },
            audio_map: if audio_out_label.is_empty() {
                None
            } else {
                Some(audio_out_label)
            },
        }
    }

    /// Build a single video filter step. Returns (filter_string, output_label).
    fn build_video_filter(
        &self,
        op: &EditOperation,
        input_label: &str,
        index: usize,
    ) -> (String, String) {
        let out = format!("[v{index}]");
        match op {
            EditOperation::Crop { x, y, width, height } => {
                let filter = format!("{input_label}crop={width}:{height}:{x}:{y}{out}");
                (filter, out)
            }
            EditOperation::Scale { width, height } => {
                let filter = format!("{input_label}scale={width}:{height}{out}");
                (filter, out)
            }
            EditOperation::Rotate { degrees } => {
                let transpose = match degrees {
                    90 => "transpose=1",
                    180 => "transpose=1,transpose=1",
                    270 => "transpose=2",
                    _ => return (String::new(), input_label.to_string()),
                };
                let filter = format!("{input_label}{transpose}{out}");
                (filter, out)
            }
            EditOperation::Speed { factor } => {
                let setpts = format!("setpts={}*PTS", 1.0 / factor);
                let filter = format!("{input_label}{setpts}{out}");
                (filter, out)
            }
            EditOperation::AddTextOverlay(config) => {
                let filter = format!(
                    "{input_label}drawtext=text='{}':x={}:y={}:fontsize={}:fontcolor={}:fontfamily='{}':enable='between(t,{},{})'{out}",
                    escape_ffmpeg_text(&config.text),
                    config.x,
                    config.y,
                    config.font_size,
                    &config.color,
                    &config.font_family,
                    config.start_ms as f64 / 1000.0,
                    config.end_ms as f64 / 1000.0,
                );
                (filter, out)
            }
            EditOperation::AddBlurRegion(config) => {
                // Use boxblur on a cropped region, then overlay it back
                let filter = format!(
                    "{input_label}split[bg{index}][fg{index}];\
                     [fg{index}]crop={}:{}:{}:{}[cr{index}];\
                     [cr{index}]boxblur={}[bl{index}];\
                     [bg{index}][bl{index}]overlay={}:{}:enable='between(t,{},{})'{out}",
                    config.width,
                    config.height,
                    config.x,
                    config.y,
                    config.strength,
                    config.x,
                    config.y,
                    config.start_ms as f64 / 1000.0,
                    config.end_ms as f64 / 1000.0,
                );
                (filter, out)
            }
            _ => (String::new(), input_label.to_string()),
        }
    }

    /// Build a single audio filter step. Returns (filter_string, output_label).
    fn build_audio_filter(
        &self,
        op: &EditOperation,
        input_label: &str,
        index: usize,
    ) -> (String, String) {
        let out = format!("[a{index}]");
        match op {
            EditOperation::AudioNormalize => {
                let filter = format!(
                    "{input_label}loudnorm=I=-14:TP=-1:LRA=11{out}"
                );
                (filter, out)
            }
            EditOperation::AudioCompress(config) => {
                let filter = format!(
                    "{input_label}acompressor=threshold={}dB:ratio={}:attack={}:release={}{out}",
                    config.threshold_db, config.ratio, config.attack_ms, config.release_ms
                );
                (filter, out)
            }
            EditOperation::AudioEq(config) => {
                let eq_parts: Vec<String> = config
                    .bands
                    .iter()
                    .map(|b| format!("equalizer=f={}:width_type=h:width={}:g={}", b.frequency, b.width, b.gain))
                    .collect();
                if eq_parts.is_empty() {
                    return (String::new(), input_label.to_string());
                }
                let filter = format!("{input_label}{}{out}", eq_parts.join(","));
                (filter, out)
            }
            EditOperation::AudioNoiseReduce => {
                let filter = format!(
                    "{input_label}arnndn=m=cb.rnnn{out}"
                );
                (filter, out)
            }
            EditOperation::AudioVolume { multiplier } => {
                let filter = format!("{input_label}volume={multiplier}{out}");
                (filter, out)
            }
            EditOperation::Speed { factor } => {
                let filter = format!("{input_label}atempo={factor}{out}");
                (filter, out)
            }
            _ => (String::new(), input_label.to_string()),
        }
    }
}

/// Escape special characters for FFmpeg drawtext filter.
fn escape_ffmpeg_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('\'', "'\\\\\\''")
        .replace(':', "\\:")
        .replace('%', "%%")
}

/// The output of the filter graph builder.
#[derive(Debug, Clone)]
pub struct FilterGraphOutput {
    /// Ordered list of input file paths (maps to -i arguments).
    pub inputs: Vec<String>,
    /// The filter_complex string, if any filters were generated.
    pub filter_complex: Option<String>,
    /// The video output map label (for -map).
    pub video_map: Option<String>,
    /// The audio output map label (for -map).
    pub audio_map: Option<String>,
}

impl FilterGraphOutput {
    /// Build FFmpeg command-line arguments from this output.
    pub fn to_ffmpeg_args(&self, output_path: &str) -> Vec<String> {
        let mut args = Vec::new();
        args.push("-hide_banner".to_string());
        args.push("-y".to_string());

        // Input files
        for input in &self.inputs {
            args.push("-i".to_string());
            args.push(input.clone());
        }

        // Filter complex
        if let Some(fc) = &self.filter_complex {
            args.push("-filter_complex".to_string());
            args.push(fc.clone());
        }

        // Output maps
        if let Some(vm) = &self.video_map {
            if self.filter_complex.is_some() {
                args.push("-map".to_string());
                args.push(format!("[{vm}]"));
            }
        }
        if let Some(am) = &self.audio_map {
            if self.filter_complex.is_some() {
                args.push("-map".to_string());
                args.push(format!("[{am}]"));
            }
        }

        args.push(output_path.to_string());
        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editing::operations::EditOperation;
    use crate::editing::project::*;

    fn make_test_project() -> EditProject {
        let mut project = EditProject::new(
            "Test".to_string(),
            "/tmp/recording.mp4".to_string(),
            1920,
            1080,
            30.0,
        );
        let mut video_track = Track::new(TrackKind::Video, "Screen".to_string());
        video_track.clips.push(Clip::new(
            "/tmp/recording.mp4".to_string(),
            TimeRange::new(0, 60000),
            0,
        ));
        let mut audio_track = Track::new(TrackKind::Audio, "Audio".to_string());
        audio_track.clips.push(Clip::new(
            "/tmp/recording.mp4".to_string(),
            TimeRange::new(0, 60000),
            0,
        ));
        project.add_track(video_track);
        project.add_track(audio_track);
        project
    }

    #[test]
    fn empty_project_produces_no_filters() {
        let project = make_test_project();
        let builder = FilterGraphBuilder::new(&project);
        let output = builder.build();
        assert!(output.filter_complex.is_none());
        assert_eq!(output.inputs.len(), 1);
    }

    #[test]
    fn crop_generates_filter() {
        let mut project = make_test_project();
        project.global_operations.push(EditOperation::Crop {
            x: 0,
            y: 0,
            width: 1280,
            height: 720,
        });
        let builder = FilterGraphBuilder::new(&project);
        let output = builder.build();
        assert!(output.filter_complex.is_some());
        let fc = output.filter_complex.unwrap();
        assert!(fc.contains("crop=1280:720:0:0"));
    }

    #[test]
    fn audio_normalize_generates_loudnorm() {
        let mut project = make_test_project();
        project.global_operations.push(EditOperation::AudioNormalize);
        let builder = FilterGraphBuilder::new(&project);
        let output = builder.build();
        assert!(output.filter_complex.is_some());
        let fc = output.filter_complex.unwrap();
        assert!(fc.contains("loudnorm"));
    }

    #[test]
    fn chained_filters_connect_correctly() {
        let mut project = make_test_project();
        project.global_operations.push(EditOperation::Crop {
            x: 0,
            y: 0,
            width: 1280,
            height: 720,
        });
        project.global_operations.push(EditOperation::Scale {
            width: 640,
            height: 360,
        });
        let builder = FilterGraphBuilder::new(&project);
        let output = builder.build();
        let fc = output.filter_complex.unwrap();
        // First filter outputs [v0], second filter takes [v0] as input
        assert!(fc.contains("[v0]"));
        assert!(fc.contains("scale=640:360"));
    }

    #[test]
    fn to_ffmpeg_args_produces_valid_arguments() {
        let mut project = make_test_project();
        project.global_operations.push(EditOperation::Crop {
            x: 0,
            y: 0,
            width: 1280,
            height: 720,
        });
        let builder = FilterGraphBuilder::new(&project);
        let output = builder.build();
        let args = output.to_ffmpeg_args("/tmp/output.mp4");
        assert!(args.contains(&"-i".to_string()));
        assert!(args.contains(&"/tmp/recording.mp4".to_string()));
        assert!(args.contains(&"-filter_complex".to_string()));
        assert!(args.contains(&"/tmp/output.mp4".to_string()));
    }
}
```

- [ ] **Step 2: Run all domain editing tests**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/src-tauri
cargo test -p domain -- editing --nocapture
```

Expected: All tests pass (project tests + operations tests + filter graph tests).

- [ ] **Step 3: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src-tauri/crates/domain/src/editing/operations.rs src-tauri/crates/domain/src/editing/filter_graph.rs
git commit -m "feat: add EditOperation enum and FFmpeg filter graph builder"
```

---

## Task 3: Infrastructure — Frame Extractor Service (FFmpeg Subprocess)

**Files:**
- Create: `src-tauri/crates/infrastructure/src/editing/mod.rs`
- Create: `src-tauri/crates/infrastructure/src/editing/frame_extractor.rs`
- Modify: `src-tauri/crates/infrastructure/src/lib.rs` (add `pub mod editing;`)

- [ ] **Step 1: Create the editing infrastructure module**

Create `src-tauri/crates/infrastructure/src/editing/mod.rs`:
```rust
pub mod frame_extractor;
pub mod project_repository;

pub use frame_extractor::*;
pub use project_repository::*;
```

Add to `src-tauri/crates/infrastructure/src/lib.rs`:
```rust
pub mod editing;
```

- [ ] **Step 2: Implement the frame extractor**

Create `src-tauri/crates/infrastructure/src/editing/frame_extractor.rs`:
```rust
use std::path::{Path, PathBuf};
use std::sync::Arc;

use domain::error::{AppError, AppResult};
use domain::ffmpeg::provider::FfmpegProvider;
use tokio::process::Command;
use tracing::{debug, error};

/// Extracts individual frames from video files using FFmpeg subprocess.
///
/// Uses the pattern: `ffmpeg -ss <time> -i <input> -frames:v 1 -f image2pipe -vcodec png pipe:1`
/// to extract a single frame at a given timestamp and return it as PNG bytes.
pub struct FrameExtractor {
    ffmpeg: Arc<dyn FfmpegProvider>,
}

impl FrameExtractor {
    pub fn new(ffmpeg: Arc<dyn FfmpegProvider>) -> Self {
        FrameExtractor { ffmpeg }
    }

    /// Extract a single frame at the given timestamp (in milliseconds) and return PNG bytes.
    pub async fn extract_frame(&self, video_path: &str, time_ms: u64) -> AppResult<Vec<u8>> {
        let ffmpeg_path = self.ffmpeg.ffmpeg_path()?;
        let time_seconds = time_ms as f64 / 1000.0;

        debug!(
            "Extracting frame at {:.3}s from {}",
            time_seconds, video_path
        );

        let output = Command::new(&ffmpeg_path)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-ss",
                &format!("{:.3}", time_seconds),
                "-i",
                video_path,
                "-frames:v",
                "1",
                "-f",
                "image2pipe",
                "-vcodec",
                "png",
                "pipe:1",
            ])
            .output()
            .await
            .map_err(|e| AppError::FfmpegExecution(format!("Failed to spawn FFmpeg: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("FFmpeg frame extraction failed: {}", stderr);
            return Err(AppError::FfmpegExecution(format!(
                "Frame extraction failed: {stderr}"
            )));
        }

        if output.stdout.is_empty() {
            return Err(AppError::FfmpegExecution(
                "Frame extraction produced no output (timestamp may be beyond video duration)"
                    .to_string(),
            ));
        }

        debug!("Extracted frame: {} bytes", output.stdout.len());
        Ok(output.stdout)
    }

    /// Extract a frame and return it as a base64-encoded PNG data URL.
    /// This is the format the frontend Canvas can directly use as an image source.
    pub async fn extract_frame_base64(
        &self,
        video_path: &str,
        time_ms: u64,
    ) -> AppResult<String> {
        let png_bytes = self.extract_frame(video_path, time_ms).await?;
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
        Ok(format!("data:image/png;base64,{b64}"))
    }

    /// Extract multiple frames at evenly-spaced intervals for thumbnail strips.
    /// Returns a Vec of (time_ms, base64_data_url) pairs.
    pub async fn extract_thumbnails(
        &self,
        video_path: &str,
        duration_ms: u64,
        count: usize,
    ) -> AppResult<Vec<(u64, String)>> {
        if count == 0 {
            return Ok(Vec::new());
        }

        let interval = if count == 1 {
            0
        } else {
            duration_ms / (count as u64 - 1).max(1)
        };

        let mut results = Vec::with_capacity(count);
        for i in 0..count {
            let time = (i as u64 * interval).min(duration_ms.saturating_sub(100));
            match self.extract_frame_base64(video_path, time).await {
                Ok(data_url) => results.push((time, data_url)),
                Err(e) => {
                    debug!("Skipping thumbnail at {}ms: {}", time, e);
                    // Don't fail the whole operation if one thumbnail fails
                }
            }
        }

        Ok(results)
    }

    /// Get video metadata (duration, width, height, fps) using ffprobe.
    pub async fn probe_video(&self, video_path: &str) -> AppResult<VideoProbeResult> {
        let ffprobe_path = self.ffmpeg.ffprobe_path()?;

        let output = Command::new(&ffprobe_path)
            .args([
                "-v",
                "quiet",
                "-print_format",
                "json",
                "-show_format",
                "-show_streams",
                video_path,
            ])
            .output()
            .await
            .map_err(|e| AppError::FfmpegExecution(format!("Failed to spawn ffprobe: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::FfmpegExecution(format!(
                "ffprobe failed: {stderr}"
            )));
        }

        let json: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| AppError::FfmpegExecution(format!("Failed to parse ffprobe output: {e}")))?;

        let streams = json["streams"]
            .as_array()
            .ok_or_else(|| AppError::FfmpegExecution("No streams in ffprobe output".to_string()))?;

        let video_stream = streams
            .iter()
            .find(|s| s["codec_type"].as_str() == Some("video"));

        let audio_stream = streams
            .iter()
            .find(|s| s["codec_type"].as_str() == Some("audio"));

        let duration_str = json["format"]["duration"]
            .as_str()
            .unwrap_or("0");
        let duration_ms = (duration_str.parse::<f64>().unwrap_or(0.0) * 1000.0) as u64;

        let (width, height, fps) = if let Some(vs) = video_stream {
            let w = vs["width"].as_u64().unwrap_or(0) as u32;
            let h = vs["height"].as_u64().unwrap_or(0) as u32;
            let fps_str = vs["r_frame_rate"].as_str().unwrap_or("30/1");
            let fps = parse_frame_rate(fps_str);
            (w, h, fps)
        } else {
            (0, 0, 30.0)
        };

        let has_audio = audio_stream.is_some();

        Ok(VideoProbeResult {
            duration_ms,
            width,
            height,
            fps,
            has_audio,
        })
    }
}

/// Parse an FFmpeg frame rate string like "30/1" or "30000/1001" into a float.
fn parse_frame_rate(s: &str) -> f64 {
    if let Some((num, den)) = s.split_once('/') {
        let n: f64 = num.parse().unwrap_or(30.0);
        let d: f64 = den.parse().unwrap_or(1.0);
        if d > 0.0 {
            n / d
        } else {
            30.0
        }
    } else {
        s.parse().unwrap_or(30.0)
    }
}

/// Result of probing a video file with ffprobe.
#[derive(Debug, Clone, serde::Serialize)]
pub struct VideoProbeResult {
    /// Total duration in milliseconds.
    pub duration_ms: u64,
    /// Video width in pixels.
    pub width: u32,
    /// Video height in pixels.
    pub height: u32,
    /// Frame rate (frames per second).
    pub fps: f64,
    /// Whether the file contains an audio stream.
    pub has_audio: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_frame_rate_fraction() {
        let fps = parse_frame_rate("30000/1001");
        assert!((fps - 29.97).abs() < 0.1);
    }

    #[test]
    fn parse_frame_rate_simple() {
        let fps = parse_frame_rate("30/1");
        assert!((fps - 30.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_frame_rate_plain_number() {
        let fps = parse_frame_rate("60");
        assert!((fps - 60.0).abs() < f64::EPSILON);
    }
}
```

- [ ] **Step 3: Add base64 dependency to infrastructure crate**

Add to `src-tauri/crates/infrastructure/Cargo.toml` under `[dependencies]`:
```toml
base64 = "0.22"
```

- [ ] **Step 4: Verify it compiles**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/src-tauri
cargo check -p infrastructure
cargo test -p infrastructure -- editing --nocapture
```

Expected: Compiles clean. 3 frame rate parsing tests pass.

- [ ] **Step 5: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src-tauri/crates/infrastructure/src/editing/ src-tauri/crates/infrastructure/src/lib.rs src-tauri/crates/infrastructure/Cargo.toml
git commit -m "feat: add frame extractor service using FFmpeg subprocess"
```

---

## Task 4: Infrastructure — Project Save/Load (JSON Repository)

**Files:**
- Create: `src-tauri/crates/infrastructure/src/editing/project_repository.rs`

- [ ] **Step 1: Implement the project repository**

Create `src-tauri/crates/infrastructure/src/editing/project_repository.rs`:
```rust
use std::path::{Path, PathBuf};

use domain::editing::project::EditProject;
use domain::error::{AppError, AppResult};
use tokio::fs;
use tracing::{debug, info};

/// Saves and loads EditProject instances as JSON files.
///
/// Projects are stored in a configurable directory, one JSON file per project.
/// The filename is `{project_id}.json`.
pub struct ProjectRepository {
    /// Directory where project files are stored.
    projects_dir: PathBuf,
}

impl ProjectRepository {
    pub fn new(projects_dir: PathBuf) -> Self {
        ProjectRepository { projects_dir }
    }

    /// Ensure the projects directory exists.
    async fn ensure_dir(&self) -> AppResult<()> {
        if !self.projects_dir.exists() {
            fs::create_dir_all(&self.projects_dir)
                .await
                .map_err(|e| {
                    AppError::Io(format!(
                        "Failed to create projects directory {}: {}",
                        self.projects_dir.display(),
                        e
                    ))
                })?;
            debug!("Created projects directory: {}", self.projects_dir.display());
        }
        Ok(())
    }

    /// Get the file path for a project by ID.
    fn project_path(&self, project_id: &str) -> PathBuf {
        self.projects_dir.join(format!("{project_id}.json"))
    }

    /// Save a project to disk as JSON.
    pub async fn save(&self, project: &EditProject) -> AppResult<PathBuf> {
        self.ensure_dir().await?;
        let path = self.project_path(&project.id);
        let json = serde_json::to_string_pretty(project).map_err(|e| {
            AppError::Editing(format!("Failed to serialize project: {e}"))
        })?;
        fs::write(&path, json).await.map_err(|e| {
            AppError::Io(format!("Failed to write project file {}: {}", path.display(), e))
        })?;
        info!("Saved project '{}' to {}", project.name, path.display());
        Ok(path)
    }

    /// Load a project from disk by ID.
    pub async fn load(&self, project_id: &str) -> AppResult<EditProject> {
        let path = self.project_path(project_id);
        if !path.exists() {
            return Err(AppError::ProjectNotFound(format!(
                "Project file not found: {}",
                path.display()
            )));
        }
        let json = fs::read_to_string(&path).await.map_err(|e| {
            AppError::Io(format!("Failed to read project file {}: {}", path.display(), e))
        })?;
        let project: EditProject = serde_json::from_str(&json).map_err(|e| {
            AppError::Editing(format!("Failed to deserialize project: {e}"))
        })?;
        debug!("Loaded project '{}' from {}", project.name, path.display());
        Ok(project)
    }

    /// Delete a project file by ID.
    pub async fn delete(&self, project_id: &str) -> AppResult<()> {
        let path = self.project_path(project_id);
        if path.exists() {
            fs::remove_file(&path).await.map_err(|e| {
                AppError::Io(format!("Failed to delete project file {}: {}", path.display(), e))
            })?;
            info!("Deleted project file: {}", path.display());
        }
        Ok(())
    }

    /// List all saved projects (returns project ID and name pairs).
    pub async fn list(&self) -> AppResult<Vec<ProjectSummary>> {
        self.ensure_dir().await?;
        let mut summaries = Vec::new();
        let mut entries = fs::read_dir(&self.projects_dir).await.map_err(|e| {
            AppError::Io(format!(
                "Failed to read projects directory {}: {}",
                self.projects_dir.display(),
                e
            ))
        })?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            AppError::Io(format!("Failed to read directory entry: {e}"))
        })? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                match fs::read_to_string(&path).await {
                    Ok(json) => {
                        if let Ok(project) = serde_json::from_str::<EditProject>(&json) {
                            summaries.push(ProjectSummary {
                                id: project.id,
                                name: project.name,
                                source_path: project.source_path,
                                modified_at: project.modified_at,
                            });
                        }
                    }
                    Err(e) => {
                        debug!("Skipping unreadable project file {}: {}", path.display(), e);
                    }
                }
            }
        }

        // Sort by modified_at descending (most recent first)
        summaries.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
        Ok(summaries)
    }
}

/// A lightweight summary of a project (for listing without loading the full project).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub source_path: String,
    pub modified_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::editing::project::*;
    use tempfile::TempDir;

    async fn make_repo() -> (ProjectRepository, TempDir) {
        let dir = TempDir::new().unwrap();
        let repo = ProjectRepository::new(dir.path().to_path_buf());
        (repo, dir)
    }

    #[tokio::test]
    async fn save_and_load_project() {
        let (repo, _dir) = make_repo().await;
        let mut project = EditProject::new(
            "Test Project".to_string(),
            "/tmp/recording.mp4".to_string(),
            1920,
            1080,
            30.0,
        );
        let mut track = Track::new(TrackKind::Video, "Screen".to_string());
        track.clips.push(Clip::new(
            "/tmp/recording.mp4".to_string(),
            TimeRange::new(0, 60000),
            0,
        ));
        project.add_track(track);

        let path = repo.save(&project).await.unwrap();
        assert!(path.exists());

        let loaded = repo.load(&project.id).await.unwrap();
        assert_eq!(loaded.name, "Test Project");
        assert_eq!(loaded.tracks.len(), 1);
        assert_eq!(loaded.tracks[0].clips.len(), 1);
    }

    #[tokio::test]
    async fn load_nonexistent_returns_error() {
        let (repo, _dir) = make_repo().await;
        let result = repo.load("nonexistent-id").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn list_projects() {
        let (repo, _dir) = make_repo().await;
        let p1 = EditProject::new(
            "Project A".to_string(),
            "/tmp/a.mp4".to_string(),
            1920,
            1080,
            30.0,
        );
        let p2 = EditProject::new(
            "Project B".to_string(),
            "/tmp/b.mp4".to_string(),
            1920,
            1080,
            30.0,
        );
        repo.save(&p1).await.unwrap();
        repo.save(&p2).await.unwrap();

        let summaries = repo.list().await.unwrap();
        assert_eq!(summaries.len(), 2);
    }

    #[tokio::test]
    async fn delete_project() {
        let (repo, _dir) = make_repo().await;
        let project = EditProject::new(
            "To Delete".to_string(),
            "/tmp/del.mp4".to_string(),
            1920,
            1080,
            30.0,
        );
        repo.save(&project).await.unwrap();
        repo.delete(&project.id).await.unwrap();
        let result = repo.load(&project.id).await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Add tempfile dev dependency for tests**

Add to `src-tauri/crates/infrastructure/Cargo.toml`:
```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Run tests**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/src-tauri
cargo test -p infrastructure -- editing --nocapture
```

Expected: All project repository tests pass (save_and_load, load_nonexistent, list, delete) plus frame rate parsing tests.

- [ ] **Step 4: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src-tauri/crates/infrastructure/src/editing/project_repository.rs src-tauri/crates/infrastructure/Cargo.toml
git commit -m "feat: add project JSON repository for save/load/list/delete"
```

---

## Task 5: App — Editing IPC Commands

**Files:**
- Create: `src-tauri/src/commands/editing.rs`
- Modify: `src-tauri/src/commands/mod.rs` (add `pub mod editing;`)
- Modify: `src-tauri/src/state.rs` (add editing services to AppState)
- Modify: `src-tauri/src/main.rs` (register editing commands and setup state)

- [ ] **Step 1: Extend AppState with editing services**

Add to `src-tauri/src/state.rs`:
```rust
use infrastructure::editing::{FrameExtractor, ProjectRepository};

// Add these fields to the AppState struct:
pub frame_extractor: Arc<FrameExtractor>,
pub project_repository: Arc<ProjectRepository>,
```

The full `AppState` should now include:
```rust
use std::sync::Arc;

use domain::ffmpeg::provider::FfmpegProvider;
use domain::platform::PlatformInfo;
use domain::settings::repository::SettingsRepository;
use infrastructure::editing::{FrameExtractor, ProjectRepository};

pub struct AppState {
    pub ffmpeg: Arc<dyn FfmpegProvider>,
    pub settings: Arc<dyn SettingsRepository>,
    pub platform: PlatformInfo,
    pub frame_extractor: Arc<FrameExtractor>,
    pub project_repository: Arc<ProjectRepository>,
}
```

- [ ] **Step 2: Create the editing IPC commands**

Create `src-tauri/src/commands/editing.rs`:
```rust
use std::sync::Mutex;

use domain::editing::filter_graph::FilterGraphBuilder;
use domain::editing::operations::EditOperation;
use domain::editing::project::*;
use infrastructure::editing::project_repository::ProjectSummary;
use infrastructure::editing::frame_extractor::VideoProbeResult;
use tauri::State;

use crate::error::CommandResult;
use crate::state::AppState;

/// In-memory editor state: the currently open project plus undo/redo stacks.
pub struct EditorState {
    /// The currently open project (None if no project is open).
    pub project: Option<EditProject>,
    /// Undo stack: list of (operation_description, project_snapshot_before).
    pub undo_stack: Vec<(String, EditProject)>,
    /// Redo stack: list of (operation_description, project_snapshot_before_undo).
    pub redo_stack: Vec<(String, EditProject)>,
}

impl EditorState {
    pub fn new() -> Self {
        EditorState {
            project: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }
}

// ─── Project management ───────────────────────────────────────

#[tauri::command]
pub async fn create_edit_project(
    state: State<'_, AppState>,
    editor: State<'_, Mutex<EditorState>>,
    name: String,
    source_path: String,
) -> CommandResult<EditProject> {
    // Probe the source video to get dimensions and duration
    let probe = state.frame_extractor.probe_video(&source_path).await?;

    let mut project = EditProject::new(
        name,
        source_path.clone(),
        probe.width,
        probe.height,
        probe.fps,
    );

    // Create default video track with the source as a single clip
    let mut video_track = Track::new(TrackKind::Video, "Screen".to_string());
    video_track.clips.push(Clip::new(
        source_path.clone(),
        TimeRange::new(0, probe.duration_ms),
        0,
    ));
    project.add_track(video_track);

    // Create default audio track if the source has audio
    if probe.has_audio {
        let mut audio_track = Track::new(TrackKind::Audio, "Audio".to_string());
        audio_track.clips.push(Clip::new(
            source_path,
            TimeRange::new(0, probe.duration_ms),
            0,
        ));
        project.add_track(audio_track);
    }

    // Save to disk
    state.project_repository.save(&project).await?;

    // Set as current project in editor state
    let mut editor = editor.lock().map_err(|e| {
        domain::error::AppError::Editing(format!("Editor lock poisoned: {e}"))
    })?;
    editor.project = Some(project.clone());
    editor.undo_stack.clear();
    editor.redo_stack.clear();

    Ok(project)
}

#[tauri::command]
pub async fn open_edit_project(
    state: State<'_, AppState>,
    editor: State<'_, Mutex<EditorState>>,
    project_id: String,
) -> CommandResult<EditProject> {
    let project = state.project_repository.load(&project_id).await?;

    let mut editor = editor.lock().map_err(|e| {
        domain::error::AppError::Editing(format!("Editor lock poisoned: {e}"))
    })?;
    editor.project = Some(project.clone());
    editor.undo_stack.clear();
    editor.redo_stack.clear();

    Ok(project)
}

#[tauri::command]
pub async fn save_edit_project(
    state: State<'_, AppState>,
    editor: State<'_, Mutex<EditorState>>,
) -> CommandResult<()> {
    let editor = editor.lock().map_err(|e| {
        domain::error::AppError::Editing(format!("Editor lock poisoned: {e}"))
    })?;
    let project = editor.project.as_ref().ok_or_else(|| {
        domain::error::AppError::Editing("No project is currently open".to_string())
    })?;
    state.project_repository.save(project).await?;
    Ok(())
}

#[tauri::command]
pub async fn list_edit_projects(
    state: State<'_, AppState>,
) -> CommandResult<Vec<ProjectSummary>> {
    let summaries = state.project_repository.list().await?;
    Ok(summaries)
}

#[tauri::command]
pub async fn delete_edit_project(
    state: State<'_, AppState>,
    project_id: String,
) -> CommandResult<()> {
    state.project_repository.delete(&project_id).await?;
    Ok(())
}

// ─── Edit operations ──────────────────────────────────────────

#[tauri::command]
pub fn get_current_project(
    editor: State<'_, Mutex<EditorState>>,
) -> CommandResult<Option<EditProject>> {
    let editor = editor.lock().map_err(|e| {
        domain::error::AppError::Editing(format!("Editor lock poisoned: {e}"))
    })?;
    Ok(editor.project.clone())
}

#[tauri::command]
pub async fn apply_operation(
    state: State<'_, AppState>,
    editor: State<'_, Mutex<EditorState>>,
    operation: EditOperation,
    target_track_id: Option<String>,
    target_clip_id: Option<String>,
) -> CommandResult<EditProject> {
    let mut editor = editor.lock().map_err(|e| {
        domain::error::AppError::Editing(format!("Editor lock poisoned: {e}"))
    })?;

    let project = editor.project.as_ref().ok_or_else(|| {
        domain::error::AppError::Editing("No project is currently open".to_string())
    })?;

    // Save snapshot for undo
    let label = operation.label().to_string();
    let snapshot = project.clone();

    // Apply the operation
    let mut updated = project.clone();

    if let (Some(track_id), Some(clip_id)) = (&target_track_id, &target_clip_id) {
        // Apply to a specific clip
        if let Some(track) = updated.track_mut(track_id) {
            if let Some(clip) = track.clips.iter_mut().find(|c| c.id == *clip_id) {
                clip.operations.push(operation);
            } else {
                return Err(domain::error::AppError::Editing(
                    format!("Clip not found: {clip_id}"),
                ).into());
            }
        } else {
            return Err(domain::error::AppError::Editing(
                format!("Track not found: {track_id}"),
            ).into());
        }
    } else {
        // Apply as a global operation
        updated.global_operations.push(operation);
    }

    updated.touch();

    // Push to undo stack, clear redo stack
    editor.undo_stack.push((label, snapshot));
    editor.redo_stack.clear();
    editor.project = Some(updated.clone());

    Ok(updated)
}

#[tauri::command]
pub fn undo_operation(
    editor: State<'_, Mutex<EditorState>>,
) -> CommandResult<Option<EditProject>> {
    let mut editor = editor.lock().map_err(|e| {
        domain::error::AppError::Editing(format!("Editor lock poisoned: {e}"))
    })?;

    if let Some((label, previous)) = editor.undo_stack.pop() {
        let current = editor.project.clone().unwrap();
        editor.redo_stack.push((label, current));
        editor.project = Some(previous.clone());
        Ok(Some(previous))
    } else {
        Ok(editor.project.clone())
    }
}

#[tauri::command]
pub fn redo_operation(
    editor: State<'_, Mutex<EditorState>>,
) -> CommandResult<Option<EditProject>> {
    let mut editor = editor.lock().map_err(|e| {
        domain::error::AppError::Editing(format!("Editor lock poisoned: {e}"))
    })?;

    if let Some((label, next)) = editor.redo_stack.pop() {
        let current = editor.project.clone().unwrap();
        editor.undo_stack.push((label, current));
        editor.project = Some(next.clone());
        Ok(Some(next))
    } else {
        Ok(editor.project.clone())
    }
}

#[tauri::command]
pub fn get_undo_redo_state(
    editor: State<'_, Mutex<EditorState>>,
) -> CommandResult<UndoRedoState> {
    let editor = editor.lock().map_err(|e| {
        domain::error::AppError::Editing(format!("Editor lock poisoned: {e}"))
    })?;

    Ok(UndoRedoState {
        can_undo: !editor.undo_stack.is_empty(),
        can_redo: !editor.redo_stack.is_empty(),
        undo_label: editor.undo_stack.last().map(|(l, _)| l.clone()),
        redo_label: editor.redo_stack.last().map(|(l, _)| l.clone()),
    })
}

#[derive(serde::Serialize)]
pub struct UndoRedoState {
    pub can_undo: bool,
    pub can_redo: bool,
    pub undo_label: Option<String>,
    pub redo_label: Option<String>,
}

// ─── Frame extraction ─────────────────────────────────────────

#[tauri::command]
pub async fn get_video_frame(
    state: State<'_, AppState>,
    video_path: String,
    time_ms: u64,
) -> CommandResult<String> {
    let data_url = state
        .frame_extractor
        .extract_frame_base64(&video_path, time_ms)
        .await?;
    Ok(data_url)
}

#[tauri::command]
pub async fn get_video_thumbnails(
    state: State<'_, AppState>,
    video_path: String,
    duration_ms: u64,
    count: usize,
) -> CommandResult<Vec<(u64, String)>> {
    let thumbnails = state
        .frame_extractor
        .extract_thumbnails(&video_path, duration_ms, count)
        .await?;
    Ok(thumbnails)
}

#[tauri::command]
pub async fn probe_video(
    state: State<'_, AppState>,
    video_path: String,
) -> CommandResult<VideoProbeResult> {
    let result = state.frame_extractor.probe_video(&video_path).await?;
    Ok(result)
}

// ─── Filter graph preview ─────────────────────────────────────

#[tauri::command]
pub fn get_filter_graph(
    editor: State<'_, Mutex<EditorState>>,
) -> CommandResult<Option<String>> {
    let editor = editor.lock().map_err(|e| {
        domain::error::AppError::Editing(format!("Editor lock poisoned: {e}"))
    })?;

    if let Some(project) = &editor.project {
        let builder = FilterGraphBuilder::new(project);
        let output = builder.build();
        Ok(output.filter_complex)
    } else {
        Ok(None)
    }
}
```

- [ ] **Step 3: Register the editing commands**

Add to `src-tauri/src/commands/mod.rs`:
```rust
pub mod editing;
```

- [ ] **Step 4: Wire editing state and commands into main.rs**

In the `.setup()` closure in `src-tauri/src/main.rs`, add after existing state setup:

```rust
use infrastructure::editing::{FrameExtractor, ProjectRepository};
use crate::commands::editing::EditorState;
use std::sync::Mutex;

// Inside .setup():
let projects_dir = app_config_dir.join("projects");
let frame_extractor = Arc::new(FrameExtractor::new(ffmpeg_resolver.clone()));
let project_repository = Arc::new(ProjectRepository::new(projects_dir));

// Add to AppState construction:
// frame_extractor,
// project_repository,

// Register editor state separately (it uses Mutex, not Arc):
app.manage(Mutex::new(EditorState::new()));
```

Add to the `generate_handler!` macro:
```rust
commands::editing::create_edit_project,
commands::editing::open_edit_project,
commands::editing::save_edit_project,
commands::editing::list_edit_projects,
commands::editing::delete_edit_project,
commands::editing::get_current_project,
commands::editing::apply_operation,
commands::editing::undo_operation,
commands::editing::redo_operation,
commands::editing::get_undo_redo_state,
commands::editing::get_video_frame,
commands::editing::get_video_thumbnails,
commands::editing::probe_video,
commands::editing::get_filter_graph,
```

- [ ] **Step 5: Verify it compiles**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/src-tauri
cargo check
```

Expected: Compiles clean.

- [ ] **Step 6: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src-tauri/src/commands/editing.rs src-tauri/src/commands/mod.rs src-tauri/src/state.rs src-tauri/src/main.rs
git commit -m "feat: add editing IPC commands with undo/redo and frame extraction"
```

---

## Task 6: Frontend — Editor Store (Project State, Undo/Redo, Selection)

**Files:**
- Create: `src/stores/editor.ts`
- Modify: `src/lib/types.ts` (add editing types)
- Modify: `src/lib/ipc.ts` (add editing IPC wrappers)

- [ ] **Step 1: Add editing types to types.ts**

Append to `src/lib/types.ts`:
```typescript
// ─── Editing types (mirrors domain::editing) ──────────────────

export interface TimeRange {
  start_ms: number;
  end_ms: number;
}

export interface Clip {
  id: string;
  source_path: string;
  source_range: TimeRange;
  timeline_start_ms: number;
  operations: EditOperation[];
}

export type TrackKind = "video" | "audio" | "overlay"; // v1: single video + single audio only // TODO(v2): Multi-track timeline

export interface Track {
  id: string;
  kind: TrackKind;
  name: string;
  clips: Clip[];
  muted: boolean;
  locked: boolean;
}

export interface EditProject {
  id: string;
  name: string;
  source_path: string;
  width: number;
  height: number;
  fps: number;
  tracks: Track[];
  global_operations: EditOperation[];
  created_at: string;
  modified_at: string;
}

export interface TextOverlayConfig {
  text: string;
  x: number;
  y: number;
  font_size: number;
  font_family: string;
  color: string;
  background_color: string;
  start_ms: number;
  end_ms: number;
}

export interface ImageOverlayConfig {
  image_path: string;
  x: number;
  y: number;
  width: number;
  height: number;
  opacity: number;
  start_ms: number;
  end_ms: number;
}

export interface AudioCompressorConfig {
  threshold_db: number;
  ratio: number;
  attack_ms: number;
  release_ms: number;
}

export interface AudioEqBand {
  frequency: number;
  width: number;
  gain: number;
}

export interface AudioEqConfig {
  bands: AudioEqBand[];
}

export interface BlurRegionConfig {
  x: number;
  y: number;
  width: number;
  height: number;
  strength: number;
  start_ms: number;
  end_ms: number;
}

export type EditOperation =
  | { type: "Trim"; config: { range: TimeRange } }
  | { type: "Crop"; config: { x: number; y: number; width: number; height: number } }
  | { type: "Scale"; config: { width: number; height: number } }
  | { type: "Rotate"; config: { degrees: number } }
  | { type: "AddTextOverlay"; config: TextOverlayConfig }
  | { type: "AddImageOverlay"; config: ImageOverlayConfig }
  | { type: "AddBlurRegion"; config: BlurRegionConfig }
  | { type: "AudioNormalize"; config: null }
  | { type: "AudioCompress"; config: AudioCompressorConfig } // TODO(v2): UI not exposed in v1
  | { type: "AudioEq"; config: AudioEqConfig } // TODO(v2): UI not exposed in v1
  | { type: "AudioNoiseReduce"; config: null } // TODO(v2): UI not exposed in v1
  | { type: "AudioVolume"; config: { multiplier: number } }
  | { type: "Speed"; config: { factor: number } };

export interface UndoRedoState {
  can_undo: boolean;
  can_redo: boolean;
  undo_label: string | null;
  redo_label: string | null;
}

export interface ProjectSummary {
  id: string;
  name: string;
  source_path: string;
  modified_at: string;
}

export interface VideoProbeResult {
  duration_ms: number;
  width: number;
  height: number;
  fps: number;
  has_audio: boolean;
}
```

- [ ] **Step 2: Add editing IPC wrappers to ipc.ts**

Append to `src/lib/ipc.ts`:
```typescript
import type {
  EditProject,
  EditOperation,
  UndoRedoState,
  ProjectSummary,
  VideoProbeResult,
} from "./types";

// ─── Editing ──────────────────────────────────────────────────

export const createEditProject = (name: string, sourcePath: string) =>
  invoke<EditProject>("create_edit_project", { name, sourcePath });

export const openEditProject = (projectId: string) =>
  invoke<EditProject>("open_edit_project", { projectId });

export const saveEditProject = () =>
  invoke<void>("save_edit_project");

export const listEditProjects = () =>
  invoke<ProjectSummary[]>("list_edit_projects");

export const deleteEditProject = (projectId: string) =>
  invoke<void>("delete_edit_project", { projectId });

export const getCurrentProject = () =>
  invoke<EditProject | null>("get_current_project");

export const applyOperation = (
  operation: EditOperation,
  targetTrackId?: string,
  targetClipId?: string,
) =>
  invoke<EditProject>("apply_operation", {
    operation,
    targetTrackId: targetTrackId ?? null,
    targetClipId: targetClipId ?? null,
  });

export const undoOperation = () =>
  invoke<EditProject | null>("undo_operation");

export const redoOperation = () =>
  invoke<EditProject | null>("redo_operation");

export const getUndoRedoState = () =>
  invoke<UndoRedoState>("get_undo_redo_state");

export const getVideoFrame = (videoPath: string, timeMs: number) =>
  invoke<string>("get_video_frame", { videoPath, timeMs });

export const getVideoThumbnails = (
  videoPath: string,
  durationMs: number,
  count: number,
) =>
  invoke<[number, string][]>("get_video_thumbnails", {
    videoPath,
    durationMs,
    count,
  });

export const probeVideo = (videoPath: string) =>
  invoke<VideoProbeResult>("probe_video", { videoPath });

export const getFilterGraph = () =>
  invoke<string | null>("get_filter_graph");
```

- [ ] **Step 3: Create the editor store**

Create `src/stores/editor.ts`:
```typescript
import { useState, useEffect, useCallback, useRef } from "preact/hooks";
import {
  createEditProject,
  openEditProject,
  saveEditProject as saveProjectIpc,
  getCurrentProject,
  applyOperation as applyOp,
  undoOperation as undoOp,
  redoOperation as redoOp,
  getUndoRedoState,
  getVideoFrame,
  getVideoThumbnails,
  probeVideo,
  getFilterGraph,
} from "../lib/ipc";
import type {
  EditProject,
  EditOperation,
  UndoRedoState,
  VideoProbeResult,
} from "../lib/types";
import { formatError } from "../lib/errors";

export interface EditorSelection {
  trackId: string | null;
  clipId: string | null;
}

export interface EditorStoreState {
  project: EditProject | null;
  undoRedo: UndoRedoState;
  probe: VideoProbeResult | null;
  currentTimeMs: number;
  currentFrame: string | null;
  isPlaying: boolean;
  selection: EditorSelection;
  loading: boolean;
  error: string | null;
}

const DEFAULT_UNDO_REDO: UndoRedoState = {
  can_undo: false,
  can_redo: false,
  undo_label: null,
  redo_label: null,
};

export function useEditor() {
  const [project, setProject] = useState<EditProject | null>(null);
  const [undoRedo, setUndoRedo] = useState<UndoRedoState>(DEFAULT_UNDO_REDO);
  const [probe, setProbe] = useState<VideoProbeResult | null>(null);
  const [currentTimeMs, setCurrentTimeMs] = useState(0);
  const [currentFrame, setCurrentFrame] = useState<string | null>(null);
  const [isPlaying, setIsPlaying] = useState(false);
  const [selection, setSelection] = useState<EditorSelection>({
    trackId: null,
    clipId: null,
  });
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const playIntervalRef = useRef<number | null>(null);

  // Refresh undo/redo state after any project change
  const refreshUndoRedo = useCallback(async () => {
    try {
      const state = await getUndoRedoState();
      setUndoRedo(state);
    } catch {
      // Non-critical — don't propagate
    }
  }, []);

  // Create a new project from a source media file
  const createProject = useCallback(
    async (name: string, sourcePath: string) => {
      try {
        setLoading(true);
        setError(null);
        const p = await createEditProject(name, sourcePath);
        setProject(p);
        const probeResult = await probeVideo(sourcePath);
        setProbe(probeResult);
        setCurrentTimeMs(0);
        await refreshUndoRedo();
        // Load the first frame
        const frame = await getVideoFrame(sourcePath, 0);
        setCurrentFrame(frame);
      } catch (err) {
        setError(formatError(err));
      } finally {
        setLoading(false);
      }
    },
    [refreshUndoRedo],
  );

  // Open an existing project
  const openProject = useCallback(
    async (projectId: string) => {
      try {
        setLoading(true);
        setError(null);
        const p = await openEditProject(projectId);
        setProject(p);
        const probeResult = await probeVideo(p.source_path);
        setProbe(probeResult);
        setCurrentTimeMs(0);
        await refreshUndoRedo();
        const frame = await getVideoFrame(p.source_path, 0);
        setCurrentFrame(frame);
      } catch (err) {
        setError(formatError(err));
      } finally {
        setLoading(false);
      }
    },
    [refreshUndoRedo],
  );

  // Save the current project to disk
  const saveProject = useCallback(async () => {
    try {
      setError(null);
      await saveProjectIpc();
    } catch (err) {
      setError(formatError(err));
    }
  }, []);

  // Apply an edit operation
  const applyOperation = useCallback(
    async (
      operation: EditOperation,
      targetTrackId?: string,
      targetClipId?: string,
    ) => {
      try {
        setError(null);
        const updated = await applyOp(operation, targetTrackId, targetClipId);
        setProject(updated);
        await refreshUndoRedo();
      } catch (err) {
        setError(formatError(err));
      }
    },
    [refreshUndoRedo],
  );

  // Undo the last operation
  const undo = useCallback(async () => {
    try {
      setError(null);
      const p = await undoOp();
      if (p) setProject(p);
      await refreshUndoRedo();
    } catch (err) {
      setError(formatError(err));
    }
  }, [refreshUndoRedo]);

  // Redo the last undone operation
  const redo = useCallback(async () => {
    try {
      setError(null);
      const p = await redoOp();
      if (p) setProject(p);
      await refreshUndoRedo();
    } catch (err) {
      setError(formatError(err));
    }
  }, [refreshUndoRedo]);

  // Seek to a specific time and load the frame
  const seekTo = useCallback(
    async (timeMs: number) => {
      if (!project) return;
      setCurrentTimeMs(timeMs);
      try {
        const frame = await getVideoFrame(project.source_path, timeMs);
        setCurrentFrame(frame);
      } catch (err) {
        // Frame load failure during scrubbing is not critical
        console.warn("Frame load failed:", err);
      }
    },
    [project],
  );

  // Play/pause toggle
  const togglePlay = useCallback(() => {
    if (!project || !probe) return;

    if (isPlaying) {
      // Stop
      if (playIntervalRef.current !== null) {
        clearInterval(playIntervalRef.current);
        playIntervalRef.current = null;
      }
      setIsPlaying(false);
    } else {
      // Start playback — advance by frame interval
      setIsPlaying(true);
      const frameInterval = 1000 / (probe.fps || 30);
      playIntervalRef.current = window.setInterval(() => {
        setCurrentTimeMs((prev) => {
          const next = prev + frameInterval;
          if (next >= probe.duration_ms) {
            clearInterval(playIntervalRef.current!);
            playIntervalRef.current = null;
            setIsPlaying(false);
            return 0;
          }
          return next;
        });
      }, frameInterval);
    }
  }, [isPlaying, project, probe]);

  // Load frame when currentTimeMs changes during playback
  useEffect(() => {
    if (!isPlaying || !project) return;
    let cancelled = false;
    (async () => {
      try {
        const frame = await getVideoFrame(project.source_path, currentTimeMs);
        if (!cancelled) setCurrentFrame(frame);
      } catch {
        // Ignore frame load errors during playback
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [currentTimeMs, isPlaying, project]);

  // Cleanup interval on unmount
  useEffect(() => {
    return () => {
      if (playIntervalRef.current !== null) {
        clearInterval(playIntervalRef.current);
      }
    };
  }, []);

  // Select a track/clip
  const select = useCallback((trackId: string | null, clipId: string | null) => {
    setSelection({ trackId, clipId });
  }, []);

  // Clear selection
  const clearSelection = useCallback(() => {
    setSelection({ trackId: null, clipId: null });
  }, []);

  return {
    // State
    project,
    undoRedo,
    probe,
    currentTimeMs,
    currentFrame,
    isPlaying,
    selection,
    loading,
    error,
    // Actions
    createProject,
    openProject,
    saveProject,
    applyOperation,
    undo,
    redo,
    seekTo,
    togglePlay,
    select,
    clearSelection,
  };
}
```

- [ ] **Step 4: Verify frontend compiles**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
pnpm exec tsc --noEmit
```

Expected: No type errors.

- [ ] **Step 5: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src/lib/types.ts src/lib/ipc.ts src/stores/editor.ts
git commit -m "feat: add editor store with undo/redo, playback, and typed IPC"
```

---

## Task 7: Frontend — Editor Page Layout

**Files:**
- Create: `src/pages/Editor.tsx`

- [ ] **Step 1: Create the editor page**

Create `src/pages/Editor.tsx`:
```tsx
import styles from "./Editor.module.scss";
import { useEditor } from "../stores/editor";
import VideoPreview from "../components/editor/VideoPreview";
import Timeline from "../components/editor/Timeline";
import ToolPanel from "../components/editor/ToolPanel";
import PropertiesPanel from "../components/editor/PropertiesPanel";
import AudioPanel from "../components/editor/AudioPanel";

interface EditorProps {
  sourcePath?: string;
  projectId?: string;
}

export default function Editor({ sourcePath, projectId }: EditorProps) {
  const editor = useEditor();

  // Initialize: create new project or open existing one
  const initialized = !!editor.project;

  if (!initialized && !editor.loading) {
    if (projectId) {
      editor.openProject(projectId);
    } else if (sourcePath) {
      const name = sourcePath.split("/").pop()?.replace(/\.[^.]+$/, "") || "Untitled";
      editor.createProject(name, sourcePath);
    }
  }

  if (editor.loading) {
    return (
      <div class="editor-page editor-loading">
        <div class="loading-spinner">Loading project...</div>
      </div>
    );
  }

  if (editor.error) {
    return (
      <div class="editor-page editor-error">
        <div class="error-message">
          <h2>Error</h2>
          <p>{editor.error}</p>
          <button onClick={() => window.history.back()}>Go Back</button>
        </div>
      </div>
    );
  }

  if (!editor.project) {
    return (
      <div class="editor-page editor-empty">
        <p>No project loaded. Select a recording to edit.</p>
      </div>
    );
  }

  return (
    <div class="editor-page">
      {/* Top toolbar */}
      <div class="editor-toolbar">
        <div class="toolbar-left">
          <button
            class="toolbar-btn"
            onClick={() => window.history.back()}
            title="Back"
          >
            Back
          </button>
          <span class="project-name">{editor.project.name}</span>
        </div>
        <div class="toolbar-center">
          <button
            class="toolbar-btn"
            disabled={!editor.undoRedo.can_undo}
            onClick={editor.undo}
            title={editor.undoRedo.undo_label ? `Undo ${editor.undoRedo.undo_label}` : "Undo"}
          >
            Undo
          </button>
          <button
            class="toolbar-btn"
            disabled={!editor.undoRedo.can_redo}
            onClick={editor.redo}
            title={editor.undoRedo.redo_label ? `Redo ${editor.undoRedo.redo_label}` : "Redo"}
          >
            Redo
          </button>
        </div>
        <div class="toolbar-right">
          <button class="toolbar-btn" onClick={editor.saveProject}>
            Save
          </button>
        </div>
      </div>

      {/* Main content area */}
      <div class="editor-main">
        {/* Left: Tool panel */}
        <div class="editor-sidebar-left">
          <ToolPanel
            project={editor.project}
            onApplyOperation={editor.applyOperation}
            currentTimeMs={editor.currentTimeMs}
            probe={editor.probe}
          />
        </div>

        {/* Center: Preview */}
        <div class="editor-preview-area">
          <VideoPreview
            currentFrame={editor.currentFrame}
            width={editor.project.width}
            height={editor.project.height}
            currentTimeMs={editor.currentTimeMs}
            isPlaying={editor.isPlaying}
            onTogglePlay={editor.togglePlay}
            onSeek={editor.seekTo}
            durationMs={editor.probe?.duration_ms ?? 0}
          />
        </div>

        {/* Right: Properties panel */}
        <div class="editor-sidebar-right">
          <PropertiesPanel
            project={editor.project}
            selection={editor.selection}
            onApplyOperation={editor.applyOperation}
          />
        </div>
      </div>

      {/* Bottom: Timeline and Audio */}
      <div class="editor-bottom">
        <Timeline
          project={editor.project}
          currentTimeMs={editor.currentTimeMs}
          selection={editor.selection}
          onSeek={editor.seekTo}
          onSelect={editor.select}
          probe={editor.probe}
        />
        <AudioPanel
          project={editor.project}
          onApplyOperation={editor.applyOperation}
        />
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Add editor page styles**

Create `src/pages/Editor.module.scss`:
```scss
/* Editor page layout — four-panel design:
   Top toolbar, left tools, center preview, right properties, bottom timeline/audio */

.editor-page {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: #1a1a2e;
  color: #e0e0e0;
  overflow: hidden;
}

.editor-loading,
.editor-error,
.editor-empty {
  display: flex;
  align-items: center;
  justify-content: center;
}

.editor-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 40px;
  padding: 0 12px;
  background: #16213e;
  border-bottom: 1px solid #0f3460;
  flex-shrink: 0;
}

.toolbar-left,
.toolbar-center,
.toolbar-right {
  display: flex;
  align-items: center;
  gap: 8px;
}

.toolbar-btn {
  padding: 4px 12px;
  border: 1px solid #0f3460;
  border-radius: 4px;
  background: #1a1a2e;
  color: #e0e0e0;
  cursor: pointer;
  font-size: 13px;
}

.toolbar-btn:hover {
  background: #0f3460;
}

.toolbar-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.project-name {
  font-size: 14px;
  font-weight: 600;
  margin-left: 12px;
}

.editor-main {
  display: flex;
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

.editor-sidebar-left {
  width: 220px;
  background: #16213e;
  border-right: 1px solid #0f3460;
  overflow-y: auto;
  flex-shrink: 0;
}

.editor-preview-area {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #0a0a1a;
  min-width: 0;
}

.editor-sidebar-right {
  width: 260px;
  background: #16213e;
  border-left: 1px solid #0f3460;
  overflow-y: auto;
  flex-shrink: 0;
}

.editor-bottom {
  height: 200px;
  background: #16213e;
  border-top: 1px solid #0f3460;
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
}

.error-message {
  text-align: center;
}

.error-message h2 {
  color: #e94560;
}

.error-message button {
  margin-top: 12px;
  padding: 8px 20px;
  border: none;
  border-radius: 4px;
  background: #e94560;
  color: white;
  cursor: pointer;
}
```

- [ ] **Step 3: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src/pages/Editor.tsx src/pages/Editor.module.scss
git commit -m "feat: add editor page layout with toolbar, preview, timeline, and panels"
```

---

## Task 8: Frontend — VideoPreview Component

**Files:**
- Create: `src/components/editor/VideoPreview.tsx`

- [ ] **Step 1: Create the video preview component**

Create `src/components/editor/VideoPreview.tsx`:
```tsx
import styles from "./VideoPreview.module.scss"; // SCSS module — semantic class names, no inline styles for layout
import { useRef, useEffect, useCallback } from "preact/hooks";

interface VideoPreviewProps {
  currentFrame: string | null;
  width: number;
  height: number;
  currentTimeMs: number;
  isPlaying: boolean;
  onTogglePlay: () => void;
  onSeek: (timeMs: number) => void;
  durationMs: number;
}

export default function VideoPreview({
  currentFrame,
  width,
  height,
  currentTimeMs,
  isPlaying,
  onTogglePlay,
  onSeek,
  durationMs,
}: VideoPreviewProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const imageRef = useRef<HTMLImageElement | null>(null);

  // Draw the current frame onto the canvas
  useEffect(() => {
    if (!currentFrame || !canvasRef.current) return;

    const canvas = canvasRef.current;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const img = new Image();
    img.onload = () => {
      imageRef.current = img;
      // Scale to fit canvas while maintaining aspect ratio
      const scale = Math.min(canvas.width / img.width, canvas.height / img.height);
      const drawWidth = img.width * scale;
      const drawHeight = img.height * scale;
      const offsetX = (canvas.width - drawWidth) / 2;
      const offsetY = (canvas.height - drawHeight) / 2;

      ctx.fillStyle = "#000000";
      ctx.fillRect(0, 0, canvas.width, canvas.height);
      ctx.drawImage(img, offsetX, offsetY, drawWidth, drawHeight);
    };
    img.src = currentFrame;
  }, [currentFrame]);

  // Resize canvas to fit container
  useEffect(() => {
    const container = containerRef.current;
    const canvas = canvasRef.current;
    if (!container || !canvas) return;

    const observer = new ResizeObserver(() => {
      const rect = container.getBoundingClientRect();
      // Use the video aspect ratio to size the canvas
      const aspect = width / Math.max(height, 1);
      let canvasW = rect.width - 20;
      let canvasH = canvasW / aspect;
      if (canvasH > rect.height - 80) {
        canvasH = rect.height - 80;
        canvasW = canvasH * aspect;
      }
      canvas.width = Math.floor(canvasW);
      canvas.height = Math.floor(canvasH);

      // Redraw current frame at new size
      if (imageRef.current) {
        const ctx = canvas.getContext("2d");
        if (ctx) {
          const img = imageRef.current;
          const scale = Math.min(canvas.width / img.width, canvas.height / img.height);
          const drawWidth = img.width * scale;
          const drawHeight = img.height * scale;
          const offsetX = (canvas.width - drawWidth) / 2;
          const offsetY = (canvas.height - drawHeight) / 2;
          ctx.fillStyle = "#000000";
          ctx.fillRect(0, 0, canvas.width, canvas.height);
          ctx.drawImage(img, offsetX, offsetY, drawWidth, drawHeight);
        }
      }
    });
    observer.observe(container);
    return () => observer.disconnect();
  }, [width, height]);

  // Format time as MM:SS.mmm
  const formatTime = useCallback((ms: number) => {
    const totalSec = Math.floor(ms / 1000);
    const min = Math.floor(totalSec / 60);
    const sec = totalSec % 60;
    const millis = Math.floor(ms % 1000);
    return `${min.toString().padStart(2, "0")}:${sec.toString().padStart(2, "0")}.${millis.toString().padStart(3, "0")}`;
  }, []);

  // Handle scrub bar click
  const handleScrubClick = useCallback(
    (e: MouseEvent) => {
      const target = e.currentTarget as HTMLDivElement;
      const rect = target.getBoundingClientRect();
      const fraction = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
      const timeMs = Math.floor(fraction * durationMs);
      onSeek(timeMs);
    },
    [durationMs, onSeek],
  );

  // Handle scrub bar drag
  const handleScrubMouseDown = useCallback(
    (e: MouseEvent) => {
      handleScrubClick(e);
      const onMove = (ev: MouseEvent) => {
        const target = (e.currentTarget as HTMLDivElement);
        const rect = target.getBoundingClientRect();
        const fraction = Math.max(0, Math.min(1, (ev.clientX - rect.left) / rect.width));
        const timeMs = Math.floor(fraction * durationMs);
        onSeek(timeMs);
      };
      const onUp = () => {
        document.removeEventListener("mousemove", onMove);
        document.removeEventListener("mouseup", onUp);
      };
      document.addEventListener("mousemove", onMove);
      document.addEventListener("mouseup", onUp);
    },
    [durationMs, onSeek, handleScrubClick],
  );

  const progress = durationMs > 0 ? (currentTimeMs / durationMs) * 100 : 0;

  return (
    <div class="video-preview" ref={containerRef} style={{ width: "100%", height: "100%" }}>
      <canvas
        ref={canvasRef}
        style={{
          display: "block",
          margin: "0 auto",
          background: "#000",
          borderRadius: "4px",
        }}
      />

      {/* Transport controls */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: "8px",
          padding: "8px 10px",
          marginTop: "4px",
        }}
      >
        <button
          onClick={onTogglePlay}
          style={{
            padding: "4px 16px",
            border: "1px solid #0f3460",
            borderRadius: "4px",
            background: isPlaying ? "#e94560" : "#1a1a2e",
            color: "#e0e0e0",
            cursor: "pointer",
            fontSize: "13px",
            minWidth: "60px",
          }}
        >
          {isPlaying ? "Pause" : "Play"}
        </button>

        <span style={{ fontSize: "12px", fontFamily: "monospace", minWidth: "100px" }}>
          {formatTime(currentTimeMs)} / {formatTime(durationMs)}
        </span>

        {/* Scrub bar */}
        <div
          onMouseDown={handleScrubMouseDown}
          style={{
            flex: 1,
            height: "6px",
            background: "#0f3460",
            borderRadius: "3px",
            cursor: "pointer",
            position: "relative",
          }}
        >
          <div
            style={{
              width: `${progress}%`,
              height: "100%",
              background: "#e94560",
              borderRadius: "3px",
              transition: isPlaying ? "none" : "width 0.1s",
            }}
          />
          <div
            style={{
              position: "absolute",
              left: `${progress}%`,
              top: "-3px",
              width: "12px",
              height: "12px",
              background: "#e94560",
              borderRadius: "50%",
              transform: "translateX(-50%)",
              border: "2px solid #fff",
            }}
          />
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
pnpm exec tsc --noEmit
```

Expected: No type errors.

- [ ] **Step 3: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src/components/editor/VideoPreview.tsx
git commit -m "feat: add VideoPreview component with canvas rendering and scrub bar"
```

---

## Task 9: Frontend — Timeline Component

**Files:**
- Create: `src/components/editor/Timeline.tsx`

- [ ] **Step 1: Create the timeline component**

Create `src/components/editor/Timeline.tsx`:
```tsx
import styles from "./Timeline.module.scss"; // SCSS module — semantic class names, no inline styles for layout
import { useRef, useCallback, useMemo } from "preact/hooks";
import type {
  EditProject,
  Track,
  Clip,
  VideoProbeResult,
} from "../../lib/types";
import type { EditorSelection } from "../../stores/editor";

interface TimelineProps {
  project: EditProject;
  currentTimeMs: number;
  selection: EditorSelection;
  onSeek: (timeMs: number) => void;
  onSelect: (trackId: string | null, clipId: string | null) => void;
  probe: VideoProbeResult | null;
}

// Pixels per millisecond at default zoom
const DEFAULT_PX_PER_MS = 0.1;

export default function Timeline({
  project,
  currentTimeMs,
  selection,
  onSeek,
  onSelect,
  probe,
}: TimelineProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const pxPerMs = DEFAULT_PX_PER_MS;

  const totalDurationMs = useMemo(() => {
    const projectDur = project.tracks
      .flatMap((t) => t.clips)
      .reduce((max, clip) => {
        const end = clip.timeline_start_ms + (clip.source_range.end_ms - clip.source_range.start_ms);
        return Math.max(max, end);
      }, 0);
    return Math.max(projectDur, probe?.duration_ms ?? 0);
  }, [project, probe]);

  const totalWidth = Math.max(totalDurationMs * pxPerMs, 600);

  // Handle click on the ruler to seek
  const handleRulerClick = useCallback(
    (e: MouseEvent) => {
      const target = e.currentTarget as HTMLDivElement;
      const rect = target.getBoundingClientRect();
      const scrollLeft = containerRef.current?.scrollLeft ?? 0;
      const x = e.clientX - rect.left + scrollLeft;
      const timeMs = Math.max(0, Math.floor(x / pxPerMs));
      onSeek(timeMs);
    },
    [pxPerMs, onSeek],
  );

  // Handle click on a clip
  const handleClipClick = useCallback(
    (trackId: string, clipId: string, e: MouseEvent) => {
      e.stopPropagation();
      onSelect(trackId, clipId);
    },
    [onSelect],
  );

  // Generate time ruler marks
  const rulerMarks = useMemo(() => {
    const marks: { x: number; label: string }[] = [];
    // Place a mark every second
    const intervalMs = 1000;
    for (let ms = 0; ms <= totalDurationMs; ms += intervalMs) {
      const x = ms * pxPerMs;
      const sec = ms / 1000;
      const min = Math.floor(sec / 60);
      const s = Math.floor(sec % 60);
      marks.push({
        x,
        label: `${min}:${s.toString().padStart(2, "0")}`,
      });
    }
    return marks;
  }, [totalDurationMs, pxPerMs]);

  const playheadX = currentTimeMs * pxPerMs;

  const trackKindColor = (kind: string) => {
    switch (kind) {
      case "video":
        return "#2d6a9f";
      case "audio":
        return "#4caf50";
      case "overlay":
        return "#ff9800";
      default:
        return "#555";
    }
  };

  return (
    <div class="timeline-container" style={{ flex: 1, overflow: "hidden", display: "flex", flexDirection: "column" }}>
      {/* Track labels (left side) + scrollable area (right side) */}
      <div style={{ display: "flex", flex: 1, minHeight: 0 }}>
        {/* Track labels */}
        <div
          style={{
            width: "120px",
            flexShrink: 0,
            borderRight: "1px solid #0f3460",
            background: "#16213e",
          }}
        >
          {/* Ruler label */}
          <div
            style={{
              height: "24px",
              borderBottom: "1px solid #0f3460",
              padding: "2px 8px",
              fontSize: "11px",
              color: "#888",
              lineHeight: "20px",
            }}
          >
            Time
          </div>
          {project.tracks.map((track) => (
            <div
              key={track.id}
              style={{
                height: "40px",
                padding: "4px 8px",
                borderBottom: "1px solid #0f3460",
                display: "flex",
                alignItems: "center",
                gap: "6px",
                fontSize: "12px",
              }}
            >
              <div
                style={{
                  width: "8px",
                  height: "8px",
                  borderRadius: "50%",
                  background: trackKindColor(track.kind),
                  flexShrink: 0,
                }}
              />
              <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                {track.name}
              </span>
              {track.muted && (
                <span style={{ fontSize: "10px", color: "#888" }}>M</span>
              )}
              {track.locked && (
                <span style={{ fontSize: "10px", color: "#888" }}>L</span>
              )}
            </div>
          ))}
        </div>

        {/* Scrollable timeline area */}
        <div
          ref={containerRef}
          style={{
            flex: 1,
            overflowX: "auto",
            overflowY: "hidden",
            position: "relative",
          }}
        >
          {/* Time ruler */}
          <div
            onClick={handleRulerClick}
            style={{
              height: "24px",
              position: "relative",
              background: "#111",
              borderBottom: "1px solid #0f3460",
              cursor: "crosshair",
              width: `${totalWidth}px`,
            }}
          >
            {rulerMarks.map((mark) => (
              <div
                key={mark.x}
                style={{
                  position: "absolute",
                  left: `${mark.x}px`,
                  top: 0,
                  height: "100%",
                  borderLeft: "1px solid #333",
                }}
              >
                <span
                  style={{
                    position: "absolute",
                    left: "3px",
                    top: "2px",
                    fontSize: "10px",
                    color: "#888",
                    whiteSpace: "nowrap",
                  }}
                >
                  {mark.label}
                </span>
              </div>
            ))}
          </div>

          {/* Track rows with clips */}
          {project.tracks.map((track) => (
            <div
              key={track.id}
              onClick={() => onSelect(track.id, null)}
              style={{
                height: "40px",
                position: "relative",
                borderBottom: "1px solid #0f3460",
                background:
                  selection.trackId === track.id && !selection.clipId
                    ? "rgba(233, 69, 96, 0.1)"
                    : "transparent",
                width: `${totalWidth}px`,
              }}
            >
              {track.clips.map((clip) => {
                const clipDuration =
                  clip.source_range.end_ms - clip.source_range.start_ms;
                const left = clip.timeline_start_ms * pxPerMs;
                const clipWidth = Math.max(clipDuration * pxPerMs, 4);
                const isSelected =
                  selection.trackId === track.id &&
                  selection.clipId === clip.id;

                return (
                  <div
                    key={clip.id}
                    onClick={(e: MouseEvent) => handleClipClick(track.id, clip.id, e)}
                    style={{
                      position: "absolute",
                      left: `${left}px`,
                      top: "4px",
                      width: `${clipWidth}px`,
                      height: "32px",
                      background: isSelected
                        ? trackKindColor(track.kind)
                        : `${trackKindColor(track.kind)}aa`,
                      border: isSelected
                        ? "2px solid #e94560"
                        : "1px solid rgba(255,255,255,0.2)",
                      borderRadius: "3px",
                      cursor: "pointer",
                      overflow: "hidden",
                      display: "flex",
                      alignItems: "center",
                      padding: "0 4px",
                    }}
                  >
                    <span
                      style={{
                        fontSize: "10px",
                        whiteSpace: "nowrap",
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                      }}
                    >
                      {clip.source_path.split("/").pop()}
                    </span>

                    {/* Trim handles */}
                    <div
                      style={{
                        position: "absolute",
                        left: 0,
                        top: 0,
                        width: "6px",
                        height: "100%",
                        cursor: "col-resize",
                        background: "rgba(255,255,255,0.15)",
                      }}
                    />
                    <div
                      style={{
                        position: "absolute",
                        right: 0,
                        top: 0,
                        width: "6px",
                        height: "100%",
                        cursor: "col-resize",
                        background: "rgba(255,255,255,0.15)",
                      }}
                    />
                  </div>
                );
              })}
            </div>
          ))}

          {/* Playhead */}
          <div
            style={{
              position: "absolute",
              left: `${playheadX}px`,
              top: 0,
              width: "2px",
              height: "100%",
              background: "#e94560",
              pointerEvents: "none",
              zIndex: 10,
            }}
          >
            {/* Playhead handle */}
            <div
              style={{
                position: "absolute",
                top: 0,
                left: "-5px",
                width: "12px",
                height: "12px",
                background: "#e94560",
                clipPath: "polygon(0 0, 100% 0, 50% 100%)",
              }}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
pnpm exec tsc --noEmit
```

Expected: No type errors.

- [ ] **Step 3: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src/components/editor/Timeline.tsx
git commit -m "feat: add Timeline component with single video + audio track, clips, playhead, and ruler"
```

---

## Task 10: Frontend — ImageEditor Component (Fabric.js)

**Files:**
- Create: `src/components/editor/ImageEditor.tsx`

- [ ] **Step 1: Install Fabric.js**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
pnpm add fabric@^6
```

- [ ] **Step 2: Create the image editor component**

Create `src/components/editor/ImageEditor.tsx`:
```tsx
import styles from "./ImageEditor.module.scss"; // SCSS module — semantic class names, no inline styles for layout
import { useRef, useEffect, useCallback, useState } from "preact/hooks";

interface ImageEditorProps {
  imageSrc: string;
  width: number;
  height: number;
  onSave: (dataUrl: string) => void;
}

type AnnotationTool = "select" | "text" | "rect" | "circle" | "arrow" | "blur" | "freehand";

export default function ImageEditor({
  imageSrc,
  width,
  height,
  onSave,
}: ImageEditorProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const fabricRef = useRef<any>(null);
  const [activeTool, setActiveTool] = useState<AnnotationTool>("select");
  const [strokeColor, setStrokeColor] = useState("#e94560");
  const [fillColor, setFillColor] = useState("transparent");
  const [strokeWidth, setStrokeWidth] = useState(3);
  const [fontSize, setFontSize] = useState(24);

  // Initialize Fabric.js canvas
  useEffect(() => {
    let canvas: any = null;

    (async () => {
      const fabric = await import("fabric");
      if (!canvasRef.current) return;

      canvas = new fabric.Canvas(canvasRef.current, {
        width,
        height,
        backgroundColor: "#000",
        selection: true,
      });
      fabricRef.current = canvas;

      // Load background image
      const img = await fabric.FabricImage.fromURL(imageSrc);
      if (!img) return;

      // Scale image to fit canvas
      const scaleX = width / (img.width ?? width);
      const scaleY = height / (img.height ?? height);
      const scale = Math.min(scaleX, scaleY);
      img.set({
        scaleX: scale,
        scaleY: scale,
        originX: "left",
        originY: "top",
        selectable: false,
        evented: false,
      });
      canvas.setBackgroundImage(img);
      canvas.renderAll();
    })();

    return () => {
      if (canvas) {
        canvas.dispose();
        fabricRef.current = null;
      }
    };
  }, [imageSrc, width, height]);

  // Add a rectangle annotation
  const addRect = useCallback(() => {
    const canvas = fabricRef.current;
    if (!canvas) return;

    (async () => {
      const fabric = await import("fabric");
      const rect = new fabric.Rect({
        left: 100,
        top: 100,
        width: 200,
        height: 150,
        fill: fillColor === "transparent" ? "" : fillColor,
        stroke: strokeColor,
        strokeWidth,
        strokeUniform: true,
      });
      canvas.add(rect);
      canvas.setActiveObject(rect);
      canvas.renderAll();
    })();
  }, [strokeColor, fillColor, strokeWidth]);

  // Add a circle annotation
  const addCircle = useCallback(() => {
    const canvas = fabricRef.current;
    if (!canvas) return;

    (async () => {
      const fabric = await import("fabric");
      const circle = new fabric.Circle({
        left: 150,
        top: 150,
        radius: 80,
        fill: fillColor === "transparent" ? "" : fillColor,
        stroke: strokeColor,
        strokeWidth,
        strokeUniform: true,
      });
      canvas.add(circle);
      canvas.setActiveObject(circle);
      canvas.renderAll();
    })();
  }, [strokeColor, fillColor, strokeWidth]);

  // Add a text annotation
  const addText = useCallback(() => {
    const canvas = fabricRef.current;
    if (!canvas) return;

    (async () => {
      const fabric = await import("fabric");
      const text = new fabric.IText("Text", {
        left: 100,
        top: 100,
        fontSize,
        fill: strokeColor,
        fontFamily: "Sans-serif",
        editable: true,
      });
      canvas.add(text);
      canvas.setActiveObject(text);
      canvas.renderAll();
    })();
  }, [strokeColor, fontSize]);

  // Add an arrow (line with arrowhead)
  const addArrow = useCallback(() => {
    const canvas = fabricRef.current;
    if (!canvas) return;

    (async () => {
      const fabric = await import("fabric");
      const line = new fabric.Line([100, 200, 300, 200], {
        stroke: strokeColor,
        strokeWidth: strokeWidth + 1,
        strokeLineCap: "round",
      });
      // Arrowhead triangle
      const triangle = new fabric.Triangle({
        left: 300,
        top: 200,
        width: 15,
        height: 20,
        fill: strokeColor,
        angle: 90,
        originX: "center",
        originY: "center",
      });
      const group = new fabric.Group([line, triangle], {
        left: 100,
        top: 180,
      });
      canvas.add(group);
      canvas.setActiveObject(group);
      canvas.renderAll();
    })();
  }, [strokeColor, strokeWidth]);

  // Enable freehand drawing
  useEffect(() => {
    const canvas = fabricRef.current;
    if (!canvas) return;

    if (activeTool === "freehand") {
      canvas.isDrawingMode = true;
      canvas.freeDrawingBrush.color = strokeColor;
      canvas.freeDrawingBrush.width = strokeWidth;
    } else {
      canvas.isDrawingMode = false;
    }
  }, [activeTool, strokeColor, strokeWidth]);

  // Delete selected objects
  const deleteSelected = useCallback(() => {
    const canvas = fabricRef.current;
    if (!canvas) return;

    const active = canvas.getActiveObjects();
    if (active && active.length > 0) {
      active.forEach((obj: any) => canvas.remove(obj));
      canvas.discardActiveObject();
      canvas.renderAll();
    }
  }, []);

  // Export canvas as data URL
  const handleSave = useCallback(() => {
    const canvas = fabricRef.current;
    if (!canvas) return;

    const dataUrl = canvas.toDataURL({
      format: "png",
      quality: 1,
      multiplier: 1,
    });
    onSave(dataUrl);
  }, [onSave]);

  // Handle tool selection
  const handleToolClick = useCallback(
    (tool: AnnotationTool) => {
      setActiveTool(tool);
      switch (tool) {
        case "rect":
          addRect();
          setActiveTool("select");
          break;
        case "circle":
          addCircle();
          setActiveTool("select");
          break;
        case "text":
          addText();
          setActiveTool("select");
          break;
        case "arrow":
          addArrow();
          setActiveTool("select");
          break;
        case "freehand":
          // Handled by the useEffect above
          break;
        case "select":
        default:
          break;
      }
    },
    [addRect, addCircle, addText, addArrow],
  );

  const tools: { id: AnnotationTool; label: string }[] = [
    { id: "select", label: "Select" },
    { id: "rect", label: "Rectangle" },
    { id: "circle", label: "Circle" },
    { id: "arrow", label: "Arrow" },
    { id: "text", label: "Text" },
    { id: "freehand", label: "Draw" },
  ];

  return (
    <div class="image-editor" style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
      {/* Toolbar */}
      <div style={{ display: "flex", gap: "4px", flexWrap: "wrap", padding: "4px 8px" }}>
        {tools.map((tool) => (
          <button
            key={tool.id}
            onClick={() => handleToolClick(tool.id)}
            style={{
              padding: "4px 10px",
              fontSize: "12px",
              border: activeTool === tool.id ? "2px solid #e94560" : "1px solid #0f3460",
              borderRadius: "4px",
              background: activeTool === tool.id ? "#e9456033" : "#1a1a2e",
              color: "#e0e0e0",
              cursor: "pointer",
            }}
          >
            {tool.label}
          </button>
        ))}

        <span style={{ borderLeft: "1px solid #333", margin: "0 4px" }} />

        {/* Color picker */}
        <label style={{ display: "flex", alignItems: "center", gap: "4px", fontSize: "12px" }}>
          Color:
          <input
            type="color"
            value={strokeColor}
            onInput={(e) => setStrokeColor((e.target as HTMLInputElement).value)}
            style={{ width: "28px", height: "24px", border: "none", cursor: "pointer" }}
          />
        </label>

        {/* Stroke width */}
        <label style={{ display: "flex", alignItems: "center", gap: "4px", fontSize: "12px" }}>
          Size:
          <input
            type="range"
            min={1}
            max={20}
            value={strokeWidth}
            onInput={(e) => setStrokeWidth(Number((e.target as HTMLInputElement).value))}
            style={{ width: "60px" }}
          />
        </label>

        <span style={{ borderLeft: "1px solid #333", margin: "0 4px" }} />

        <button
          onClick={deleteSelected}
          style={{
            padding: "4px 10px",
            fontSize: "12px",
            border: "1px solid #e94560",
            borderRadius: "4px",
            background: "#1a1a2e",
            color: "#e94560",
            cursor: "pointer",
          }}
        >
          Delete
        </button>

        <button
          onClick={handleSave}
          style={{
            padding: "4px 10px",
            fontSize: "12px",
            border: "1px solid #4caf50",
            borderRadius: "4px",
            background: "#1a1a2e",
            color: "#4caf50",
            cursor: "pointer",
            marginLeft: "auto",
          }}
        >
          Save
        </button>
      </div>

      {/* Canvas */}
      <div style={{ display: "flex", justifyContent: "center", background: "#0a0a1a", padding: "8px" }}>
        <canvas ref={canvasRef} />
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Verify it compiles**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
pnpm exec tsc --noEmit
```

Expected: No type errors.

- [ ] **Step 4: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src/components/editor/ImageEditor.tsx package.json pnpm-lock.yaml
git commit -m "feat: add Fabric.js ImageEditor with rectangle, circle, text, arrow, and freehand tools"
```

---

## Task 11: Frontend — AudioPanel Component

**Files:**
- Create: `src/components/editor/AudioPanel.tsx`

- [ ] **Step 1: Install wavesurfer.js**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
pnpm add wavesurfer.js@^7
```

- [ ] **Step 2: Create the audio panel component**

Create `src/components/editor/AudioPanel.tsx`:
```tsx
import styles from "./AudioPanel.module.scss"; // SCSS module — semantic class names, no inline styles for layout
import { useRef, useEffect, useState, useCallback } from "preact/hooks";
import type { EditProject, EditOperation } from "../../lib/types";

interface AudioPanelProps {
  project: EditProject;
  onApplyOperation: (
    operation: EditOperation,
    trackId?: string,
    clipId?: string,
  ) => void;
}

export default function AudioPanel({ project, onApplyOperation }: AudioPanelProps) {
  const waveformRef = useRef<HTMLDivElement>(null);
  const wavesurferRef = useRef<any>(null);
  const [isWavesurferReady, setIsWavesurferReady] = useState(false);

  // Volume state
  const [volume, setVolume] = useState(1.0);
  // // TODO(v2): Compressor UI — state kept as stub for future use.
  // const [compThreshold, setCompThreshold] = useState(-20);
  // const [compRatio, setCompRatio] = useState(4);
  // const [compAttack, setCompAttack] = useState(5);
  // const [compRelease, setCompRelease] = useState(50);

  // Find the audio track
  const audioTrack = project.tracks.find((t) => t.kind === "audio");
  const audioClip = audioTrack?.clips[0];

  // Initialize wavesurfer
  useEffect(() => {
    if (!waveformRef.current || !audioClip) return;

    let ws: any = null;
    let cancelled = false;

    (async () => {
      const WaveSurfer = (await import("wavesurfer.js")).default;
      if (cancelled || !waveformRef.current) return;

      ws = WaveSurfer.create({
        container: waveformRef.current,
        waveColor: "#4caf5088",
        progressColor: "#4caf50",
        cursorColor: "#e94560",
        height: 60,
        barWidth: 2,
        barGap: 1,
        barRadius: 2,
        normalize: true,
        interact: true,
        backend: "WebAudio",
      });

      ws.on("ready", () => {
        if (!cancelled) setIsWavesurferReady(true);
      });

      // Load the audio file
      // wavesurfer.js can load from a file URL. Tauri converts asset paths.
      try {
        await ws.load(audioClip.source_path);
      } catch (err) {
        console.warn("Could not load audio waveform:", err);
      }

      wavesurferRef.current = ws;
    })();

    return () => {
      cancelled = true;
      if (ws) {
        ws.destroy();
        wavesurferRef.current = null;
        setIsWavesurferReady(false);
      }
    };
  }, [audioClip?.source_path]);

  // Apply volume operation
  const handleApplyVolume = useCallback(() => {
    if (!audioTrack || !audioClip) return;
    onApplyOperation(
      { type: "AudioVolume", config: { multiplier: volume } },
      audioTrack.id,
      audioClip.id,
    );
  }, [volume, audioTrack, audioClip, onApplyOperation]);

  // Apply normalize
  const handleNormalize = useCallback(() => {
    if (!audioTrack || !audioClip) return;
    onApplyOperation(
      { type: "AudioNormalize", config: null },
      audioTrack.id,
      audioClip.id,
    );
  }, [audioTrack, audioClip, onApplyOperation]);

  // // TODO(v2): Noise reduction UI — stub for future use.
  // const handleNoiseReduce = useCallback(() => {
  //   if (!audioTrack || !audioClip) return;
  //   onApplyOperation(
  //     { type: "AudioNoiseReduce", config: null },
  //     audioTrack.id,
  //     audioClip.id,
  //   );
  // }, [audioTrack, audioClip, onApplyOperation]);

  // // TODO(v2): Compressor UI — stub for future use.
  // const handleCompress = useCallback(() => {
  //   if (!audioTrack || !audioClip) return;
  //   onApplyOperation(
  //     {
  //       type: "AudioCompress",
  //       config: {
  //         threshold_db: compThreshold,
  //         ratio: compRatio,
  //         attack_ms: compAttack,
  //         release_ms: compRelease,
  //       },
  //     },
  //     audioTrack.id,
  //     audioClip.id,
  //   );
  // }, [compThreshold, compRatio, compAttack, compRelease, audioTrack, audioClip, onApplyOperation]);

  if (!audioTrack) {
    return (
      <div style={{ padding: "8px", fontSize: "12px", color: "#888" }}>
        No audio track in this project.
      </div>
    );
  }

  return (
    <div
      class="audio-panel"
      style={{
        display: "flex",
        flexDirection: "column",
        gap: "4px",
        padding: "4px 8px",
        fontSize: "12px",
      }}
    >
      {/* Waveform */}
      <div
        ref={waveformRef}
        style={{
          width: "100%",
          height: "60px",
          background: "#111",
          borderRadius: "4px",
        }}
      />

      {/* Audio controls */}
      <div style={{ display: "flex", gap: "12px", flexWrap: "wrap", alignItems: "center" }}>
        {/* Volume */}
        <div style={{ display: "flex", alignItems: "center", gap: "4px" }}>
          <label>Volume:</label>
          <input
            type="range"
            min={0}
            max={3}
            step={0.05}
            value={volume}
            onInput={(e) => setVolume(Number((e.target as HTMLInputElement).value))}
            style={{ width: "80px" }}
          />
          <span style={{ minWidth: "35px" }}>{(volume * 100).toFixed(0)}%</span>
          <button
            onClick={handleApplyVolume}
            style={{
              padding: "2px 8px",
              border: "1px solid #0f3460",
              borderRadius: "3px",
              background: "#1a1a2e",
              color: "#e0e0e0",
              cursor: "pointer",
              fontSize: "11px",
            }}
          >
            Apply
          </button>
        </div>

        {/* Quick actions */}
        <button
          onClick={handleNormalize}
          style={{
            padding: "2px 8px",
            border: "1px solid #4caf50",
            borderRadius: "3px",
            background: "#1a1a2e",
            color: "#4caf50",
            cursor: "pointer",
            fontSize: "11px",
          }}
        >
          Normalize
        </button>

        {/* // TODO(v2): Noise Reduce button — not exposed in v1 UI */}
        {/* // TODO(v2): Compress button — not exposed in v1 UI */}
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Verify it compiles**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
pnpm exec tsc --noEmit
```

Expected: No type errors.

- [ ] **Step 4: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src/components/editor/AudioPanel.tsx package.json pnpm-lock.yaml
git commit -m "feat: add AudioPanel with wavesurfer.js waveform, volume, and normalize (v1)"
```

---

## Task 12: Frontend — ToolPanel and PropertiesPanel

**Files:**
- Create: `src/components/editor/ToolPanel.tsx`
- Create: `src/components/editor/PropertiesPanel.tsx`

- [ ] **Step 1: Create the tool panel**

Create `src/components/editor/ToolPanel.tsx`:
```tsx
import styles from "./ToolPanel.module.scss"; // SCSS module — semantic class names, no inline styles for layout
import { useState, useCallback } from "preact/hooks";
import type {
  EditProject,
  EditOperation,
  VideoProbeResult,
} from "../../lib/types";

interface ToolPanelProps {
  project: EditProject;
  onApplyOperation: (
    operation: EditOperation,
    trackId?: string,
    clipId?: string,
  ) => void;
  currentTimeMs: number;
  probe: VideoProbeResult | null;
}

export default function ToolPanel({
  project,
  onApplyOperation,
  currentTimeMs,
  probe,
}: ToolPanelProps) {
  // Text overlay state
  const [textContent, setTextContent] = useState("Sample Text");
  const [textX, setTextX] = useState(100);
  const [textY, setTextY] = useState(100);
  const [textFontSize, setTextFontSize] = useState(32);
  const [textColor, setTextColor] = useState("#FFFFFF");
  const [textBgColor, setTextBgColor] = useState("");
  const [textDuration, setTextDuration] = useState(5000);

  // Blur region state
  const [blurX, setBlurX] = useState(0);
  const [blurY, setBlurY] = useState(0);
  const [blurW, setBlurW] = useState(200);
  const [blurH, setBlurH] = useState(200);
  const [blurStrength, setBlurStrength] = useState(20);
  const [blurDuration, setBlurDuration] = useState(5000);

  // Speed state
  const [speedFactor, setSpeedFactor] = useState(1.0);

  // Crop state
  const [cropX, setCropX] = useState(0);
  const [cropY, setCropY] = useState(0);
  const [cropW, setCropW] = useState(project.width);
  const [cropH, setCropH] = useState(project.height);

  // Scale state
  const [scaleW, setScaleW] = useState(project.width);
  const [scaleH, setScaleH] = useState(project.height);

  const handleAddText = useCallback(() => {
    onApplyOperation({
      type: "AddTextOverlay",
      config: {
        text: textContent,
        x: textX,
        y: textY,
        font_size: textFontSize,
        font_family: "Sans",
        color: textColor,
        background_color: textBgColor,
        start_ms: currentTimeMs,
        end_ms: currentTimeMs + textDuration,
      },
    });
  }, [
    textContent, textX, textY, textFontSize, textColor, textBgColor,
    textDuration, currentTimeMs, onApplyOperation,
  ]);

  const handleAddBlur = useCallback(() => {
    onApplyOperation({
      type: "AddBlurRegion",
      config: {
        x: blurX,
        y: blurY,
        width: blurW,
        height: blurH,
        strength: blurStrength,
        start_ms: currentTimeMs,
        end_ms: currentTimeMs + blurDuration,
      },
    });
  }, [blurX, blurY, blurW, blurH, blurStrength, blurDuration, currentTimeMs, onApplyOperation]);

  const handleSpeed = useCallback(() => {
    onApplyOperation({ type: "Speed", config: { factor: speedFactor } });
  }, [speedFactor, onApplyOperation]);

  const handleCrop = useCallback(() => {
    onApplyOperation({
      type: "Crop",
      config: { x: cropX, y: cropY, width: cropW, height: cropH },
    });
  }, [cropX, cropY, cropW, cropH, onApplyOperation]);

  const handleScale = useCallback(() => {
    onApplyOperation({
      type: "Scale",
      config: { width: scaleW, height: scaleH },
    });
  }, [scaleW, scaleH, onApplyOperation]);

  const sectionStyle = {
    padding: "8px",
    borderBottom: "1px solid #0f3460",
  };
  const labelStyle = {
    display: "block",
    fontSize: "11px",
    color: "#888",
    marginBottom: "2px",
    marginTop: "4px",
  };
  const inputStyle = {
    width: "100%",
    padding: "3px 6px",
    border: "1px solid #0f3460",
    borderRadius: "3px",
    background: "#1a1a2e",
    color: "#e0e0e0",
    fontSize: "12px",
    boxSizing: "border-box" as const,
  };
  const btnStyle = {
    width: "100%",
    padding: "5px",
    marginTop: "6px",
    border: "1px solid #0f3460",
    borderRadius: "4px",
    background: "#1a1a2e",
    color: "#e0e0e0",
    cursor: "pointer",
    fontSize: "12px",
  };

  return (
    <div class="tool-panel" style={{ fontSize: "12px" }}>
      <div style={{ padding: "8px", fontWeight: 600, borderBottom: "1px solid #0f3460" }}>
        Tools
      </div>

      {/* Text Overlay */}
      <div style={sectionStyle}>
        <div style={{ fontWeight: 600, marginBottom: "4px" }}>Text Overlay</div>
        <label style={labelStyle}>Text</label>
        <input
          type="text"
          value={textContent}
          onInput={(e) => setTextContent((e.target as HTMLInputElement).value)}
          style={inputStyle}
        />
        <div style={{ display: "flex", gap: "4px" }}>
          <div style={{ flex: 1 }}>
            <label style={labelStyle}>X</label>
            <input
              type="number"
              value={textX}
              onInput={(e) => setTextX(Number((e.target as HTMLInputElement).value))}
              style={inputStyle}
            />
          </div>
          <div style={{ flex: 1 }}>
            <label style={labelStyle}>Y</label>
            <input
              type="number"
              value={textY}
              onInput={(e) => setTextY(Number((e.target as HTMLInputElement).value))}
              style={inputStyle}
            />
          </div>
        </div>
        <div style={{ display: "flex", gap: "4px" }}>
          <div style={{ flex: 1 }}>
            <label style={labelStyle}>Size</label>
            <input
              type="number"
              value={textFontSize}
              onInput={(e) => setTextFontSize(Number((e.target as HTMLInputElement).value))}
              style={inputStyle}
            />
          </div>
          <div style={{ flex: 1 }}>
            <label style={labelStyle}>Color</label>
            <input
              type="color"
              value={textColor}
              onInput={(e) => setTextColor((e.target as HTMLInputElement).value)}
              style={{ ...inputStyle, height: "26px", padding: "1px" }}
            />
          </div>
        </div>
        <label style={labelStyle}>Duration (ms)</label>
        <input
          type="number"
          value={textDuration}
          onInput={(e) => setTextDuration(Number((e.target as HTMLInputElement).value))}
          style={inputStyle}
        />
        <button onClick={handleAddText} style={btnStyle}>
          Add Text
        </button>
      </div>

      {/* Blur Region */}
      <div style={sectionStyle}>
        <div style={{ fontWeight: 600, marginBottom: "4px" }}>Blur Region</div>
        <div style={{ display: "flex", gap: "4px" }}>
          <div style={{ flex: 1 }}>
            <label style={labelStyle}>X</label>
            <input
              type="number"
              value={blurX}
              onInput={(e) => setBlurX(Number((e.target as HTMLInputElement).value))}
              style={inputStyle}
            />
          </div>
          <div style={{ flex: 1 }}>
            <label style={labelStyle}>Y</label>
            <input
              type="number"
              value={blurY}
              onInput={(e) => setBlurY(Number((e.target as HTMLInputElement).value))}
              style={inputStyle}
            />
          </div>
        </div>
        <div style={{ display: "flex", gap: "4px" }}>
          <div style={{ flex: 1 }}>
            <label style={labelStyle}>Width</label>
            <input
              type="number"
              value={blurW}
              onInput={(e) => setBlurW(Number((e.target as HTMLInputElement).value))}
              style={inputStyle}
            />
          </div>
          <div style={{ flex: 1 }}>
            <label style={labelStyle}>Height</label>
            <input
              type="number"
              value={blurH}
              onInput={(e) => setBlurH(Number((e.target as HTMLInputElement).value))}
              style={inputStyle}
            />
          </div>
        </div>
        <label style={labelStyle}>Strength</label>
        <input
          type="range"
          min={1}
          max={50}
          value={blurStrength}
          onInput={(e) => setBlurStrength(Number((e.target as HTMLInputElement).value))}
          style={{ width: "100%" }}
        />
        <label style={labelStyle}>Duration (ms)</label>
        <input
          type="number"
          value={blurDuration}
          onInput={(e) => setBlurDuration(Number((e.target as HTMLInputElement).value))}
          style={inputStyle}
        />
        <button onClick={handleAddBlur} style={btnStyle}>
          Add Blur
        </button>
      </div>

      {/* Crop */}
      <div style={sectionStyle}>
        <div style={{ fontWeight: 600, marginBottom: "4px" }}>Crop</div>
        <div style={{ display: "flex", gap: "4px" }}>
          <div style={{ flex: 1 }}>
            <label style={labelStyle}>X</label>
            <input type="number" value={cropX} onInput={(e) => setCropX(Number((e.target as HTMLInputElement).value))} style={inputStyle} />
          </div>
          <div style={{ flex: 1 }}>
            <label style={labelStyle}>Y</label>
            <input type="number" value={cropY} onInput={(e) => setCropY(Number((e.target as HTMLInputElement).value))} style={inputStyle} />
          </div>
        </div>
        <div style={{ display: "flex", gap: "4px" }}>
          <div style={{ flex: 1 }}>
            <label style={labelStyle}>Width</label>
            <input type="number" value={cropW} onInput={(e) => setCropW(Number((e.target as HTMLInputElement).value))} style={inputStyle} />
          </div>
          <div style={{ flex: 1 }}>
            <label style={labelStyle}>Height</label>
            <input type="number" value={cropH} onInput={(e) => setCropH(Number((e.target as HTMLInputElement).value))} style={inputStyle} />
          </div>
        </div>
        <button onClick={handleCrop} style={btnStyle}>
          Apply Crop
        </button>
      </div>

      {/* Scale */}
      <div style={sectionStyle}>
        <div style={{ fontWeight: 600, marginBottom: "4px" }}>Scale</div>
        <div style={{ display: "flex", gap: "4px" }}>
          <div style={{ flex: 1 }}>
            <label style={labelStyle}>Width</label>
            <input type="number" value={scaleW} onInput={(e) => setScaleW(Number((e.target as HTMLInputElement).value))} style={inputStyle} />
          </div>
          <div style={{ flex: 1 }}>
            <label style={labelStyle}>Height</label>
            <input type="number" value={scaleH} onInput={(e) => setScaleH(Number((e.target as HTMLInputElement).value))} style={inputStyle} />
          </div>
        </div>
        <button onClick={handleScale} style={btnStyle}>
          Apply Scale
        </button>
      </div>

      {/* Speed */}
      <div style={sectionStyle}>
        <div style={{ fontWeight: 600, marginBottom: "4px" }}>Speed</div>
        <label style={labelStyle}>Factor ({speedFactor.toFixed(2)}x)</label>
        <input
          type="range"
          min={0.25}
          max={4}
          step={0.25}
          value={speedFactor}
          onInput={(e) => setSpeedFactor(Number((e.target as HTMLInputElement).value))}
          style={{ width: "100%" }}
        />
        <button onClick={handleSpeed} style={btnStyle}>
          Apply Speed
        </button>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Create the properties panel**

Create `src/components/editor/PropertiesPanel.tsx`:
```tsx
import styles from "./PropertiesPanel.module.scss"; // SCSS module — semantic class names, no inline styles for layout
import { useMemo } from "preact/hooks";
import type { EditProject, EditOperation } from "../../lib/types";
import type { EditorSelection } from "../../stores/editor";

interface PropertiesPanelProps {
  project: EditProject;
  selection: EditorSelection;
  onApplyOperation: (
    operation: EditOperation,
    trackId?: string,
    clipId?: string,
  ) => void;
}

export default function PropertiesPanel({
  project,
  selection,
}: PropertiesPanelProps) {
  // Find the selected track and clip
  const selectedTrack = useMemo(() => {
    if (!selection.trackId) return null;
    return project.tracks.find((t) => t.id === selection.trackId) ?? null;
  }, [project, selection.trackId]);

  const selectedClip = useMemo(() => {
    if (!selectedTrack || !selection.clipId) return null;
    return selectedTrack.clips.find((c) => c.id === selection.clipId) ?? null;
  }, [selectedTrack, selection.clipId]);

  const sectionStyle = {
    padding: "8px",
    borderBottom: "1px solid #0f3460",
  };
  const rowStyle = {
    display: "flex",
    justifyContent: "space-between",
    padding: "2px 0",
    fontSize: "12px",
  };
  const labelStyle = { color: "#888" };
  const valueStyle = { color: "#e0e0e0", fontFamily: "monospace" };

  return (
    <div class="properties-panel" style={{ fontSize: "12px" }}>
      <div style={{ padding: "8px", fontWeight: 600, borderBottom: "1px solid #0f3460" }}>
        Properties
      </div>

      {/* Project info */}
      <div style={sectionStyle}>
        <div style={{ fontWeight: 600, marginBottom: "4px" }}>Project</div>
        <div style={rowStyle}>
          <span style={labelStyle}>Name</span>
          <span style={valueStyle}>{project.name}</span>
        </div>
        <div style={rowStyle}>
          <span style={labelStyle}>Size</span>
          <span style={valueStyle}>{project.width}x{project.height}</span>
        </div>
        <div style={rowStyle}>
          <span style={labelStyle}>FPS</span>
          <span style={valueStyle}>{project.fps.toFixed(2)}</span>
        </div>
        <div style={rowStyle}>
          <span style={labelStyle}>Tracks</span>
          <span style={valueStyle}>{project.tracks.length}</span>
        </div>
        <div style={rowStyle}>
          <span style={labelStyle}>Operations</span>
          <span style={valueStyle}>{project.global_operations.length}</span>
        </div>
      </div>

      {/* Selected track info */}
      {selectedTrack && (
        <div style={sectionStyle}>
          <div style={{ fontWeight: 600, marginBottom: "4px" }}>Track</div>
          <div style={rowStyle}>
            <span style={labelStyle}>Name</span>
            <span style={valueStyle}>{selectedTrack.name}</span>
          </div>
          <div style={rowStyle}>
            <span style={labelStyle}>Type</span>
            <span style={valueStyle}>{selectedTrack.kind}</span>
          </div>
          <div style={rowStyle}>
            <span style={labelStyle}>Clips</span>
            <span style={valueStyle}>{selectedTrack.clips.length}</span>
          </div>
          <div style={rowStyle}>
            <span style={labelStyle}>Muted</span>
            <span style={valueStyle}>{selectedTrack.muted ? "Yes" : "No"}</span>
          </div>
          <div style={rowStyle}>
            <span style={labelStyle}>Locked</span>
            <span style={valueStyle}>{selectedTrack.locked ? "Yes" : "No"}</span>
          </div>
        </div>
      )}

      {/* Selected clip info */}
      {selectedClip && (
        <div style={sectionStyle}>
          <div style={{ fontWeight: 600, marginBottom: "4px" }}>Clip</div>
          <div style={rowStyle}>
            <span style={labelStyle}>Source</span>
            <span
              style={{ ...valueStyle, maxWidth: "150px", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}
              title={selectedClip.source_path}
            >
              {selectedClip.source_path.split("/").pop()}
            </span>
          </div>
          <div style={rowStyle}>
            <span style={labelStyle}>Start</span>
            <span style={valueStyle}>{formatMs(selectedClip.source_range.start_ms)}</span>
          </div>
          <div style={rowStyle}>
            <span style={labelStyle}>End</span>
            <span style={valueStyle}>{formatMs(selectedClip.source_range.end_ms)}</span>
          </div>
          <div style={rowStyle}>
            <span style={labelStyle}>Duration</span>
            <span style={valueStyle}>
              {formatMs(selectedClip.source_range.end_ms - selectedClip.source_range.start_ms)}
            </span>
          </div>
          <div style={rowStyle}>
            <span style={labelStyle}>Timeline pos</span>
            <span style={valueStyle}>{formatMs(selectedClip.timeline_start_ms)}</span>
          </div>
          <div style={rowStyle}>
            <span style={labelStyle}>Operations</span>
            <span style={valueStyle}>{selectedClip.operations.length}</span>
          </div>

          {/* List operations on this clip */}
          {selectedClip.operations.length > 0 && (
            <div style={{ marginTop: "8px" }}>
              <div style={{ fontSize: "11px", color: "#888", marginBottom: "4px" }}>
                Applied operations:
              </div>
              {selectedClip.operations.map((op, i) => (
                <div
                  key={i}
                  style={{
                    padding: "2px 6px",
                    marginBottom: "2px",
                    background: "#0f3460",
                    borderRadius: "3px",
                    fontSize: "11px",
                  }}
                >
                  {op.type}
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* Global operations list */}
      {project.global_operations.length > 0 && (
        <div style={sectionStyle}>
          <div style={{ fontWeight: 600, marginBottom: "4px" }}>Global Operations</div>
          {project.global_operations.map((op, i) => (
            <div
              key={i}
              style={{
                padding: "3px 6px",
                marginBottom: "2px",
                background: "#0f3460",
                borderRadius: "3px",
                fontSize: "11px",
              }}
            >
              {op.type}
            </div>
          ))}
        </div>
      )}

      {/* No selection message */}
      {!selectedTrack && (
        <div style={{ padding: "12px", color: "#666", fontSize: "12px", textAlign: "center" }}>
          Select a track or clip to view its properties.
        </div>
      )}
    </div>
  );
}

function formatMs(ms: number): string {
  const totalSec = Math.floor(ms / 1000);
  const min = Math.floor(totalSec / 60);
  const sec = totalSec % 60;
  const millis = ms % 1000;
  return `${min}:${sec.toString().padStart(2, "0")}.${millis.toString().padStart(3, "0")}`;
}
```

- [ ] **Step 3: Verify it compiles**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
pnpm exec tsc --noEmit
```

Expected: No type errors.

- [ ] **Step 4: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src/components/editor/ToolPanel.tsx src/components/editor/PropertiesPanel.tsx
git commit -m "feat: add ToolPanel (text, blur, crop, scale, speed) and PropertiesPanel"
```

---

## Task 13: Integration — Wire Editor Page into App Routing

**Files:**
- Modify: `src/App.tsx` (add editor route)
- Modify: `src/pages/Home.tsx` (add "Edit" button that navigates to editor)

- [ ] **Step 1: Add editor route to App.tsx**

Modify `src/App.tsx` to add the editor route. The exact modification depends on the routing solution from Plan 1. If using simple hash-based routing:

```tsx
import { useState } from "preact/hooks";
import Layout from "./components/Layout";
import Home from "./pages/Home";
import Settings from "./pages/Settings";
import Editor from "./pages/Editor";

type Route =
  | { page: "home" }
  | { page: "settings" }
  | { page: "editor"; sourcePath?: string; projectId?: string };

export default function App() {
  const [route, setRoute] = useState<Route>({ page: "home" });

  const navigate = (r: Route) => setRoute(r);

  const renderPage = () => {
    switch (route.page) {
      case "settings":
        return <Settings />;
      case "editor":
        return (
          <Editor
            sourcePath={route.sourcePath}
            projectId={route.projectId}
          />
        );
      case "home":
      default:
        return (
          <Home
            onOpenEditor={(sourcePath: string) =>
              navigate({ page: "editor", sourcePath })
            }
            onOpenProject={(projectId: string) =>
              navigate({ page: "editor", projectId })
            }
          />
        );
    }
  };

  return (
    <Layout
      onNavigate={(page: string) => navigate({ page: page as Route["page"] })}
    >
      {renderPage()}
    </Layout>
  );
}
```

- [ ] **Step 2: Add editor entry points to Home.tsx**

Add to `src/pages/Home.tsx` (integrate with existing recording list or add a section):

```tsx
import { useState, useEffect } from "preact/hooks";
import { listEditProjects } from "../lib/ipc";
import type { ProjectSummary } from "../lib/types";

interface HomeProps {
  onOpenEditor: (sourcePath: string) => void;
  onOpenProject: (projectId: string) => void;
}

export default function Home({ onOpenEditor, onOpenProject }: HomeProps) {
  const [projects, setProjects] = useState<ProjectSummary[]>([]);

  useEffect(() => {
    listEditProjects()
      .then(setProjects)
      .catch(console.error);
  }, []);

  // Handler for opening a file to edit (would use Tauri file dialog)
  const handleOpenFile = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const path = await open({
        multiple: false,
        filters: [
          { name: "Media", extensions: ["mp4", "webm", "mkv", "mov", "png", "jpg", "jpeg"] },
        ],
      });
      if (path && typeof path === "string") {
        onOpenEditor(path);
      }
    } catch (err) {
      console.error("File open failed:", err);
    }
  };

  return (
    <div class="home-page" style={{ padding: "20px" }}>
      <h1 style={{ fontSize: "24px", marginBottom: "16px" }}>Screen Dream</h1>

      {/* Recording controls would go here (from Plan 2) */}
      <div style={{ marginBottom: "24px" }}>
        <button
          onClick={handleOpenFile}
          style={{
            padding: "10px 20px",
            border: "1px solid #0f3460",
            borderRadius: "6px",
            background: "#1a1a2e",
            color: "#e0e0e0",
            cursor: "pointer",
            fontSize: "14px",
          }}
        >
          Open File to Edit
        </button>
      </div>

      {/* Recent projects */}
      {projects.length > 0 && (
        <div>
          <h2 style={{ fontSize: "18px", marginBottom: "8px" }}>Recent Projects</h2>
          <div style={{ display: "flex", flexDirection: "column", gap: "4px" }}>
            {projects.map((p) => (
              <div
                key={p.id}
                onClick={() => onOpenProject(p.id)}
                style={{
                  padding: "8px 12px",
                  background: "#16213e",
                  borderRadius: "4px",
                  cursor: "pointer",
                  display: "flex",
                  justifyContent: "space-between",
                  fontSize: "13px",
                }}
              >
                <span>{p.name}</span>
                <span style={{ color: "#888", fontSize: "11px" }}>
                  {p.source_path.split("/").pop()}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 3: Verify the full app builds**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
pnpm exec tsc --noEmit
cd src-tauri && cargo check
```

Expected: Both frontend and backend compile clean.

- [ ] **Step 4: Run the dev server**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
pnpm tauri dev
```

Expected: App launches. The home page shows an "Open File to Edit" button. Clicking it opens a file dialog. Selecting a video file navigates to the editor page with the four-panel layout (tools, preview, properties, timeline/audio).

- [ ] **Step 5: Commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add src/App.tsx src/pages/Home.tsx src/pages/Editor.tsx
git commit -m "feat: wire editor page into app routing with project list and file open"
```

---

## Task 14: End-to-End Verification and Cleanup

**Files:**
- No new files. This task verifies everything works together.

- [ ] **Step 1: Run all Rust tests**

```bash
cd /home/rw3iss/Sites/others/screen-recorder/src-tauri
cargo test --workspace -- --nocapture
```

Expected: All tests pass:
- Domain: TimeRange, Clip, Track, EditProject tests
- Domain: EditOperation serialization/deserialization tests
- Domain: FilterGraphBuilder tests (empty, crop, audio normalize, chained, to_ffmpeg_args)
- Infrastructure: Frame rate parsing tests
- Infrastructure: Project repository tests (save/load, list, delete)

- [ ] **Step 2: Run the full app and verify editing IPC commands**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
pnpm tauri dev
```

In the app's webview devtools console, verify the editing commands work:

```javascript
// Create a test project (use a real video path if available)
const project = await window.__TAURI__.core.invoke("create_edit_project", {
  name: "Test",
  sourcePath: "/tmp/test.mp4"
});
console.log("Created project:", project.id);

// Apply an operation
const updated = await window.__TAURI__.core.invoke("apply_operation", {
  operation: { type: "Crop", config: { x: 0, y: 0, width: 1280, height: 720 } },
  targetTrackId: null,
  targetClipId: null,
});
console.log("Operations:", updated.global_operations.length);

// Check undo/redo state
const undoRedo = await window.__TAURI__.core.invoke("get_undo_redo_state");
console.log("Can undo:", undoRedo.can_undo); // true

// Undo
const undone = await window.__TAURI__.core.invoke("undo_operation");
console.log("After undo, operations:", undone.global_operations.length); // 0

// Redo
const redone = await window.__TAURI__.core.invoke("redo_operation");
console.log("After redo, operations:", redone.global_operations.length); // 1

// Get filter graph
const fg = await window.__TAURI__.core.invoke("get_filter_graph");
console.log("Filter graph:", fg); // Should contain "crop=1280:720:0:0"

// List projects
const projects = await window.__TAURI__.core.invoke("list_edit_projects");
console.log("Projects:", projects.length);
```

- [ ] **Step 3: Verify the editor UI renders correctly**

Open a video file using the "Open File to Edit" button and verify:
- The video preview shows the first frame
- The timeline shows tracks with clips
- The tool panel has text overlay, blur, crop, scale, and speed sections
- The properties panel shows project info
- Play/pause and scrubbing work
- Applying an operation (e.g., Crop) updates the project state
- Undo/redo buttons work and update state correctly

- [ ] **Step 4: Check that project files are saved**

```bash
ls -la ~/.config/screen-dream/projects/
cat ~/.config/screen-dream/projects/*.json | head -50
```

Expected: JSON project files exist with the correct structure.

- [ ] **Step 5: Final commit**

```bash
cd /home/rw3iss/Sites/others/screen-recorder
git add -A
git commit -m "chore: end-to-end verification of media editing pipeline"
```

---

## Summary

After completing all 14 tasks, you will have:

| Component | Status |
|-----------|--------|
| EditProject domain model (tracks, clips, time ranges) | Working, tested |
| EditOperation enum (trim, crop, scale, rotate, text, image, blur, audio effects, speed) | Working, tested |
| FFmpeg filter graph builder (operations to filter_complex strings) | Working, tested |
| Frame extractor service (FFmpeg subprocess to PNG frames) | Working, tested |
| Project JSON repository (save/load/list/delete) | Working, tested |
| Editing IPC commands (create, open, save, delete, apply operation, undo, redo, get frame) | Working |
| Editor store (project state, undo/redo history, playback, selection) | Working |
| Editor page layout (toolbar + four-panel design) | Working |
| VideoPreview component (Canvas rendering, scrub bar, play/pause) | Working |
| Timeline component (single video + audio track, clips, playhead, ruler, selection) | Working | <!-- // TODO(v2): Multi-track timeline -->
| ImageEditor component (Fabric.js with rect, circle, text, arrow, freehand, delete) | Working |
| AudioPanel component (wavesurfer.js waveform, volume, normalize) | Working | <!-- // TODO(v2): compress, EQ, noise reduce UI -->
| ToolPanel (text overlay, blur region, crop, scale, speed controls) | Working |
| PropertiesPanel (project/track/clip info, operation list) | Working |
| App routing integration (home to editor navigation, project list) | Working |

**Next:** Plan 4 (Export & Sharing) consumes the `EditProject` and `FilterGraphBuilder` from this plan to render final output via the FFmpeg sidecar, with export presets, progress reporting, and upload functionality.
