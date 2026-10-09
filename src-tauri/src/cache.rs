use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
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
    /// Legacy 15-minute slot (0..=95). Kept for history written before
    /// minute-level sampling; newer records leave this `None` and carry
    /// `minute` instead. Never both.
    #[serde(default)]
    pub slot: Option<i16>,
    /// Minutes since local midnight (0..=1439) — the sync minute itself, so a
    /// 1-minute auto-sync cadence yields one trend point per minute instead
    /// of overwriting the enclosing quarter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minute: Option<i16>,
    pub provider_id: String,
    pub requests: Option<u64>,
    pub total_tokens: Option<u64>,
    pub estimated_cost_cny: Option<f64>,
    /// Snapshot of the provider's remaining balance (¥) at sync time. A stock
    /// value, not a flow: the day's latest sample is the closing balance.
    /// Negative is legal — overdraft is a real account state.
    #[serde(default)]
    pub balance_cny: Option<f64>,
}

impl DailyUsageRecord {
    /// The record's position within its day as minutes since midnight.
    /// Legacy slots resolve to their quarter start; `None` (daily rollup)
    /// sorts before every intraday sample.
    pub fn minute_of_day(&self) -> i32 {
        match (self.minute, self.slot) {
            (Some(minute), _) => minute as i32,
            (None, Some(slot)) => slot as i32 * 15,
            (None, None) => -1,
        }
    }
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
        let mut records = self.load_or_quarantine()?;
        records.retain(|existing| {
            existing.date != record.date
                || existing.minute_of_day() != record.minute_of_day()
                || existing.provider_id != record.provider_id
        });
        records.push(record);
        self.write_records(&mut records, &today)
    }

    /// Batch merge for transfer import. Incoming records are validated and
    /// deduped among themselves (last wins, mirroring `upsert`), then folded
    /// into the existing file under the same write lock. The merged file must
    /// respect `MAX_HISTORY_BYTES` — an oversized file is quarantined by the
    /// next write, not repaired — so when the combination would not fit, the
    /// oldest incoming records are dropped first; existing records are never
    /// dropped by an import. Returns how many incoming records were merged.
    pub fn merge(&self, incoming: Vec<DailyUsageRecord>) -> Result<usize, CacheError> {
        let mut incoming: Vec<DailyUsageRecord> =
            incoming.into_iter().filter(is_valid_daily_record).collect();
        if incoming.is_empty() {
            return Ok(0);
        }
        let _guard = DAILY_USAGE_WRITE_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut records = self.load_or_quarantine()?;
        // Within-batch dedupe: same (date, minute, provider) keeps the last.
        let mut batch: Vec<DailyUsageRecord> = Vec::with_capacity(incoming.len());
        for record in incoming.drain(..) {
            batch.retain(|existing| {
                existing.date != record.date
                    || existing.minute_of_day() != record.minute_of_day()
                    || existing.provider_id != record.provider_id
            });
            batch.push(record);
        }
        batch.sort_by(|left, right| {
            left.date
                .cmp(&right.date)
                .then(left.minute_of_day().cmp(&right.minute_of_day()))
                .then(left.provider_id.cmp(&right.provider_id))
        });
        // Size budget: the ascending sort makes draining from the front drop
        // the oldest records, which are the least valuable for trends.
        while !batch.is_empty() {
            let mut candidate = records.clone();
            candidate.extend(batch.iter().cloned());
            let fits = serde_json::to_vec(&candidate)
                .map(|bytes| bytes.len() <= MAX_HISTORY_BYTES)
                .unwrap_or(false);
            if fits {
                break;
            }
            let drop = (batch.len() / 10).max(1);
            batch.drain(..drop);
        }
        let merged = batch.len();
        if merged == 0 {
            return Ok(0);
        }
        let keys: std::collections::HashSet<(String, i32, String)> = batch
            .iter()
            .map(|record| {
                (
                    record.date.clone(),
                    record.minute_of_day(),
                    record.provider_id.clone(),
                )
            })
            .collect();
        records.retain(|existing| {
            !keys.contains(&(
                existing.date.clone(),
                existing.minute_of_day(),
                existing.provider_id.clone(),
            ))
        });
        records.extend(batch);
        let today = newest_date(&records).to_string();
        self.write_records(&mut records, &today).map(|()| merged)
    }

    /// Loads the history, quarantining an unreadable file so a corrupt or
    /// oversized store self-heals instead of failing every future write. Only
    /// content failures self-heal; IO errors stay errors.
    fn load_or_quarantine(&self) -> Result<Vec<DailyUsageRecord>, CacheError> {
        match self.load() {
            Ok(records) => Ok(records),
            Err(CacheError::Json | CacheError::Invalid) => {
                quarantine_file(&self.path).map_err(|_| CacheError::Io)?;
                Ok(Vec::new())
            }
            Err(error) => Err(error),
        }
    }

    /// Rolls up, sorts, and atomically writes the full record set.
    fn write_records(
        &self,
        records: &mut Vec<DailyUsageRecord>,
        today: &str,
    ) -> Result<(), CacheError> {
        rollup_expired_records(records, today);
        records.sort_by(|left, right| {
            left.date
                .cmp(&right.date)
                .then(left.minute_of_day().cmp(&right.minute_of_day()))
                .then(left.provider_id.cmp(&right.provider_id))
        });
        let parent = self.path.parent().ok_or(CacheError::Invalid)?;
        std::fs::create_dir_all(parent).map_err(|_| CacheError::Io)?;
        let bytes = serde_json::to_vec(records).map_err(|_| CacheError::Json)?;
        atomic_write(&self.path, &bytes)
    }

    pub fn load(&self) -> Result<Vec<DailyUsageRecord>, CacheError> {
        // Crash recovery: if the atomic-write rename never landed, the .tmp
        // copy is the only surviving data — restore it when it parses.
        if !self.path.exists() {
            recover_from_tmp(&self.path, parses_history);
        }
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
        self.restore(provider_id, kind, snapshot, now_ms()?)
    }

    /// Saves a snapshot keeping its original capture time. Transfer import
    /// uses this so a restored snapshot keeps reporting the moment it was
    /// actually fetched on the source machine, not the import time.
    pub fn restore(
        &self,
        provider_id: &str,
        kind: &str,
        snapshot: Value,
        saved_at_ms: i64,
    ) -> Result<(), CacheError> {
        if !is_safe_id(provider_id) || !is_safe_id(kind) {
            return Err(CacheError::Invalid);
        }
        std::fs::create_dir_all(&self.dir).map_err(|_| CacheError::Io)?;
        let entry = CachedSnapshot {
            provider_id: provider_id.to_string(),
            kind: kind.to_string(),
            saved_at_ms,
            snapshot,
        };
        let bytes = serde_json::to_vec(&entry).map_err(|_| CacheError::Json)?;
        let path = self.dir.join(format!("{provider_id}.json"));
        atomic_write(&path, &bytes)
    }

    pub fn load_all(&self) -> Result<Vec<CachedSnapshot>, CacheError> {
        let mut snapshots = Vec::new();
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(snapshots),
            Err(_) => return Err(CacheError::Io),
        };
        let paths: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .collect();
        // Crash recovery: restore any snapshot whose atomic-write rename never
        // landed (valid .tmp next to a missing .json). Recovered targets join
        // the load list so they are visible in the same pass.
        let mut recovered: Vec<PathBuf> = Vec::new();
        for path in &paths {
            if path.extension().and_then(|value| value.to_str()) != Some("tmp") {
                continue;
            }
            let target = path.with_extension("json");
            if target.exists() {
                continue;
            }
            if let Ok(bytes) = std::fs::read(path) {
                if serde_json::from_slice::<CachedSnapshot>(&bytes).is_ok() {
                    let _ = std::fs::rename(path, &target);
                    recovered.push(target);
                }
            }
        }
        for path in paths.iter().chain(recovered.iter()) {
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            // One corrupt file (e.g. a crash-truncated write) must not blank
            // every provider's cached snapshot: quarantine it and keep reading.
            let parsed = std::fs::read(path)
                .map_err(|_| CacheError::Io)
                .and_then(|bytes| {
                    serde_json::from_slice::<CachedSnapshot>(&bytes).map_err(|_| CacheError::Json)
                });
            match parsed {
                Ok(snapshot) => snapshots.push(snapshot),
                Err(_) => {
                    let _ = quarantine_file(path);
                }
            }
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

/// Durable tmp-then-rename write shared by the history and snapshot caches.
/// The tmp copy is fsynced before the swap so a power cut cannot leave an
/// empty target; `recover_from_tmp` bounds the damage of a crash between the
/// remove and the rename (Windows `rename` cannot overwrite an existing file,
/// hence the remove).
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), CacheError> {
    let temp = path.with_extension("tmp");
    let mut file = std::fs::File::create(&temp).map_err(|_| CacheError::Io)?;
    file.write_all(bytes).map_err(|_| CacheError::Io)?;
    file.sync_all().map_err(|_| CacheError::Io)?;
    drop(file);
    if path.exists() {
        std::fs::remove_file(path).map_err(|_| CacheError::Io)?;
    }
    std::fs::rename(&temp, path).map_err(|_| CacheError::Io)?;
    sync_parent_dir(path);
    Ok(())
}

