# Screen Dream — Architecture Plan

**App Name:** Screen Dream (configurable via `APP_NAME` constant — all user-facing references import from a single config)

**License:** GPLv3 (code is modular enough to swap license if FFmpeg bundling is removed later)

## Project Goal

Build a cross-platform native desktop application for screen recording, screenshots, and media editing. The app should feel like a lightweight, modern alternative to tools like Cap, OBS, or Screen Studio — with an integrated editing workflow so users can capture, trim, annotate, and export without leaving the app.

**Priority platforms:** Fedora/Linux first, then macOS and Windows.
**Minimum macOS version:** 12.3+ (ScreenCaptureKit required)

---

## Core Features

### Recording
- Record entire screen (any monitor)
- Record a specific application window
- Record a user-selected region (freeform rectangle)
- Webcam overlay (picture-in-picture)
- System audio capture
- Microphone audio capture
- Configurable frame rate (30/60fps) and resolution
- Global keyboard shortcuts (start/stop/pause/screenshot from any app)
- System tray presence for quick access

### Screenshots
- Full screen capture
- Window capture
- Region selection capture
- Instant copy-to-clipboard
- Save to file with configurable format (PNG, JPEG, WebP)

### Video Editing
- Trim/cut segments
- Crop and scale
- Add text overlays and annotations
- Add image overlays (watermarks, logos)
- Cursor highlighting and zoom effects
- Multi-track timeline (screen + webcam + audio)
- Export to MP4 (H.264/H.265), WebM (VP9), GIF

### Image Editing
- Crop, scale, rotate
- Add text annotations, arrows, shapes
- Blur/highlight regions
- Layer composition
- Export to PNG, JPEG, WebP

### Audio Editing
- Trim and cut
- Normalization (loudnorm)
- Compression (dynamic range)
- Noise reduction (RNN-based or FFT-based)
- Basic EQ
- Mix multiple tracks

### Sharing / Export
- Save to local disk
- Upload to configurable cloud endpoint (S3, custom server)
- Copy shareable link
- Configurable export presets (quality, format, resolution)

---

## Recommended Technology Stack

### App Framework: Tauri v2

**Why Tauri:**
- ~10-15MB binary vs ~150MB (Electron) or ~200MB+ (Qt)
- Web-based UI (HTML/CSS/JS) — fastest way to build a rich editing interface
- Rust backend — native performance for capture and media processing
- First-party plugins for global shortcuts, system tray, file dialogs, notifications
- Proven by Cap and open-recorder for screen recording apps
- Active ecosystem, strong community

**Frontend framework:** Preact (lightweight React-compatible alternative, with Vite + @preact/preset-vite)

**Styling:** Custom SCSS framework (utility mixins, responsive functions, theme variables via CSS custom properties)

### Screen Capture: Platform-Specific Rust Modules

No single crate reliably handles all platforms. The recommended approach (validated by Cap and open-recorder) is platform-specific capture backends behind a unified Rust trait/interface.

```
                  ┌─────────────────────────────┐
                  │     CaptureBackend trait     │
                  │  start/stop/pause/frame()    │
                  └──────────┬──────────────────┘
                             │
            ┌────────────────┼────────────────┐
            │                │                │
   ┌────────▼──────┐ ┌──────▼───────┐ ┌──────▼──────────┐
   │    Windows     │ │    macOS     │ │     Linux        │
   │  WGC / DXGI   │ │ ScreenCap-   │ │ X11: XCB/xcap   │
   │  windows-      │ │ tureKit via  │ │ Wayland: PipeWire│
   │  capture crate │ │ objc/cidre   │ │ + xdg-portal     │
   └───────────────┘ └──────────────┘ └──────────────────┘
```

**Key crates:**
- `xcap` (v0.9+) — cross-platform screen/window enumeration and capture. Use as a foundation, supplement with platform-specific code where needed.
- `windows-capture` — DXGI Desktop Duplication on Windows (high-performance)
- `objc2` / `cidre` — ScreenCaptureKit bindings on macOS
- `pipewire` + `zbus` — PipeWire + xdg-desktop-portal on Wayland
- `cpal` — cross-platform audio capture (microphone + system audio where available)

**Known platform limitations:**

