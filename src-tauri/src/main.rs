mod app;

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            app::configure_glm,
            app::sync_glm,
            app::has_glm_credential
        ])
        .run(tauri::generate_context!())
        .expect("failed to run LLM Usage");
}