/// Persist the rename itself so a power cut right after the swap cannot lose
/// the directory entry. Directory sync is only available on Unix.
#[cfg(unix)]
fn sync_parent_dir(path: &Path) {
    if let Some(parent) = path.parent() {
        if let Ok(dir) = std::fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }
}

#[cfg(not(unix))]
fn sync_parent_dir(_path: &Path) {}

/// Restores a stranded `.tmp` over its missing target when the bytes parse.
/// No-op when the target exists or the tmp copy is absent/unusable.
fn recover_from_tmp(path: &Path, parses: impl FnOnce(&[u8]) -> bool) {
    let temp = path.with_extension("tmp");
    if path.exists() || !temp.is_file() {
        return;
    }
    if let Ok(bytes) = std::fs::read(&temp) {
        if parses(&bytes) {
            let _ = std::fs::rename(&temp, path);
        }
    }
}

fn parses_history(bytes: &[u8]) -> bool {
    serde_json::from_slice::<Vec<DailyUsageRecord>>(bytes)
        .map(|records| records.iter().all(is_valid_daily_record))
        .is_ok()
}

/// Renames an unusable JSON file aside (`.corrupt`) for forensics, replacing
/// any previous quarantine copy. Best effort — callers treat failure as IO.
fn quarantine_file(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let corrupt = path.with_extension("corrupt");
    if corrupt.exists() {
        std::fs::remove_file(&corrupt)?;
    }
    std::fs::rename(path, &corrupt)
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
            .minute
            .is_none_or(|minute| (0..=1439).contains(&minute))
        && (record.minute.is_none() || record.slot.is_none())
        && record
            .estimated_cost_cny
            .is_none_or(|value| value.is_finite() && value >= 0.0)
        && record.balance_cny.is_none_or(|value| value.is_finite())
}

