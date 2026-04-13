//! Spectacle D-Bus capture backend for KDE Plasma.
//!
//! Uses Spectacle's D-Bus interface (`org.kde.Spectacle`) for native-resolution
//! screenshot capture. Spectacle must be running in background mode.
//!
//! All screenshot methods are NoReply — the result comes via the
//! `ScreenshotTaken(fileName)` or `ScreenshotFailed(message)` signal.

use std::path::{Path, PathBuf};

use domain::capture::CapturedFrame;
use domain::error::{AppError, AppResult};
use tracing::{debug, info};

/// Wrapper around Spectacle's D-Bus interface for native-resolution screenshots.
pub struct SpectacleCapture {
    runtime: tokio::runtime::Runtime,
    /// Whether we started Spectacle ourselves (so we can kill it on drop).
    owns_spectacle: bool,
}

impl SpectacleCapture {
    /// Ensure Spectacle is running in background mode.
    /// If not already running, starts it and waits for the D-Bus service to appear.
    pub fn new() -> AppResult<Self> {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| AppError::Capture(format!("Failed to create tokio runtime: {e}")))?;

        let owns_spectacle = runtime.block_on(ensure_spectacle_running())?;

        Ok(Self {
            runtime,
            owns_spectacle,
        })
    }

    /// Capture all monitors at native resolution.
    /// Returns path to the saved screenshot.
    pub fn full_screen(&self) -> AppResult<PathBuf> {
        self.runtime.block_on(async {
            let conn = session_connection().await?;
            // FullScreen(includeMousePointer: i) — 0 = no cursor
            call_and_wait_signal(&conn, "FullScreen", &(0i32,)).await
        })
    }

    /// Capture the current/active monitor at native resolution.
    pub fn current_screen(&self) -> AppResult<PathBuf> {
        self.runtime.block_on(async {
            let conn = session_connection().await?;
            // CurrentScreen(includeMousePointer: i) — 0 = no cursor
            call_and_wait_signal(&conn, "CurrentScreen", &(0i32,)).await
        })
    }

    /// Capture the active window (with decorations, no cursor, no shadow).
    pub fn active_window(&self) -> AppResult<PathBuf> {
        self.runtime.block_on(async {
            let conn = session_connection().await?;
            // ActiveWindow(includeWindowDecorations: i, includeMousePointer: i, includeWindowShadow: i)
            // 1 = with decorations, 0 = no cursor, 0 = no shadow
            call_and_wait_signal(&conn, "ActiveWindow", &(1i32, 0i32, 0i32)).await
        })
    }

    /// Capture a specific window by first activating it via KWin scripting,
    /// then calling ActiveWindow.
    ///
    /// `window_uuid` is the KWin internal UUID for the target window.
    pub fn capture_window(&self, window_uuid: &str) -> AppResult<PathBuf> {
        let uuid = window_uuid.to_string();
        self.runtime.block_on(async {
            let conn = session_connection().await?;

            // Step 1: Activate the target window via KWin scripting.
            activate_window_by_uuid(&conn, &uuid).await?;

            // Step 2: Brief delay for focus to take effect.
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            // Step 3: Capture the now-active window.
            // ActiveWindow(includeWindowDecorations=1, includeMousePointer=0, includeWindowShadow=0)
            call_and_wait_signal(&conn, "ActiveWindow", &(1i32, 0i32, 0i32)).await
        })
    }

    /// Load a screenshot image file as a `CapturedFrame` (RGBA pixel data).
    pub fn load_as_frame(path: &Path) -> AppResult<CapturedFrame> {
        super::portal_screenshot::load_png_as_frame(path)
    }
}

