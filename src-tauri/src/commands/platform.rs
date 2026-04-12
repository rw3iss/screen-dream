use domain::platform::PlatformInfo;
use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub fn get_platform_info(state: State<'_, AppState>) -> PlatformInfo {
    state.platform.clone()
}
