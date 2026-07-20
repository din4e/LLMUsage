use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Upper bound for the history file. 15-minute granularity multiplies record
/// count by up to ~96×, so the legacy 1 MiB ceiling is too tight once intraday
/// samples accumulate; `rollup_expired_records` keeps growth bounded in time.
const MAX_HISTORY_BYTES: usize = 8 * 1_048_576;

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
    #[serde(default)]
    pub slot: Option<i16>,
    pub provider_id: String,
    pub requests: Option<u64>,
    pub total_tokens: Option<u64>,
    pub estimated_cost_cny: Option<f64>,
}

pub struct DailyUsageHistory {
    path: PathBuf,
}

// Every provider writes to the same daily-usage.json via read-modify-write.
// Parallel syncs (sync_glm + each sync_online_provider) call upsert
// concurrently, so the whole load→modify→write must be serialized to avoid
// lost updates, collisions on the shared .tmp path, or a corrupted file that
// would fail every provider's recording step.
static DAILY_USAGE_WRITE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

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
        let _guard = DAILY_USAGE_WRITE_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let today = record.date.clone();
        let mut records = self.load()?;
        records.retain(|existing| {
            existing.date != record.date
                || existing.slot != record.slot
                || existing.provider_id != record.provider_id
        });
        records.push(record);
        rollup_expired_records(&mut records, &today);
        records.sort_by(|left, right| {
            left.date
                .cmp(&right.date)
                .then(slot_rank(left).cmp(&slot_rank(right)))
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
        if bytes.len() > MAX_HISTORY_BYTES {
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

    /// Remove a provider's cached snapshot. Safe to call when nothing is cached
    /// for the provider; rejects ids that could escape the cache directory.
    pub fn delete(&self, provider_id: &str) -> Result<(), CacheError> {
        if !is_safe_id(provider_id) {
            return Err(CacheError::Invalid);
        }
        let path = self.dir.join(format!("{provider_id}.json"));
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(CacheError::Io),
        }
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
            .slot
            .is_none_or(|slot| (0..=95).contains(&slot))
        && record
            .estimated_cost_cny
            .is_none_or(|value| value.is_finite() && value >= 0.0)
}

/// Sort rank for the 15-minute slot within a day. `None` (daily rollup or a
/// legacy pre-slot record) sorts before every real slot.
fn slot_rank(record: &DailyUsageRecord) -> i16 {
    record.slot.unwrap_or(-1)
}

/// Collapse 15-minute detail older than 30 days into one daily rollup
/// (`slot = None`) per `(date, provider)`, keeping the day's latest sample.
/// Usage figures are same-day cumulative snapshots, so the latest slot is the
/// correct daily representative — never sum slots, which would inflate a day's
/// tokens by the sample count. Bounds storage while keeping long-range daily
/// trends. `today` is a `YYYY-MM-DD` reference taken from the new record's date.
fn rollup_expired_records(records: &mut Vec<DailyUsageRecord>, today: &str) {
    let Ok(today_date) = NaiveDate::parse_from_str(today, "%Y-%m-%d") else {
        return;
    };
    let cutoff = today_date - chrono::Duration::days(30);
    let mut fresh = Vec::new();
    let mut expired: std::collections::BTreeMap<(String, String), DailyUsageRecord> =
        std::collections::BTreeMap::new();
    for record in records.drain(..) {
        let keep_detail = match NaiveDate::parse_from_str(&record.date, "%Y-%m-%d") {
            Ok(date) => date >= cutoff,
            Err(_) => true,
        };
        if keep_detail {
            fresh.push(record);
            continue;
        }
        let key = (record.date.clone(), record.provider_id.clone());
        match expired.entry(key) {
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let existing = entry.get_mut();
                if slot_rank(&record) > slot_rank(existing) {
                    *existing = record;
                }
            }
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(record);
            }
        }
    }
    for record in expired.values_mut() {
        record.slot = None;
    }
    records.extend(fresh);
    records.extend(expired.into_values());
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
        // Tests run in parallel threads inside one process, so a path keyed
        // only on the pid would collide between tests and corrupt each other's
        // assertions. Hand out a fresh directory per call instead.
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "llm-usage-cache-test-{}-{seq}",
            std::process::id()
        ))
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
    fn delete_removes_a_provider_snapshot() {
        let dir = test_dir();
        let _ = std::fs::remove_dir_all(&dir);
        let cache = SnapshotCache::new(&dir);

        cache
            .save(
                "minimax_cn",
                "online",
                serde_json::json!({"primaryValue": "10%"}),
            )
            .expect("save cache");
        assert_eq!(cache.load_all().expect("load").len(), 1);

        cache.delete("minimax_cn").expect("delete snapshot");

        assert!(cache
            .load_all()
            .expect("load after delete")
            .is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_is_idempotent_when_no_snapshot_exists() {
        let dir = test_dir();
        let _ = std::fs::remove_dir_all(&dir);
        let cache = SnapshotCache::new(&dir);

        // Nothing was ever cached for this provider; forgetting it still works.
        cache.delete("kimi_cn").expect("delete is idempotent");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_rejects_provider_ids_that_could_escape_the_cache_dir() {
        let cache = SnapshotCache::new(Path::new("C:/app"));

        assert!(matches!(cache.delete("../glm"), Err(CacheError::Invalid)));
        assert!(matches!(cache.delete("GLM"), Err(CacheError::Invalid)));
        assert!(matches!(cache.delete(""), Err(CacheError::Invalid)));
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
                slot: None,
                provider_id: "glm".into(),
                requests: Some(2),
                total_tokens: Some(200),
                estimated_cost_cny: None,
            })
            .expect("save first observation");
        history
            .upsert(DailyUsageRecord {
                date: "2026-07-13".into(),
                slot: None,
                provider_id: "glm".into(),
                requests: Some(3),
                total_tokens: Some(350),
                estimated_cost_cny: None,
            })
            .expect("replace same day observation");
        history
            .upsert(DailyUsageRecord {
                date: "2026-07-13".into(),
                slot: None,
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
    fn serializes_concurrent_upserts_from_parallel_providers() {
        let dir = std::env::temp_dir().join(format!(
            "llm-usage-daily-concurrency-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let history = std::sync::Arc::new(DailyUsageHistory::new(&dir));

        let provider_ids: Vec<String> = (0..8).map(|index| format!("provider_{index}")).collect();
        let handles: Vec<_> = provider_ids
            .iter()
            .cloned()
            .map(|id| {
                let history = history.clone();
                std::thread::spawn(move || {
                    history
                        .upsert(DailyUsageRecord {
                            date: "2026-07-19".into(),
                            slot: None,
                            provider_id: id,
                            requests: Some(1),
                            total_tokens: Some(100),
                            estimated_cost_cny: None,
                        })
                        .expect("upsert succeeds under the write lock")
                })
            })
            .collect();
        for handle in handles {
            handle.join().expect("worker thread completes");
        }

        let records = history.load().expect("load history");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(records.len(), provider_ids.len());
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
            slot: None,
            provider_id: "glm".into(),
            requests: Some(1),
            total_tokens: Some(10),
            estimated_cost_cny: None,
        });
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(result, Err(CacheError::Invalid));
    }

    #[test]
    fn dedups_same_15_minute_slot_and_keeps_distinct_slots() {
        let dir = std::env::temp_dir().join(format!(
            "llm-usage-slot-dedup-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let history = DailyUsageHistory::new(&dir);

        let detail = |slot, tokens| DailyUsageRecord {
            date: "2026-07-19".into(),
            slot: Some(slot),
            provider_id: "glm".into(),
            requests: Some(1),
            total_tokens: Some(tokens),
            estimated_cost_cny: None,
        };
        history.upsert(detail(48, 100)).expect("slot 48 first");
        history.upsert(detail(48, 150)).expect("slot 48 overwrite");
        history.upsert(detail(49, 200)).expect("slot 49 distinct");

        let records = history.load().expect("load");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].slot, Some(48));
        assert_eq!(records[0].total_tokens, Some(150));
        assert_eq!(records[1].slot, Some(49));
        assert_eq!(records[1].total_tokens, Some(200));
    }

    #[test]
    fn rolls_up_expired_15_minute_detail_to_latest_daily_sample() {
        let dir = std::env::temp_dir().join(format!(
            "llm-usage-rollup-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let history = DailyUsageHistory::new(&dir);

        let detail = |slot, tokens| DailyUsageRecord {
            date: "2026-05-01".into(),
            slot: Some(slot),
            provider_id: "glm".into(),
            requests: Some(1),
            total_tokens: Some(tokens),
            estimated_cost_cny: None,
        };
        history.upsert(detail(0, 10)).expect("detail slot 0");
        history.upsert(detail(48, 50)).expect("detail slot 48");
        history.upsert(detail(95, 200)).expect("detail slot 95");
        // A newer record drives the rollup cutoff: today = 2026-07-19, so the
        // 30-day cutoff is 2026-06-19 and the 2026-05-01 detail collapses into a
        // single daily sample holding the latest slot's value.
        history
            .upsert(DailyUsageRecord {
                date: "2026-07-19".into(),
                slot: None,
                provider_id: "glm".into(),
                requests: Some(2),
                total_tokens: Some(500),
                estimated_cost_cny: None,
            })
            .expect("today record");

        let records = history.load().expect("load");
        let _ = std::fs::remove_dir_all(&dir);
        let expired: Vec<_> = records.iter().filter(|r| r.date == "2026-05-01").collect();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].slot, None);
        assert_eq!(expired[0].total_tokens, Some(200));
        assert_eq!(
            records.iter().filter(|r| r.date == "2026-07-19").count(),
            1
        );
    }

    #[test]
    fn loads_legacy_records_without_slot_field() {
        let dir = std::env::temp_dir().join(format!(
            "llm-usage-legacy-load-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let history = DailyUsageHistory::new(&dir);
        std::fs::create_dir_all(dir.join("history")).expect("create history dir");
        std::fs::write(
            dir.join("history").join("daily-usage.json"),
            r#"[{"date":"2026-07-01","providerId":"glm","requests":1,"totalTokens":100,"estimatedCostCny":null}]"#,
        )
        .expect("write legacy file");

        let records = history.load().expect("legacy records load");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].slot, None);
        assert_eq!(records[0].provider_id, "glm");
        assert_eq!(records[0].total_tokens, Some(100));
    }
}