| Platform | Limitation | Mitigation |
|----------|-----------|------------|
| Linux Wayland | Cannot enumerate windows; user must pick via system dialog | Accept this — it's a security feature of Wayland. Show a "select source" flow that triggers the portal picker. |
| macOS | System audio requires ScreenCaptureKit (macOS 12.3+) or virtual audio device | Use ScreenCaptureKit for macOS 12.3+; show setup guide for older versions |
| Linux X11 | XCB GetImage is CPU-bound, marginal at 30fps for high resolutions | Use XDamage for incremental capture, or encourage Wayland where possible |

### Region Selection: Transparent Overlay Window

Tauri v2 supports creating a secondary window with:
```json
{ "transparent": true, "decorations": false, "alwaysOnTop": true, "fullscreen": true }
```

The frontend renders a semi-transparent overlay. User drags to draw a selection rectangle. Coordinates are sent to the Rust backend via Tauri commands. The backend captures the full screen and crops to the selected region.

### Video Encoding: FFmpeg (Sidecar Binary)

FFmpeg handles encoding, audio processing, format conversion, and final export rendering. The app abstracts FFmpeg resolution behind a `FfmpegProvider` so it works whether FFmpeg is bundled or system-installed.

```
┌─────────────────────────────────────────────┐
│            FfmpegProvider trait              │
│  resolve() -> PathBuf                       │
│  version() -> String                        │
│  has_codec(name) -> bool                    │
└──────────┬──────────────────────────────────┘
           │
    ┌──────┴──────┐
    │             │
┌───▼───┐  ┌─────▼──────┐
│Bundled │  │  System    │
│sidecar │  │  ($PATH)   │
└───────┘  └────────────┘
```

**Resolution order at startup:**
1. Check for bundled sidecar binary (shipped with app)
2. Check for user-configured path (settings)
3. Check system `$PATH` for `ffmpeg`
4. If none found → show first-launch setup screen with install instructions

**Bundled build (default for release):** Ship a full GPL FFmpeg build (including x264, x265, libvpx, libaom, opus) as a sidecar. Since this project is open source, GPL compliance is straightforward — include FFmpeg's license and source link in the distribution.

**System FFmpeg fallback:** For development, minimal installs, or users who prefer their own build. The app detects available codecs at runtime via `ffmpeg -codecs` and adjusts export options accordingly (e.g., hides H.265 if x265 is not available).

**Install helpers per platform:**
- Linux: `sudo dnf install ffmpeg-free` or Flatpak runtime
- macOS: `brew install ffmpeg`
- Windows: Auto-download from trusted static build (gyan.dev / BtbN)

**Why sidecar (subprocess) instead of linking libav:**
- Simpler to build and maintain
- Proven pattern (Cap, open-recorder, Shotcut all use this)
- Swappable — user can upgrade FFmpeg independently of the app
- Clear license boundary

**For frame decoding and preview,** use `ffmpeg-next` (Rust bindings to libav) in the Rust backend. This avoids subprocess overhead for interactive operations like scrubbing and thumbnail generation.

### Editing UI: Canvas/WebGL in Webview

```
┌──────────────────────────────────────────────────┐
│                   Tauri Webview                    │
│                                                    │
│  ┌────────────────────────────────────────────┐   │
│  │         Canvas / WebGL Preview              │   │
│  │  - Fabric.js for image annotations          │   │
│  │  - WebGL for video frame rendering          │   │
│  │  - Interactive crop/scale handles           │   │
│  │  - Text/shape overlay tools                 │   │
│  └────────────────────────────────────────────┘   │
│                                                    │
│  ┌────────────────────────────────────────────┐   │
│  │         Timeline Component                  │   │
│  │  - Multi-track (screen, webcam, audio)      │   │
│  │  - Trim handles, split points               │   │
│  │  - Waveform visualization                   │   │
│  └────────────────────────────────────────────┘   │
│                                                    │
│  ┌────────────────────────────────────────────┐   │
│  │         Properties / Effects Panel          │   │
│  │  - Crop, scale, rotate controls             │   │
│  │  - Audio effects (EQ, compression, etc.)    │   │
│  │  - Export settings                          │   │
│  └────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────┘
        │                              │
        │  Tauri Commands (IPC)        │
        ▼                              ▼
┌──────────────────┐    ┌──────────────────────┐
│  Rust: Decode    │    │  Rust: FFmpeg sidecar │
│  frames via      │    │  for final render,    │
│  ffmpeg-next     │    │  audio processing     │
│  (libav bindings)│    │                       │
└──────────────────┘    └──────────────────────┘
```

