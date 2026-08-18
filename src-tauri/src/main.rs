#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod single_instance;
mod tray;

fn main() {
    let _startup_guard = match single_instance::acquire_startup_guard() {
        Ok(guard) => guard,
        Err(error) => {
            eprintln!("{error}");
            return;
        }
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main_window(app);
        }))
        // Boot-start registration: Windows writes the HKCU Run key, macOS uses
        // a LaunchAgent, Linux a .desktop autostart entry. Toggled from the rail.
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            tray::setup(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            app::configure_glm,
            app::configure_online_provider,
            app::sync_glm,
            app::sync_online_provider,
            app::delete_provider,
            app::load_provider_credential,
            app::list_provider_instances,
            app::load_cached_snapshots,
            app::load_daily_usage
        ])
        .run(tauri::generate_context!())
        .expect("failed to run LLM Usage");
}
