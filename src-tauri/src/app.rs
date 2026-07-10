use llm_usage_core::providers::glm::{GlmClient, GlmUsageSnapshot};
use llm_usage_core::secret::{SecretError, SecretVault};
use serde::Serialize;
use tauri::Manager;
use zeroize::Zeroize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    code: &'static str,
    message: &'static str,
}

impl CommandError {
    fn credential() -> Self {
        Self {
            code: "CREDENTIAL_ERROR",
            message: "无法访问 Windows 凭据管理器",
        }
    }

    fn not_configured() -> Self {
        Self {
            code: "PROVIDER_NOT_CONFIGURED",
            message: "请先配置 GLM API Key",
        }
    }

    fn provider() -> Self {
        Self {
            code: "GLM_SYNC_FAILED",
            message: "GLM 在线用量同步失败，请检查密钥或稍后重试",
        }
    }
}

fn glm_vault(app: &tauri::AppHandle) -> Result<SecretVault, CommandError> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|_| CommandError::credential())?;
    SecretVault::new(&app_data, "glm").map_err(|_| CommandError::credential())
}

#[tauri::command(rename_all = "camelCase")]
pub fn has_glm_credential(app: tauri::AppHandle) -> bool {
    glm_vault(&app).is_ok_and(|vault| vault.exists())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn configure_glm(
    app: tauri::AppHandle,
    api_key: String,
    start_time: String,
    end_time: String,
) -> Result<GlmUsageSnapshot, CommandError> {
    let mut api_key = api_key;
    let client = GlmClient::new(&api_key).map_err(|_| CommandError::provider())?;
    let snapshot = client
        .fetch_snapshot(&start_time, &end_time)
        .await
        .map_err(|_| CommandError::provider())?;
    glm_vault(&app)?
        .save(api_key.trim())
        .map_err(|_| CommandError::credential())?;
    api_key.zeroize();
    Ok(snapshot)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn sync_glm(
    app: tauri::AppHandle,
    start_time: String,
    end_time: String,
) -> Result<GlmUsageSnapshot, CommandError> {
    let mut api_key = glm_vault(&app)?.load().map_err(|error| match error {
        SecretError::Missing => CommandError::not_configured(),
        _ => CommandError::credential(),
    })?;
    let client = GlmClient::new(&api_key).map_err(|_| CommandError::provider())?;
    api_key.zeroize();
    client
        .fetch_snapshot(&start_time, &end_time)
        .await
        .map_err(|_| CommandError::provider())
}
