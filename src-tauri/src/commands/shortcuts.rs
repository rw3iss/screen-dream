use tauri::State;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tracing::{info, warn};

use crate::error::CommandResult;
use crate::state::AppState;

/// Registers all global shortcuts from the current settings.
/// Called during app startup and when shortcuts are changed.
#[tauri::command]
pub fn register_shortcuts(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let settings = state.settings.load().map_err(|e| -> crate::error::CommandError { e.into() })?;

    let shortcut_manager = app.global_shortcut();

    // Unregister all existing shortcuts first
    if let Err(e) = shortcut_manager.unregister_all() {
        warn!("Failed to unregister existing shortcuts: {e}");
    }

    // Register start/stop recording shortcut
    let start_stop = &settings.shortcuts.start_stop_recording;
    match start_stop.parse::<tauri_plugin_global_shortcut::Shortcut>() {
        Ok(shortcut) => {
            match shortcut_manager.on_shortcut(shortcut, |_app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    info!("Start/stop recording shortcut triggered");
                    // TODO: Plan 2 will emit an event to toggle recording
                }
            }) {
                Ok(_) => info!("Registered shortcut: {start_stop} (start/stop recording)"),
                Err(e) => warn!("Failed to register {start_stop}: {e}"),
            }
        }
        Err(e) => warn!("Failed to parse shortcut '{start_stop}': {e}"),
    }

    // Register screenshot shortcut
    let screenshot = &settings.shortcuts.take_screenshot;
    match screenshot.parse::<tauri_plugin_global_shortcut::Shortcut>() {
        Ok(shortcut) => {
            match shortcut_manager.on_shortcut(shortcut, |_app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    info!("Screenshot shortcut triggered");
                    // TODO: Plan 2 will emit an event to take screenshot
                }
            }) {
                Ok(_) => info!("Registered shortcut: {screenshot} (screenshot)"),
                Err(e) => warn!("Failed to register {screenshot}: {e}"),
            }
        }
        Err(e) => warn!("Failed to parse shortcut '{screenshot}': {e}"),
    }

    Ok(())
}
