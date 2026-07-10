mod app;

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            app::configure_glm,
            app::configure_online_provider,
            app::sync_glm,
            app::sync_online_provider,
            app::has_glm_credential,
            app::has_online_credential
        ])
        .run(tauri::generate_context!())
        .expect("failed to run LLM Usage");
}
