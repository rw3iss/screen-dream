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
    /// KWin window UUID (e.g. "{8aa2bfb9-...}"). Set by KWin backend, None for xcap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
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
    /// KWin window UUID (e.g. "{8aa2bfb9-...}"). Set by KWin backend, None for xcap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
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
