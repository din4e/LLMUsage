use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnlineProvider {
    KimiCn,
    KimiGlobal,
    DeepSeek,
    MiniMaxCn,
    MiniMaxGlobal,
    SiliconFlowCn,
    SiliconFlowGlobal,
    OpenRouter,
}

impl OnlineProvider {
    pub fn from_id(value: &str) -> Option<Self> {
        match value {
            "kimi_cn" => Some(Self::KimiCn),
            "kimi_global" => Some(Self::KimiGlobal),
            "deepseek" => Some(Self::DeepSeek),
            "minimax_cn" => Some(Self::MiniMaxCn),
            "minimax_global" => Some(Self::MiniMaxGlobal),
            "siliconflow_cn" => Some(Self::SiliconFlowCn),
            "siliconflow_global" => Some(Self::SiliconFlowGlobal),
            "openrouter" => Some(Self::OpenRouter),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::KimiCn => "kimi_cn",
            Self::KimiGlobal => "kimi_global",
            Self::DeepSeek => "deepseek",
            Self::MiniMaxCn => "minimax_cn",
            Self::MiniMaxGlobal => "minimax_global",
            Self::SiliconFlowCn => "siliconflow_cn",
            Self::SiliconFlowGlobal => "siliconflow_global",
            Self::OpenRouter => "openrouter",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::KimiCn => "Kimi Code",
            Self::KimiGlobal => "Kimi Global",
            Self::DeepSeek => "DeepSeek",
            Self::MiniMaxCn => "MiniMax 国内",
            Self::MiniMaxGlobal => "MiniMax Global",
            Self::SiliconFlowCn => "硅基流动",
            Self::SiliconFlowGlobal => "SiliconFlow Global",
            Self::OpenRouter => "OpenRouter",
        }
    }

    fn endpoints(self) -> &'static [&'static str] {
        match self {
            Self::KimiCn => &[
                "https://api.kimi.com/coding/v1/usages",
                "https://api.moonshot.cn/v1/users/me/balance",
            ],
            Self::KimiGlobal => &["https://api.moonshot.ai/v1/users/me/balance"],
            Self::DeepSeek => &["https://api.deepseek.com/user/balance"],
            Self::MiniMaxCn => &[
                "https://www.minimaxi.com/v1/token_plan/remains",
                "https://api.minimaxi.com/v1/token_plan/remains",
            ],
            Self::MiniMaxGlobal => &[
                "https://www.minimax.io/v1/token_plan/remains",
                "https://api.minimax.io/v1/token_plan/remains",
            ],
            Self::SiliconFlowCn => &["https://api.siliconflow.cn/v1/user/info"],
            Self::SiliconFlowGlobal => &["https://api.siliconflow.com/v1/user/info"],
            Self::OpenRouter => &["https://openrouter.ai/api/v1/credits"],
        }
    }

    fn source(self) -> &'static str {
        match self {
            Self::KimiCn => "experimental_kimi_code",
            Self::KimiGlobal => "official_balance",
            Self::DeepSeek => "official_balance",
            Self::SiliconFlowCn | Self::SiliconFlowGlobal => "official_balance",
            Self::OpenRouter => "official_credits",
            Self::MiniMaxCn | Self::MiniMaxGlobal => "experimental_token_plan",
        }
    }

    fn experimental(self) -> bool {
        matches!(self, Self::KimiCn | Self::MiniMaxCn | Self::MiniMaxGlobal)
    }
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnlineSnapshot {
    pub provider_id: String,
    pub label: String,
    pub source: String,
    pub experimental: bool,
    pub balance_cny: Option<f64>,
    pub balance_original: Option<Money>,
    pub quota_used_percent: Option<f64>,
    pub cooldown_ends_at_ms: Option<i64>,
    pub requests: Option<u64>,
    pub total_tokens: Option<u64>,
    pub estimated_cost_cny: Option<f64>,
    pub primary_label: String,
    pub primary_value: String,
    pub secondary_value: String,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Money {
    pub amount: f64,
    pub currency: String,
}

#[derive(Debug, PartialEq)]
pub enum OnlineError {
    InvalidProvider,
    InvalidCredential,
    InvalidJson,
    ApiRejected,
    SchemaMismatch,
    RequestFailed,
}

impl OnlineError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidProvider => "ONLINE_INVALID_PROVIDER",
            Self::InvalidCredential => "ONLINE_INVALID_CREDENTIAL",
            Self::InvalidJson => "ONLINE_INVALID_JSON",
            Self::ApiRejected => "ONLINE_API_REJECTED",
            Self::SchemaMismatch => "ONLINE_SCHEMA_MISMATCH",
            Self::RequestFailed => "ONLINE_REQUEST_FAILED",
        }
    }
}

