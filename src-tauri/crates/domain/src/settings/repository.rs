use crate::error::AppResult;
use super::model::AppSettings;

/// Persistence abstraction for application settings.
/// The infrastructure layer provides the concrete implementation
/// (JSON file, SQLite, etc.).
pub trait SettingsRepository: Send + Sync {
    /// Load settings from storage. Returns defaults if not yet saved.
    fn load(&self) -> AppResult<AppSettings>;

    /// Save settings to storage.
    fn save(&self, settings: &AppSettings) -> AppResult<()>;

    /// Reset to defaults and save.
    fn reset(&self) -> AppResult<AppSettings> {
        let defaults = AppSettings::default();
        self.save(&defaults)?;
        Ok(defaults)
    }
}
