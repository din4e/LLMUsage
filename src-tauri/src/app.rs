use llm_usage_core::cache::{
    CachedSnapshot, DailyUsageHistory, DailyUsageRecord, SnapshotCache,
};
use llm_usage_core::providers::glm::{GlmClient, GlmUsageSnapshot};
use llm_usage_core::providers::online::{
    OnlineClient, OnlineError, OnlineProvider, OnlineSnapshot, OnlineUsageRange, ProviderInstance,
    split_instance_suffix,
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
        // Same code across platforms; the message points the user at the
        // correct credential store instead of always mentioning Windows.
        let message = if cfg!(target_os = "windows") {
            "无法访问 Windows 凭据管理器"
        } else if cfg!(target_os = "macos") {
            "无法访问 macOS 钥匙串（Keychain），请在系统钥匙串中允许访问，或删除该供应商后重新配置"
        } else {
            "无法访问本机凭据存储"
        };
        Self {
            code: "CREDENTIAL_ERROR",
            message,
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

    fn anthropic_admin_provider() -> Self {
        Self {
            code: "ANTHROPIC_API_SYNC_FAILED",
            message:
                "Anthropic API 同步失败：需要组织 Admin API Key；该报告统计 Messages API 用量，余额与 Claude Code 订阅额度不在此列",
        }
    }

    fn xai_management_provider() -> Self {
        Self {
            code: "XAI_MANAGEMENT_SYNC_FAILED",
            message: "xAI 同步失败：需要控制台生成的 Management Key 和团队 ID；推理用 API Key 无法查询预付余额",
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

fn glm_vault(app: &tauri::AppHandle, instance_id: &str) -> Result<SecretVault, CommandError> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|_| CommandError::credential())?;
    SecretVault::new(&app_data, instance_id).map_err(|_| CommandError::credential())
}

/// Validates a GLM instance id: `glm` (instance 1) or `glm_2` and up.
fn glm_instance(provider_id: &str) -> Result<String, CommandError> {
    if provider_id == "glm" {
        return Ok(provider_id.to_string());
    }
    let (base, _index) =
        split_instance_suffix(provider_id).ok_or_else(CommandError::invalid_provider)?;
    if base != "glm" {
        return Err(CommandError::invalid_provider());
    }
    Ok(provider_id.to_string())
}

/// Maps a credential file stem to its base provider id and instance index.
/// Unknown or unsafe stems are ignored when listing configured instances.
fn credential_instance(value: &str) -> Option<(String, u32)> {
    if value == "glm" || OnlineProvider::from_id(value).is_some() {
        return Some((value.to_string(), 1));
    }
    let (base, index) = split_instance_suffix(value)?;
    if base == "glm" || OnlineProvider::from_id(base).is_some() {
        Some((base.to_string(), index))
    } else {
        None
    }
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

fn provider_vault(app: &tauri::AppHandle, instance_id: &str) -> Result<SecretVault, CommandError> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|_| CommandError::credential())?;
    SecretVault::new(&app_data, instance_id).map_err(|_| CommandError::credential())
}

fn online_instance(provider_id: &str) -> Result<ProviderInstance, CommandError> {
    OnlineProvider::parse_instance(provider_id).ok_or_else(CommandError::invalid_provider)
}

/// Stamps a fetched snapshot with the instance identity so the frontend,
/// snapshot cache, and daily history all address the same instance row.
fn apply_instance_identity(snapshot: &mut OnlineSnapshot, instance: &ProviderInstance) {
    snapshot.provider_id = instance.id.clone();
    if instance.index >= 2 {
        snapshot.label = format!("{} · 实例 {}", snapshot.label, instance.index);
    }
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
        OnlineError::InvalidCredential if provider == OnlineProvider::AnthropicApi => {
            CommandError::anthropic_admin_provider()
        }
        OnlineError::InvalidCredential if provider == OnlineProvider::Xai => {
            CommandError::xai_management_provider()
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
            if provider == OnlineProvider::AnthropicApi =>
        {
            CommandError::anthropic_admin_provider()
        }
        OnlineError::InvalidJson | OnlineError::ApiRejected | OnlineError::SchemaMismatch
            if provider == OnlineProvider::Xai =>
        {
            CommandError::xai_management_provider()
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
pub fn list_provider_instances(app: tauri::AppHandle) -> Vec<String> {
    let app_data = match app.path().app_data_dir() {
        Ok(app_data) => app_data,
        Err(_) => return Vec::new(),
    };
    let entries = match std::fs::read_dir(app_data.join("credentials")) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    let mut instances: Vec<(String, u32, String)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("dpapi") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if let Some((base, index)) = credential_instance(stem) {
            instances.push((base, index, stem.to_string()));
        }
    }
    instances.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then(left.1.cmp(&right.1))
            .then(left.2.cmp(&right.2))
    });
    instances.into_iter().map(|(_, _, id)| id).collect()
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
    provider_id: String,
    api_key: String,
    local_date: String,
    slot: Option<i16>,
    start_time: String,
    end_time: String,
) -> Result<GlmUsageSnapshot, CommandError> {
    let instance_id = glm_instance(&provider_id)?;
    let mut api_key = api_key;
    let client = GlmClient::new(&api_key).map_err(|_| CommandError::provider())?;
    let snapshot = match client.fetch_snapshot(&start_time, &end_time).await {
        Ok(snapshot) => snapshot,
        Err(_) => {
            api_key.zeroize();
            return Err(CommandError::provider());
        }
    };
    if let Err(error) = glm_vault(&app, &instance_id)?.save(api_key.trim()) {
        api_key.zeroize();
        return Err(match error {
            SecretError::Invalid
            | SecretError::Protect
            | SecretError::Io
            | SecretError::Missing => CommandError::credential(),
        });
    }
    api_key.zeroize();
    cache_snapshot(&app, &instance_id, "glm", &snapshot)?;
    record_daily_usage(
        &app,
        DailyUsageRecord {
            date: local_date,
            slot,
            provider_id: instance_id,
            requests: Some(snapshot.requests),
            total_tokens: Some(snapshot.total_tokens),
            estimated_cost_cny: None,
            balance_cny: None,
        },
    )?;
    Ok(snapshot)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn sync_glm(
    app: tauri::AppHandle,
    provider_id: String,
    local_date: String,
    slot: Option<i16>,
    start_time: String,
    end_time: String,
) -> Result<GlmUsageSnapshot, CommandError> {
    let instance_id = glm_instance(&provider_id)?;
    let mut api_key = glm_vault(&app, &instance_id)?.load().map_err(|error| match error {
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
    cache_snapshot(&app, &instance_id, "glm", &snapshot)?;
    record_daily_usage(
        &app,
        DailyUsageRecord {
            date: local_date,
            slot,
            provider_id: instance_id,
            requests: Some(snapshot.requests),
            total_tokens: Some(snapshot.total_tokens),
            estimated_cost_cny: None,
            balance_cny: None,
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
    slot: Option<i16>,
    start_time_ms: i64,
    end_time_ms: i64,
) -> Result<OnlineSnapshot, CommandError> {
    let instance = online_instance(&provider_id)?;
    let range = OnlineUsageRange::new(start_time_ms, end_time_ms)
        .map_err(|error| online_error(instance.provider, error))?;
    let mut api_key = api_key;
    let client = match OnlineClient::new(instance.provider, &api_key) {
        Ok(client) => client,
        Err(error) => {
            api_key.zeroize();
            return Err(online_error(instance.provider, error));
        }
    };
    let mut snapshot = match client.fetch_snapshot_for_range(range).await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            api_key.zeroize();
            return Err(online_error(instance.provider, error));
        }
    };
    if provider_vault(&app, &instance.id)?.save(api_key.trim()).is_err() {
        api_key.zeroize();
        return Err(CommandError::credential());
    }
    api_key.zeroize();
    apply_instance_identity(&mut snapshot, &instance);
    cache_snapshot(&app, &instance.id, "online", &snapshot)?;
    record_daily_usage(
        &app,
        DailyUsageRecord {
            date: local_date,
            slot,
            provider_id: instance.id.clone(),
            requests: snapshot.requests,
            total_tokens: snapshot.total_tokens,
            estimated_cost_cny: snapshot.estimated_cost_cny,
            balance_cny: snapshot.balance_cny,
        },
    )?;
    Ok(snapshot)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn sync_online_provider(
    app: tauri::AppHandle,
    provider_id: String,
    local_date: String,
    slot: Option<i16>,
    start_time_ms: i64,
    end_time_ms: i64,
) -> Result<OnlineSnapshot, CommandError> {
    let instance = online_instance(&provider_id)?;
    let range = OnlineUsageRange::new(start_time_ms, end_time_ms)
        .map_err(|error| online_error(instance.provider, error))?;
    let mut api_key = provider_vault(&app, &instance.id)?
        .load()
        .map_err(|error| match error {
            SecretError::Missing => CommandError::not_configured(),
            _ => CommandError::credential(),
        })?;
    let client = match OnlineClient::new(instance.provider, &api_key) {
        Ok(client) => client,
        Err(error) => {
            api_key.zeroize();
            return Err(online_error(instance.provider, error));
        }
    };
    api_key.zeroize();
    let mut snapshot = client
        .fetch_snapshot_for_range(range)
        .await
        .map_err(|error| online_error(instance.provider, error))?;
    apply_instance_identity(&mut snapshot, &instance);
    cache_snapshot(&app, &instance.id, "online", &snapshot)?;
    record_daily_usage(
        &app,
        DailyUsageRecord {
            date: local_date,
            slot,
            provider_id: instance.id.clone(),
            requests: snapshot.requests,
            total_tokens: snapshot.total_tokens,
            estimated_cost_cny: snapshot.estimated_cost_cny,
            balance_cny: snapshot.balance_cny,
        },
    )?;
    Ok(snapshot)
}

/// Returns the decrypted credential for an already-configured instance so the
/// edit dialog can prefill (and reveal) the previously saved values.
#[tauri::command(rename_all = "camelCase")]
pub fn load_provider_credential(
    app: tauri::AppHandle,
    provider_id: String,
) -> Result<String, CommandError> {
    // Same instance-id dispatch as delete_provider: GLM ids plus online ids.
    let instance_id = if provider_id == "glm" || provider_id.starts_with("glm_") {
        glm_instance(&provider_id)?
    } else {
        online_instance(&provider_id)?.id
    };
    provider_vault(&app, &instance_id)?
        .load()
        .map_err(|error| match error {
            SecretError::Missing => CommandError::not_configured(),
            _ => CommandError::credential(),
        })
}

#[tauri::command(rename_all = "camelCase")]
pub fn delete_provider(app: tauri::AppHandle, provider_id: String) -> Result<(), CommandError> {
    // 1. Remove the stored credential. Forgetting a provider is idempotent:
    //    `SecretVault::delete` swallows a Missing entry (never configured, or
    //    already removed) so the UI never has to special-case prior state.
    //    GLM instance ids (`glm`, `glm_2`, …) share the vault-by-id helper;
    //    every other id must resolve to a known online provider instance.
    let instance_id = if provider_id == "glm" || provider_id.starts_with("glm_") {
        glm_instance(&provider_id)?
    } else {
        online_instance(&provider_id)?.id
    };
    let secret_result = match provider_vault(&app, &instance_id) {
        Ok(vault) => vault.delete(),
        Err(command_error) => return Err(command_error),
    };
    if secret_result.is_err() {
        // Only genuine storage failures survive past the idempotent delete().
        return Err(CommandError::credential());
    }
    // 2. Drop the cached snapshot so the dashboard does not keep stale data.
    snapshot_cache(&app)?
        .delete(&provider_id)
        .map_err(|_| CommandError::credential())?;
    Ok(())
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

    #[test]
    fn explains_anthropic_api_and_xai_management_credentials() {
        let anthropic = online_error(OnlineProvider::AnthropicApi, OnlineError::ApiRejected);
        assert_eq!(anthropic.code, "ANTHROPIC_API_SYNC_FAILED");
        assert!(anthropic.message.contains("Admin API Key"));
        assert!(anthropic.message.contains("Messages"));

        let xai = online_error(OnlineProvider::Xai, OnlineError::InvalidCredential);
        assert_eq!(xai.code, "XAI_MANAGEMENT_SYNC_FAILED");
        assert!(xai.message.contains("Management Key"));
        assert!(xai.message.contains("团队 ID"));
    }

    #[test]
    fn validates_glm_instance_ids_for_every_command() {
        assert_eq!(glm_instance("glm").ok().as_deref(), Some("glm"));
        assert_eq!(glm_instance("glm_2").ok().as_deref(), Some("glm_2"));
        assert_eq!(glm_instance("glm_12").ok().as_deref(), Some("glm_12"));

        assert!(glm_instance("kimi_cn").is_err());
        assert!(glm_instance("kimi_cn_2").is_err());
        assert!(glm_instance("glm_1").is_err());
        assert!(glm_instance("glm_02").is_err());
        assert!(glm_instance("../glm_2").is_err());
        assert!(glm_instance("").is_err());
    }

    #[test]
    fn recognizes_credential_files_of_every_provider_instance() {
        assert_eq!(
            credential_instance("glm"),
            Some(("glm".to_string(), 1))
        );
        assert_eq!(
            credential_instance("glm_3"),
            Some(("glm".to_string(), 3))
        );
        assert_eq!(
            credential_instance("kimi_cn"),
            Some(("kimi_cn".to_string(), 1))
        );
        assert_eq!(
            credential_instance("qwen_global_2"),
            Some(("qwen_global".to_string(), 2))
        );

        assert_eq!(credential_instance("unknown"), None);
        assert_eq!(credential_instance("kimi_cn_1"), None);
        assert_eq!(credential_instance("GLM"), None);
    }

    #[test]
    fn stamps_snapshots_with_the_instance_identity() {
        let mut snapshot = empty_snapshot("kimi_cn", "Kimi Code");
        let base = OnlineProvider::parse_instance("kimi_cn").expect("base instance");
        apply_instance_identity(&mut snapshot, &base);
        assert_eq!(snapshot.provider_id, "kimi_cn");
        assert_eq!(snapshot.label, "Kimi Code");

        let second = OnlineProvider::parse_instance("kimi_cn_2").expect("second instance");
        apply_instance_identity(&mut snapshot, &second);
        assert_eq!(snapshot.provider_id, "kimi_cn_2");
        assert_eq!(snapshot.label, "Kimi Code · 实例 2");
    }

    fn empty_snapshot(provider_id: &str, label: &str) -> OnlineSnapshot {
        OnlineSnapshot {
            provider_id: provider_id.to_string(),
            label: label.to_string(),
            source: "official_balance".to_string(),
            experimental: false,
            balance_cny: None,
            balance_original: None,
            quota_used_percent: None,
            cooldown_ends_at_ms: None,
            requests: None,
            total_tokens: None,
            estimated_cost_cny: None,
            primary_label: "余额".to_string(),
            primary_value: "¥0.00".to_string(),
            secondary_value: String::new(),
            detail_sections: Vec::new(),
        }
    }
}