impl Drop for SpectacleCapture {
    fn drop(&mut self) {
        if self.owns_spectacle {
            debug!("SpectacleCapture owns Spectacle process — sending quit via D-Bus");
            // Best-effort: ask Spectacle to quit via D-Bus.
            let _ = self.runtime.block_on(async {
                if let Ok(conn) = zbus::Connection::session().await {
                    let proxy = zbus::Proxy::new(
                        &conn,
                        "org.kde.Spectacle",
                        "/",
                        "org.kde.Spectacle",
                    )
                    .await;
                    if let Ok(p) = proxy {
                        let _: Result<(), _> = p.call("quit", &()).await;
                    }
                }
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Get a D-Bus session connection.
async fn session_connection() -> AppResult<zbus::Connection> {
    zbus::Connection::session()
        .await
        .map_err(|e| AppError::Capture(format!("D-Bus session connection failed: {e}")))
}

/// Check whether Spectacle is running on D-Bus and start it if not.
/// Returns `true` if we started it ourselves.
async fn ensure_spectacle_running() -> AppResult<bool> {
    let conn = session_connection().await?;

    let dbus = zbus::fdo::DBusProxy::new(&conn)
        .await
        .map_err(|e| AppError::Capture(format!("Failed to create DBus proxy: {e}")))?;

    let is_running = dbus
        .name_has_owner(
            "org.kde.Spectacle"
                .try_into()
                .map_err(|e| AppError::Capture(format!("Invalid bus name: {e}")))?,
        )
        .await
        .unwrap_or(false);

    if is_running {
        info!("Spectacle is already running on D-Bus");
        return Ok(false);
    }

    info!("Starting Spectacle in background mode (no notification)");
    std::process::Command::new("spectacle")
        .args(["--background", "--nonotify"])
        .spawn()
        .map_err(|e| AppError::Capture(format!("Failed to start spectacle --background: {e}")))?;

    // Wait up to 3 seconds for the D-Bus service to appear.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let running = dbus
            .name_has_owner(
                "org.kde.Spectacle"
                    .try_into()
                    .map_err(|e| AppError::Capture(format!("Invalid bus name: {e}")))?,
            )
            .await
            .unwrap_or(false);

        if running {
            info!("Spectacle D-Bus service is now available");
            return Ok(true);
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(AppError::Capture(
                "Timed out waiting for Spectacle D-Bus service to appear after 3 seconds"
                    .to_string(),
            ));
        }
    }
}

/// Call a Spectacle screenshot method and wait for the `ScreenshotTaken` signal.
///
/// All Spectacle screenshot methods have the `NoReply` annotation — they return
/// immediately and deliver the result path via a `ScreenshotTaken(fileName: s)` signal.
/// On failure, a `ScreenshotFailed(message: s)` signal is emitted instead.
async fn call_and_wait_signal<B>(
    conn: &zbus::Connection,
    method: &str,
    args: &B,
) -> AppResult<PathBuf>
where
    B: serde::Serialize + zbus::zvariant::DynamicType + Sync,
{
    use futures_util::StreamExt as _;

    // Subscribe to ScreenshotTaken signal BEFORE calling the method to avoid races.
    let taken_rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("org.kde.Spectacle")
        .map_err(|e| AppError::Capture(format!("MatchRule interface error: {e}")))?
        .member("ScreenshotTaken")
        .map_err(|e| AppError::Capture(format!("MatchRule member error: {e}")))?
        .build();

    let failed_rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("org.kde.Spectacle")
        .map_err(|e| AppError::Capture(format!("MatchRule interface error: {e}")))?
        .member("ScreenshotFailed")
        .map_err(|e| AppError::Capture(format!("MatchRule member error: {e}")))?
        .build();

    let mut taken_stream =
        zbus::MessageStream::for_match_rule(taken_rule, conn, Some(16))
            .await
            .map_err(|e| {
                AppError::Capture(format!(
                    "Failed to subscribe to ScreenshotTaken signal: {e}"
                ))
            })?;

    let mut failed_stream =
        zbus::MessageStream::for_match_rule(failed_rule, conn, Some(16))
            .await
            .map_err(|e| {
                AppError::Capture(format!(
                    "Failed to subscribe to ScreenshotFailed signal: {e}"
                ))
            })?;

    // Call the method via a Proxy. Spectacle methods have NoReply annotation.
    let proxy = zbus::Proxy::new(
        conn,
        "org.kde.Spectacle",
        "/",
        "org.kde.Spectacle",
    )
    .await
    .map_err(|e| AppError::Capture(format!("Failed to create Spectacle proxy: {e}")))?;

    proxy
        .call_noreply(method, args)
        .await
        .map_err(|e| AppError::Capture(format!("Spectacle {method} call failed: {e}")))?;

    debug!("Spectacle {method} called, waiting for signal...");

    // Wait for either ScreenshotTaken or ScreenshotFailed, with a 5-second timeout.
    let timeout = std::time::Duration::from_secs(5);

    tokio::select! {
        result = async {
            while let Some(msg) = taken_stream.next().await {
                match msg {
                    Ok(m) => {
                        if let Ok(file_name) = m.body().deserialize::<String>() {
                            return Ok(file_name);
                        }
                        // Try (String,) tuple format as well.
                        if let Ok((file_name,)) = m.body().deserialize::<(String,)>() {
                            return Ok(file_name);
                        }
                        debug!("Received ScreenshotTaken signal but could not deserialize body");
                    }
                    Err(e) => {
                        return Err(AppError::Capture(format!("Error reading ScreenshotTaken signal: {e}")));
                    }
                }
            }
            Err(AppError::Capture("ScreenshotTaken signal stream ended unexpectedly".to_string()))
        } => {
            match result {
                Ok(file_name) => {
                    info!("Spectacle screenshot saved to: {file_name}");
                    Ok(PathBuf::from(file_name))
                }
                Err(e) => Err(e),
            }
        }
        result = async {
            while let Some(msg) = failed_stream.next().await {
                match msg {
                    Ok(m) => {
                        if let Ok(error_msg) = m.body().deserialize::<String>() {
                            return Err(AppError::Capture(format!("Spectacle screenshot failed: {error_msg}")));
                        }
                        if let Ok((error_msg,)) = m.body().deserialize::<(String,)>() {
                            return Err(AppError::Capture(format!("Spectacle screenshot failed: {error_msg}")));
                        }
                        return Err(AppError::Capture("Spectacle screenshot failed (unknown error)".to_string()));
                    }
                    Err(e) => {
                        return Err(AppError::Capture(format!("Error reading ScreenshotFailed signal: {e}")));
                    }
                }
            }
            Err(AppError::Capture("ScreenshotFailed signal stream ended unexpectedly".to_string()))
        } => {
            result
        }
        _ = tokio::time::sleep(timeout) => {
            Err(AppError::Capture(format!(
                "Timed out waiting for Spectacle {method} signal after {timeout:?}"
            )))
        }
    }
}

/// Activate a window by its KWin internal UUID using KWin scripting.
async fn activate_window_by_uuid(conn: &zbus::Connection, uuid: &str) -> AppResult<()> {
    // Write a KWin script that finds and activates the window.
    let script_content = format!(
        r#"const clients = workspace.windowList();
for (let i = 0; i < clients.length; i++) {{
    if (clients[i].internalId === "{}") {{
        workspace.activeWindow = clients[i];
        break;
    }}
}}"#,
        uuid
    );

    let tmp_dir = std::env::temp_dir();
    let script_path = tmp_dir.join("sd_spectacle_activate.js");
    std::fs::write(&script_path, &script_content).map_err(|e| {
        AppError::Capture(format!("Failed to write KWin activate script: {e}"))
    })?;

    let script_path_str = script_path.to_string_lossy().to_string();

    let scripting_proxy = zbus::Proxy::new(
        conn,
        "org.kde.KWin",
        "/Scripting",
        "org.kde.kwin.Scripting",
    )
    .await
    .map_err(|e| AppError::Capture(format!("Failed to create KWin Scripting proxy: {e}")))?;

    // loadScript(path, name) -> script_id
    let script_id: i32 = scripting_proxy
        .call("loadScript", &(script_path_str.as_str(), "sd_activate"))
        .await
        .map_err(|e| AppError::Capture(format!("KWin loadScript failed: {e}")))?;

    debug!("KWin activate script loaded with ID {script_id}");

    // Run the script.
    let _: () = scripting_proxy
        .call("start", &())
        .await
        .map_err(|e| AppError::Capture(format!("KWin script start() failed: {e}")))?;

    // Brief delay for script execution.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Unload the script (best effort).
    let _: Result<(), _> = scripting_proxy.call("unloadScript", &"sd_activate").await;

    // Clean up temp file (best effort).
    let _ = std::fs::remove_file(&script_path);

    debug!("Window {uuid} activation script executed");
    Ok(())
}
