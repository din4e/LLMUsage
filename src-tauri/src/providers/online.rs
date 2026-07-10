use chrono::{DateTime, SecondsFormat};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;
use zeroize::Zeroize;

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
    OpenAiCodex,
    ClaudeCode,
    Gemini,
    QwenCn,
    QwenGlobal,
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
            "openai_codex" => Some(Self::OpenAiCodex),
            "claude_code" => Some(Self::ClaudeCode),
            "gemini" => Some(Self::Gemini),
            "qwen_cn" => Some(Self::QwenCn),
            "qwen_global" => Some(Self::QwenGlobal),
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
            Self::OpenAiCodex => "openai_codex",
            Self::ClaudeCode => "claude_code",
            Self::Gemini => "gemini",
            Self::QwenCn => "qwen_cn",
            Self::QwenGlobal => "qwen_global",
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
            Self::OpenAiCodex => "OpenAI / Codex API",
            Self::ClaudeCode => "Claude Code",
            Self::Gemini => "Gemini Code Assist",
            Self::QwenCn => "Qwen / 百炼国内",
            Self::QwenGlobal => "Qwen / Model Studio Global",
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
            Self::OpenAiCodex => &[
                "https://api.openai.com/v1/organization/usage/completions",
                "https://api.openai.com/v1/organization/costs",
            ],
            Self::ClaudeCode => {
                &["https://api.anthropic.com/v1/organizations/usage_report/claude_code"]
            }
            Self::Gemini | Self::QwenCn | Self::QwenGlobal => &[],
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
            Self::OpenAiCodex => "official_organization_usage",
            Self::ClaudeCode => "official_claude_code_analytics",
            Self::Gemini => "official_cloud_monitoring",
            Self::QwenCn | Self::QwenGlobal => "official_prometheus_monitoring",
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
    pub detail_sections: Vec<OnlineDetailSection>,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnlineDetailSection {
    pub title: String,
    pub entries: Vec<OnlineDetailEntry>,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnlineDetailEntry {
    pub label: String,
    pub used: Option<String>,
    pub remaining: Option<String>,
    pub limit: Option<String>,
    pub unit: String,
    pub used_percent: Option<f64>,
    pub window: Option<String>,
    pub start_at_ms: Option<i64>,
    pub reset_at_ms: Option<i64>,
    pub remaining_ms: Option<i64>,
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
    api_key: reqwest::header::HeaderValue,
    analytics_credential: Option<AnalyticsCredential>,
    kimi_code_credential: bool,
}

#[derive(Debug)]
enum AnalyticsCredential {
    Gemini { project_id: String },
    Qwen { endpoint: reqwest::Url },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GeminiCredential {
    project_id: String,
    access_token: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QwenCredential {
    endpoint: String,
    access_key_id: String,
    access_key_secret: String,
}

#[derive(Debug, Clone, Copy)]
pub struct OnlineUsageRange {
    start_time_ms: i64,
    end_time_ms: i64,
}

impl OnlineUsageRange {
    pub fn new(start_time_ms: i64, end_time_ms: i64) -> Result<Self, OnlineError> {
        const MAX_RANGE_MS: i64 = 31 * 86_400_000;
        if start_time_ms <= 0
            || end_time_ms <= start_time_ms
            || end_time_ms - start_time_ms > MAX_RANGE_MS
        {
            return Err(OnlineError::InvalidCredential);
        }
        Ok(Self {
            start_time_ms,
            end_time_ms,
        })
    }

    fn current_utc_day() -> Result<Self, OnlineError> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| OnlineError::RequestFailed)?
            .as_millis();
        let now_ms = i64::try_from(now_ms).map_err(|_| OnlineError::RequestFailed)?;
        let start_time_ms = now_ms - now_ms.rem_euclid(86_400_000);
        Self::new(start_time_ms, start_time_ms + 86_400_000)
    }

    fn utc_date(self) -> Result<String, OnlineError> {
        DateTime::from_timestamp_millis(self.end_time_ms - 1)
            .map(|date| date.format("%Y-%m-%d").to_string())
            .ok_or(OnlineError::InvalidCredential)
    }
}

impl OnlineClient {
    pub fn new(provider: OnlineProvider, api_key: &str) -> Result<Self, OnlineError> {
        let trimmed = api_key.trim();
        if trimmed.is_empty() || trimmed.len() > 4096 {
            return Err(OnlineError::InvalidCredential);
        }
        let client = reqwest::Client::builder()
            .https_only(true)
            .timeout(Duration::from_secs(15))
            .user_agent("LLMUsage/0.1")
            .build()
            .map_err(|_| OnlineError::RequestFailed)?;
        let (authorization, api_key, analytics_credential, kimi_code_credential) = match provider {
            OnlineProvider::Gemini => {
                let mut credential: GeminiCredential =
                    serde_json::from_str(trimmed).map_err(|_| OnlineError::InvalidCredential)?;
                if !valid_google_project_id(&credential.project_id)
                    || credential.access_token.is_empty()
                    || credential.access_token.len() > 3072
                {
                    credential.access_token.zeroize();
                    return Err(OnlineError::InvalidCredential);
                }
                let authorization = match sensitive_bearer_header(&credential.access_token) {
                    Ok(header) => header,
                    Err(error) => {
                        credential.access_token.zeroize();
                        return Err(error);
                    }
                };
                let api_key = sensitive_header("unused")?;
                credential.access_token.zeroize();
                (
                    authorization,
                    api_key,
                    Some(AnalyticsCredential::Gemini {
                        project_id: credential.project_id,
                    }),
                    false,
                )
            }
            OnlineProvider::QwenCn | OnlineProvider::QwenGlobal => {
                let mut credential: QwenCredential =
                    serde_json::from_str(trimmed).map_err(|_| OnlineError::InvalidCredential)?;
                let endpoint = match validate_qwen_endpoint(&credential.endpoint) {
                    Ok(endpoint) => endpoint,
                    Err(error) => {
                        credential.access_key_id.zeroize();
                        credential.access_key_secret.zeroize();
                        return Err(error);
                    }
                };
                if credential.access_key_id.is_empty()
                    || credential.access_key_id.len() > 256
                    || credential.access_key_secret.is_empty()
                    || credential.access_key_secret.len() > 1024
                {
                    credential.access_key_id.zeroize();
                    credential.access_key_secret.zeroize();
                    return Err(OnlineError::InvalidCredential);
                }
                let probe = match client
                    .get(endpoint.clone())
                    .basic_auth(
                        &credential.access_key_id,
                        Some(&credential.access_key_secret),
                    )
                    .build()
                {
                    Ok(request) => request,
                    Err(_) => {
                        credential.access_key_id.zeroize();
                        credential.access_key_secret.zeroize();
                        return Err(OnlineError::InvalidCredential);
                    }
                };
                let mut authorization =
                    match probe.headers().get(reqwest::header::AUTHORIZATION).cloned() {
                        Some(header) => header,
                        None => {
                            credential.access_key_id.zeroize();
                            credential.access_key_secret.zeroize();
                            return Err(OnlineError::InvalidCredential);
                        }
                    };
                authorization.set_sensitive(true);
                credential.access_key_id.zeroize();
                credential.access_key_secret.zeroize();
                (
                    authorization,
                    sensitive_header("unused")?,
                    Some(AnalyticsCredential::Qwen { endpoint }),
                    false,
                )
            }
            _ => (
                sensitive_bearer_header(trimmed)?,
                sensitive_header(trimmed)?,
                None,
                trimmed.starts_with("sk-kimi-"),
            ),
        };

        Ok(Self {
            provider,
            client,
            authorization,
            api_key,
            analytics_credential,
            kimi_code_credential,
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
        self.fetch_snapshot_for_range(OnlineUsageRange::current_utc_day()?)
            .await
    }

    pub async fn fetch_snapshot_for_range(
        &self,
        range: OnlineUsageRange,
    ) -> Result<OnlineSnapshot, OnlineError> {
        match self.provider {
            OnlineProvider::OpenAiCodex => return self.fetch_openai_analytics(range).await,
            OnlineProvider::ClaudeCode => return self.fetch_claude_code_analytics(range).await,
            OnlineProvider::Gemini => return self.fetch_gemini_analytics(range).await,
            OnlineProvider::QwenCn | OnlineProvider::QwenGlobal => {
                return self.fetch_qwen_analytics(range).await;
            }
            _ => {}
        }
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

    async fn fetch_openai_analytics(
        &self,
        range: OnlineUsageRange,
    ) -> Result<OnlineSnapshot, OnlineError> {
        let start_time = range.start_time_ms / 1_000;
        let end_time = range.end_time_ms / 1_000;
        let mut usage_url = reqwest::Url::parse(self.provider.endpoints()[0])
            .map_err(|_| OnlineError::RequestFailed)?;
        usage_url
            .query_pairs_mut()
            .append_pair("start_time", &start_time.to_string())
            .append_pair("end_time", &end_time.to_string())
            .append_pair("bucket_width", "1d")
            .append_pair("group_by[]", "model")
            .append_pair("limit", "31");
        let mut cost_url = reqwest::Url::parse(self.provider.endpoints()[1])
            .map_err(|_| OnlineError::RequestFailed)?;
        cost_url
            .query_pairs_mut()
            .append_pair("start_time", &start_time.to_string())
            .append_pair("end_time", &end_time.to_string())
            .append_pair("bucket_width", "1d")
            .append_pair("group_by[]", "line_item")
            .append_pair("limit", "31");
        let usage = self.fetch_bearer_json(usage_url).await?;
        let costs = self.fetch_bearer_json(cost_url).await?;
        let rate = self.fetch_usd_cny_rate().await;
        parse_openai_analytics(&usage, &costs, rate)
    }

    async fn fetch_claude_code_analytics(
        &self,
        range: OnlineUsageRange,
    ) -> Result<OnlineSnapshot, OnlineError> {
        let mut url = reqwest::Url::parse(self.provider.endpoints()[0])
            .map_err(|_| OnlineError::RequestFailed)?;
        url.query_pairs_mut()
            .append_pair("starting_at", &range.utc_date()?)
            .append_pair("limit", "1000");
        let request = self
            .client
            .get(url)
            .header("x-api-key", self.api_key.clone())
            .header("anthropic-version", "2023-06-01")
            .header(reqwest::header::ACCEPT, "application/json")
            .build()
            .map_err(|_| OnlineError::RequestFailed)?;
        let json = self.execute_json(request).await?;
        let rate = self.fetch_usd_cny_rate().await;
        parse_claude_code_analytics(&json, rate)
    }

    async fn fetch_gemini_analytics(
        &self,
        range: OnlineUsageRange,
    ) -> Result<OnlineSnapshot, OnlineError> {
        let calls = self
            .execute_json(self.gemini_metric_request(range, "code_assist/api_calls_count")?)
            .await?;
        let tokens = self
            .execute_json(self.gemini_metric_request(range, "code_assist/used_tokens_count")?)
            .await?;
        parse_gemini_analytics(&calls, &tokens)
    }

    fn gemini_metric_request(
        &self,
        range: OnlineUsageRange,
        metric: &str,
    ) -> Result<reqwest::Request, OnlineError> {
        if !matches!(
            metric,
            "code_assist/api_calls_count" | "code_assist/used_tokens_count"
        ) {
            return Err(OnlineError::InvalidProvider);
        }
        let AnalyticsCredential::Gemini { project_id } = self
            .analytics_credential
            .as_ref()
            .ok_or(OnlineError::InvalidCredential)?
        else {
            return Err(OnlineError::InvalidCredential);
        };
        let mut url = reqwest::Url::parse("https://monitoring.googleapis.com/v3/projects/")
            .map_err(|_| OnlineError::RequestFailed)?;
        url.path_segments_mut()
            .map_err(|_| OnlineError::RequestFailed)?
            .push(project_id)
            .push("timeSeries");
        let start = rfc3339_millis(range.start_time_ms)?;
        let end = rfc3339_millis(range.end_time_ms)?;
        let alignment_seconds = ((range.end_time_ms - range.start_time_ms) / 1_000).max(60);
        url.query_pairs_mut()
            .append_pair("filter", &format!("metric.type = \"{metric}\""))
            .append_pair("interval.startTime", &start)
            .append_pair("interval.endTime", &end)
            .append_pair(
                "aggregation.alignmentPeriod",
                &format!("{alignment_seconds}s"),
            )
            .append_pair("aggregation.perSeriesAligner", "ALIGN_SUM")
            .append_pair("aggregation.crossSeriesReducer", "REDUCE_SUM")
            .append_pair("view", "FULL")
            .append_pair("pageSize", "1000");
        self.client
            .get(url)
            .header(reqwest::header::AUTHORIZATION, self.authorization.clone())
            .header(reqwest::header::ACCEPT, "application/json")
            .build()
            .map_err(|_| OnlineError::RequestFailed)
    }

    async fn fetch_qwen_analytics(
        &self,
        range: OnlineUsageRange,
    ) -> Result<OnlineSnapshot, OnlineError> {
        let calls = self
            .execute_json(self.qwen_metric_request(range, "model_call_count")?)
            .await?;
        let tokens = self
            .execute_json(self.qwen_metric_request(range, "model_usage")?)
            .await?;
        parse_qwen_analytics(self.provider, &calls, &tokens)
    }

    fn qwen_metric_request(
        &self,
        range: OnlineUsageRange,
        metric: &str,
    ) -> Result<reqwest::Request, OnlineError> {
        if !matches!(metric, "model_call_count" | "model_usage") {
            return Err(OnlineError::InvalidProvider);
        }
        let AnalyticsCredential::Qwen { endpoint } = self
            .analytics_credential
            .as_ref()
            .ok_or(OnlineError::InvalidCredential)?
        else {
            return Err(OnlineError::InvalidCredential);
        };
        let mut url = endpoint.clone();
        let base_path = url.path().trim_end_matches('/');
        url.set_path(&format!("{base_path}/api/v1/query_range"));
        url.set_query(None);
        url.query_pairs_mut()
            .append_pair("query", &format!("sum by (model) ({metric})"))
            .append_pair("start", &rfc3339_millis(range.start_time_ms)?)
            .append_pair("end", &rfc3339_millis(range.end_time_ms)?)
            .append_pair("step", "3600s");
        self.client
            .get(url)
            .header(reqwest::header::AUTHORIZATION, self.authorization.clone())
            .header(reqwest::header::ACCEPT, "application/json")
            .build()
            .map_err(|_| OnlineError::RequestFailed)
    }

    async fn fetch_bearer_json(&self, url: reqwest::Url) -> Result<String, OnlineError> {
        let request = self
            .client
            .get(url)
            .header(reqwest::header::AUTHORIZATION, self.authorization.clone())
            .header(reqwest::header::ACCEPT, "application/json")
            .build()
            .map_err(|_| OnlineError::RequestFailed)?;
        self.execute_json(request).await
    }

    async fn execute_json(&self, request: reqwest::Request) -> Result<String, OnlineError> {
        let response = self
            .client
            .execute(request)
            .await
            .map_err(|_| OnlineError::RequestFailed)?;
        if !response.status().is_success() {
            return Err(OnlineError::ApiRejected);
        }
        response
            .text()
            .await
            .map_err(|_| OnlineError::RequestFailed)
    }

    async fn fetch_usd_cny_rate(&self) -> Option<f64> {
        let response = self
            .client
            .get("https://api.frankfurter.app/latest?from=USD&to=CNY")
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        let value: Value = response.json().await.ok()?;
        value
            .get("rates")
            .and_then(|rates| rates.get("CNY"))
            .and_then(number_like_f64)
            .filter(|rate| rate.is_finite() && (1.0..=20.0).contains(rate))
    }
}

fn sensitive_header(value: &str) -> Result<reqwest::header::HeaderValue, OnlineError> {
    let mut header = reqwest::header::HeaderValue::from_str(value)
        .map_err(|_| OnlineError::InvalidCredential)?;
    header.set_sensitive(true);
    Ok(header)
}

fn sensitive_bearer_header(value: &str) -> Result<reqwest::header::HeaderValue, OnlineError> {
    let mut bearer = format!("Bearer {value}");
    let result = sensitive_header(&bearer);
    bearer.zeroize();
    result
}

fn valid_google_project_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    (6..=63).contains(&bytes.len())
        && bytes.first().is_some_and(u8::is_ascii_lowercase)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

fn validate_qwen_endpoint(value: &str) -> Result<reqwest::Url, OnlineError> {
    let mut endpoint =
        reqwest::Url::parse(value.trim()).map_err(|_| OnlineError::InvalidCredential)?;
    let host = endpoint
        .host_str()
        .map(str::to_ascii_lowercase)
        .ok_or(OnlineError::InvalidCredential)?;
    let allowed_host = host == "aliyuncs.com"
        || host.ends_with(".aliyuncs.com")
        || host == "alibabacloud.com"
        || host.ends_with(".alibabacloud.com");
    if endpoint.scheme() != "https"
        || !allowed_host
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(OnlineError::InvalidCredential);
    }
    let normalized_path = endpoint.path().trim_end_matches('/').to_string();
    endpoint.set_path(&normalized_path);
    Ok(endpoint)
}

fn rfc3339_millis(value: i64) -> Result<String, OnlineError> {
    DateTime::from_timestamp_millis(value)
        .map(|date| date.to_rfc3339_opts(SecondsFormat::Secs, true))
        .ok_or(OnlineError::InvalidCredential)
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
        OnlineProvider::ClaudeCode => parse_claude_code_analytics(json, None),
        OnlineProvider::OpenAiCodex
        | OnlineProvider::Gemini
        | OnlineProvider::QwenCn
        | OnlineProvider::QwenGlobal => Err(OnlineError::SchemaMismatch),
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
    let detail_sections = kimi_detail_sections(weekly, limits, &value);

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
        detail_sections,
    })
}

struct ParsedQuota {
    limit: f64,
    used: f64,
    remaining: f64,
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
    let used = value
        .get("used")
        .and_then(number_like_f64)
        .unwrap_or(limit - remaining);
    if !limit.is_finite()
        || !used.is_finite()
        || !remaining.is_finite()
        || limit <= 0.0
        || used < 0.0
        || used > limit
        || remaining < 0.0
        || remaining > limit
    {
        return Err(OnlineError::SchemaMismatch);
    }
    let remaining_percent = remaining / limit * 100.0;
    Ok(ParsedQuota {
        limit,
        used,
        remaining,
        used_percent: percentage(used, limit),
        remaining_percent,
    })
}

fn kimi_detail_sections(
    weekly: &Value,
    limits: &[Value],
    response: &Value,
) -> Vec<OnlineDetailSection> {
    let mut windows = Vec::new();
    if let Ok(quota) = parse_quota(weekly) {
        windows.push(quota_detail_entry(
            "周额度".to_string(),
            None,
            "%",
            &quota,
            timestamp_field(weekly, "resetTime"),
        ));
    }
    for (index, limit) in limits
        .iter()
        .take(MAX_DETAIL_ENTRIES.saturating_sub(windows.len()))
        .enumerate()
    {
        let Some(detail) = limit.get("detail") else {
            continue;
        };
        let Ok(quota) = parse_quota(detail) else {
            continue;
        };
        let (label, window) = kimi_window_label(limit.get("window"), index);
        windows.push(quota_detail_entry(
            label,
            window,
            "%",
            &quota,
            timestamp_field(detail, "resetTime"),
        ));
    }

    let mut other = Vec::new();
    if let Some(entry) = response
        .get("parallel")
        .and_then(|value| partial_quota_entry("并发上限", "路", value))
    {
        other.push(entry);
    }
    if let Some(entry) = response
        .get("totalQuota")
        .and_then(|value| partial_quota_entry("总额度", "%", value))
    {
        other.push(entry);
    }

    let mut sections = Vec::new();
    if !windows.is_empty() {
        sections.push(OnlineDetailSection {
            title: "额度窗口".to_string(),
            entries: windows,
        });
    }
    if !other.is_empty() {
        sections.push(OnlineDetailSection {
            title: "其他限制".to_string(),
            entries: other,
        });
    }
    sections
}

fn quota_detail_entry(
    label: String,
    window: Option<String>,
    unit: &str,
    quota: &ParsedQuota,
    reset_at_ms: Option<i64>,
) -> OnlineDetailEntry {
    OnlineDetailEntry {
        label,
        used: Some(format_detail_number(quota.used)),
        remaining: Some(format_detail_number(quota.remaining)),
        limit: Some(format_detail_number(quota.limit)),
        unit: unit.to_string(),
        used_percent: Some(quota.used_percent),
        window,
        start_at_ms: None,
        reset_at_ms,
        remaining_ms: None,
    }
}

fn partial_quota_entry(label: &str, unit: &str, value: &Value) -> Option<OnlineDetailEntry> {
    let limit = value.get("limit").and_then(number_like_f64)?;
    let used = value.get("used").and_then(number_like_f64);
    let remaining = value.get("remaining").and_then(number_like_f64);
    if !limit.is_finite()
        || limit <= 0.0
        || used.is_some_and(|amount| !amount.is_finite() || amount < 0.0 || amount > limit)
        || remaining.is_some_and(|amount| !amount.is_finite() || amount < 0.0 || amount > limit)
    {
        return None;
    }
    let used_percent = used
        .or_else(|| remaining.map(|amount| limit - amount))
        .map(|amount| percentage(amount, limit));
    Some(OnlineDetailEntry {
        label: label.to_string(),
        used: used.map(format_detail_number),
        remaining: remaining.map(format_detail_number),
        limit: Some(format_detail_number(limit)),
        unit: unit.to_string(),
        used_percent,
        window: None,
        start_at_ms: None,
        reset_at_ms: timestamp_field(value, "resetTime"),
        remaining_ms: None,
    })
}

fn kimi_window_label(window: Option<&Value>, index: usize) -> (String, Option<String>) {
    let duration = window
        .and_then(|value| value.get("duration"))
        .and_then(number_like_i64)
        .filter(|duration| *duration > 0);
    let unit = window
        .and_then(|value| value.get("timeUnit"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_uppercase();
    match (duration, unit.as_str()) {
        (Some(minutes), unit) if unit.contains("MINUTE") && minutes % 60 == 0 => (
            format!("{} 小时窗口", minutes / 60),
            Some(format!("{minutes} 分钟")),
        ),
        (Some(minutes), unit) if unit.contains("MINUTE") => (
            format!("{minutes} 分钟窗口"),
            Some(format!("{minutes} 分钟")),
        ),
        (Some(hours), unit) if unit.contains("HOUR") => {
            (format!("{hours} 小时窗口"), Some(format!("{hours} 小时")))
        }
        (Some(days), unit) if unit.contains("DAY") => {
            (format!("{days} 天窗口"), Some(format!("{days} 天")))
        }
        _ => (format!("额度窗口 {}", index + 1), None),
    }
}

fn format_detail_number(value: f64) -> String {
    let formatted = format!("{value:.2}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn percentage(part: f64, total: f64) -> f64 {
    (part / total * 10_000.0).round() / 100.0
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
    let detail_sections = minimax_detail_sections(&value);
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
        detail_sections,
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

fn parse_openai_analytics(
    usage_json: &str,
    costs_json: &str,
    usd_cny_rate: Option<f64>,
) -> Result<OnlineSnapshot, OnlineError> {
    let usage: Value = serde_json::from_str(usage_json).map_err(|_| OnlineError::InvalidJson)?;
    let costs: Value = serde_json::from_str(costs_json).map_err(|_| OnlineError::InvalidJson)?;
    let usage_buckets = usage
        .get("data")
        .and_then(Value::as_array)
        .ok_or(OnlineError::SchemaMismatch)?;
    let cost_buckets = costs
        .get("data")
        .and_then(Value::as_array)
        .ok_or(OnlineError::SchemaMismatch)?;

    let mut requests = 0_u64;
    let mut total_tokens = 0_u64;
    let mut models = BTreeMap::<String, OpenAiModelUsage>::new();
    for bucket in usage_buckets {
        let results = bucket
            .get("results")
            .and_then(Value::as_array)
            .ok_or(OnlineError::SchemaMismatch)?;
        for result in results {
            let input = analytics_u64(result, "input_tokens")?;
            let output = analytics_u64(result, "output_tokens")?;
            let cached = analytics_u64_optional(result, "input_cached_tokens")?;
            let model_requests = analytics_u64(result, "num_model_requests")?;
            requests = requests
                .checked_add(model_requests)
                .ok_or(OnlineError::SchemaMismatch)?;
            total_tokens = total_tokens
                .checked_add(input)
                .and_then(|value| value.checked_add(output))
                .ok_or(OnlineError::SchemaMismatch)?;
            let model = analytics_label(result.get("model").and_then(Value::as_str), "未分组模型");
            let usage = models.entry(model).or_default();
            usage.input = usage
                .input
                .checked_add(input)
                .ok_or(OnlineError::SchemaMismatch)?;
            usage.output = usage
                .output
                .checked_add(output)
                .ok_or(OnlineError::SchemaMismatch)?;
            usage.cached = usage
                .cached
                .checked_add(cached)
                .ok_or(OnlineError::SchemaMismatch)?;
            usage.requests = usage
                .requests
                .checked_add(model_requests)
                .ok_or(OnlineError::SchemaMismatch)?;
        }
    }

    let mut usd_cost = 0.0_f64;
    let mut cost_entries = Vec::new();
    for bucket in cost_buckets {
        let results = bucket
            .get("results")
            .and_then(Value::as_array)
            .ok_or(OnlineError::SchemaMismatch)?;
        for result in results {
            let amount = result
                .get("amount")
                .and_then(Value::as_object)
                .ok_or(OnlineError::SchemaMismatch)?;
            let value = amount
                .get("value")
                .and_then(number_like_f64)
                .filter(|value| value.is_finite() && *value >= 0.0)
                .ok_or(OnlineError::SchemaMismatch)?;
            amount
                .get("currency")
                .and_then(Value::as_str)
                .filter(|currency| currency.eq_ignore_ascii_case("usd"))
                .ok_or(OnlineError::SchemaMismatch)?;
            usd_cost += value;
            if !usd_cost.is_finite() {
                return Err(OnlineError::SchemaMismatch);
            }
            cost_entries.push(OnlineDetailEntry {
                label: analytics_label(
                    result.get("line_item").and_then(Value::as_str),
                    "未分组成本",
                ),
                used: Some(format_detail_number(value)),
                remaining: None,
                limit: None,
                unit: " USD".to_string(),
                used_percent: None,
                window: Some("官方组织成本".to_string()),
                start_at_ms: None,
                reset_at_ms: None,
                remaining_ms: None,
            });
        }
    }

    let model_entries = models
        .into_iter()
        .take(MAX_DETAIL_ENTRIES)
        .map(|(model, usage)| OnlineDetailEntry {
            label: model,
            used: Some((usage.input + usage.output).to_string()),
            remaining: None,
            limit: None,
            unit: " Token".to_string(),
            used_percent: None,
            window: Some(format!(
                "输入 {} · 输出 {} · 缓存输入 {} · 请求 {}",
                usage.input, usage.output, usage.cached, usage.requests
            )),
            start_at_ms: None,
            reset_at_ms: None,
            remaining_ms: None,
        })
        .collect::<Vec<_>>();

    let mut detail_sections = vec![OnlineDetailSection {
        title: "模型用量".to_string(),
        entries: model_entries,
    }];
    if !cost_entries.is_empty() {
        detail_sections.push(OnlineDetailSection {
            title: "成本".to_string(),
            entries: cost_entries,
        });
    }
    Ok(OnlineSnapshot {
        provider_id: OnlineProvider::OpenAiCodex.id().to_string(),
        label: OnlineProvider::OpenAiCodex.label().to_string(),
        source: OnlineProvider::OpenAiCodex.source().to_string(),
        experimental: false,
        balance_cny: None,
        balance_original: None,
        quota_used_percent: None,
        cooldown_ends_at_ms: None,
        requests: Some(requests),
        total_tokens: Some(total_tokens),
        estimated_cost_cny: converted_cny(usd_cost, usd_cny_rate),
        primary_label: "今日 API 成本".to_string(),
        primary_value: format!("${usd_cost:.2}"),
        secondary_value: format!(
            "API 组织 · {requests} 次请求 · {total_tokens} Token · 非 ChatGPT 套餐"
        ),
        detail_sections,
    })
}

#[derive(Default)]
struct OpenAiModelUsage {
    input: u64,
    output: u64,
    cached: u64,
    requests: u64,
}

fn parse_claude_code_analytics(
    json: &str,
    usd_cny_rate: Option<f64>,
) -> Result<OnlineSnapshot, OnlineError> {
    let value: Value = serde_json::from_str(json).map_err(|_| OnlineError::InvalidJson)?;
    let records = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or(OnlineError::SchemaMismatch)?;
    let mut sessions = 0_u64;
    let mut added = 0_u64;
    let mut removed = 0_u64;
    let mut commits = 0_u64;
    let mut pull_requests = 0_u64;
    let mut total_tokens = 0_u64;
    let mut cost_cents = 0.0_f64;
    let mut models = BTreeMap::<String, ClaudeModelUsage>::new();

    for record in records {
        let core = record
            .get("core_metrics")
            .ok_or(OnlineError::SchemaMismatch)?;
        sessions = checked_analytics_add(sessions, analytics_u64(core, "num_sessions")?)?;
        let lines = core.get("lines_of_code").unwrap_or(&Value::Null);
        added = checked_analytics_add(added, analytics_u64_optional(lines, "added")?)?;
        removed = checked_analytics_add(removed, analytics_u64_optional(lines, "removed")?)?;
        commits = checked_analytics_add(
            commits,
            analytics_u64_optional(core, "commits_by_claude_code")?,
        )?;
        pull_requests = checked_analytics_add(
            pull_requests,
            analytics_u64_optional(core, "pull_requests_by_claude_code")?,
        )?;
        let breakdown = record
            .get("model_breakdown")
            .and_then(Value::as_array)
            .ok_or(OnlineError::SchemaMismatch)?;
        for item in breakdown {
            let model = analytics_label(item.get("model").and_then(Value::as_str), "未分组模型");
            let tokens = item.get("tokens").ok_or(OnlineError::SchemaMismatch)?;
            let input = analytics_u64(tokens, "input")?;
            let output = analytics_u64(tokens, "output")?;
            let cache_read = analytics_u64_optional(tokens, "cache_read")?;
            let cache_creation = analytics_u64_optional(tokens, "cache_creation")?;
            let item_total = [input, output, cache_read, cache_creation]
                .into_iter()
                .try_fold(0_u64, checked_analytics_add)?;
            total_tokens = checked_analytics_add(total_tokens, item_total)?;
            let estimated = item
                .get("estimated_cost")
                .and_then(Value::as_object)
                .ok_or(OnlineError::SchemaMismatch)?;
            estimated
                .get("currency")
                .and_then(Value::as_str)
                .filter(|currency| currency.eq_ignore_ascii_case("USD"))
                .ok_or(OnlineError::SchemaMismatch)?;
            let cents = estimated
                .get("amount")
                .and_then(number_like_f64)
                .filter(|amount| amount.is_finite() && *amount >= 0.0)
                .ok_or(OnlineError::SchemaMismatch)?;
            cost_cents += cents;
            if !cost_cents.is_finite() {
                return Err(OnlineError::SchemaMismatch);
            }
            let usage = models.entry(model).or_default();
            usage.input = checked_analytics_add(usage.input, input)?;
            usage.output = checked_analytics_add(usage.output, output)?;
            usage.cache_read = checked_analytics_add(usage.cache_read, cache_read)?;
            usage.cache_creation = checked_analytics_add(usage.cache_creation, cache_creation)?;
            usage.cost_cents += cents;
        }
    }

    let usd_cost = cost_cents / 100.0;
    let model_entries = models
        .into_iter()
        .take(MAX_DETAIL_ENTRIES)
        .map(|(model, usage)| OnlineDetailEntry {
            label: model,
            used: Some(
                (usage.input + usage.output + usage.cache_read + usage.cache_creation).to_string(),
            ),
            remaining: None,
            limit: None,
            unit: " Token".to_string(),
            used_percent: None,
            window: Some(format!(
                "输入 {} · 输出 {} · 缓存读取 {} · 缓存创建 {} · ${:.2}",
                usage.input,
                usage.output,
                usage.cache_read,
                usage.cache_creation,
                usage.cost_cents / 100.0
            )),
            start_at_ms: None,
            reset_at_ms: None,
            remaining_ms: None,
        })
        .collect();
    let productivity_entries = [
        ("会话", sessions),
        ("新增代码行", added),
        ("删除代码行", removed),
        ("提交", commits),
        ("Pull Request", pull_requests),
    ]
    .into_iter()
    .map(|(label, value)| OnlineDetailEntry {
        label: label.to_string(),
        used: Some(value.to_string()),
        remaining: None,
        limit: None,
        unit: "".to_string(),
        used_percent: None,
        window: Some("UTC 日汇总".to_string()),
        start_at_ms: None,
        reset_at_ms: None,
        remaining_ms: None,
    })
    .collect();

    Ok(OnlineSnapshot {
        provider_id: OnlineProvider::ClaudeCode.id().to_string(),
        label: OnlineProvider::ClaudeCode.label().to_string(),
        source: OnlineProvider::ClaudeCode.source().to_string(),
        experimental: false,
        balance_cny: None,
        balance_original: None,
        quota_used_percent: None,
        cooldown_ends_at_ms: None,
        requests: Some(sessions),
        total_tokens: Some(total_tokens),
        estimated_cost_cny: converted_cny(usd_cost, usd_cny_rate),
        primary_label: "今日估算成本".to_string(),
        primary_value: format!("${usd_cost:.2}"),
        secondary_value: format!("UTC 日汇总 · {sessions} 个会话 · {total_tokens} Token"),
        detail_sections: vec![
            OnlineDetailSection {
                title: "模型用量".to_string(),
                entries: model_entries,
            },
            OnlineDetailSection {
                title: "开发活动".to_string(),
                entries: productivity_entries,
            },
        ],
    })
}

#[derive(Default)]
struct ClaudeModelUsage {
    input: u64,
    output: u64,
    cache_read: u64,
    cache_creation: u64,
    cost_cents: f64,
}

fn analytics_u64(value: &Value, key: &str) -> Result<u64, OnlineError> {
    value
        .get(key)
        .and_then(number_like_u64)
        .ok_or(OnlineError::SchemaMismatch)
}

fn analytics_u64_optional(value: &Value, key: &str) -> Result<u64, OnlineError> {
    match value.get(key) {
        Some(value) => number_like_u64(value).ok_or(OnlineError::SchemaMismatch),
        None => Ok(0),
    }
}

fn checked_analytics_add(left: u64, right: u64) -> Result<u64, OnlineError> {
    left.checked_add(right).ok_or(OnlineError::SchemaMismatch)
}

fn analytics_label(value: Option<&str>, fallback: &str) -> String {
    let value = value.unwrap_or(fallback);
    let sanitized: String = value
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .take(100)
        .collect();
    if sanitized.is_empty() {
        fallback.to_string()
    } else {
        sanitized
    }
}

fn converted_cny(usd: f64, rate: Option<f64>) -> Option<f64> {
    let rate = rate.filter(|rate| rate.is_finite() && (1.0..=20.0).contains(rate))?;
    let value = usd * rate;
    value.is_finite().then_some(value)
}

fn parse_gemini_analytics(
    calls_json: &str,
    tokens_json: &str,
) -> Result<OnlineSnapshot, OnlineError> {
    let calls = parse_google_metric(calls_json)?;
    let tokens = parse_google_metric(tokens_json)?;
    Ok(OnlineSnapshot {
        provider_id: OnlineProvider::Gemini.id().to_string(),
        label: OnlineProvider::Gemini.label().to_string(),
        source: OnlineProvider::Gemini.source().to_string(),
        experimental: false,
        balance_cny: None,
        balance_original: None,
        quota_used_percent: None,
        cooldown_ends_at_ms: None,
        requests: Some(calls),
        total_tokens: Some(tokens),
        estimated_cost_cny: None,
        primary_label: "今日 Token".to_string(),
        primary_value: format_integer_with_commas(tokens),
        secondary_value: format!("Cloud Monitoring · {calls} 次 API 调用"),
        detail_sections: vec![OnlineDetailSection {
            title: "监控指标".to_string(),
            entries: vec![
                analytics_count_entry("API 调用", calls, "次"),
                analytics_count_entry("已用 Token", tokens, " Token"),
            ],
        }],
    })
}

fn parse_google_metric(json: &str) -> Result<u64, OnlineError> {
    let value: Value = serde_json::from_str(json).map_err(|_| OnlineError::InvalidJson)?;
    let Some(series) = value.get("timeSeries") else {
        return Ok(0);
    };
    let series = series.as_array().ok_or(OnlineError::SchemaMismatch)?;
    let mut total = 0_u64;
    for item in series {
        let points = item
            .get("points")
            .and_then(Value::as_array)
            .ok_or(OnlineError::SchemaMismatch)?;
        for point in points {
            let metric_value = point.get("value").ok_or(OnlineError::SchemaMismatch)?;
            let count = if let Some(value) = metric_value.get("int64Value") {
                number_like_u64(value).ok_or(OnlineError::SchemaMismatch)?
            } else if let Some(value) = metric_value.get("doubleValue") {
                count_from_f64(number_like_f64(value).ok_or(OnlineError::SchemaMismatch)?)?
            } else {
                return Err(OnlineError::SchemaMismatch);
            };
            total = checked_analytics_add(total, count)?;
        }
    }
    Ok(total)
}

fn parse_qwen_analytics(
    provider: OnlineProvider,
    calls_json: &str,
    tokens_json: &str,
) -> Result<OnlineSnapshot, OnlineError> {
    if !matches!(
        provider,
        OnlineProvider::QwenCn | OnlineProvider::QwenGlobal
    ) {
        return Err(OnlineError::InvalidProvider);
    }
    let calls = parse_prometheus_metric(calls_json)?;
    let tokens = parse_prometheus_metric(tokens_json)?;
    let mut models = BTreeMap::<String, QwenModelUsage>::new();
    for (model, value) in calls {
        models.entry(model).or_default().calls = value;
    }
    for (model, value) in tokens {
        models.entry(model).or_default().tokens = value;
    }
    let requests = models.values().try_fold(0_u64, |total, model| {
        checked_analytics_add(total, model.calls)
    })?;
    let total_tokens = models.values().try_fold(0_u64, |total, model| {
        checked_analytics_add(total, model.tokens)
    })?;
    let entries = models
        .into_iter()
        .take(MAX_DETAIL_ENTRIES)
        .map(|(model, usage)| OnlineDetailEntry {
            label: model,
            used: Some(usage.tokens.to_string()),
            remaining: None,
            limit: None,
            unit: " Token".to_string(),
            used_percent: None,
            window: Some(format!("{} 次调用 · Prometheus 采样合计", usage.calls)),
            start_at_ms: None,
            reset_at_ms: None,
            remaining_ms: None,
        })
        .collect();
    Ok(OnlineSnapshot {
        provider_id: provider.id().to_string(),
        label: provider.label().to_string(),
        source: provider.source().to_string(),
        experimental: false,
        balance_cny: None,
        balance_original: None,
        quota_used_percent: None,
        cooldown_ends_at_ms: None,
        requests: Some(requests),
        total_tokens: Some(total_tokens),
        estimated_cost_cny: None,
        primary_label: "今日 Token".to_string(),
        primary_value: format_integer_with_commas(total_tokens),
        secondary_value: format!("Prometheus · {requests} 次模型调用"),
        detail_sections: vec![OnlineDetailSection {
            title: "模型用量".to_string(),
            entries,
        }],
    })
}

#[derive(Default)]
struct QwenModelUsage {
    calls: u64,
    tokens: u64,
}

fn parse_prometheus_metric(json: &str) -> Result<BTreeMap<String, u64>, OnlineError> {
    let value: Value = serde_json::from_str(json).map_err(|_| OnlineError::InvalidJson)?;
    if value.get("status").and_then(Value::as_str) != Some("success") {
        return Err(OnlineError::ApiRejected);
    }
    let results = value
        .get("data")
        .and_then(|data| data.get("result"))
        .and_then(Value::as_array)
        .ok_or(OnlineError::SchemaMismatch)?;
    let mut totals = BTreeMap::<String, u64>::new();
    for result in results.iter().take(MAX_DETAIL_ENTRIES) {
        let metric = result
            .get("metric")
            .and_then(Value::as_object)
            .ok_or(OnlineError::SchemaMismatch)?;
        let model = analytics_label(
            metric
                .get("model")
                .or_else(|| metric.get("model_name"))
                .and_then(Value::as_str),
            "全部模型",
        );
        let mut series_total = 0_u64;
        if let Some(values) = result.get("values").and_then(Value::as_array) {
            for sample in values {
                let sample = sample.as_array().ok_or(OnlineError::SchemaMismatch)?;
                let value = sample.get(1).ok_or(OnlineError::SchemaMismatch)?;
                series_total = checked_analytics_add(series_total, prometheus_count(value)?)?;
            }
        } else if let Some(sample) = result.get("value").and_then(Value::as_array) {
            let value = sample.get(1).ok_or(OnlineError::SchemaMismatch)?;
            series_total = prometheus_count(value)?;
        } else {
            return Err(OnlineError::SchemaMismatch);
        }
        let current = totals.entry(model).or_default();
        *current = checked_analytics_add(*current, series_total)?;
    }
    Ok(totals)
}

fn prometheus_count(value: &Value) -> Result<u64, OnlineError> {
    match value {
        Value::String(text) => text
            .parse::<f64>()
            .map_err(|_| OnlineError::SchemaMismatch)
            .and_then(count_from_f64),
        _ => number_like_f64(value)
            .ok_or(OnlineError::SchemaMismatch)
            .and_then(count_from_f64),
    }
}

fn count_from_f64(value: f64) -> Result<u64, OnlineError> {
    if !value.is_finite()
        || value < 0.0
        || value > u64::MAX as f64
        || value.fract().abs() > f64::EPSILON
    {
        return Err(OnlineError::SchemaMismatch);
    }
    Ok(value as u64)
}

fn analytics_count_entry(label: &str, value: u64, unit: &str) -> OnlineDetailEntry {
    OnlineDetailEntry {
        label: label.to_string(),
        used: Some(value.to_string()),
        remaining: None,
        limit: None,
        unit: unit.to_string(),
        used_percent: None,
        window: Some("请求区间汇总".to_string()),
        start_at_ms: None,
        reset_at_ms: None,
        remaining_ms: None,
    }
}

fn format_integer_with_commas(value: u64) -> String {
    let raw = value.to_string();
    let mut formatted = String::with_capacity(raw.len() + raw.len() / 3);
    for (index, character) in raw.chars().enumerate() {
        if index > 0 && (raw.len() - index).is_multiple_of(3) {
            formatted.push(',');
        }
        formatted.push(character);
    }
    formatted
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
        detail_sections: Vec::new(),
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
            if let Some(remaining_percent) = map
                .get("current_interval_remaining_percent")
                .or_else(|| map.get("currentIntervalRemainingPercent"))
                .and_then(number_like_f64)
                .filter(|percent| percent.is_finite() && (0.0..=100.0).contains(percent))
            {
                let model = minimax_model_label(map);
                return Some(MiniMaxQuota {
                    used_percent: 100.0 - remaining_percent,
                    detail: format!("{model} · 剩余 {remaining_percent:.1}%"),
                    reset_at_ms: find_reset_timestamp(value),
                });
            }
            let pairs = [
                (
                    "current_interval_usage_count",
                    "current_interval_total_count",
                    true,
                ),
                (
                    "current_weekly_usage_count",
                    "current_weekly_total_count",
                    true,
                ),
                ("usage_count", "total_count", false),
                ("used", "total", false),
            ];
            for (usage_key, total_key, usage_is_remaining) in pairs {
                let usage = map.get(usage_key).and_then(number_like_u64);
                let total = map.get(total_key).and_then(number_like_u64);
                if let (Some(usage), Some(total)) = (usage, total) {
                    if total == 0 || usage > total {
                        continue;
                    }
                    let (used, remaining) = if usage_is_remaining {
                        (total - usage, usage)
                    } else {
                        (usage, total - usage)
                    };
                    let model = map
                        .get("model_name")
                        .or_else(|| map.get("modelName"))
                        .and_then(Value::as_str)
                        .filter(|name| !name.trim().is_empty());
                    let detail = match model {
                        Some(model) => format!("{model} · 剩余 {remaining} / {total}"),
                        None => format!("剩余 {remaining} / {total}"),
                    };
                    return Some(MiniMaxQuota {
                        used_percent: percentage(used as f64, total as f64),
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

fn minimax_detail_sections(value: &Value) -> Vec<OnlineDetailSection> {
    let mut entries = Vec::new();
    collect_minimax_detail_entries(value, &mut entries);
    if entries.is_empty() {
        let remaining =
            find_f64_key(value, "usage_percent").or_else(|| find_f64_key(value, "usagePercent"));
        let Some(remaining) = remaining.filter(|value| (0.0..=100.0).contains(value)) else {
            return Vec::new();
        };
        let used = 100.0 - remaining;
        vec![OnlineDetailSection {
            title: "套餐额度".to_string(),
            entries: vec![OnlineDetailEntry {
                label: "套餐 · 套餐用量".to_string(),
                used: Some(format_detail_number(used)),
                remaining: Some(format_detail_number(remaining)),
                limit: Some("100".to_string()),
                unit: "%".to_string(),
                used_percent: Some(used),
                window: None,
                start_at_ms: None,
                reset_at_ms: find_reset_timestamp(value),
                remaining_ms: None,
            }],
        }]
    } else {
        vec![OnlineDetailSection {
            title: "模型额度".to_string(),
            entries,
        }]
    }
}

const MAX_DETAIL_ENTRIES: usize = 256;

fn collect_minimax_detail_entries(value: &Value, entries: &mut Vec<OnlineDetailEntry>) {
    if entries.len() >= MAX_DETAIL_ENTRIES {
        return;
    }
    match value {
        Value::Object(map) => {
            let model = minimax_model_label(map);
            if let Some(entry) = minimax_count_detail_entry(
                format!("{model} · 当前窗口"),
                map,
                MiniMaxQuotaWindow::Current,
            ) {
                if entries.len() < MAX_DETAIL_ENTRIES {
                    entries.push(entry);
                }
            }
            if let Some(entry) = minimax_count_detail_entry(
                format!("{model} · 周额度"),
                map,
                MiniMaxQuotaWindow::Weekly,
            ) {
                if entries.len() < MAX_DETAIL_ENTRIES {
                    entries.push(entry);
                }
            }
            if !map.contains_key("current_interval_usage_count")
                && !map.contains_key("current_weekly_usage_count")
            {
                if let Some(entry) = minimax_count_detail_entry(
                    format!("{model} · 套餐用量"),
                    map,
                    MiniMaxQuotaWindow::Legacy,
                ) {
                    if entries.len() < MAX_DETAIL_ENTRIES {
                        entries.push(entry);
                    }
                }
            }
            for child in map.values() {
                collect_minimax_detail_entries(child, entries);
                if entries.len() >= MAX_DETAIL_ENTRIES {
                    break;
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_minimax_detail_entries(item, entries);
                if entries.len() >= MAX_DETAIL_ENTRIES {
                    break;
                }
            }
        }
        _ => {}
    }
}

#[derive(Clone, Copy)]
enum MiniMaxQuotaWindow {
    Current,
    Weekly,
    Legacy,
}

fn minimax_count_detail_entry(
    label: String,
    map: &serde_json::Map<String, Value>,
    window: MiniMaxQuotaWindow,
) -> Option<OnlineDetailEntry> {
    let (used_key, total_key, remaining_percent_key, status_key, usage_is_remaining, weekly) =
        match window {
            MiniMaxQuotaWindow::Current => (
                "current_interval_usage_count",
                "current_interval_total_count",
                "current_interval_remaining_percent",
                "current_interval_status",
                true,
                false,
            ),
            MiniMaxQuotaWindow::Weekly => (
                "current_weekly_usage_count",
                "current_weekly_total_count",
                "current_weekly_remaining_percent",
                "current_weekly_status",
                true,
                true,
            ),
            MiniMaxQuotaWindow::Legacy => (
                "usage_count",
                "total_count",
                "usage_percent",
                "status",
                false,
                false,
            ),
        };
    let total = map.get(total_key).and_then(number_like_u64);
    let usage = map.get(used_key).and_then(number_like_u64);
    let remaining_percent = map
        .get(remaining_percent_key)
        .and_then(number_like_f64)
        .filter(|percent| percent.is_finite() && (0.0..=100.0).contains(percent));
    let status = map.get(status_key).and_then(number_like_i64);
    let unlimited = status == Some(3);
    let boost = if weekly {
        map.get("weekly_boost_permille")
            .or_else(|| map.get("weeklyBoostPermille"))
            .and_then(number_like_f64)
            .map(|permille| (permille / 1000.0).max(0.0))
            .unwrap_or(1.0)
    } else {
        1.0
    };

    let (used, remaining, limit, unit, used_percent) = if unlimited {
        (
            None,
            Some("无限".to_string()),
            Some("无限".to_string()),
            "".to_string(),
            Some(0.0),
        )
    } else if let (Some(total), Some(remaining)) = (total, usage) {
        if total == 0 || remaining > total {
            percent_quota_values(remaining_percent?, boost)
        } else {
            let (used, remaining) = if usage_is_remaining {
                (total - remaining, remaining)
            } else {
                (remaining, total - remaining)
            };
            (
                Some(used.to_string()),
                Some(remaining.to_string()),
                Some(total.to_string()),
                "次".to_string(),
                Some(percentage(used as f64, total as f64)),
            )
        }
    } else {
        percent_quota_values(remaining_percent?, boost)
    };

    let start_at_ms = if weekly {
        map.get("weekly_start_time")
            .or_else(|| map.get("weeklyStartTime"))
            .and_then(timestamp_value)
    } else {
        map.get("start_time")
            .or_else(|| map.get("startTime"))
            .and_then(timestamp_value)
    };
    let reset_at_ms = if weekly {
        map.get("weekly_end_time")
            .or_else(|| map.get("weeklyEndTime"))
            .or_else(|| map.get("next_weekly_reset_time"))
            .and_then(timestamp_value)
    } else {
        map.get("end_time")
            .or_else(|| map.get("endTime"))
            .and_then(timestamp_value)
    };
    let remaining_ms = if weekly {
        map.get("weekly_remains_time")
            .or_else(|| map.get("weeklyRemainsTime"))
            .and_then(number_like_i64)
            .filter(|duration| *duration >= 0)
    } else {
        map.get("remains_time")
            .or_else(|| map.get("remainsTime"))
            .and_then(number_like_i64)
            .filter(|duration| *duration >= 0)
    };
    Some(OnlineDetailEntry {
        label,
        used,
        remaining,
        limit,
        unit,
        used_percent,
        window: window_duration(start_at_ms, reset_at_ms),
        start_at_ms,
        reset_at_ms,
        remaining_ms,
    })
}

fn percent_quota_values(
    remaining_percent: f64,
    boost: f64,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    Option<f64>,
) {
    let limit = 100.0 * boost;
    let remaining = (remaining_percent * boost).clamp(0.0, limit);
    let used = limit - remaining;
    (
        Some(format_detail_number(used)),
        Some(format_detail_number(remaining)),
        Some(format_detail_number(limit)),
        "%".to_string(),
        Some(if limit > 0.0 {
            percentage(used, limit)
        } else {
            0.0
        }),
    )
}

fn minimax_model_label(map: &serde_json::Map<String, Value>) -> String {
    let raw = map
        .get("model_name")
        .or_else(|| map.get("modelName"))
        .or_else(|| map.get("model_type"))
        .or_else(|| map.get("modelType"))
        .and_then(Value::as_str)
        .unwrap_or("套餐");
    let label: String = raw
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .take(80)
        .collect();
    if label.is_empty() {
        "套餐".to_string()
    } else {
        label
    }
}

fn window_duration(start_at_ms: Option<i64>, reset_at_ms: Option<i64>) -> Option<String> {
    let duration = reset_at_ms?.checked_sub(start_at_ms?)?;
    if duration <= 0 {
        return None;
    }
    const HOUR_MS: i64 = 3_600_000;
    const MINUTE_MS: i64 = 60_000;
    if duration % HOUR_MS == 0 {
        Some(format!("{} 小时", duration / HOUR_MS))
    } else if duration % MINUTE_MS == 0 {
        Some(format!("{} 分钟", duration / MINUTE_MS))
    } else {
        Some(format!("{} 秒", duration / 1_000))
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
          "user": {"email": "private@example.com", "name": "Private User"},
          "usage": {
            "limit": "100",
            "used": "48",
            "remaining": "52",
            "resetTime": "2026-01-08T00:00:00Z"
          },
          "limits": [
            {
              "window": {"duration": 300, "timeUnit": "TIME_UNIT_MINUTE"},
              "detail": {
                "limit": "100",
                "used": "7",
                "remaining": "93",
                "resetTime": "2026-01-01T05:00:00Z"
              }
            },
            {
              "window": {"duration": 60, "timeUnit": "TIME_UNIT_MINUTE"},
              "detail": {
                "limit": "200",
                "used": "20",
                "remaining": "180",
                "resetTime": "2026-01-01T01:00:00Z"
              }
            }
          ],
          "parallel": {"limit": "20"},
          "totalQuota": {"limit": "100", "used": "1", "remaining": "99"}
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

        assert_eq!(snapshot.detail_sections.len(), 2);
        let windows = &snapshot.detail_sections[0];
        assert_eq!(windows.title, "额度窗口");
        assert_eq!(windows.entries.len(), 3);
        assert_eq!(windows.entries[0].label, "周额度");
        assert_eq!(windows.entries[0].used.as_deref(), Some("48"));
        assert_eq!(windows.entries[0].remaining.as_deref(), Some("52"));
        assert_eq!(windows.entries[0].limit.as_deref(), Some("100"));
        assert_eq!(windows.entries[0].unit, "%");
        assert_eq!(windows.entries[0].used_percent, Some(48.0));
        assert_eq!(windows.entries[0].reset_at_ms, Some(1_767_830_400_000));

        assert_eq!(windows.entries[1].label, "5 小时窗口");
        assert_eq!(windows.entries[1].window.as_deref(), Some("300 分钟"));
        assert_eq!(windows.entries[1].used_percent, Some(7.0));
        assert_eq!(windows.entries[2].label, "1 小时窗口");
        assert_eq!(windows.entries[2].window.as_deref(), Some("60 分钟"));

        let other = &snapshot.detail_sections[1];
        assert_eq!(other.title, "其他限制");
        assert_eq!(other.entries.len(), 2);
        assert_eq!(other.entries[0].label, "并发上限");
        assert_eq!(other.entries[0].limit.as_deref(), Some("20"));
        assert_eq!(other.entries[0].unit, "路");
        assert_eq!(other.entries[1].label, "总额度");
        assert_eq!(other.entries[1].remaining.as_deref(), Some("99"));

        let serialized = serde_json::to_string(&snapshot).expect("serialized snapshot");
        assert!(!serialized.contains("private@example.com"));
        assert!(!serialized.contains("Private User"));
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
        assert_eq!(snapshot.quota_used_percent, Some(62.5));
        assert_eq!(snapshot.cooldown_ends_at_ms, Some(1_783_686_600_000));
        assert!(snapshot.experimental);
    }

    #[test]
    fn parses_minimax_model_remains_and_normalizes_seconds_timestamp() {
        let json = r#"{
          "model_remains": [
            {
              "start_time": 1783668600,
              "end_time": 1783686600,
              "remains_time": 600000,
              "current_interval_total_count": 1000,
              "current_interval_usage_count": 375,
              "current_weekly_total_count": 5000,
              "current_weekly_usage_count": 1000,
              "model_name": "MiniMax-M2.5"
            },
            {
              "start_time": 1783668600000,
              "end_time": 1783686600000,
              "remains_time": 900000,
              "current_interval_total_count": 10,
              "current_interval_usage_count": 2,
              "model_name": "image-01"
            }
          ],
          "base_resp": {"status_code": 0, "status_msg": "success"}
        }"#;

        let snapshot = parse_snapshot(OnlineProvider::MiniMaxCn, json).expect("snapshot");

        assert_eq!(snapshot.quota_used_percent, Some(62.5));
        assert_eq!(snapshot.cooldown_ends_at_ms, Some(1_783_686_600_000));
        assert_eq!(snapshot.secondary_value, "MiniMax-M2.5 · 剩余 375 / 1000");

        assert_eq!(snapshot.detail_sections.len(), 1);
        let models = &snapshot.detail_sections[0];
        assert_eq!(models.title, "模型额度");
        assert_eq!(models.entries.len(), 3);
        assert_eq!(models.entries[0].label, "MiniMax-M2.5 · 当前窗口");
        assert_eq!(models.entries[0].used.as_deref(), Some("625"));
        assert_eq!(models.entries[0].remaining.as_deref(), Some("375"));
        assert_eq!(models.entries[0].limit.as_deref(), Some("1000"));
        assert_eq!(models.entries[0].unit, "次");
        assert_eq!(models.entries[0].used_percent, Some(62.5));
        assert_eq!(models.entries[0].start_at_ms, Some(1_783_668_600_000));
        assert_eq!(models.entries[0].reset_at_ms, Some(1_783_686_600_000));
        assert_eq!(models.entries[0].remaining_ms, Some(600_000));

        assert_eq!(models.entries[1].label, "MiniMax-M2.5 · 周额度");
        assert_eq!(models.entries[1].used.as_deref(), Some("4000"));
        assert_eq!(models.entries[1].remaining.as_deref(), Some("1000"));
        assert_eq!(models.entries[1].used_percent, Some(80.0));

        assert_eq!(models.entries[2].label, "image-01 · 当前窗口");
        assert_eq!(models.entries[2].used.as_deref(), Some("8"));
        assert_eq!(models.entries[2].remaining.as_deref(), Some("2"));
        assert_eq!(models.entries[2].used_percent, Some(80.0));
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
        assert_eq!(snapshot.detail_sections.len(), 1);
        let detail = &snapshot.detail_sections[0].entries[0];
        assert_eq!(detail.label, "general · 套餐用量");
        assert_eq!(detail.used.as_deref(), Some("27.5"));
        assert_eq!(detail.remaining.as_deref(), Some("72.5"));
        assert_eq!(detail.limit.as_deref(), Some("100"));
        assert_eq!(detail.unit, "%");
        assert_eq!(detail.used_percent, Some(27.5));
        assert_eq!(detail.reset_at_ms, Some(1_783_686_600_000));
    }

    #[test]
    fn parses_all_minimax_resources_and_treats_coding_plan_counts_as_remaining() {
        let json = r#"{
          "base_resp": {"status_code": 0, "status_msg": "success"},
          "model_remains": [
            {
              "model_name": "general",
              "start_time": 1783668600000,
              "end_time": 1783686600000,
              "remains_time": 600000,
              "current_interval_total_count": 0,
              "current_interval_usage_count": 0,
              "current_interval_remaining_percent": 94,
              "current_weekly_total_count": 0,
              "current_weekly_usage_count": 0,
              "current_weekly_remaining_percent": 98,
              "weekly_boost_permille": 1500,
              "weekly_start_time": 1783555200000,
              "weekly_end_time": 1784160000000,
              "weekly_remains_time": 432000000
            },
            {
              "model_name": "video",
              "current_interval_total_count": 3,
              "current_interval_usage_count": 3,
              "current_interval_remaining_percent": 100,
              "current_weekly_total_count": 21,
              "current_weekly_usage_count": 21,
              "current_weekly_remaining_percent": 100
            },
            {
              "model_name": "speech-hd",
              "current_interval_total_count": 9000,
              "current_interval_usage_count": 8000
            },
            {
              "model_name": "image-01",
              "current_interval_total_count": 100,
              "current_interval_usage_count": 80
            },
            {
              "model_name": "music-2.0",
              "current_interval_total_count": 50,
              "current_interval_usage_count": 40
            }
          ]
        }"#;

        let snapshot = parse_snapshot(OnlineProvider::MiniMaxCn, json).expect("snapshot");
        let entries = &snapshot.detail_sections[0].entries;

        assert_eq!(entries.len(), 7);
        assert_eq!(entries[0].label, "general · 当前窗口");
        assert_eq!(entries[0].used.as_deref(), Some("6"));
        assert_eq!(entries[0].remaining.as_deref(), Some("94"));
        assert_eq!(entries[0].unit, "%");
        assert_eq!(entries[0].used_percent, Some(6.0));
        assert_eq!(entries[1].label, "general · 周额度");
        assert_eq!(entries[1].remaining.as_deref(), Some("147"));
        assert_eq!(entries[1].limit.as_deref(), Some("150"));

        let video = entries
            .iter()
            .find(|entry| entry.label == "video · 当前窗口")
            .expect("video quota");
        assert_eq!(video.used.as_deref(), Some("0"));
        assert_eq!(video.remaining.as_deref(), Some("3"));
        assert_eq!(video.used_percent, Some(0.0));

        for resource in ["speech-hd", "image-01", "music-2.0"] {
            assert!(entries
                .iter()
                .any(|entry| entry.label == format!("{resource} · 当前窗口")));
        }
    }

    #[test]
    fn bounds_untrusted_minimax_detail_arrays() {
        let model_remains = (0..300)
            .map(|index| {
                serde_json::json!({
                    "model_name": format!("model-{index}"),
                    "current_interval_total_count": 100,
                    "current_interval_usage_count": 1
                })
            })
            .collect::<Vec<_>>();
        let json = serde_json::json!({
            "model_remains": model_remains,
            "base_resp": {"status_code": 0, "status_msg": "success"}
        })
        .to_string();

        let snapshot = parse_snapshot(OnlineProvider::MiniMaxCn, &json).expect("snapshot");

        assert_eq!(snapshot.detail_sections[0].entries.len(), 256);
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
    fn parses_openai_codex_organization_usage_and_cost_without_claiming_subscription_quota() {
        let usage = r#"{
          "data": [{"results": [
            {"model": "gpt-5.4-codex", "input_tokens": 1000, "output_tokens": 500, "input_cached_tokens": 800, "num_model_requests": 5},
            {"model": "gpt-5.4-mini", "input_tokens": 200, "output_tokens": 50, "input_cached_tokens": 0, "num_model_requests": 2}
          ]}],
          "has_more": false
        }"#;
        let costs = r#"{
          "data": [{"results": [
            {"amount": {"value": 0.06, "currency": "usd"}, "line_item": "Responses API"}
          ]}],
          "has_more": false
        }"#;

        let snapshot = parse_openai_analytics(usage, costs, Some(7.2)).expect("snapshot");

        assert_eq!(snapshot.provider_id, "openai_codex");
        assert_eq!(snapshot.requests, Some(7));
        assert_eq!(snapshot.total_tokens, Some(1750));
        assert_eq!(snapshot.estimated_cost_cny, Some(0.432));
        assert_eq!(snapshot.primary_value, "$0.06");
        assert!(snapshot.secondary_value.contains("API 组织"));
        assert!(snapshot.secondary_value.contains("非 ChatGPT 套餐"));
        assert!(snapshot.detail_sections[0]
            .entries
            .iter()
            .any(|entry| entry.label == "gpt-5.4-codex"));
    }

    #[test]
    fn parses_claude_code_daily_analytics_without_exposing_actor_identity() {
        let json = r#"{
          "data": [{
            "date": "2026-07-11T00:00:00Z",
            "actor": {"type": "user_actor", "email_address": "private@example.com"},
            "core_metrics": {"num_sessions": 5, "lines_of_code": {"added": 1543, "removed": 892}, "commits_by_claude_code": 12, "pull_requests_by_claude_code": 2},
            "model_breakdown": [{
              "model": "claude-opus-4-8",
              "tokens": {"input": 100000, "output": 35000, "cache_read": 10000, "cache_creation": 5000},
              "estimated_cost": {"currency": "USD", "amount": 1025}
            }]
          }],
          "has_more": false,
          "next_page": null
        }"#;

        let snapshot = parse_claude_code_analytics(json, Some(7.2)).expect("snapshot");
        let serialized = serde_json::to_string(&snapshot).expect("serialize");

        assert_eq!(snapshot.provider_id, "claude_code");
        assert_eq!(snapshot.requests, Some(5));
        assert_eq!(snapshot.total_tokens, Some(150000));
        assert_eq!(snapshot.estimated_cost_cny, Some(73.8));
        assert_eq!(snapshot.primary_value, "$10.25");
        assert!(!serialized.contains("private@example.com"));
        assert!(snapshot
            .detail_sections
            .iter()
            .any(|section| section.title == "模型用量"));
    }

    #[test]
    fn recognizes_requested_analytics_provider_ids() {
        assert_eq!(
            OnlineProvider::from_id("openai_codex"),
            Some(OnlineProvider::OpenAiCodex)
        );
        assert_eq!(
            OnlineProvider::from_id("claude_code"),
            Some(OnlineProvider::ClaudeCode)
        );
        assert_eq!(
            OnlineProvider::from_id("gemini"),
            Some(OnlineProvider::Gemini)
        );
        assert_eq!(
            OnlineProvider::from_id("qwen_cn"),
            Some(OnlineProvider::QwenCn)
        );
        assert_eq!(
            OnlineProvider::from_id("qwen_global"),
            Some(OnlineProvider::QwenGlobal)
        );
    }

    #[test]
    fn validates_gemini_and_qwen_structured_credentials_at_the_rust_boundary() {
        let gemini = OnlineClient::new(
            OnlineProvider::Gemini,
            r#"{"projectId":"sample-project","accessToken":"ya29.test-token"}"#,
        )
        .expect("Gemini client");
        let gemini_request = gemini
            .gemini_metric_request(
                OnlineUsageRange::new(1_783_641_600_000, 1_783_728_000_000).expect("range"),
                "code_assist/api_calls_count",
            )
            .expect("Gemini metric request");
        assert_eq!(gemini_request.url().scheme(), "https");
        assert_eq!(
            gemini_request.url().host_str(),
            Some("monitoring.googleapis.com")
        );
        assert!(gemini_request.url().path().contains("sample-project"));
        assert!(gemini_request
            .headers()
            .get(reqwest::header::AUTHORIZATION)
            .is_some_and(reqwest::header::HeaderValue::is_sensitive));

        assert_eq!(
            OnlineClient::new(
                OnlineProvider::Gemini,
                r#"{"projectId":"../bad","accessToken":"ya29.test-token"}"#,
            )
            .expect_err("invalid project")
            .code(),
            "ONLINE_INVALID_CREDENTIAL"
        );

        let qwen = OnlineClient::new(
            OnlineProvider::QwenCn,
            r#"{"endpoint":"https://prometheus.cn-hangzhou.aliyuncs.com","accessKeyId":"LTAI-test","accessKeySecret":"secret-test"}"#,
        )
        .expect("Qwen client");
        let qwen_request = qwen
            .qwen_metric_request(
                OnlineUsageRange::new(1_783_641_600_000, 1_783_728_000_000).expect("range"),
                "model_usage",
            )
            .expect("Qwen metric request");
        assert_eq!(qwen_request.url().scheme(), "https");
        assert!(qwen_request.url().path().ends_with("/api/v1/query_range"));
        assert!(qwen_request
            .headers()
            .get(reqwest::header::AUTHORIZATION)
            .is_some_and(reqwest::header::HeaderValue::is_sensitive));
        assert_eq!(
            OnlineClient::new(
                OnlineProvider::QwenCn,
                r#"{"endpoint":"https://example.com","accessKeyId":"LTAI-test","accessKeySecret":"secret-test"}"#,
            )
            .expect_err("untrusted host")
            .code(),
            "ONLINE_INVALID_CREDENTIAL"
        );
    }

    #[test]
    fn parses_gemini_code_assist_monitoring_metrics() {
        let calls = r#"{"timeSeries":[{"points":[{"value":{"int64Value":"7"}},{"value":{"doubleValue":5}}]}]}"#;
        let tokens = r#"{"timeSeries":[{"points":[{"value":{"int64Value":"3000"}}]}]}"#;

        let snapshot = parse_gemini_analytics(calls, tokens).expect("snapshot");

        assert_eq!(snapshot.provider_id, "gemini");
        assert_eq!(snapshot.requests, Some(12));
        assert_eq!(snapshot.total_tokens, Some(3000));
        assert_eq!(snapshot.primary_value, "3,000");
        assert!(snapshot.secondary_value.contains("Cloud Monitoring"));
    }

    #[test]
    fn parses_qwen_prometheus_usage_for_every_model() {
        let calls = r#"{
          "status":"success","data":{"resultType":"matrix","result":[
            {"metric":{"model":"qwen3.7-plus"},"values":[[1783641600,"3"],[1783645200,"4"]]},
            {"metric":{"model":"qwen3-coder"},"values":[[1783641600,"2"]]}
          ]}
        }"#;
        let tokens = r#"{
          "status":"success","data":{"resultType":"matrix","result":[
            {"metric":{"model":"qwen3.7-plus"},"values":[[1783641600,"1000"],[1783645200,"2000"]]},
            {"metric":{"model":"qwen3-coder"},"values":[[1783641600,"500"]]}
          ]}
        }"#;

        let snapshot =
            parse_qwen_analytics(OnlineProvider::QwenCn, calls, tokens).expect("snapshot");

        assert_eq!(snapshot.provider_id, "qwen_cn");
        assert_eq!(snapshot.requests, Some(9));
        assert_eq!(snapshot.total_tokens, Some(3500));
        assert_eq!(snapshot.detail_sections[0].entries.len(), 2);
        assert!(snapshot.detail_sections[0]
            .entries
            .iter()
            .any(|entry| entry.label == "qwen3.7-plus"));
        assert!(snapshot.detail_sections[0]
            .entries
            .iter()
            .any(|entry| entry.label == "qwen3-coder"));
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
