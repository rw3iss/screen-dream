use domain::settings::AppSettings;
use tauri::State;

use crate::error::CommandResult;
use crate::state::AppState;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> CommandResult<AppSettings> {
    state.settings.load().map_err(Into::into)
}

#[tauri::command]
pub fn save_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> CommandResult<()> {
    state.settings.save(&settings).map_err(Into::into)
}

#[tauri::command]
pub fn reset_settings(state: State<'_, AppState>) -> CommandResult<AppSettings> {
    state.settings.reset().map_err(Into::into)
}
