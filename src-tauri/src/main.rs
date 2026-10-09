#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod auto_sync;
mod single_instance;
mod tray;
mod update;

fn main() {
    let _startup_guard = match single_instance::acquire_startup_guard() {
        Ok(guard) => guard,
        Err(error) => {
            eprintln!("{error}");
            return;
        }
    };

    use tauri::Manager as _;

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
        // Native save/open dialogs for provider backup import/export. Only
        // the picked path crosses the bridge; file I/O stays in Rust.
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            tray::setup(app.handle())?;
            // Rust-side auto-sync cadence; the frontend calls
            // set_auto_sync_interval on boot and whenever the rail setting
            // changes, so hidden WebView timers no longer gate data refresh.
            app.manage(auto_sync::AutoSyncState::default());
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
            app::load_daily_usage,
            app::export_provider_backup,
            app::import_provider_backup,
            app::open_project_repository,
            auto_sync::set_auto_sync_interval,
            update::check_for_update,
            update::download_and_install_update
        ])
        .run(tauri::generate_context!())
        .expect("failed to run LLM Usage");
}
