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
}

impl OnlineProvider {
    pub fn from_id(value: &str) -> Option<Self> {
        match value {
            "kimi_cn" => Some(Self::KimiCn),
            "kimi_global" => Some(Self::KimiGlobal),
            "deepseek" => Some(Self::DeepSeek),
            "minimax_cn" => Some(Self::MiniMaxCn),
            "minimax_global" => Some(Self::MiniMaxGlobal),
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
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::KimiCn => "Kimi 国内",
            Self::KimiGlobal => "Kimi Global",
            Self::DeepSeek => "DeepSeek",
            Self::MiniMaxCn => "MiniMax 国内",
            Self::MiniMaxGlobal => "MiniMax Global",
        }
    }

    fn endpoint(self) -> &'static str {
        match self {
            Self::KimiCn => "https://api.moonshot.cn/v1/users/me/balance",
            Self::KimiGlobal => "https://api.moonshot.ai/v1/users/me/balance",
            Self::DeepSeek => "https://api.deepseek.com/user/balance",
            Self::MiniMaxCn => "https://www.minimaxi.com/v1/token_plan/remains",
            Self::MiniMaxGlobal => "https://www.minimax.io/v1/token_plan/remains",
        }
    }

    fn source(self) -> &'static str {
        match self {
            Self::KimiCn | Self::KimiGlobal => "official_balance",
            Self::DeepSeek => "official_balance",
            Self::MiniMaxCn | Self::MiniMaxGlobal => "experimental_token_plan",
        }
    }

    fn experimental(self) -> bool {
        matches!(self, Self::MiniMaxCn | Self::MiniMaxGlobal)
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
}

impl OnlineClient {
    pub fn new(provider: OnlineProvider, api_key: &str) -> Result<Self, OnlineError> {
        let trimmed = api_key.trim();
        if trimmed.is_empty() || trimmed.len() > 4096 {
            return Err(OnlineError::InvalidCredential);
        }

        let mut authorization = reqwest::header::HeaderValue::from_str(&format!("Bearer {trimmed}"))
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
        })
    }

    pub fn request(&self) -> Result<reqwest::Request, OnlineError> {
        self.client
            .get(self.provider.endpoint())
            .header(reqwest::header::AUTHORIZATION, self.authorization.clone())
            .header(reqwest::header::ACCEPT, "application/json")
            .build()
            .map_err(|_| OnlineError::RequestFailed)
    }

    pub async fn fetch_snapshot(&self) -> Result<OnlineSnapshot, OnlineError> {
        let response = self
            .client
            .execute(self.request()?)
            .await
            .map_err(|_| OnlineError::RequestFailed)?;
        if !response.status().is_success() {
            return Err(OnlineError::ApiRejected);
        }
        let text = response
            .text()
            .await
            .map_err(|_| OnlineError::RequestFailed)?;
        parse_snapshot(self.provider, &text)
    }
}

pub fn parse_snapshot(provider: OnlineProvider, json: &str) -> Result<OnlineSnapshot, OnlineError> {
    match provider {
        OnlineProvider::KimiCn | OnlineProvider::KimiGlobal => parse_kimi(provider, json),
        OnlineProvider::DeepSeek => parse_deepseek(provider, json),
        OnlineProvider::MiniMaxCn | OnlineProvider::MiniMaxGlobal => parse_minimax(provider, json),
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
    Ok(balance_snapshot(
        provider,
        data.available_balance,
        "CNY",
        format!("现金 ¥{:.2} · 赠金 ¥{:.2}", data.cash_balance, data.voucher_balance),
    ))
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
    let (used, total) = find_quota_pair(&value).ok_or(OnlineError::SchemaMismatch)?;
    if total == 0 || used > total {
        return Err(OnlineError::SchemaMismatch);
    }
    let percent = used as f64 / total as f64 * 100.0;
    let cooldown = find_i64_key(&value, "next_reset_time")
        .or_else(|| find_i64_key(&value, "reset_time"))
        .or_else(|| find_i64_key(&value, "expire_time"));
    Ok(OnlineSnapshot {
        provider_id: provider.id().to_string(),
        label: provider.label().to_string(),
        source: provider.source().to_string(),
        experimental: provider.experimental(),
        balance_cny: None,
        balance_original: None,
        quota_used_percent: Some(percent),
        cooldown_ends_at_ms: cooldown,
        requests: None,
        total_tokens: None,
        estimated_cost_cny: None,
        primary_label: "套餐用量".to_string(),
        primary_value: format!("{percent:.1}%"),
        secondary_value: format!("已用 {used} / {total}"),
    })
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
        primary_value: format!("¥{amount:.2}"),
        secondary_value,
    }
}

fn parse_money(value: &str) -> Result<f64, OnlineError> {
    let amount: f64 = value.parse().map_err(|_| OnlineError::SchemaMismatch)?;
    amount.is_finite().then_some(amount).ok_or(OnlineError::SchemaMismatch)
}

fn looks_like_minimax_rejection(value: &Value) -> bool {
    let status_code = find_i64_key(value, "status_code")
        .or_else(|| find_i64_key(value, "code"))
        .unwrap_or(0);
    status_code != 0
}

fn find_quota_pair(value: &Value) -> Option<(u64, u64)> {
    let direct = [
        ("current_interval_usage_count", "current_interval_total_count"),
        ("current_weekly_usage_count", "current_weekly_total_count"),
        ("usage_count", "total_count"),
        ("used", "total"),
    ];
    for (used_key, total_key) in direct {
        if let (Some(used), Some(total)) = (find_u64_key(value, used_key), find_u64_key(value, total_key)) {
            return Some((used, total));
        }
    }
    None
}

fn find_u64_key(value: &Value, key: &str) -> Option<u64> {
    match value {
        Value::Object(map) => {
            if let Some(found) = map.get(key).and_then(number_like_u64) {
                return Some(found);
            }
            map.values().find_map(|child| find_u64_key(child, key))
        }
        Value::Array(items) => items.iter().find_map(|child| find_u64_key(child, key)),
        _ => None,
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

fn number_like_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64(),
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
        let client = OnlineClient::new(OnlineProvider::KimiCn, "moonshot-key").expect("client");
        let request = client.request().expect("request");

        assert_eq!(
            request.url().as_str(),
            "https://api.moonshot.cn/v1/users/me/balance"
        );
        assert_eq!(request.headers()["authorization"], "Bearer moonshot-key");
        assert!(request.headers()["authorization"].is_sensitive());

        let minimax = OnlineClient::new(OnlineProvider::MiniMaxCn, "token-plan-key")
            .expect("client")
            .request()
            .expect("request");
        assert_eq!(
            minimax.url().as_str(),
            "https://www.minimaxi.com/v1/token_plan/remains"
        );
    }

    #[test]
    fn parses_kimi_balance() {
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
