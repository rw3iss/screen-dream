/// Application name — all user-facing references should use this constant.
pub const APP_NAME: &str = "Screen Dream";

/// Application identifier for system registration (Tauri, D-Bus, etc.).
pub const APP_ID: &str = "com.screendream.app";

/// Application version — kept in sync with Cargo.toml.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
