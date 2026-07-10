use serde::Deserialize;
use std::fmt;

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
}

impl GlmParseError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidJson => "GLM_INVALID_JSON",
            Self::ApiRejected => "GLM_API_REJECTED",
            Self::SchemaMismatch => "GLM_SCHEMA_MISMATCH",
        }
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
}