#[derive(Debug)]
pub struct OnlineClient {
    provider: OnlineProvider,
    client: reqwest::Client,
    authorization: reqwest::header::HeaderValue,
    kimi_code_credential: bool,
}

impl OnlineClient {
    pub fn new(provider: OnlineProvider, api_key: &str) -> Result<Self, OnlineError> {
        let trimmed = api_key.trim();
        if trimmed.is_empty() || trimmed.len() > 4096 {
            return Err(OnlineError::InvalidCredential);
        }

        let mut authorization =
            reqwest::header::HeaderValue::from_str(&format!("Bearer {trimmed}"))
                .map_err(|_| OnlineError::InvalidCredential)?;
        authorization.set_sensitive(true);
        let client = reqwest::Client::builder()
            .https_only(true)
            .timeout(Duration::from_secs(15))
            .user_agent("LLMUsage/0.1")
            .build()
            .map_err(|_| OnlineError::RequestFailed)?;

        Ok(Self {
            provider,
            client,
            authorization,
            kimi_code_credential: trimmed.starts_with("sk-kimi-"),
        })
    }

    pub fn request(&self) -> Result<reqwest::Request, OnlineError> {
        let endpoint = self
            .request_endpoints()
            .first()
            .ok_or(OnlineError::InvalidProvider)?;
        self.request_for(endpoint)
    }

    fn requests(&self) -> Result<Vec<reqwest::Request>, OnlineError> {
        self.request_endpoints()
            .iter()
            .map(|endpoint| self.request_for(endpoint))
            .collect()
    }

    fn request_endpoints(&self) -> &[&'static str] {
        let endpoints = self.provider.endpoints();
        if self.provider != OnlineProvider::KimiCn {
            return endpoints;
        }
        if self.kimi_code_credential {
            &endpoints[..1]
        } else {
            &endpoints[1..]
        }
    }

    fn request_for(&self, endpoint: &str) -> Result<reqwest::Request, OnlineError> {
        self.client
            .get(endpoint)
            .header(reqwest::header::AUTHORIZATION, self.authorization.clone())
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .build()
            .map_err(|_| OnlineError::RequestFailed)
    }

    pub async fn fetch_snapshot(&self) -> Result<OnlineSnapshot, OnlineError> {
        let mut last_error = OnlineError::RequestFailed;
        for request in self.requests()? {
            let response = match self.client.execute(request).await {
                Ok(response) => response,
                Err(_) => {
                    last_error = OnlineError::RequestFailed;
                    continue;
                }
            };
            if !response.status().is_success() {
                last_error = OnlineError::ApiRejected;
                continue;
            }
            let text = match response.text().await {
                Ok(text) => text,
                Err(_) => {
                    last_error = OnlineError::RequestFailed;
                    continue;
                }
            };
            match parse_snapshot(self.provider, &text) {
                Ok(snapshot) => return Ok(snapshot),
                Err(error) => last_error = error,
            }
        }
        Err(last_error)
    }
}

pub fn parse_snapshot(provider: OnlineProvider, json: &str) -> Result<OnlineSnapshot, OnlineError> {
    match provider {
        OnlineProvider::KimiCn => {
            parse_kimi_code(provider, json).or_else(|_| parse_kimi(provider, json))
        }
        OnlineProvider::KimiGlobal => parse_kimi(provider, json),
        OnlineProvider::DeepSeek => parse_deepseek(provider, json),
        OnlineProvider::MiniMaxCn | OnlineProvider::MiniMaxGlobal => parse_minimax(provider, json),
        OnlineProvider::SiliconFlowCn | OnlineProvider::SiliconFlowGlobal => {
            parse_siliconflow(provider, json)
        }
        OnlineProvider::OpenRouter => parse_openrouter(provider, json),
    }
}

