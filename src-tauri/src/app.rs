use llm_usage_core::cache::{CachedSnapshot, SnapshotCache};
use llm_usage_core::providers::glm::{GlmClient, GlmUsageSnapshot};
use llm_usage_core::providers::online::{
    OnlineClient, OnlineError, OnlineProvider, OnlineSnapshot,
};
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
            message: "请先配置该供应商 API Key",
        }
    }

    fn provider() -> Self {
        Self {
            code: "GLM_SYNC_FAILED",
            message: "GLM 在线用量同步失败，请检查密钥或稍后重试",
        }
    }

    fn invalid_provider() -> Self {
        Self {
            code: "INVALID_PROVIDER",
            message: "暂不支持该供应商",
        }
    }

    fn online_provider() -> Self {
        Self {
            code: "ONLINE_SYNC_FAILED",
            message: "供应商在线同步失败，请检查密钥或稍后重试",
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

fn snapshot_cache(app: &tauri::AppHandle) -> Result<SnapshotCache, CommandError> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|_| CommandError::credential())?;
    Ok(SnapshotCache::new(&app_data))
}

fn cache_snapshot<T: Serialize>(
    app: &tauri::AppHandle,
    provider_id: &str,
    kind: &str,
    snapshot: &T,
) -> Result<(), CommandError> {
    let value = serde_json::to_value(snapshot).map_err(|_| CommandError::credential())?;
    snapshot_cache(app)?
        .save(provider_id, kind, value)
        .map_err(|_| CommandError::credential())
}

fn provider_vault(
    app: &tauri::AppHandle,
    provider: OnlineProvider,
) -> Result<SecretVault, CommandError> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|_| CommandError::credential())?;
    SecretVault::new(&app_data, provider.id()).map_err(|_| CommandError::credential())
}

fn online_provider(provider_id: &str) -> Result<OnlineProvider, CommandError> {
    OnlineProvider::from_id(provider_id).ok_or_else(CommandError::invalid_provider)
}

fn online_error(error: OnlineError) -> CommandError {
    match error {
        OnlineError::InvalidProvider => CommandError::invalid_provider(),
        OnlineError::InvalidCredential => CommandError::online_provider(),
        OnlineError::InvalidJson
        | OnlineError::ApiRejected
        | OnlineError::SchemaMismatch
        | OnlineError::RequestFailed => CommandError::online_provider(),
    }
}

#[tauri::command(rename_all = "camelCase")]
pub fn has_glm_credential(app: tauri::AppHandle) -> bool {
    glm_vault(&app).is_ok_and(|vault| vault.exists())
}

#[tauri::command(rename_all = "camelCase")]
pub fn has_online_credential(app: tauri::AppHandle, provider_id: String) -> bool {
    online_provider(&provider_id)
        .and_then(|provider| provider_vault(&app, provider))
        .is_ok_and(|vault| vault.exists())
}

#[tauri::command(rename_all = "camelCase")]
pub fn load_cached_snapshots(app: tauri::AppHandle) -> Vec<CachedSnapshot> {
    snapshot_cache(&app)
        .and_then(|cache| cache.load_all().map_err(|_| CommandError::credential()))
        .unwrap_or_default()
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
    let snapshot = match client.fetch_snapshot(&start_time, &end_time).await {
        Ok(snapshot) => snapshot,
        Err(_) => {
            api_key.zeroize();
            return Err(CommandError::provider());
        }
    };
    if let Err(error) = glm_vault(&app)?.save(api_key.trim()) {
        api_key.zeroize();
        return Err(match error {
            SecretError::Invalid
            | SecretError::Protect
            | SecretError::Io
            | SecretError::Missing => CommandError::credential(),
        });
    }
    api_key.zeroize();
    cache_snapshot(&app, "glm", "glm", &snapshot)?;
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
    let client = match GlmClient::new(&api_key) {
        Ok(client) => client,
        Err(_) => {
            api_key.zeroize();
            return Err(CommandError::provider());
        }
    };
    api_key.zeroize();
    let snapshot = client
        .fetch_snapshot(&start_time, &end_time)
        .await
        .map_err(|_| CommandError::provider())?;
    cache_snapshot(&app, "glm", "glm", &snapshot)?;
    Ok(snapshot)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn configure_online_provider(
    app: tauri::AppHandle,
    provider_id: String,
    api_key: String,
) -> Result<OnlineSnapshot, CommandError> {
    let provider = online_provider(&provider_id)?;
    let mut api_key = api_key;
    let client = match OnlineClient::new(provider, &api_key) {
        Ok(client) => client,
        Err(error) => {
            api_key.zeroize();
            return Err(online_error(error));
        }
    };
    let snapshot = match client.fetch_snapshot().await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            api_key.zeroize();
            return Err(online_error(error));
        }
    };
    if provider_vault(&app, provider)?
        .save(api_key.trim())
        .is_err()
    {
        api_key.zeroize();
        return Err(CommandError::credential());
    }
    api_key.zeroize();
    cache_snapshot(&app, provider.id(), "online", &snapshot)?;
    Ok(snapshot)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn sync_online_provider(
    app: tauri::AppHandle,
    provider_id: String,
) -> Result<OnlineSnapshot, CommandError> {
    let provider = online_provider(&provider_id)?;
    let mut api_key = provider_vault(&app, provider)?
        .load()
        .map_err(|error| match error {
            SecretError::Missing => CommandError::not_configured(),
            _ => CommandError::credential(),
        })?;
    let client = match OnlineClient::new(provider, &api_key) {
        Ok(client) => client,
        Err(error) => {
            api_key.zeroize();
            return Err(online_error(error));
        }
    };
    api_key.zeroize();
    let snapshot = client.fetch_snapshot().await.map_err(online_error)?;
    cache_snapshot(&app, provider.id(), "online", &snapshot)?;
    Ok(snapshot)
}
