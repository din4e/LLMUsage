use serde::Deserialize;
use std::fmt;
use std::time::Duration;

const GLM_BASE_URL: &str = "https://open.bigmodel.cn";

#[derive(Debug, PartialEq)]
pub struct GlmUsageSnapshot {
    pub plan_level: String,
    pub used_percent: f64,
    pub cooldown_ends_at_ms: i64,
    pub requests: u64,
    pub total_tokens: u64,
}

#[derive(Debug, PartialEq)]
pub enum GlmParseError {
    InvalidJson,
    ApiRejected,
    SchemaMismatch,
    InvalidCredential,
    InvalidDateRange,
    RequestFailed,
}

impl GlmParseError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidJson => "GLM_INVALID_JSON",
            Self::ApiRejected => "GLM_API_REJECTED",
            Self::SchemaMismatch => "GLM_SCHEMA_MISMATCH",
            Self::InvalidCredential => "GLM_INVALID_CREDENTIAL",
            Self::InvalidDateRange => "GLM_INVALID_DATE_RANGE",
            Self::RequestFailed => "GLM_REQUEST_FAILED",
        }
    }
}

#[derive(Debug)]
pub struct GlmClient {
    client: reqwest::Client,
    api_key: reqwest::header::HeaderValue,
}

impl GlmClient {
    pub fn new(api_key: &str) -> Result<Self, GlmParseError> {
        let trimmed = api_key.trim();
        if trimmed.is_empty() || trimmed.len() > 4096 {
            return Err(GlmParseError::InvalidCredential);
        }

        let mut header = reqwest::header::HeaderValue::from_str(trimmed)
            .map_err(|_| GlmParseError::InvalidCredential)?;
        header.set_sensitive(true);
        let client = reqwest::Client::builder()
            .https_only(true)
            .timeout(Duration::from_secs(15))
            .user_agent("LLMUsage/0.1")
            .build()
            .map_err(|_| GlmParseError::RequestFailed)?;

        Ok(Self {
            client,
            api_key: header,
        })
    }

    fn request(&self, path: &str) -> reqwest::RequestBuilder {
        self.client
            .get(format!("{GLM_BASE_URL}{path}"))
            .header(reqwest::header::AUTHORIZATION, self.api_key.clone())
            .header(reqwest::header::ACCEPT, "application/json")
    }

    pub fn quota_request(&self) -> Result<reqwest::Request, GlmParseError> {
        self.request("/api/monitor/usage/quota/limit")
            .build()
            .map_err(|_| GlmParseError::RequestFailed)
    }

    pub fn model_usage_request(
        &self,
        start_time: &str,
        end_time: &str,
    ) -> Result<reqwest::Request, GlmParseError> {
        if !is_monitor_datetime(start_time)
            || !is_monitor_datetime(end_time)
            || start_time > end_time
        {
            return Err(GlmParseError::InvalidDateRange);
        }

        self.request("/api/monitor/usage/model-usage")
            .query(&[("startTime", start_time), ("endTime", end_time)])
            .build()
            .map_err(|_| GlmParseError::RequestFailed)
    }

    pub async fn fetch_snapshot(
        &self,
        start_time: &str,
        end_time: &str,
    ) -> Result<GlmUsageSnapshot, GlmParseError> {
        let quota_request = self.quota_request()?;
        let usage_request = self.model_usage_request(start_time, end_time)?;
        let (quota, usage) = tokio::try_join!(
            self.execute_text(quota_request),
            self.execute_text(usage_request)
        )?;
        parse_snapshot(&quota, &usage)
    }

    async fn execute_text(&self, request: reqwest::Request) -> Result<String, GlmParseError> {
        let response = self
            .client
            .execute(request)
            .await
            .map_err(|_| GlmParseError::RequestFailed)?;
        if !response.status().is_success() {
            return Err(GlmParseError::ApiRejected);
        }
        response
            .text()
            .await
            .map_err(|_| GlmParseError::RequestFailed)
    }
}

fn is_monitor_datetime(value: &str) -> bool {
    if value.len() != 19 {
        return false;
    }
    value.bytes().enumerate().all(|(index, byte)| match index {
        4 | 7 => byte == b'-',
        10 => byte == b' ',
        13 | 16 => byte == b':',
        _ => byte.is_ascii_digit(),
    })
}

impl fmt::Display for GlmParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

#[derive(Deserialize)]
struct QuotaResponse {
    code: i64,
    data: Option<QuotaData>,
}

#[derive(Deserialize)]
struct QuotaData {
    level: String,
    limits: Vec<TokenLimit>,
}

#[derive(Deserialize)]
struct TokenLimit {
    #[serde(rename = "type")]
    kind: String,
    percentage: f64,
    #[serde(rename = "nextResetTime")]
    next_reset_time: i64,
}

#[derive(Deserialize)]
struct UsageResponse {
    code: i64,
    data: Option<UsageData>,
}