#[derive(Deserialize)]
struct KimiBalanceResponse {
    status: bool,
    data: Option<KimiBalance>,
}

#[derive(Deserialize)]
struct KimiBalance {
    available_balance: f64,
    voucher_balance: f64,
    cash_balance: f64,
}

fn parse_kimi(provider: OnlineProvider, json: &str) -> Result<OnlineSnapshot, OnlineError> {
    let response: KimiBalanceResponse =
        serde_json::from_str(json).map_err(|_| OnlineError::InvalidJson)?;
    let data = response
        .status
        .then_some(response.data)
        .flatten()
        .ok_or(OnlineError::ApiRejected)?;
    if !data.available_balance.is_finite()
        || !data.voucher_balance.is_finite()
        || !data.cash_balance.is_finite()
    {
        return Err(OnlineError::SchemaMismatch);
    }
    let mut snapshot = balance_snapshot(
        provider,
        data.available_balance,
        "CNY",
        format!(
            "现金 ¥{:.2} · 赠金 ¥{:.2}",
            data.cash_balance, data.voucher_balance
        ),
    );
    if provider == OnlineProvider::KimiCn {
        snapshot.label = "Moonshot API 国内".to_string();
        snapshot.source = "official_balance".to_string();
        snapshot.experimental = false;
    }
    Ok(snapshot)
}

fn parse_kimi_code(provider: OnlineProvider, json: &str) -> Result<OnlineSnapshot, OnlineError> {
    let value: Value = serde_json::from_str(json).map_err(|_| OnlineError::InvalidJson)?;
    let weekly = value.get("usage").ok_or(OnlineError::SchemaMismatch)?;
    let weekly_quota = parse_quota(weekly)?;
    let limits = value
        .get("limits")
        .and_then(Value::as_array)
        .ok_or(OnlineError::SchemaMismatch)?;
    let five_hour = limits
        .iter()
        .find(|limit| is_five_hour_window(limit))
        .or_else(|| limits.first())
        .and_then(|limit| limit.get("detail"))
        .ok_or(OnlineError::SchemaMismatch)?;
    let five_hour_quota = parse_quota(five_hour)?;
    let reset_at = timestamp_field(five_hour, "resetTime");

    Ok(OnlineSnapshot {
        provider_id: provider.id().to_string(),
        label: provider.label().to_string(),
        source: provider.source().to_string(),
        experimental: provider.experimental(),
        balance_cny: None,
        balance_original: None,
        quota_used_percent: Some(five_hour_quota.used_percent),
        cooldown_ends_at_ms: reset_at,
        requests: None,
        total_tokens: None,
        estimated_cost_cny: None,
        primary_label: "5 小时用量".to_string(),
        primary_value: format!("{:.1}%", five_hour_quota.used_percent),
        secondary_value: format!(
            "5 小时剩余 {} · 周额度剩余 {}",
            format_percent(five_hour_quota.remaining_percent),
            format_percent(weekly_quota.remaining_percent)
        ),
    })
}

struct ParsedQuota {
    used_percent: f64,
    remaining_percent: f64,
}

fn parse_quota(value: &Value) -> Result<ParsedQuota, OnlineError> {
    let limit = value
        .get("limit")
        .and_then(number_like_f64)
        .ok_or(OnlineError::SchemaMismatch)?;
    let remaining = value
        .get("remaining")
        .and_then(number_like_f64)
        .ok_or(OnlineError::SchemaMismatch)?;
    if !limit.is_finite()
        || !remaining.is_finite()
        || limit <= 0.0
        || remaining < 0.0
        || remaining > limit
    {
        return Err(OnlineError::SchemaMismatch);
    }
    let remaining_percent = remaining / limit * 100.0;
    Ok(ParsedQuota {
        used_percent: 100.0 - remaining_percent,
        remaining_percent,
    })
}