**Interactive editing** (dragging crop handles, placing text, drawing shapes) runs entirely in the webview at 60fps. No subprocess calls during interaction.

**Preview playback** decodes frames via `ffmpeg-next` in Rust, passes them to the webview as image data or via shared memory, and renders on Canvas/WebGL.

**Final render** invokes FFmpeg sidecar with a constructed filter graph that applies all edits (crop, scale, overlays, audio effects) in a single pass.

---

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        Tauri App                             │
│                                                              │
│  ┌──────────────────────┐    ┌────────────────────────────┐ │
│  │   Frontend (WebView)  │    │     Rust Backend            │ │
│  │                        │◄──►                              │ │
│  │  - Preact / React    │IPC │  - CaptureManager           │ │
│  │  - Canvas/WebGL editor│    │    (platform backends)       │ │
│  │  - Timeline component │    │  - AudioCapture (cpal)       │ │
│  │  - Recording controls │    │  - FrameDecoder (ffmpeg-next)│ │
│  │  - Export UI          │    │  - ProjectManager (state)    │ │
│  │  - Settings           │    │  - FFmpegSidecar (render)    │ │
│  └──────────────────────┘    │  - UploadManager             │ │
│                               └────────────────────────────┘ │
│                                              │               │
│  ┌──────────────────┐    ┌──────────────────▼──────────┐    │
│  │  Tauri Plugins    │    │  FFmpeg Binary (sidecar)     │    │
│  │  - global-shortcut│    │  - Encoding (H.264/VP9)      │    │
│  │  - system tray    │    │  - Audio processing          │    │
│  │  - file dialog    │    │  - Export rendering           │    │
│  │  - notification   │    │  - Format conversion          │    │
│  │  - clipboard      │    └─────────────────────────────┘    │
│  └──────────────────┘                                        │
└─────────────────────────────────────────────────────────────┘
```

---

## Implementation Phases

### Phase 1: Foundation
- Initialize Tauri v2 project with Rust backend and web frontend
- Set up build pipeline for all three platforms
- Implement system tray and global shortcut registration
- Implement FFmpeg resolver (bundled or system-installed)

### Phase 2: Screen Capture
- Implement CaptureBackend trait with platform-specific backends
- Linux X11 capture (XCB via xcap)
- Linux Wayland capture (PipeWire + xdg-desktop-portal)
- macOS capture (ScreenCaptureKit)
- Windows capture (WGC / DXGI)
- Transparent overlay window for region selection
- Audio capture via cpal (microphone + system audio)

### Phase 3: Recording Pipeline
- Frame buffer management (Rust ring buffer)
- FFmpeg sidecar encoding pipeline (frames piped to FFmpeg stdin)
- Recording controls (start, stop, pause, resume)
- Webcam capture overlay
- Recording indicator / countdown timer

### Phase 4: Screenshots
- Full screen, window, and region screenshot capture
- Copy to clipboard
- Save to file with format options
- Quick annotation overlay (optional)

### Phase 5: Video Editing
- Timeline component (multi-track)
- Frame decoding via ffmpeg-next for preview/scrubbing
- Canvas/WebGL video frame rendering
- Trim, split, crop, scale operations
- Text and image overlay tools
- Cursor highlight and zoom effects
- Waveform visualization for audio tracks

### Phase 6: Image Editing
- Fabric.js or similar canvas library for interactive editing
- Crop, scale, rotate tools
- Annotation tools (text, arrows, shapes, blur)
- Layer management
- Export with FFmpeg or canvas-based rendering

### Phase 7: Audio Editing
- Audio waveform display and trim
- FFmpeg-based effects (normalization, compression, noise reduction, EQ)
- Audio track mixing
- Preview playback with effects applied

### Phase 8: Export & Sharing
- Export presets (format, quality, resolution)
- FFmpeg render pipeline with progress reporting
- Upload to configurable endpoints
- Shareable link generation

### Phase 9: Polish
- Settings and preferences UI
- Keyboard shortcut customization
- Recording history and project management
- Auto-update mechanism
- Platform-specific installers (AppImage/Flatpak for Linux, DMG for macOS, MSI for Windows)

---

## Key Dependencies (Rust / Cargo)

```toml
# App framework
tauri = { version = "2", features = ["tray-icon"] }
tauri-plugin-global-shortcut = "2"
tauri-plugin-dialog = "2"
tauri-plugin-notification = "2"
tauri-plugin-clipboard-manager = "2"
tauri-plugin-shell = "2"          # For FFmpeg sidecar