#[derive(Deserialize)]
struct UsageData {
    #[serde(rename = "totalUsage")]
    total_usage: TotalUsage,
}

#[derive(Deserialize)]
struct TotalUsage {
    #[serde(rename = "totalModelCallCount")]
    total_model_call_count: u64,
    #[serde(rename = "totalTokensUsage")]
    total_tokens_usage: u64,
}

pub fn parse_snapshot(
    quota_json: &str,
    usage_json: &str,
) -> Result<GlmUsageSnapshot, GlmParseError> {
    let quota: QuotaResponse =
        serde_json::from_str(quota_json).map_err(|_| GlmParseError::InvalidJson)?;
    let usage: UsageResponse =
        serde_json::from_str(usage_json).map_err(|_| GlmParseError::InvalidJson)?;

    if quota.code != 200 || usage.code != 200 {
        return Err(GlmParseError::ApiRejected);
    }

    let quota_data = quota.data.ok_or(GlmParseError::SchemaMismatch)?;
    let usage_data = usage.data.ok_or(GlmParseError::SchemaMismatch)?;
    let token_limit = quota_data
        .limits
        .into_iter()
        .filter(|limit| limit.kind == "TOKENS_LIMIT")
        .min_by_key(|limit| limit.next_reset_time)
        .ok_or(GlmParseError::SchemaMismatch)?;

    if quota_data.level.trim().is_empty()
        || !token_limit.percentage.is_finite()
        || !(0.0..=100.0).contains(&token_limit.percentage)
        || token_limit.next_reset_time <= 0
    {
        return Err(GlmParseError::SchemaMismatch);
    }

    Ok(GlmUsageSnapshot {
        plan_level: quota_data.level,
        used_percent: token_limit.percentage,
        cooldown_ends_at_ms: token_limit.next_reset_time,
        requests: usage_data.total_usage.total_model_call_count,
        total_tokens: usage_data.total_usage.total_tokens_usage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_current_token_window_and_today_usage() {
        let quota = r#"{
          "code": 200,
          "data": {
            "level": "PRO",
            "limits": [
              {"type":"MCP_LIMIT","percentage":12,"nextResetTime":1783699200000},
              {"type":"TOKENS_LIMIT","percentage":68.5,"nextResetTime":1783686600000,"currentValue":68500}
            ]
          }
        }"#;
        let usage = r#"{
          "code": 200,
          "data": {
            "x_time":["2026-07-10"],
            "totalUsage":{"totalModelCallCount":17,"totalTokensUsage":123456},
            "modelCallCount":[17],
            "tokensUsage":[123456]
          }
        }"#;

        let snapshot = parse_snapshot(quota, usage).expect("valid GLM monitor responses");

        assert_eq!(snapshot.plan_level, "PRO");
        assert_eq!(snapshot.used_percent, 68.5);
        assert_eq!(snapshot.cooldown_ends_at_ms, 1_783_686_600_000);
        assert_eq!(snapshot.requests, 17);
        assert_eq!(snapshot.total_tokens, 123_456);
    }

    #[test]
    fn rejects_success_responses_without_a_token_limit() {
        let quota = r#"{"code":200,"data":{"level":"PRO","limits":[]}}"#;
        let usage = r#"{"code":200,"data":{"x_time":[],"totalUsage":{"totalModelCallCount":0,"totalTokensUsage":0},"modelCallCount":[],"tokensUsage":[]}}"#;

        let error =
            parse_snapshot(quota, usage).expect_err("schema drift must not become fake data");

        assert_eq!(error.code(), "GLM_SCHEMA_MISMATCH");
    }

    #[test]
    fn builds_a_sensitive_authenticated_usage_request() {
        let client = GlmClient::new("secret-key").expect("valid API key");

        let request = client
            .model_usage_request("2026-07-10 00:00:00", "2026-07-10 23:59:59")
            .expect("valid date range");

        assert_eq!(
            request.url().as_str(),
            "https://open.bigmodel.cn/api/monitor/usage/model-usage?startTime=2026-07-10+00%3A00%3A00&endTime=2026-07-10+23%3A59%3A59"
        );
        assert!(request.headers()["authorization"].is_sensitive());
        assert_eq!(request.headers()["authorization"], "secret-key");
    }

    #[test]
    fn rejects_empty_keys_and_invalid_date_ranges() {
        assert_eq!(
            GlmClient::new("  ")
                .expect_err("empty keys must fail")
                .code(),
            "GLM_INVALID_CREDENTIAL"
        );

        let client = GlmClient::new("secret-key").expect("valid API key");
        assert_eq!(
            client
                .model_usage_request("not-a-date", "2026-07-10 23:59:59")
                .expect_err("invalid dates must fail")
                .code(),
            "GLM_INVALID_DATE_RANGE"
        );
    }
}
