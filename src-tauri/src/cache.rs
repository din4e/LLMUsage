use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, PartialEq)]
pub enum CacheError {
    Invalid,
    Io,
    Json,
    Time,
}

#[derive(Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedSnapshot {
    pub provider_id: String,
    pub kind: String,
    pub saved_at_ms: i64,
    pub snapshot: Value,
}

pub struct SnapshotCache {
    dir: PathBuf,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyUsageRecord {
    pub date: String,
    pub provider_id: String,
    pub requests: Option<u64>,
    pub total_tokens: Option<u64>,
    pub estimated_cost_cny: Option<f64>,
}

pub struct DailyUsageHistory {
    path: PathBuf,
}

impl DailyUsageHistory {
    pub fn new(app_data_dir: &Path) -> Self {
        Self {
            path: app_data_dir.join("history").join("daily-usage.json"),
        }
    }

    pub fn upsert(&self, record: DailyUsageRecord) -> Result<(), CacheError> {
        if !is_valid_daily_record(&record) {
            return Err(CacheError::Invalid);
        }
        let mut records = self.load()?;
        records.retain(|existing| {
            existing.date != record.date || existing.provider_id != record.provider_id
        });
        records.push(record);
        records.sort_by(|left, right| {
            left.date
                .cmp(&right.date)
                .then(left.provider_id.cmp(&right.provider_id))
        });
        let parent = self.path.parent().ok_or(CacheError::Invalid)?;
        std::fs::create_dir_all(parent).map_err(|_| CacheError::Io)?;
        let bytes = serde_json::to_vec(&records).map_err(|_| CacheError::Json)?;
        let temp = self.path.with_extension("tmp");
        std::fs::write(&temp, bytes).map_err(|_| CacheError::Io)?;
        if self.path.exists() {
            std::fs::remove_file(&self.path).map_err(|_| CacheError::Io)?;
        }
        std::fs::rename(temp, &self.path).map_err(|_| CacheError::Io)
    }

    pub fn load(&self) -> Result<Vec<DailyUsageRecord>, CacheError> {
        let bytes = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(_) => return Err(CacheError::Io),
        };
        if bytes.len() > 1_048_576 {
            return Err(CacheError::Invalid);
        }
        let records: Vec<DailyUsageRecord> =
            serde_json::from_slice(&bytes).map_err(|_| CacheError::Json)?;
        if records.iter().all(is_valid_daily_record) {
            Ok(records)
        } else {
            Err(CacheError::Invalid)
        }
    }
}

impl SnapshotCache {
    pub fn new(app_data_dir: &Path) -> Self {
        Self {
            dir: app_data_dir.join("cache"),
        }
    }

    pub fn save(&self, provider_id: &str, kind: &str, snapshot: Value) -> Result<(), CacheError> {
        if !is_safe_id(provider_id) || !is_safe_id(kind) {
            return Err(CacheError::Invalid);
        }
        std::fs::create_dir_all(&self.dir).map_err(|_| CacheError::Io)?;
        let entry = CachedSnapshot {
            provider_id: provider_id.to_string(),
            kind: kind.to_string(),
            saved_at_ms: now_ms()?,
            snapshot,
        };
        let bytes = serde_json::to_vec(&entry).map_err(|_| CacheError::Json)?;
        let path = self.dir.join(format!("{provider_id}.json"));
        let temp = self.dir.join(format!("{provider_id}.tmp"));
        std::fs::write(&temp, bytes).map_err(|_| CacheError::Io)?;
        if path.exists() {
            std::fs::remove_file(&path).map_err(|_| CacheError::Io)?;
        }
        std::fs::rename(temp, path).map_err(|_| CacheError::Io)
    }

