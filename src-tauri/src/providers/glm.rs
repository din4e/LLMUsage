use crate::providers::online::{OnlineDetailEntry, OnlineDetailSection};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;

const GLM_BASE_URL: &str = "https://open.bigmodel.cn";

/// GLM Coding Plan TOKENS_LIMIT is a 5-hour rolling usage window.
/// The monitor API exposes only the next reset time, so the window start is
/// derived by walking the known duration back from the reset timestamp.
const GLM_HOUR_MS: i64 = 60 * 60 * 1000;
const GLM_DAY_MS: i64 = 24 * GLM_HOUR_MS;
const GLM_WEEK_MS: i64 = 7 * GLM_DAY_MS;
const GLM_WINDOW_MS: i64 = 5 * GLM_HOUR_MS;

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlmUsageSnapshot {
    pub plan_level: String,
    pub used_percent: f64,
    pub cooldown_ends_at_ms: i64,
    pub requests: u64,
    pub total_tokens: u64,
    pub detail_sections: Vec<OnlineDetailSection>,
}

#[derive(Debug, PartialEq)]
pub enum GlmParseError {
    InvalidJson,
    ApiRejected,
    /// The BigModel monitor endpoints answer pay-as-you-go accounts with
    /// `{"code":500,"msg":"当前用户不存在coding plan"}` over HTTP 200. A valid
    /// key without a Coding Plan subscription is this error, not a rejection.
    NoCodingPlan,
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
            Self::NoCodingPlan => "GLM_NO_CODING_PLAN",
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

/// True when a rejected response blames a missing Coding Plan subscription.
/// Verified against production: both monitor endpoints answer a pay-as-you-go
/// key with `msg: "当前用户不存在coding plan"`.
fn hints_missing_coding_plan(quota: &QuotaResponse, usage: &UsageResponse) -> bool {
    [quota.msg.as_deref(), usage.msg.as_deref()]
        .into_iter()
        .flatten()
        .any(|message| message.to_ascii_lowercase().contains("coding plan"))
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

fn format_glm_value(value: f64) -> String {
    let formatted = format!("{value:.1}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn is_limit_kind(limit: &QuotaLimit, kind: &str) -> bool {
    limit.kind.eq_ignore_ascii_case(kind)
}

/// Maps BigModel's monitor window enum without inventing semantics for values
/// the adapter has not observed. Legacy TOKENS_LIMIT responses omitted both
/// fields and represented the original 5-hour window.
fn quota_window(limit: &QuotaLimit) -> (Option<String>, Option<i64>) {
    let count = limit.number.filter(|value| *value > 0);
    match (limit.unit, count) {
        (Some(3), Some(hours)) => (
            Some(format!("{hours} 小时")),
            hours.checked_mul(GLM_HOUR_MS),
        ),
        (Some(4), Some(days)) => (Some(format!("{days} 天")), days.checked_mul(GLM_DAY_MS)),
        (Some(6), Some(weeks)) => (
            Some(if weeks == 1 {
                "每周".to_string()
            } else {
                format!("{weeks} 周")
            }),
            weeks.checked_mul(GLM_WEEK_MS),
        ),
        (None, None) if is_limit_kind(limit, "TOKENS_LIMIT") => {
            (Some("5 小时".to_string()), Some(GLM_WINDOW_MS))
        }
        _ => (None, None),
    }
}

impl fmt::Display for GlmParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

#[derive(Deserialize)]
struct QuotaResponse {
    code: i64,
    #[serde(default)]
    msg: Option<String>,
    data: Option<QuotaData>,
}

#[derive(Deserialize)]
struct QuotaData {
    level: String,
    limits: Vec<QuotaLimit>,
}

#[derive(Deserialize)]
struct QuotaLimit {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    unit: Option<i64>,
    #[serde(default)]
    number: Option<i64>,
    percentage: f64,
    #[serde(rename = "nextResetTime")]
    next_reset_time: i64,
}

#[derive(Deserialize)]
struct UsageResponse {
    code: i64,
    #[serde(default)]
    msg: Option<String>,
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
        if hints_missing_coding_plan(&quota, &usage) {
            return Err(GlmParseError::NoCodingPlan);
        }
        return Err(GlmParseError::ApiRejected);
    }

    let quota_data = quota.data.ok_or(GlmParseError::SchemaMismatch)?;
    let usage_data = usage.data.ok_or(GlmParseError::SchemaMismatch)?;
    let QuotaData { level, limits } = quota_data;
    let selected_kind = if limits
        .iter()
        .any(|limit| is_limit_kind(limit, "CREDIT_LIMIT"))
    {
        "CREDIT_LIMIT"
    } else {
        "TOKENS_LIMIT"
    };
    let mut quota_limits = limits
        .into_iter()
        .filter(|limit| is_limit_kind(limit, selected_kind))
        .collect::<Vec<_>>();

    if level.trim().is_empty()
        || quota_limits.is_empty()
        || quota_limits.iter().any(|limit| {
            !limit.percentage.is_finite()
                || !(0.0..=100.0).contains(&limit.percentage)
                || limit.next_reset_time <= 0
        })
    {
        return Err(GlmParseError::SchemaMismatch);
    }

    // The dashboard summary represents the shortest quota window. Sorting by
    // reset time alone can accidentally select the weekly window when its
    // reset happens before the next rolling 5-hour reset.
    quota_limits.sort_by_key(|limit| {
        (
            quota_window(limit).1.unwrap_or(i64::MAX),
            limit.next_reset_time,
        )
    });
    let used_percent = quota_limits[0].percentage;
    let reset_at = quota_limits[0].next_reset_time;
    let entries = quota_limits
        .into_iter()
        .map(|limit| {
            let remaining_percent = (100.0 - limit.percentage).clamp(0.0, 100.0);
            let (window, duration_ms) = quota_window(&limit);
            OnlineDetailEntry {
                label: if is_limit_kind(&limit, "CREDIT_LIMIT") {
                    "额度用量".to_string()
                } else {
                    "Token 用量".to_string()
                },
                used: Some(format_glm_value(limit.percentage)),
                remaining: Some(format_glm_value(remaining_percent)),
                limit: Some("100".to_string()),
                unit: "%".to_string(),
                used_percent: Some(limit.percentage),
                window,
                start_at_ms: duration_ms
                    .and_then(|duration| limit.next_reset_time.checked_sub(duration)),
                reset_at_ms: Some(limit.next_reset_time),
                remaining_ms: None,
            }
        })
        .collect();
    let detail_sections = vec![OnlineDetailSection {
        title: "额度窗口".to_string(),
        entries,
    }];

    Ok(GlmUsageSnapshot {
        plan_level: level,
        used_percent,
        cooldown_ends_at_ms: reset_at,
        requests: usage_data.total_usage.total_model_call_count,
        total_tokens: usage_data.total_usage.total_tokens_usage,
        detail_sections,
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

        assert_eq!(snapshot.detail_sections.len(), 1);
        let entry = &snapshot.detail_sections[0].entries[0];
        assert_eq!(entry.label, "Token 用量");
        assert_eq!(entry.window.as_deref(), Some("5 小时"));
        assert_eq!(entry.unit, "%");
        assert_eq!(entry.used_percent, Some(68.5));
        assert_eq!(entry.used.as_deref(), Some("68.5"));
        assert_eq!(entry.remaining.as_deref(), Some("31.5"));
        assert_eq!(entry.reset_at_ms, Some(1_783_686_600_000));
        assert_eq!(entry.start_at_ms, Some(1_783_686_600_000 - GLM_WINDOW_MS));
    }

    #[test]
    fn parses_max_plan_credit_limits_as_five_hour_and_weekly_windows() {
        // Production shape returned by GLM Max accounts since 2026-08. The
        // provider renamed token quotas to CREDIT_LIMIT and now returns both
        // the 5-hour and weekly windows.
        let quota = r#"{
          "code": 200,
          "msg": "操作成功",
          "data": {
            "level": "max",
            "limits": [
              {
                "type": "CREDIT_LIMIT",
                "unit": 3,
                "number": 5,
                "usage": 28000,
                "currentValue": 9898,
                "remaining": 18102,
                "percentage": 35,
                "nextResetTime": 1787577676307
              },
              {
                "type": "CREDIT_LIMIT",
                "unit": 6,
                "number": 1,
                "usage": 140000,
                "currentValue": 9898,
                "remaining": 130102,
                "percentage": 7,
                "nextResetTime": 1788163653998
              }
            ]
          }
        }"#;
        let usage = r#"{
          "code": 200,
          "msg": "操作成功",
          "data": {
            "totalUsage": {
              "totalModelCallCount": 12,
              "totalTokensUsage": 34567
            }
          }
        }"#;

        let snapshot = parse_snapshot(quota, usage).expect("valid GLM Max response");

        assert_eq!(snapshot.plan_level, "max");
        assert_eq!(snapshot.used_percent, 35.0);
        assert_eq!(snapshot.cooldown_ends_at_ms, 1_787_577_676_307);
        assert_eq!(snapshot.requests, 12);
        assert_eq!(snapshot.total_tokens, 34_567);

        let entries = &snapshot.detail_sections[0].entries;
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].window.as_deref(), Some("5 小时"));
        assert_eq!(entries[0].used_percent, Some(35.0));
        assert_eq!(
            entries[0].start_at_ms,
            Some(1_787_577_676_307 - GLM_WINDOW_MS)
        );
        assert_eq!(entries[1].window.as_deref(), Some("每周"));
        assert_eq!(entries[1].used_percent, Some(7.0));
        assert_eq!(
            entries[1].start_at_ms,
            Some(1_788_163_653_998 - 7 * 24 * 60 * 60 * 1000)
        );
    }

    #[test]
    fn prefers_current_credit_limits_when_old_and_new_shapes_coexist() {
        let quota = r#"{
          "code": 200,
          "data": {
            "level": "max",
            "limits": [
              {
                "type": "TOKENS_LIMIT",
                "percentage": 91,
                "nextResetTime": 1787577000000
              },
              {
                "type": "CREDIT_LIMIT",
                "unit": 3,
                "number": 5,
                "percentage": 35,
                "nextResetTime": 1787577676307
              }
            ]
          }
        }"#;
        let usage = r#"{
          "code": 200,
          "data": {
            "totalUsage": {
              "totalModelCallCount": 1,
              "totalTokensUsage": 2
            }
          }
        }"#;

        let snapshot = parse_snapshot(quota, usage).expect("current shape wins");

        assert_eq!(snapshot.used_percent, 35.0);
        assert_eq!(snapshot.detail_sections[0].entries.len(), 1);
        assert_eq!(
            snapshot.detail_sections[0].entries[0].window.as_deref(),
            Some("5 小时")
        );
    }

    #[test]
    fn maps_missing_coding_plan_rejections_to_a_dedicated_error() {
        // Verbatim production body returned to a valid pay-as-you-go key.
        let no_plan = r#"{"code":500,"msg":"当前用户不存在coding plan","success":false}"#;
        let usage = r#"{
          "code": 200,
          "data": {
            "totalUsage":{"totalModelCallCount":0,"totalTokensUsage":0}
          }
        }"#;

        let error = parse_snapshot(no_plan, usage).expect_err("no plan means no monitor data");
        assert_eq!(error.code(), "GLM_NO_CODING_PLAN");

        // The usage endpoint answers the same body, even when quota succeeds.
        let quota_ok = r#"{
          "code": 200,
          "data": {
            "level": "PRO",
            "limits": [{"type":"TOKENS_LIMIT","percentage":10,"nextResetTime":1783686600000}]
          }
        }"#;
        let error = parse_snapshot(quota_ok, no_plan).expect_err("no plan means no monitor data");
        assert_eq!(error, GlmParseError::NoCodingPlan);
    }

    #[test]
    fn keeps_generic_rejection_for_other_error_bodies() {
        let quota = r#"{"code":1002,"msg":"Authorization Token invalid","success":false}"#;
        let usage =
            r#"{"code":200,"data":{"totalUsage":{"totalModelCallCount":0,"totalTokensUsage":0}}}"#;

        let error = parse_snapshot(quota, usage).expect_err("other rejections stay generic");

        assert_eq!(error, GlmParseError::ApiRejected);
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
