use llm_usage_core::cache::{
    CachedSnapshot, DailyUsageHistory, DailyUsageRecord, SnapshotCache,
};
use llm_usage_core::providers::glm::{GlmClient, GlmUsageSnapshot};
use llm_usage_core::providers::online::{
    OnlineClient, OnlineError, OnlineProvider, OnlineSnapshot, OnlineUsageRange,
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

    fn kimi_code_provider() -> Self {
        Self {
            code: "KIMI_CODE_SYNC_FAILED",
            message: "Kimi Code 同步失败：请使用 Kimi 会员控制台生成的 Key；Moonshot 开放平台 Key 会自动改查 API 余额",
        }
    }

    fn minimax_token_plan_provider() -> Self {
        Self {
            code: "MINIMAX_TOKEN_PLAN_SYNC_FAILED",
            message: "MiniMax Token Plan 同步失败：请使用订阅 Key，不能使用按量付费 API Key",
        }
    }

    fn openai_admin_provider() -> Self {
        Self {
            code: "OPENAI_ADMIN_SYNC_FAILED",
            message: "OpenAI / Codex API 同步失败：需要 Organization Admin API Key；该数据不包含个人 ChatGPT 套餐剩余额度",
        }
    }

    fn claude_code_admin_provider() -> Self {
        Self {
            code: "CLAUDE_CODE_ADMIN_SYNC_FAILED",
            message:
                "Claude Code 同步失败：需要组织 Admin API Key；个人 Pro/Max 套餐没有公开统计 API",
        }
    }

    fn gemini_monitoring_provider() -> Self {
        Self {
            code: "GEMINI_MONITORING_SYNC_FAILED",
            message: "Gemini 同步失败：请检查 Google Cloud Project ID、Monitoring Viewer 权限和未过期的 OAuth Access Token",
        }
    }

    fn qwen_monitoring_provider() -> Self {
        Self {
            code: "QWEN_MONITORING_SYNC_FAILED",
            message: "Qwen 同步失败：请使用百炼高级监控的 Prometheus 公网地址与 AccessKey；Coding Plan Key 不支持自动查询",
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

fn daily_usage_history(app: &tauri::AppHandle) -> Result<DailyUsageHistory, CommandError> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|_| CommandError::credential())?;
    Ok(DailyUsageHistory::new(&app_data))
}

fn record_daily_usage(
    app: &tauri::AppHandle,
    record: DailyUsageRecord,
) -> Result<(), CommandError> {
    daily_usage_history(app)?
        .upsert(record)
        .map_err(|_| CommandError::credential())
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

fn online_error(provider: OnlineProvider, error: OnlineError) -> CommandError {
    match error {
        OnlineError::InvalidProvider => CommandError::invalid_provider(),
        OnlineError::InvalidCredential if provider == OnlineProvider::OpenAiCodex => {
            CommandError::openai_admin_provider()
        }
        OnlineError::InvalidCredential if provider == OnlineProvider::ClaudeCode => {
            CommandError::claude_code_admin_provider()
        }
        OnlineError::InvalidCredential if provider == OnlineProvider::Gemini => {
            CommandError::gemini_monitoring_provider()
        }
        OnlineError::InvalidCredential
            if matches!(
                provider,
                OnlineProvider::QwenCn | OnlineProvider::QwenGlobal
            ) =>
        {
            CommandError::qwen_monitoring_provider()
        }
        OnlineError::InvalidCredential => CommandError::online_provider(),
        OnlineError::InvalidJson | OnlineError::ApiRejected | OnlineError::SchemaMismatch
            if provider == OnlineProvider::KimiCn =>
        {
            CommandError::kimi_code_provider()
        }
        OnlineError::InvalidJson | OnlineError::ApiRejected | OnlineError::SchemaMismatch
            if matches!(
                provider,
                OnlineProvider::MiniMaxCn | OnlineProvider::MiniMaxGlobal
            ) =>
        {
            CommandError::minimax_token_plan_provider()
        }
        OnlineError::InvalidJson | OnlineError::ApiRejected | OnlineError::SchemaMismatch
            if provider == OnlineProvider::OpenAiCodex =>
        {
            CommandError::openai_admin_provider()
        }
        OnlineError::InvalidJson | OnlineError::ApiRejected | OnlineError::SchemaMismatch
            if provider == OnlineProvider::ClaudeCode =>
        {
            CommandError::claude_code_admin_provider()
        }
        OnlineError::InvalidJson | OnlineError::ApiRejected | OnlineError::SchemaMismatch
            if provider == OnlineProvider::Gemini =>
        {
            CommandError::gemini_monitoring_provider()
        }
        OnlineError::InvalidJson | OnlineError::ApiRejected | OnlineError::SchemaMismatch
            if matches!(
                provider,
                OnlineProvider::QwenCn | OnlineProvider::QwenGlobal
            ) =>
        {
            CommandError::qwen_monitoring_provider()
        }
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
pub fn load_daily_usage(app: tauri::AppHandle) -> Vec<DailyUsageRecord> {
    daily_usage_history(&app)
        .and_then(|history| history.load().map_err(|_| CommandError::credential()))
        .unwrap_or_default()
}

#[tauri::command(rename_all = "camelCase")]
pub async fn configure_glm(
    app: tauri::AppHandle,
    api_key: String,
    local_date: String,
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
    record_daily_usage(
        &app,
        DailyUsageRecord {
            date: local_date,
            provider_id: "glm".into(),
            requests: Some(snapshot.requests),
            total_tokens: Some(snapshot.total_tokens),
            estimated_cost_cny: None,
        },
    )?;
    Ok(snapshot)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn sync_glm(
    app: tauri::AppHandle,
    local_date: String,
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
    record_daily_usage(
        &app,
        DailyUsageRecord {
            date: local_date,
            provider_id: "glm".into(),
            requests: Some(snapshot.requests),
            total_tokens: Some(snapshot.total_tokens),
            estimated_cost_cny: None,
        },
    )?;
    Ok(snapshot)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn configure_online_provider(
    app: tauri::AppHandle,
    provider_id: String,
    api_key: String,
    local_date: String,
    start_time_ms: i64,
    end_time_ms: i64,
) -> Result<OnlineSnapshot, CommandError> {
    let provider = online_provider(&provider_id)?;
    let range = OnlineUsageRange::new(start_time_ms, end_time_ms)
        .map_err(|error| online_error(provider, error))?;
    let mut api_key = api_key;
    let client = match OnlineClient::new(provider, &api_key) {
        Ok(client) => client,
        Err(error) => {
            api_key.zeroize();
            return Err(online_error(provider, error));
        }
    };
    let snapshot = match client.fetch_snapshot_for_range(range).await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            api_key.zeroize();
            return Err(online_error(provider, error));
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
    record_daily_usage(
        &app,
        DailyUsageRecord {
            date: local_date,
            provider_id: provider.id().into(),
            requests: snapshot.requests,
            total_tokens: snapshot.total_tokens,
            estimated_cost_cny: snapshot.estimated_cost_cny,
        },
    )?;
    Ok(snapshot)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn sync_online_provider(
    app: tauri::AppHandle,
    provider_id: String,
    local_date: String,
    start_time_ms: i64,
    end_time_ms: i64,
) -> Result<OnlineSnapshot, CommandError> {
    let provider = online_provider(&provider_id)?;
    let range = OnlineUsageRange::new(start_time_ms, end_time_ms)
        .map_err(|error| online_error(provider, error))?;
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
            return Err(online_error(provider, error));
        }
    };
    api_key.zeroize();
    let snapshot = client
        .fetch_snapshot_for_range(range)
        .await
        .map_err(|error| online_error(provider, error))?;
    cache_snapshot(&app, provider.id(), "online", &snapshot)?;
    record_daily_usage(
        &app,
        DailyUsageRecord {
            date: local_date,
            provider_id: provider.id().into(),
            requests: snapshot.requests,
            total_tokens: snapshot.total_tokens,
            estimated_cost_cny: snapshot.estimated_cost_cny,
        },
    )?;
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explains_kimi_code_and_minimax_key_types() {
        let kimi = online_error(OnlineProvider::KimiCn, OnlineError::ApiRejected);
        assert_eq!(kimi.code, "KIMI_CODE_SYNC_FAILED");
        assert!(kimi.message.contains("Kimi 会员控制台"));
        assert!(kimi.message.contains("Moonshot"));

        let minimax = online_error(OnlineProvider::MiniMaxCn, OnlineError::ApiRejected);
        assert_eq!(minimax.code, "MINIMAX_TOKEN_PLAN_SYNC_FAILED");
        assert!(minimax.message.contains("订阅 Key"));
        assert!(minimax.message.contains("按量付费"));
    }

    #[test]
    fn explains_admin_and_monitoring_credentials_for_analytics_providers() {
        let openai = online_error(OnlineProvider::OpenAiCodex, OnlineError::ApiRejected);
        assert!(openai.message.contains("Admin API Key"));
        assert!(openai.message.contains("ChatGPT"));

        let claude = online_error(OnlineProvider::ClaudeCode, OnlineError::ApiRejected);
        assert!(claude.message.contains("Admin API Key"));
        assert!(claude.message.contains("个人"));

        let gemini = online_error(OnlineProvider::Gemini, OnlineError::ApiRejected);
        assert!(gemini.message.contains("OAuth"));
        assert!(gemini.message.contains("Project ID"));

        let qwen = online_error(OnlineProvider::QwenCn, OnlineError::ApiRejected);
        assert!(qwen.message.contains("Prometheus"));
        assert!(qwen.message.contains("Coding Plan"));
    }
}