fn is_five_hour_window(value: &Value) -> bool {
    let Some(window) = value.get("window") else {
        return false;
    };
    let duration = window.get("duration").and_then(number_like_i64);
    let unit = window
        .get("timeUnit")
        .and_then(Value::as_str)
        .unwrap_or_default();
    duration == Some(300) && unit.to_ascii_uppercase().contains("MINUTE")
}

fn format_percent(value: f64) -> String {
    if value.fract().abs() < f64::EPSILON {
        format!("{value:.0}%")
    } else {
        format!("{value:.1}%")
    }
}

#[derive(Deserialize)]
struct DeepSeekBalanceResponse {
    is_available: bool,
    balance_infos: Vec<DeepSeekBalance>,
}

#[derive(Deserialize)]
struct DeepSeekBalance {
    currency: String,
    total_balance: String,
    granted_balance: String,
    topped_up_balance: String,
}

fn parse_deepseek(provider: OnlineProvider, json: &str) -> Result<OnlineSnapshot, OnlineError> {
    let response: DeepSeekBalanceResponse =
        serde_json::from_str(json).map_err(|_| OnlineError::InvalidJson)?;
    let balance = response
        .balance_infos
        .into_iter()
        .find(|item| item.currency == "CNY")
        .ok_or(OnlineError::SchemaMismatch)?;
    let total = parse_money(&balance.total_balance)?;
    let granted = parse_money(&balance.granted_balance)?;
    let topped_up = parse_money(&balance.topped_up_balance)?;
    let mut snapshot = balance_snapshot(
        provider,
        total,
        "CNY",
        format!("充值 ¥{topped_up:.2} · 赠金 ¥{granted:.2}"),
    );
    if !response.is_available {
        snapshot.secondary_value = "余额不足或账号不可用".to_string();
    }
    Ok(snapshot)
}

fn parse_minimax(provider: OnlineProvider, json: &str) -> Result<OnlineSnapshot, OnlineError> {
    let value: Value = serde_json::from_str(json).map_err(|_| OnlineError::InvalidJson)?;
    if looks_like_minimax_rejection(&value) {
        return Err(OnlineError::ApiRejected);
    }
    let quota = find_minimax_count_quota(&value)
        .or_else(|| {
            let remaining_percent = find_f64_key(&value, "usage_percent")
                .or_else(|| find_f64_key(&value, "usagePercent"))?;
            if !remaining_percent.is_finite() || !(0.0..=100.0).contains(&remaining_percent) {
                None
            } else {
                let used_percent = 100.0 - remaining_percent;
                Some(MiniMaxQuota {
                    used_percent,
                    detail: format!("剩余 {remaining_percent:.1}%"),
                    reset_at_ms: None,
                })
            }
        })
        .ok_or(OnlineError::SchemaMismatch)?;
    let cooldown = quota.reset_at_ms.or_else(|| find_reset_timestamp(&value));
    Ok(OnlineSnapshot {
        provider_id: provider.id().to_string(),
        label: provider.label().to_string(),
        source: provider.source().to_string(),
        experimental: provider.experimental(),
        balance_cny: None,
        balance_original: None,
        quota_used_percent: Some(quota.used_percent),
        cooldown_ends_at_ms: cooldown,
        requests: None,
        total_tokens: None,
        estimated_cost_cny: None,
        primary_label: "套餐用量".to_string(),
        primary_value: format!("{:.1}%", quota.used_percent),
        secondary_value: quota.detail,
    })
}