    pub fn load_all(&self) -> Result<Vec<CachedSnapshot>, CacheError> {
        let mut snapshots = Vec::new();
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(snapshots),
            Err(_) => return Err(CacheError::Io),
        };
        for entry in entries {
            let entry = entry.map_err(|_| CacheError::Io)?;
            if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let bytes = std::fs::read(entry.path()).map_err(|_| CacheError::Io)?;
            snapshots.push(serde_json::from_slice(&bytes).map_err(|_| CacheError::Json)?);
        }
        snapshots.sort_by(|left, right| left.provider_id.cmp(&right.provider_id));
        Ok(snapshots)
    }
}

fn is_safe_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn is_valid_date(value: &str) -> bool {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok()
}

fn is_valid_daily_record(record: &DailyUsageRecord) -> bool {
    is_valid_date(&record.date)
        && is_safe_id(&record.provider_id)
        && record
            .estimated_cost_cny
            .is_none_or(|value| value.is_finite() && value >= 0.0)
}

fn now_ms() -> Result<i64, CacheError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| CacheError::Time)?;
    i64::try_from(duration.as_millis()).map_err(|_| CacheError::Time)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir() -> PathBuf {
        std::env::temp_dir().join(format!("llm-usage-cache-test-{}", std::process::id()))
    }

    #[test]
    fn saves_and_loads_provider_snapshots_without_secrets() {
        let dir = test_dir();
        let _ = std::fs::remove_dir_all(&dir);
        let cache = SnapshotCache::new(&dir);

        cache
            .save(
                "kimi_cn",
                "online",
                serde_json::json!({"providerId": "kimi_cn", "primaryValue": "¥10.00"}),
            )
            .expect("save cache");

        let snapshots = cache.load_all().expect("load cache");
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].provider_id, "kimi_cn");
        assert_eq!(snapshots[0].kind, "online");
        assert_eq!(snapshots[0].snapshot["primaryValue"], "¥10.00");
    }

    #[test]
    fn rejects_cache_ids_that_could_escape_the_cache_dir() {
        let cache = SnapshotCache::new(Path::new("C:/app"));

        assert!(matches!(
            cache.save("../glm", "online", serde_json::json!({})),
            Err(CacheError::Invalid)
        ));
        assert!(matches!(
            cache.save("glm", "../online", serde_json::json!({})),
            Err(CacheError::Invalid)
        ));
    }

    #[test]
    fn upserts_daily_usage_by_date_and_provider() {
        let dir = std::env::temp_dir().join(format!(
            "llm-usage-daily-history-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let history = DailyUsageHistory::new(&dir);

        history
            .upsert(DailyUsageRecord {
                date: "2026-07-13".into(),
                provider_id: "glm".into(),
                requests: Some(2),
                total_tokens: Some(200),
                estimated_cost_cny: None,
            })
            .expect("save first observation");
        history
            .upsert(DailyUsageRecord {
                date: "2026-07-13".into(),
                provider_id: "glm".into(),
                requests: Some(3),
                total_tokens: Some(350),
                estimated_cost_cny: None,
            })
            .expect("replace same day observation");
        history
            .upsert(DailyUsageRecord {
                date: "2026-07-13".into(),
                provider_id: "openai_codex".into(),
                requests: Some(4),
                total_tokens: Some(700),
                estimated_cost_cny: Some(1.2),
            })
            .expect("save another provider");

        let records = history.load().expect("load history");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].provider_id, "glm");
        assert_eq!(records[0].total_tokens, Some(350));
        assert_eq!(records[1].provider_id, "openai_codex");
    }

    #[test]
    fn rejects_invalid_calendar_dates_in_daily_history() {
        let dir = std::env::temp_dir().join(format!(
            "llm-usage-invalid-history-test-{}",
            std::process::id()
        ));
        let history = DailyUsageHistory::new(&dir);
        let result = history.upsert(DailyUsageRecord {
            date: "2026-13-40".into(),
            provider_id: "glm".into(),
            requests: Some(1),
            total_tokens: Some(10),
            estimated_cost_cny: None,
        });
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(result, Err(CacheError::Invalid));
    }
}