/// Collapse intraday detail older than 30 days into one daily rollup
/// (`slot = None`, `minute = None`) per `(date, provider)`, keeping the day's
/// latest sample. Usage figures are same-day cumulative snapshots, so the
/// latest sample is the correct daily representative — never sum samples,
/// which would inflate a day's tokens by the sample count. Bounds storage
/// while keeping long-range daily trends. `today` is a `YYYY-MM-DD`
/// reference taken from the new record's date.
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
                if record.minute_of_day() > existing.minute_of_day() {
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
        record.minute = None;
    }
    records.extend(fresh);
    records.extend(expired.into_values());
}

/// The newest date present in the record set, as the rollup cutoff
/// reference for merges (the newest record drives the cutoff, mirroring how
/// `upsert` uses the incoming record's date).
fn newest_date(records: &[DailyUsageRecord]) -> &str {
    records
        .iter()
        .map(|record| record.date.as_str())
        .max()
        .unwrap_or_default()
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
    fn restore_keeps_the_original_capture_time() {
        let dir = test_dir();
        let _ = std::fs::remove_dir_all(&dir);
        let cache = SnapshotCache::new(&dir);

        cache
            .restore(
                "kimi_cn",
                "online",
                serde_json::json!({"providerId": "kimi_cn", "primaryValue": "¥10.00"}),
                1_787_194_023_645,
            )
            .expect("restore snapshot");

        let snapshots = cache.load_all().expect("load cache");
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].saved_at_ms, 1_787_194_023_645);
        assert_eq!(snapshots[0].snapshot["primaryValue"], "¥10.00");
    }

    #[test]
    fn merge_folds_incoming_records_without_losing_existing_ones() {
        let dir = test_dir();
        let _ = std::fs::remove_dir_all(&dir);
        let history = DailyUsageHistory::new(&dir);
        history
            .upsert(DailyUsageRecord {
                date: "2026-10-08".into(),
                slot: None,
                minute: Some(727),
                provider_id: "glm".into(),
                requests: Some(1),
                total_tokens: Some(100),
                estimated_cost_cny: None,
                balance_cny: None,
            })
            .expect("seed existing record");

        let merged = history
            .merge(vec![
                DailyUsageRecord {
                    date: "2026-10-07".into(),
                    slot: None,
                    minute: Some(10),
                    provider_id: "kimi_cn".into(),
                    requests: Some(2),
                    total_tokens: Some(200),
                    estimated_cost_cny: None,
                    balance_cny: Some(52.5),
                },
                // Semantically invalid records are dropped, not fatal.
                DailyUsageRecord {
                    date: "2026-13-40".into(),
                    slot: None,
                    minute: None,
                    provider_id: "kimi_cn".into(),
                    requests: None,
                    total_tokens: None,
                    estimated_cost_cny: None,
                    balance_cny: None,
                },
            ])
            .expect("merge history");

        let records = history.load().expect("load history");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(merged, 1);
        assert_eq!(records.len(), 2);
        assert!(records.iter().any(|record| record.provider_id == "glm"));
        assert!(records
            .iter()
            .any(|record| record.provider_id == "kimi_cn" && record.total_tokens == Some(200)));
    }

    #[test]
    fn merge_dedupes_within_the_batch_and_replaces_existing_keys() {
        let dir = test_dir();
        let _ = std::fs::remove_dir_all(&dir);
        let history = DailyUsageHistory::new(&dir);
        let record = |tokens: u64| DailyUsageRecord {
            date: "2026-10-08".into(),
            slot: None,
            minute: Some(727),
            provider_id: "glm".into(),
            requests: Some(1),
            total_tokens: Some(tokens),
            estimated_cost_cny: None,
            balance_cny: None,
        };
        history.upsert(record(100)).expect("seed existing");

        let merged = history
            .merge(vec![record(200), record(300)])
            .expect("merge history");

        let records = history.load().expect("load history");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(merged, 1);
        assert_eq!(records.len(), 1);
        // Within the batch the last record wins, and it replaces the existing
        // sample for the same (date, minute, provider) key — same as upsert.
        assert_eq!(records[0].total_tokens, Some(300));
    }

    #[test]
    fn merge_returns_zero_for_an_empty_or_fully_invalid_batch() {
        let dir = test_dir();
        let _ = std::fs::remove_dir_all(&dir);
        let history = DailyUsageHistory::new(&dir);

        assert_eq!(history.merge(Vec::new()).expect("empty merge"), 0);

        let invalid = vec![DailyUsageRecord {
            date: "not-a-date".into(),
            slot: None,
            minute: None,
            provider_id: "glm".into(),
            requests: None,
            total_tokens: None,
            estimated_cost_cny: None,
            balance_cny: None,
        }];
        assert_eq!(history.merge(invalid).expect("invalid merge"), 0);
        // Nothing was written, so no history directory materializes.
        assert!(!dir.join("history").join("daily-usage.json").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn minute_samples_dedupe_per_minute_and_bridge_legacy_slots() {
        let dir = std::env::temp_dir().join(format!(
            "llm-usage-minute-history-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let history = DailyUsageHistory::new(&dir);
        let record = |minute: Option<i16>, tokens: u64| DailyUsageRecord {
            date: "2026-10-08".into(),
            slot: None,
            minute,
            provider_id: "glm".into(),
            requests: None,
            total_tokens: Some(tokens),
            estimated_cost_cny: None,
            balance_cny: None,
        };

        // Two syncs inside the same minute replace each other…
        history.upsert(record(Some(727), 100)).expect("first minute sample");
        history.upsert(record(Some(727), 150)).expect("same-minute replacement");
        // …adjacent minutes both survive, so a 1-minute cadence records one
        // point per minute instead of collapsing into a quarter.
        history.upsert(record(Some(728), 200)).expect("next minute sample");
        // A legacy 15-minute record lands on its quarter start (720) and is a
        // distinct point from minute 727.
        history.upsert(DailyUsageRecord {
            date: "2026-10-08".into(),
            slot: Some(48),
            minute: None,
            provider_id: "glm".into(),
            requests: None,
            total_tokens: Some(80),
            estimated_cost_cny: None,
            balance_cny: None,
        })
        .expect("legacy slot sample");

        let records = history.load().expect("load history");
        let minutes: Vec<(i32, u64)> = records
            .iter()
            .map(|record| (record.minute_of_day(), record.total_tokens.unwrap_or_default()))
            .collect();
        assert_eq!(minutes, vec![(720, 80), (727, 150), (728, 200)]);
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
                minute: None,
                provider_id: "glm".into(),
                requests: Some(2),
                total_tokens: Some(200),
                estimated_cost_cny: None,
                balance_cny: None,
            })
            .expect("save first observation");
        history
            .upsert(DailyUsageRecord {
                date: "2026-07-13".into(),
                slot: None,
                minute: None,
                provider_id: "glm".into(),
                requests: Some(3),
                total_tokens: Some(350),
                estimated_cost_cny: None,
                balance_cny: None,
            })
            .expect("replace same day observation");
        history
            .upsert(DailyUsageRecord {
                date: "2026-07-13".into(),
                slot: None,
                minute: None,
                provider_id: "openai_codex".into(),
                requests: Some(4),
                total_tokens: Some(700),
                estimated_cost_cny: Some(1.2),
                balance_cny: None,
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
                            minute: None,
                            provider_id: id,
                            requests: Some(1),
                            total_tokens: Some(100),
                            estimated_cost_cny: None,
                            balance_cny: None,
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
            minute: None,
            provider_id: "glm".into(),
            requests: Some(1),
            total_tokens: Some(10),
            estimated_cost_cny: None,
            balance_cny: None,
        });
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(result, Err(CacheError::Invalid));
    }

    #[test]
    fn records_balance_samples_and_rejects_non_finite_values() {
        let dir = test_dir();
        let _ = std::fs::remove_dir_all(&dir);
        let history = DailyUsageHistory::new(&dir);

        history
            .upsert(DailyUsageRecord {
                date: "2026-07-19".into(),
                slot: Some(48),
                minute: None,
                provider_id: "kimi_cn".into(),
                requests: None,
                total_tokens: None,
                estimated_cost_cny: None,
                balance_cny: Some(52.5),
            })
            .expect("balance sample accepted");

        // Balances are stocks: overdrafts are a real account state, so only
        // non-finite values are rejected.
        history
            .upsert(DailyUsageRecord {
                date: "2026-07-19".into(),
                slot: Some(49),
                minute: None,
                provider_id: "kimi_cn".into(),
                requests: None,
                total_tokens: None,
                estimated_cost_cny: None,
                balance_cny: Some(-1.0),
            })
            .expect("negative balance accepted");

        let result = history.upsert(DailyUsageRecord {
            date: "2026-07-19".into(),
            slot: Some(50),
            minute: None,
            provider_id: "kimi_cn".into(),
            requests: None,
            total_tokens: None,
            estimated_cost_cny: None,
            balance_cny: Some(f64::NAN),
        });
        assert_eq!(result, Err(CacheError::Invalid));

        let records = history.load().expect("load");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].balance_cny, Some(52.5));
        assert_eq!(records[1].balance_cny, Some(-1.0));
    }

    #[test]
    fn quarantines_corrupt_history_and_restarts_from_the_new_record() {
        let dir = test_dir();
        let _ = std::fs::remove_dir_all(&dir);
        let history = DailyUsageHistory::new(&dir);
        let history_dir = dir.join("history");
        std::fs::create_dir_all(&history_dir).expect("create history dir");
        std::fs::write(
            history_dir.join("daily-usage.json"),
            r#"[{"date":"2026-07-01","slot":nonsense]"#,
        )
        .expect("seed corrupt file");

        // The corrupt file must not fail the write forever: it is quarantined
        // and history restarts from the incoming record.
        history
            .upsert(DailyUsageRecord {
                date: "2026-07-19".into(),
                slot: None,
                minute: None,
                provider_id: "glm".into(),
                requests: Some(1),
                total_tokens: Some(100),
                estimated_cost_cny: None,
                balance_cny: None,
            })
            .expect("upsert self-heals over a corrupt file");

        let records = history.load().expect("load");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].provider_id, "glm");
        assert!(history_dir.join("daily-usage.corrupt").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recovers_history_from_a_stranded_tmp_file() {
        let dir = test_dir();
        let _ = std::fs::remove_dir_all(&dir);
        let history = DailyUsageHistory::new(&dir);
        let history_dir = dir.join("history");
        std::fs::create_dir_all(&history_dir).expect("create history dir");
        // Simulate a crash between remove and rename: only the tmp survives.
        std::fs::write(
            history_dir.join("daily-usage.tmp"),
            r#"[{"date":"2026-07-01","slot":null,"providerId":"glm","requests":3,"totalTokens":300,"estimatedCostCny":null}]"#,
        )
        .expect("seed stranded tmp");

        let records = history.load().expect("load recovers the tmp copy");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].total_tokens, Some(300));
        assert!(history_dir.join("daily-usage.json").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_all_skips_and_quarantines_a_corrupt_snapshot_file() {
        let dir = test_dir();
        let _ = std::fs::remove_dir_all(&dir);
        let cache = SnapshotCache::new(&dir);
        cache
            .save("kimi_cn", "online", serde_json::json!({"a": 1}))
            .expect("save first");
        cache
            .save("minimax_cn", "online", serde_json::json!({"b": 2}))
            .expect("save second");
        std::fs::write(
            dir.join("cache").join("kimi_cn.json"),
            r#"{half-written"#,
        )
        .expect("corrupt one cache file");

        // The healthy snapshot stays visible; the corrupt one is quarantined.
        let snapshots = cache.load_all().expect("load_all tolerates bad files");
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].provider_id, "minimax_cn");
        assert!(dir.join("cache").join("kimi_cn.corrupt").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_all_recovers_a_stranded_tmp_snapshot() {
        let dir = test_dir();
        let _ = std::fs::remove_dir_all(&dir);
        let cache = SnapshotCache::new(&dir);
        std::fs::create_dir_all(dir.join("cache")).expect("create cache dir");
        std::fs::write(
            dir.join("cache").join("kimi_cn.tmp"),
            r#"{"providerId":"kimi_cn","kind":"online","savedAtMs":1,"snapshot":{}}"#,
        )
        .expect("seed stranded tmp");

        let snapshots = cache.load_all().expect("load_all recovers tmp");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].provider_id, "kimi_cn");
    }

    #[test]
    fn loads_legacy_history_without_balance_field() {
        let dir = test_dir();
        // A pre-balance history file: every record lacks `balanceCny`, which
        // must deserialize as None rather than failing the whole load.
        let parent = dir.join("history");
        std::fs::create_dir_all(&parent).expect("create history dir");
        std::fs::write(
            parent.join("daily-usage.json"),
            r#"[{"date":"2026-07-01","slot":null,"providerId":"glm","requests":3,"totalTokens":300,"estimatedCostCny":null}]"#,
        )
        .expect("seed legacy file");

        let records = DailyUsageHistory::new(&dir).load().expect("legacy load");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].balance_cny, None);
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
            minute: None,
            provider_id: "glm".into(),
            requests: Some(1),
            total_tokens: Some(tokens),
            estimated_cost_cny: None,
            balance_cny: None,
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
            minute: None,
            provider_id: "glm".into(),
            requests: Some(1),
            total_tokens: Some(tokens),
            estimated_cost_cny: None,
            balance_cny: None,
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
                minute: None,
                provider_id: "glm".into(),
                requests: Some(2),
                total_tokens: Some(500),
                estimated_cost_cny: None,
                balance_cny: None,
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