#[derive(Deserialize)]
struct SiliconFlowUserResponse {
    code: i64,
    status: bool,
    data: Option<SiliconFlowUser>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SiliconFlowUser {
    balance: String,
    charge_balance: String,
    total_balance: String,
    status: String,
}

fn parse_siliconflow(provider: OnlineProvider, json: &str) -> Result<OnlineSnapshot, OnlineError> {
    let response: SiliconFlowUserResponse =
        serde_json::from_str(json).map_err(|_| OnlineError::InvalidJson)?;
    if response.code != 20000 || !response.status {
        return Err(OnlineError::ApiRejected);
    }
    let data = response.data.ok_or(OnlineError::SchemaMismatch)?;
    let total = parse_money(&data.total_balance)?;
    let free = parse_money(&data.balance)?;
    let charged = parse_money(&data.charge_balance)?;
    let mut snapshot = balance_snapshot(
        provider,
        total,
        "CNY",
        format!("充值 ¥{charged:.2} · 赠金 ¥{free:.2}"),
    );
    if data.status != "normal" {
        snapshot.secondary_value = format!("账号状态：{}", data.status);
    }
    Ok(snapshot)
}

#[derive(Deserialize)]
struct OpenRouterCreditsResponse {
    data: OpenRouterCredits,
}

#[derive(Deserialize)]
struct OpenRouterCredits {
    total_credits: f64,
    total_usage: f64,
}

fn parse_openrouter(provider: OnlineProvider, json: &str) -> Result<OnlineSnapshot, OnlineError> {
    let response: OpenRouterCreditsResponse =
        serde_json::from_str(json).map_err(|_| OnlineError::InvalidJson)?;
    if !response.data.total_credits.is_finite()
        || !response.data.total_usage.is_finite()
        || response.data.total_credits < response.data.total_usage
    {
        return Err(OnlineError::SchemaMismatch);
    }
    let remaining = response.data.total_credits - response.data.total_usage;
    Ok(balance_snapshot(
        provider,
        remaining,
        "USD",
        format!(
            "总额 ${:.2} · 已用 ${:.2}",
            response.data.total_credits, response.data.total_usage
        ),
    ))
}

fn balance_snapshot(
    provider: OnlineProvider,
    amount: f64,
    currency: &str,
    secondary_value: String,
) -> OnlineSnapshot {
    OnlineSnapshot {
        provider_id: provider.id().to_string(),
        label: provider.label().to_string(),
        source: provider.source().to_string(),
        experimental: provider.experimental(),
        balance_cny: (currency == "CNY").then_some(amount),
        balance_original: Some(Money {
            amount,
            currency: currency.to_string(),
        }),
        quota_used_percent: None,
        cooldown_ends_at_ms: None,
        requests: None,
        total_tokens: None,
        estimated_cost_cny: None,
        primary_label: "可用余额".to_string(),
        primary_value: format_money(amount, currency),
        secondary_value,
    }
}

fn format_money(amount: f64, currency: &str) -> String {
    match currency {
        "CNY" => format!("¥{amount:.2}"),
        "USD" => format!("${amount:.2}"),
        _ => format!("{amount:.2} {currency}"),
    }
}

fn parse_money(value: &str) -> Result<f64, OnlineError> {
    let amount: f64 = value.parse().map_err(|_| OnlineError::SchemaMismatch)?;
    amount
        .is_finite()
        .then_some(amount)
        .ok_or(OnlineError::SchemaMismatch)
}

fn looks_like_minimax_rejection(value: &Value) -> bool {
    let status_code = find_i64_key(value, "status_code")
        .or_else(|| find_i64_key(value, "code"))
        .unwrap_or(0);
    status_code != 0
}

struct MiniMaxQuota {
    used_percent: f64,
    detail: String,
    reset_at_ms: Option<i64>,
}

fn find_minimax_count_quota(value: &Value) -> Option<MiniMaxQuota> {
    match value {
        Value::Object(map) => {
            let pairs = [
                (
                    "current_interval_usage_count",
                    "current_interval_total_count",
                ),
                ("current_weekly_usage_count", "current_weekly_total_count"),
                ("usage_count", "total_count"),
                ("used", "total"),
            ];
            for (used_key, total_key) in pairs {
                let used = map.get(used_key).and_then(number_like_u64);
                let total = map.get(total_key).and_then(number_like_u64);
                if let (Some(used), Some(total)) = (used, total) {
                    if total == 0 || used > total {
                        continue;
                    }
                    let model = map
                        .get("model_name")
                        .or_else(|| map.get("modelName"))
                        .and_then(Value::as_str)
                        .filter(|name| !name.trim().is_empty());
                    let detail = match model {
                        Some(model) => format!("{model} · 已用 {used} / {total}"),
                        None => format!("已用 {used} / {total}"),
                    };
                    return Some(MiniMaxQuota {
                        used_percent: used as f64 / total as f64 * 100.0,
                        detail,
                        reset_at_ms: find_reset_timestamp(value),
                    });
                }
            }
            map.values().find_map(find_minimax_count_quota)
        }
        Value::Array(items) => items.iter().find_map(find_minimax_count_quota),
        _ => None,
    }
}

fn find_reset_timestamp(value: &Value) -> Option<i64> {
    const KEYS: [&str; 7] = [
        "next_reset_time",
        "nextResetTime",
        "reset_time",
        "resetTime",
        "end_time",
        "endTime",
        "expire_time",
    ];
    match value {
        Value::Object(map) => {
            for key in KEYS {
                if let Some(timestamp) = map.get(key).and_then(timestamp_value) {
                    return Some(timestamp);
                }
            }
            map.values().find_map(find_reset_timestamp)
        }
        Value::Array(items) => items.iter().find_map(find_reset_timestamp),
        _ => None,
    }
}

fn timestamp_field(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(timestamp_value)
}

fn timestamp_value(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64().and_then(normalize_epoch_ms),
        Value::String(text) => text
            .parse::<i64>()
            .ok()
            .and_then(normalize_epoch_ms)
            .or_else(|| {
                DateTime::parse_from_rfc3339(text)
                    .ok()
                    .and_then(|timestamp| normalize_epoch_ms(timestamp.timestamp_millis()))
            }),
        _ => None,
    }
}

