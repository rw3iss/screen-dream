mod commands;
mod error;
mod state;

use std::path::PathBuf;
use std::sync::Arc;

use domain::ffmpeg::FfmpegProvider;
use domain::platform::PlatformInfo;
use domain::settings::SettingsRepository;
use infrastructure::ffmpeg::FfmpegResolver;
use infrastructure::settings::JsonSettingsRepository;
use state::AppState;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager,
};
use tracing::{info, warn};

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    TrayIconBuilder::new()
        .menu(&menu)
        .tooltip(domain::app_config::APP_NAME)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize tracing/logging
    tracing_subscriber::fmt()
        .with_env_filter("screen_dream=debug,infrastructure=debug,domain=debug")
        .init();

    let platform = PlatformInfo::detect();
    info!("Platform: {:?}", platform);

    tauri::Builder::default()
        // Plugins
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        // Setup hook — runs before the main loop
        .setup(move |app| {
            use tauri::Manager;

            let app_config_dir = app
                .path()
                .app_config_dir()
                .expect("Failed to resolve app config directory");

            let app_resource_dir = app.path().resource_dir().ok();

            // Load settings to check FFmpeg preference
            let settings_repo = Arc::new(JsonSettingsRepository::new(app_config_dir.clone()));
            let settings = settings_repo.load().unwrap_or_default();

            // Resolve FFmpeg
            let sidecar_dir = app_resource_dir.map(|d| d.join("sidecars"));
            let custom_path: Option<PathBuf> = settings.ffmpeg.custom_path.as_ref().map(|p| p.into());
            let ffmpeg_resolver = Arc::new(FfmpegResolver::new(sidecar_dir, custom_path));

            info!("FFmpeg source: {}", ffmpeg_resolver.source_description());

            // Register app state
            app.manage(AppState {
                ffmpeg: ffmpeg_resolver,
                settings: settings_repo,
                platform,
            });

            // Setup system tray
            setup_tray(app)?;

            // Register global shortcuts on startup
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Small delay to ensure the app is fully initialized
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                if let Err(e) = commands::shortcuts::register_shortcuts(
                    handle.clone(),
                    handle.state(),
                ) {
                    warn!("Failed to register shortcuts on startup: {e:?}");
                }
            });

            Ok(())
        })
        // IPC command handlers
        .invoke_handler(tauri::generate_handler![
            commands::platform::get_platform_info,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::reset_settings,
            commands::ffmpeg::get_ffmpeg_status,
            commands::shortcuts::register_shortcuts,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
