#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod tray;

fn main() {
    tauri::Builder::default()
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
            app::has_glm_credential,
            app::has_online_credential,
            app::load_cached_snapshots,
            app::load_daily_usage
        ])
        .run(tauri::generate_context!())
        .expect("failed to run LLM Usage");
}