# Screen capture
xcap = "0.9"                       # Cross-platform capture foundation
# Platform-specific (conditional compilation):
# windows-capture, objc2, pipewire, zbus

# Audio
cpal = "0.15"                      # Cross-platform audio I/O

# Media processing
ffmpeg-next = "7"                  # libav bindings for frame decoding/preview
image = "0.25"                     # Image format handling

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

## Key Dependencies (Frontend / npm)

```json
{
  "dependencies": {
    "@tauri-apps/api": "^2",
    "@tauri-apps/plugin-global-shortcut": "^2",
    "@tauri-apps/plugin-dialog": "^2",
    "@tauri-apps/plugin-clipboard-manager": "^2",
    "fabric": "^6",
    "preact": "^10",
    "wavesurfer.js": "^7"
  }
}
```

---

## Reference Projects

| Project | Stack | What to learn from it |
|---------|-------|-----------------------|
| [Cap](https://github.com/CapSoftware/Cap) | Tauri v2 + Rust + SolidJS | Overall architecture, platform-specific capture modules, FFmpeg sidecar pattern |
| [open-recorder](https://github.com/imbhargav5/open-recorder) | Tauri v2 + Rust | Auto-zoom, cursor animation, window enumeration, FFmpeg sidecar |
| [Shotcut](https://github.com/mltframework/shotcut) | Qt + MLT + FFmpeg | Timeline editing architecture, MLT integration for multi-track compositing |
| [OBS Studio](https://github.com/obsproject/obs-studio) | C++ + Qt | Platform-specific capture implementations (gold standard) |
| [Flameshot](https://github.com/flameshot-org/flameshot) | C++ + Qt | Region selection UX, screenshot annotation workflow |

---

## Resolved Decisions

1. **Audio on macOS:** RESOLVED — macOS 12.3+ minimum. Use ScreenCaptureKit for system audio. No fallback needed.

2. **Wayland window capture:** RESOLVED — Detect XWayland sessions and use X11 enumeration when available. Fall back to portal picker on pure Wayland.

3. **FFmpeg licensing:** RESOLVED — Bundle full GPL FFmpeg build. App is GPLv3. Code is modular enough to remove FFmpeg bundling and change license later if needed.

4. **MLT Framework:** RESOLVED — No MLT. Simple single-track timeline for v1 (one video + one audio track, trim/crop/overlay). Multi-track is a possible v2 feature.

5. **Cloud/sharing:** RESOLVED — Local save only for v1. S3-compatible upload deferred to v1.1.

6. **Project file format:** RESOLVED — JSON-based project format. Serializable edit decisions that translate to FFmpeg filter graphs.

7. **Recording format:** RESOLVED — MP4 (H.264 + AAC) default. WebM and other formats as options in settings.

8. **Resolution:** RESOLVED — Native source resolution by default. Downscale, quality, and audio/video spec options in settings.

9. **Webcam PiP:** RESOLVED — Deferred to v1.1. Stubbed/noted in code for future implementation.

10. **Image editor:** RESOLVED — Fabric.js for annotation/overlay editing.

11. **Audio editing:** RESOLVED — Basic for v1 (waveform, trim, normalize). Advanced effects (compress, EQ, noise reduction) noted in code for v2.

12. **GIF export:** RESOLVED — Included in v1 for small clips. Configurable in export options.

13. **Styling:** RESOLVED — Custom SCSS framework with utility mixins, customizable variables, responsive functions, and variable overrides. No Tailwind.

14. **Theme:** RESOLVED — Dark mode default. Theme switching architecture with CSS custom properties, so light mode or custom themes can be swapped in later.

15. **Documentation:** Short README.md, extensive Development.md for setup and architecture docs. Well-documented code.
