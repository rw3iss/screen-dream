use std::path::PathBuf;
use std::sync::RwLock;

use domain::error::{AppError, AppResult};
use domain::settings::{AppSettings, SettingsRepository};
use tracing::{debug, info};

/// Persists settings as a JSON file in the app's config directory.
pub struct JsonSettingsRepository {
    path: PathBuf,
    cache: RwLock<Option<AppSettings>>,
}

impl JsonSettingsRepository {
    pub fn new(config_dir: PathBuf) -> Self {
        let path = config_dir.join("settings.json");
        debug!("Settings file path: {}", path.display());
        JsonSettingsRepository {
            path,
            cache: RwLock::new(None),
        }
    }
}

impl SettingsRepository for JsonSettingsRepository {
    fn load(&self) -> AppResult<AppSettings> {
        // Return cached if available
        if let Ok(guard) = self.cache.read() {
            if let Some(settings) = guard.as_ref() {
                return Ok(settings.clone());
            }
        }

        // Read from file
        let settings = if self.path.is_file() {
            let contents = std::fs::read_to_string(&self.path).map_err(|e| {
                AppError::Settings(format!("Failed to read {}: {e}", self.path.display()))
            })?;
            serde_json::from_str(&contents).unwrap_or_else(|e| {
                info!("Settings file corrupt ({e}), using defaults");
                AppSettings::default()
            })
        } else {
            info!("No settings file found, using defaults");
            AppSettings::default()
        };

        // Cache it
        if let Ok(mut guard) = self.cache.write() {
            *guard = Some(settings.clone());
        }

        Ok(settings)
    }

    fn save(&self, settings: &AppSettings) -> AppResult<()> {
        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AppError::Settings(format!(
                    "Failed to create config dir {}: {e}",
                    parent.display()
                ))
            })?;
        }

        let json = serde_json::to_string_pretty(settings).map_err(|e| {
            AppError::Settings(format!("Failed to serialize settings: {e}"))
        })?;

        std::fs::write(&self.path, json).map_err(|e| {
            AppError::Settings(format!("Failed to write {}: {e}", self.path.display()))
        })?;

        // Update cache
        if let Ok(mut guard) = self.cache.write() {
            *guard = Some(settings.clone());
        }

        debug!("Settings saved to {}", self.path.display());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn load_returns_defaults_when_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let repo = JsonSettingsRepository::new(dir.path().to_path_buf());
        let settings = repo.load().unwrap();
        assert_eq!(settings.recording.fps, 30);
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let repo = JsonSettingsRepository::new(dir.path().to_path_buf());

        let mut settings = AppSettings::default();
        settings.recording.fps = 60;
        repo.save(&settings).unwrap();

        // Clear cache to force file read
        *repo.cache.write().unwrap() = None;

        let loaded = repo.load().unwrap();
        assert_eq!(loaded.recording.fps, 60);

        // Verify file exists
        assert!(dir.path().join("settings.json").is_file());
    }

    #[test]
    fn load_handles_corrupt_file_gracefully() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "not valid json {{{").unwrap();

        let repo = JsonSettingsRepository::new(dir.path().to_path_buf());
        let settings = repo.load().unwrap();
        // Should fall back to defaults
        assert_eq!(settings.recording.fps, 30);
    }
}
