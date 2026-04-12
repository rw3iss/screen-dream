use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Linux,
    Macos,
    Windows,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DisplayServer {
    X11,
    Wayland,
    Quartz,
    Win32,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlatformInfo {
    pub os: Os,
    pub display_server: DisplayServer,
    pub arch: String,
}

impl PlatformInfo {
    pub fn detect() -> Self {
        let os = if cfg!(target_os = "linux") {
            Os::Linux
        } else if cfg!(target_os = "macos") {
            Os::Macos
        } else if cfg!(target_os = "windows") {
            Os::Windows
        } else {
            Os::Linux // fallback
        };

        let display_server = match &os {
            Os::Linux => detect_linux_display_server(),
            Os::Macos => DisplayServer::Quartz,
            Os::Windows => DisplayServer::Win32,
        };

        PlatformInfo {
            os,
            display_server,
            arch: std::env::consts::ARCH.to_string(),
        }
    }

    pub fn is_wayland(&self) -> bool {
        self.display_server == DisplayServer::Wayland
    }
}

fn detect_linux_display_server() -> DisplayServer {
    // XDG_SESSION_TYPE is the most reliable indicator
    if let Ok(session_type) = std::env::var("XDG_SESSION_TYPE") {
        match session_type.to_lowercase().as_str() {
            "wayland" => return DisplayServer::Wayland,
            "x11" => return DisplayServer::X11,
            _ => {}
        }
    }
    // Fallback: check WAYLAND_DISPLAY
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        return DisplayServer::Wayland;
    }
    // Fallback: check DISPLAY (X11)
    if std::env::var("DISPLAY").is_ok() {
        return DisplayServer::X11;
    }
    DisplayServer::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_returns_valid_platform() {
        let info = PlatformInfo::detect();
        assert!(!info.arch.is_empty());
        // On the CI/dev machine, we should get a known OS
        #[cfg(target_os = "linux")]
        assert_eq!(info.os, Os::Linux);
        #[cfg(target_os = "macos")]
        assert_eq!(info.os, Os::Macos);
        #[cfg(target_os = "windows")]
        assert_eq!(info.os, Os::Windows);
    }

    #[test]
    fn is_wayland_returns_correct_value() {
        let mut info = PlatformInfo {
            os: Os::Linux,
            display_server: DisplayServer::Wayland,
            arch: "x86_64".to_string(),
        };
        assert!(info.is_wayland());
        info.display_server = DisplayServer::X11;
        assert!(!info.is_wayland());
    }
}