fn normalize_epoch_ms(timestamp: i64) -> Option<i64> {
    if timestamp <= 0 {
        return None;
    }
    if timestamp < 100_000_000_000 {
        timestamp.checked_mul(1_000)
    } else {
        Some(timestamp)
    }
}

fn find_i64_key(value: &Value, key: &str) -> Option<i64> {
    match value {
        Value::Object(map) => {
            if let Some(found) = map.get(key).and_then(number_like_i64) {
                return Some(found);
            }
            map.values().find_map(|child| find_i64_key(child, key))
        }
        Value::Array(items) => items.iter().find_map(|child| find_i64_key(child, key)),
        _ => None,
    }
}

fn find_f64_key(value: &Value, key: &str) -> Option<f64> {
    match value {
        Value::Object(map) => {
            if let Some(found) = map.get(key).and_then(number_like_f64) {
                return Some(found);
            }
            map.values().find_map(|child| find_f64_key(child, key))
        }
        Value::Array(items) => items.iter().find_map(|child| find_f64_key(child, key)),
        _ => None,
    }
}

fn number_like_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

fn number_like_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

fn number_like_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_sensitive_bearer_requests_for_each_endpoint() {
        let client = OnlineClient::new(OnlineProvider::KimiCn, "sk-kimi-test").expect("client");
        let request = client.request().expect("request");

        assert_eq!(
            request.url().as_str(),
            "https://api.kimi.com/coding/v1/usages"
        );
        assert_eq!(request.headers()["authorization"], "Bearer sk-kimi-test");
        assert!(request.headers()["authorization"].is_sensitive());

        let minimax = OnlineClient::new(OnlineProvider::MiniMaxCn, "token-plan-key")
            .expect("client")
            .request()
            .expect("request");
        assert_eq!(
            minimax.url().as_str(),
            "https://www.minimaxi.com/v1/token_plan/remains"
        );
        assert_eq!(minimax.headers()["content-type"], "application/json");
    }

    #[test]
    fn routes_kimi_key_types_without_cross_product_fallback() {
        let kimi = OnlineClient::new(OnlineProvider::KimiCn, "sk-kimi-test")
            .expect("client")
            .requests()
            .expect("requests")
            .into_iter()
            .map(|request| request.url().to_string())
            .collect::<Vec<_>>();
        assert_eq!(kimi, vec!["https://api.kimi.com/coding/v1/usages"]);

        let moonshot = OnlineClient::new(OnlineProvider::KimiCn, "moonshot-test")
            .expect("client")
            .requests()
            .expect("requests")
            .into_iter()
            .map(|request| request.url().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            moonshot,
            vec!["https://api.moonshot.cn/v1/users/me/balance"]
        );
    }

    #[test]
    fn keeps_same_region_host_fallbacks_for_minimax() {
        let minimax = OnlineClient::new(OnlineProvider::MiniMaxCn, "sk-cp-test")
            .expect("client")
            .requests()
            .expect("requests")
            .into_iter()
            .map(|request| request.url().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            minimax,
            vec![
                "https://www.minimaxi.com/v1/token_plan/remains",
                "https://api.minimaxi.com/v1/token_plan/remains",
            ]
        );
    }

    #[test]
    fn parses_moonshot_balance_as_kimi_cn_fallback() {
        let json = r#"{
          "code": 0,
          "data": {"available_balance": 49.58894, "voucher_balance": 46.58893, "cash_balance": 3.00001},
          "scode": "0x0",
          "status": true
        }"#;

        let snapshot = parse_snapshot(OnlineProvider::KimiCn, json).expect("snapshot");

        assert_eq!(snapshot.provider_id, "kimi_cn");
        assert_eq!(snapshot.balance_cny, Some(49.58894));
        assert_eq!(snapshot.primary_value, "¥49.59");
        assert_eq!(snapshot.estimated_cost_cny, None);
        assert_eq!(snapshot.source, "official_balance");
        assert!(!snapshot.experimental);
    }

    #[test]
    fn parses_kimi_code_weekly_and_five_hour_usage() {
        let json = r#"{
          "usage": {
            "limit": "100",
            "used": "48",
            "remaining": "52",
            "resetTime": "2026-01-08T00:00:00Z"
          },
          "limits": [{
            "window": {"duration": 300, "timeUnit": "TIME_UNIT_MINUTE"},
            "detail": {
              "limit": "100",
              "used": "7",
              "remaining": "93",
              "resetTime": "2026-01-01T05:00:00Z"
            }
          }]
        }"#;

        let snapshot = parse_snapshot(OnlineProvider::KimiCn, json).expect("snapshot");

        assert_eq!(snapshot.provider_id, "kimi_cn");
        assert_eq!(snapshot.source, "experimental_kimi_code");
        assert!(snapshot.experimental);
        assert_eq!(snapshot.quota_used_percent, Some(7.0));
        assert_eq!(snapshot.cooldown_ends_at_ms, Some(1_767_243_600_000));
        assert_eq!(snapshot.primary_label, "5 小时用量");
        assert_eq!(snapshot.primary_value, "7.0%");
        assert_eq!(snapshot.secondary_value, "5 小时剩余 93% · 周额度剩余 52%");
    }

    #[test]
    fn keeps_kimi_code_usage_when_reset_time_is_absent() {
        let json = r#"{
          "usage": {"limit": "100", "remaining": "52"},
          "limits": [{
            "window": {"duration": "300", "timeUnit": "TIME_UNIT_MINUTE"},
            "detail": {"limit": 100, "remaining": 93}
          }]
        }"#;

        let snapshot = parse_snapshot(OnlineProvider::KimiCn, json).expect("snapshot");

        assert_eq!(snapshot.quota_used_percent, Some(7.0));
        assert_eq!(snapshot.cooldown_ends_at_ms, None);
        assert_eq!(snapshot.secondary_value, "5 小时剩余 93% · 周额度剩余 52%");
    }

    #[test]
    fn parses_deepseek_cny_balance() {
        let json = r#"{
          "is_available": true,
          "balance_infos": [{
            "currency": "CNY",
            "total_balance": "110.00",
            "granted_balance": "10.00",
            "topped_up_balance": "100.00"
          }]
        }"#;

        let snapshot = parse_snapshot(OnlineProvider::DeepSeek, json).expect("snapshot");

        assert_eq!(snapshot.balance_cny, Some(110.0));
        assert_eq!(snapshot.primary_value, "¥110.00");
        assert_eq!(snapshot.secondary_value, "充值 ¥100.00 · 赠金 ¥10.00");
    }

    #[test]
    fn parses_minimax_token_plan_usage_from_nested_shape() {
        let json = r#"{
          "base_resp": {"status_code": 0, "status_msg": ""},
          "data": [{
            "model_type": "general",
            "current_interval_total_count": 1000,
            "current_interval_usage_count": 375,
            "next_reset_time": 1783686600000
          }]
        }"#;

        let snapshot = parse_snapshot(OnlineProvider::MiniMaxGlobal, json).expect("snapshot");

        assert_eq!(snapshot.provider_id, "minimax_global");
        assert_eq!(snapshot.quota_used_percent, Some(37.5));
        assert_eq!(snapshot.cooldown_ends_at_ms, Some(1_783_686_600_000));
        assert!(snapshot.experimental);
    }

    #[test]
    fn parses_minimax_model_remains_and_normalizes_seconds_timestamp() {
        let json = r#"{
          "model_remains": [{
            "start_time": 1783668600,
            "end_time": 1783686600,
            "remains_time": 600000,
            "current_interval_total_count": 1000,
            "current_interval_usage_count": 375,
            "model_name": "MiniMax-M2.5"
          }],
          "base_resp": {"status_code": 0, "status_msg": "success"}
        }"#;

        let snapshot = parse_snapshot(OnlineProvider::MiniMaxCn, json).expect("snapshot");

        assert_eq!(snapshot.quota_used_percent, Some(37.5));
        assert_eq!(snapshot.cooldown_ends_at_ms, Some(1_783_686_600_000));
        assert_eq!(snapshot.secondary_value, "MiniMax-M2.5 · 已用 375 / 1000");
    }

    #[test]
    fn parses_minimax_remaining_percent_as_used_percent() {
        let json = r#"{
          "base_resp": {"status_code": 0, "status_msg": ""},
          "data": [{
            "model_type": "general",
            "usage_percent": 72.5,
            "end_time": 1783686600000
          }]
        }"#;

        let snapshot = parse_snapshot(OnlineProvider::MiniMaxCn, json).expect("snapshot");

        assert_eq!(snapshot.provider_id, "minimax_cn");
        assert_eq!(snapshot.quota_used_percent, Some(27.5));
        assert_eq!(snapshot.primary_value, "27.5%");
        assert_eq!(snapshot.secondary_value, "剩余 72.5%");
        assert_eq!(snapshot.cooldown_ends_at_ms, Some(1_783_686_600_000));
    }

    #[test]
    fn parses_siliconflow_user_balance() {
        let json = r#"{
          "code": 20000,
          "message": "success",
          "status": true,
          "data": {
            "id": "user-id",
            "name": "tester",
            "image": "",
            "email": "test@example.com",
            "balance": "12.50",
            "chargeBalance": "30.00",
            "totalBalance": "42.50",
            "status": "normal"
          }
        }"#;

        let snapshot = parse_snapshot(OnlineProvider::SiliconFlowCn, json).expect("snapshot");

        assert_eq!(snapshot.provider_id, "siliconflow_cn");
        assert_eq!(snapshot.balance_cny, Some(42.5));
        assert_eq!(snapshot.primary_value, "¥42.50");
        assert_eq!(snapshot.secondary_value, "充值 ¥30.00 · 赠金 ¥12.50");
    }

    #[test]
    fn parses_openrouter_remaining_credits_as_usd() {
        let json = r#"{"data":{"total_credits":25.0,"total_usage":7.25}}"#;

        let snapshot = parse_snapshot(OnlineProvider::OpenRouter, json).expect("snapshot");

        assert_eq!(snapshot.provider_id, "openrouter");
        assert_eq!(snapshot.balance_cny, None);
        assert_eq!(
            snapshot.balance_original,
            Some(Money {
                amount: 17.75,
                currency: "USD".to_string()
            })
        );
        assert_eq!(snapshot.primary_value, "$17.75");
        assert_eq!(snapshot.source, "official_credits");
    }

    #[test]
    fn rejects_unknown_provider_and_empty_credentials() {
        assert_eq!(OnlineProvider::from_id("openai"), None);
        assert_eq!(
            OnlineClient::new(OnlineProvider::DeepSeek, " ")
                .expect_err("empty key")
                .code(),
            "ONLINE_INVALID_CREDENTIAL"
        );
    }
}
