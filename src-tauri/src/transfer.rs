//! Batch import/export of provider credentials with remaining-status info.
//!
//! Full backups are a complete migration unit: every decrypted credential,
//! each instance's verbatim cached snapshot, and the whole daily usage
//! history travel together, so an import restores the exact dashboard and
//! trend data the source machine showed. Status reports stay the lean,
//! shareable shape (normalized status blocks, no credentials, no history).
//!
//! All logic here is pure with respect to Tauri: it operates on plain data
//! and `&Path`s so the payload assembly, parsing, id allocation, and import
//! application can be unit-tested without an app handle. Credentials are
//! never logged and never written anywhere except the secret vault during
//! import and the single user-chosen export file.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cache::{CachedSnapshot, DailyUsageHistory, DailyUsageRecord, SnapshotCache};
use crate::providers::glm::GlmClient;
use crate::providers::online::{split_instance_suffix, OnlineClient, OnlineProvider};
use crate::secret::{registry_ids, SecretVault};

pub const TRANSFER_FORMAT_VERSION: u32 = 1;
/// Full backups embed the whole daily usage history, which `MAX_HISTORY_BYTES`
/// caps at 8 MiB compact on the source machine; the transfer cap leaves room
/// for credentials and snapshots on top.
const MAX_TRANSFER_FILE_BYTES: usize = 10 * 1024 * 1024;
const MAX_TRANSFER_INSTANCES: usize = 200;
/// Mirrors the frontend `INSTANCE_REMARK_MAX_LENGTH` (src/providers.ts).
const REMARK_MAX_CHARS: usize = 24;
/// Vault ids are capped at 32 chars of `[a-z0-9_]` (secret.rs). Base ids are
/// at most `siliconflow_global` (17 chars), so `_` plus even a 10-digit
/// u32-derived index stays inside the budget; the length check is the guard.
const MAX_INSTANCE_ID_LEN: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransferMode {
    Full,
    Status,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferPayload {
    pub version: u32,
    pub mode: TransferMode,
    pub exported_at_ms: i64,
    pub instances: Vec<TransferInstance>,
    /// Daily usage history for the exported instances, keyed by their source
    /// instance ids. Full backups only; import remaps the ids onto the
    /// freshly assigned local ones. Absent (empty) in older files.
    #[serde(default)]
    pub history: Vec<DailyUsageRecord>,
}

/// One configured instance inside a transfer file. `credential` is the exact
/// serialized vault string (bare key, or camelCase JSON for multi-field
/// providers) so import can persist it verbatim.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferInstance {
    pub provider_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remark: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
    #[serde(default)]
    pub status: Option<TransferStatus>,
    /// Verbatim cached snapshot from the source machine (full backups only).
    /// Import restores it into the local snapshot cache so the dashboard
    /// shows the last-known state immediately instead of "等待同步".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<TransferSnapshot>,
}

/// The raw cached snapshot that travels inside a full backup: the exact
/// `CachedSnapshot` payload plus its capture time, so a restored row renders
/// — and reports its freshness — exactly as it did on the source machine.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferSnapshot {
    pub kind: String,
    pub saved_at_ms: i64,
    pub snapshot: Value,
}

/// Remaining-status snapshot lifted out of the cached provider snapshot.
/// A superset over GLM and online shapes; unknown shapes degrade to None
/// fields instead of failing the export.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferStatus {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balance_cny: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quota_used_percent: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooldown_ends_at_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requests: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_cost_cny: Option<f64>,
    pub saved_at_ms: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TransferFileError {
    TooLarge,
    InvalidJson,
    UnsupportedVersion,
    Malformed,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSummary {
    pub instance_count: usize,
    pub history_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportEntryResult {
    pub source_provider_id: String,
    pub assigned_instance_id: Option<String>,
    pub remark: Option<String>,
    pub outcome: &'static str,
    pub reason: Option<&'static str>,
}

/// Result of applying a whole transfer file: per-entry outcomes plus how many
/// history records were merged into the local daily usage file.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOutcome {
    pub entries: Vec<ImportEntryResult>,
    pub history_merged: usize,
}

/// Maps a credential file stem to its base provider id and instance index.
/// Unknown or unsafe stems are ignored when listing configured instances.
pub fn credential_instance(value: &str) -> Option<(String, u32)> {
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

/// Lists configured instance ids (GLM and online), sorted by base id then
/// instance index. Windows enumerates DPAPI credential files; non-Windows
/// credentials live in the system keyring, which cannot be enumerated by
/// service, so the instance registry maintained by `SecretVault` joins the
/// DPAPI scan (absent on Windows — the merge is a no-op there).
pub fn enumerate_instances(app_data: &Path) -> Vec<String> {
    let mut instances: Vec<(String, u32, String)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut push_instance = |id: &str| {
        if !seen.insert(id.to_string()) {
            return;
        }
        if let Some((base, index)) = credential_instance(id) {
            instances.push((base, index, id.to_string()));
        }
    };
    if let Ok(entries) = std::fs::read_dir(app_data.join("credentials")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("dpapi") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            push_instance(stem);
        }
    }
    for id in registry_ids(app_data) {
        push_instance(&id);
    }
    instances.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then(left.1.cmp(&right.1))
            .then(left.2.cmp(&right.2))
    });
    instances.into_iter().map(|(_, _, id)| id).collect()
}

/// Normalizes a cached snapshot `Value` into a transfer status block. GLM
/// snapshots carry `usedPercent` instead of `quotaUsedPercent`; display
/// strings are never synthesized here — the frontend owns them.
pub fn normalize_status(kind: &str, snapshot: &Value, saved_at_ms: i64) -> TransferStatus {
    let mut status = TransferStatus {
        kind: kind.to_string(),
        saved_at_ms,
        ..TransferStatus::default()
    };
    if kind == "glm" {
        status.quota_used_percent = snapshot.get("usedPercent").and_then(Value::as_f64);
        status.cooldown_ends_at_ms = snapshot.get("cooldownEndsAtMs").and_then(Value::as_i64);
        status.requests = snapshot.get("requests").and_then(Value::as_u64);
        status.total_tokens = snapshot.get("totalTokens").and_then(Value::as_u64);
        return status;
    }
    status.label = string_field(snapshot, "label");
    status.primary_label = string_field(snapshot, "primaryLabel");
    status.primary_value = string_field(snapshot, "primaryValue");
    status.secondary_value = string_field(snapshot, "secondaryValue");
    status.balance_cny = snapshot.get("balanceCny").and_then(Value::as_f64);
    status.quota_used_percent = snapshot.get("quotaUsedPercent").and_then(Value::as_f64);
    status.cooldown_ends_at_ms = snapshot.get("cooldownEndsAtMs").and_then(Value::as_i64);
    status.requests = snapshot.get("requests").and_then(Value::as_u64);
    status.total_tokens = snapshot.get("totalTokens").and_then(Value::as_u64);
    status.estimated_cost_cny = snapshot.get("estimatedCostCny").and_then(Value::as_f64);
    status
}

fn string_field(snapshot: &Value, key: &str) -> Option<String> {
    snapshot
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Assembles the export payload. `credentials` is pre-loaded by the caller;
/// a missing entry (e.g. corrupt DPAPI file) exports without the credential
/// rather than failing the whole backup. Full mode embeds each instance's
/// verbatim cached snapshot plus the shared usage history; status mode
/// exports the normalized, shareable status blocks only.
pub fn assemble_payload(
    mode: TransferMode,
    remarks: &BTreeMap<String, String>,
    instances: &[String],
    snapshots: &[CachedSnapshot],
    credentials: &BTreeMap<String, String>,
    history: &[DailyUsageRecord],
    exported_at_ms: i64,
) -> TransferPayload {
    let exported: HashSet<&str> = instances.iter().map(String::as_str).collect();
    let entries = instances
        .iter()
        .map(|instance_id| {
            let remark = remarks
                .get(instance_id)
                .map(String::as_str)
                .filter(|remark| !remark.is_empty())
                .map(str::to_string);
            let credential = if mode == TransferMode::Full {
                credentials.get(instance_id).cloned()
            } else {
                None
            };
            let cached = snapshots
                .iter()
                .find(|cached| cached.provider_id == *instance_id);
            let (snapshot, status) = match cached {
                Some(cached) => match mode {
                    TransferMode::Full => (
                        Some(TransferSnapshot {
                            kind: cached.kind.clone(),
                            saved_at_ms: cached.saved_at_ms,
                            snapshot: cached.snapshot.clone(),
                        }),
                        None,
                    ),
                    TransferMode::Status => (
                        None,
                        Some(normalize_status(
                            &cached.kind,
                            &cached.snapshot,
                            cached.saved_at_ms,
                        )),
                    ),
                },
                None => (None, None),
            };
            TransferInstance {
                provider_id: instance_id.clone(),
                remark,
                credential,
                status,
                snapshot,
            }
        })
        .collect();
    let history = if mode == TransferMode::Full {
        history
            .iter()
            .filter(|record| exported.contains(record.provider_id.as_str()))
            .cloned()
            .collect()
    } else {
        Vec::new()
    };
    TransferPayload {
        version: TRANSFER_FORMAT_VERSION,
        mode,
        exported_at_ms,
        instances: entries,
        history,
    }
}

/// Serializes the payload for disk. Full backups carry the whole usage
/// history, so they serialize compact to stay well inside the transfer size
/// cap; status reports stay pretty-printed for humans.
pub fn encode_transfer_file(payload: &TransferPayload) -> Result<Vec<u8>, TransferFileError> {
    let mut bytes = match payload.mode {
        TransferMode::Full => serde_json::to_vec(payload),
        TransferMode::Status => serde_json::to_vec_pretty(payload),
    }
    .map_err(|_| TransferFileError::Malformed)?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Reads transfer-file bytes (BOM tolerated), enforces size and instance
/// caps, and validates the format version.
pub fn parse_transfer_file(bytes: &[u8]) -> Result<TransferPayload, TransferFileError> {
    if bytes.len() > MAX_TRANSFER_FILE_BYTES {
        return Err(TransferFileError::TooLarge);
    }
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let payload: TransferPayload =
        serde_json::from_slice(bytes).map_err(|_| TransferFileError::InvalidJson)?;
    if payload.version != TRANSFER_FORMAT_VERSION {
        return Err(TransferFileError::UnsupportedVersion);
    }
    if payload.instances.len() > MAX_TRANSFER_INSTANCES {
        return Err(TransferFileError::Malformed);
    }
    Ok(payload)
}

/// Allocates a non-colliding instance id for import, mirroring the frontend
/// `nextInstanceId` semantics: a bare id counts as instance 1 and collisions
/// take `_{max+1}`. The assigned id joins `taken` so later entries in the
/// same batch cannot collide with it. Returns None for unknown bases or an
/// index budget breach (both treated as invalid by the importer).
pub fn assign_instance_id(source_id: &str, taken: &mut HashSet<String>) -> Option<String> {
    let (base, _) = credential_instance(source_id)?;
    let mut max_index = 0u64;
    for existing in taken.iter() {
        let Some((existing_base, existing_index)) = credential_instance(existing) else {
            continue;
        };
        if existing_base == base {
            max_index = max_index.max(u64::from(existing_index));
        }
    }
    let assigned = if max_index == 0 {
        base.clone()
    } else {
        format!("{base}_{}", max_index + 1)
    };
    if assigned.len() > MAX_INSTANCE_ID_LEN {
        return None;
    }
    taken.insert(assigned.clone());
    Some(assigned)
}

/// Trims, collapses whitespace, and truncates a remark to 24 Unicode scalar
/// values (CJK-safe), mirroring the frontend sanitizer. Empty stays None.
pub fn sanitize_remark(raw: &str) -> Option<String> {
    let mut collapsed = String::with_capacity(raw.len());
    let mut in_whitespace = false;
    for character in raw.trim().chars() {
        if character.is_whitespace() {
            in_whitespace = true;
            continue;
        }
        if in_whitespace && !collapsed.is_empty() {
            collapsed.push(' ');
        }
        in_whitespace = false;
        collapsed.push(character);
    }
    let remark: String = collapsed.chars().take(REMARK_MAX_CHARS).collect();
    let remark = remark.trim_end().to_string();
    (!remark.is_empty()).then_some(remark)
}

/// Offline credential format validation. Both constructors only build a
/// client and headers — no network I/O — which is exactly the check an
/// import-without-sync can rely on.
pub fn validate_credential(base: &str, credential: &str) -> bool {
    if base == "glm" {
        return GlmClient::new(credential).is_ok();
    }
    match OnlineProvider::from_id(base) {
        Some(provider) => OnlineClient::new(provider, credential).is_ok(),
        None => false,
    }
}

/// Applies a parsed transfer file: saves each importable credential into the
/// vault under a non-colliding instance id, restores the transferred
/// snapshots into the local cache, and merges the remapped usage history.
/// A credential that already exists locally under another instance of the
/// same provider is skipped so re-importing a backup cannot duplicate the
/// account or double-count its history. Existing instances are never
/// overwritten, and the returned results carry no credential bytes.
pub fn apply_import(
    payload: &TransferPayload,
    app_data: &Path,
    existing: &[String],
) -> ImportOutcome {
    let mut taken: HashSet<String> = existing.iter().cloned().collect();
    let mut entries = Vec::with_capacity(payload.instances.len());
    let mut assigned_by_source: BTreeMap<String, String> = BTreeMap::new();
    for entry in &payload.instances {
        let result = import_entry(payload.mode, entry, app_data, &mut taken);
        if result.outcome == "saved" {
            if let Some(assigned) = result.assigned_instance_id.as_ref() {
                assigned_by_source.insert(entry.provider_id.clone(), assigned.clone());
            }
        }
        entries.push(result);
    }
    let history_merged = merge_import_history(payload, app_data, &assigned_by_source);
    ImportOutcome {
        entries,
        history_merged,
    }
}

fn import_entry(
    mode: TransferMode,
    entry: &TransferInstance,
    app_data: &Path,
    taken: &mut HashSet<String>,
) -> ImportEntryResult {
    let remark = entry.remark.as_deref().and_then(sanitize_remark);
    let credential = entry
        .credential
        .as_deref()
        .map(str::trim)
        .filter(|credential| !credential.is_empty());
    let Some(credential) = credential else {
        return ImportEntryResult {
            source_provider_id: entry.provider_id.clone(),
            assigned_instance_id: None,
            remark,
            outcome: "skipped",
            reason: Some(if mode == TransferMode::Status {
                "状态报告不含凭据"
            } else {
                "该条目缺少凭据"
            }),
        };
    };
    let Some((base, _)) = credential_instance(&entry.provider_id) else {
        return ImportEntryResult {
            source_provider_id: entry.provider_id.clone(),
            assigned_instance_id: None,
            remark,
            outcome: "invalid",
            reason: Some("供应商不受支持或已下线"),
        };
    };
    if !validate_credential(&base, credential) {
        return ImportEntryResult {
            source_provider_id: entry.provider_id.clone(),
            assigned_instance_id: None,
            remark,
            outcome: "invalid",
            reason: Some("凭据格式无效"),
        };
    }
    if credential_exists_locally(&base, credential, app_data, taken) {
        return ImportEntryResult {
            source_provider_id: entry.provider_id.clone(),
            assigned_instance_id: None,
            remark,
            outcome: "skipped",
            reason: Some("本机已存在相同密钥的实例"),
        };
    }
    let Some(assigned) = assign_instance_id(&entry.provider_id, taken) else {
        return ImportEntryResult {
            source_provider_id: entry.provider_id.clone(),
            assigned_instance_id: None,
            remark,
            outcome: "invalid",
            reason: Some("供应商不受支持或已下线"),
        };
    };
    let saved = SecretVault::new(app_data, &assigned)
        .and_then(|vault| vault.save(credential));
    match saved {
        Ok(()) => {
            if let Some(snapshot) = entry.snapshot.as_ref() {
                restore_snapshot(&assigned, snapshot, app_data);
            }
            ImportEntryResult {
                source_provider_id: entry.provider_id.clone(),
                assigned_instance_id: Some(assigned),
                remark,
                outcome: "saved",
                reason: None,
            }
        }
        Err(_) => ImportEntryResult {
            source_provider_id: entry.provider_id.clone(),
            assigned_instance_id: None,
            remark,
            outcome: "invalid",
            reason: Some("本机凭据存储不可用"),
        },
    }
}

/// Whether an instance of the same provider already stores this exact
/// credential. The comparison stays in-process and is never logged; vaults
/// that cannot be read are treated as non-matches so a corrupt store cannot
/// block the import.
fn credential_exists_locally(
    base: &str,
    credential: &str,
    app_data: &Path,
    taken: &HashSet<String>,
) -> bool {
    taken.iter().any(|existing| {
        credential_instance(existing)
            .is_some_and(|(existing_base, _)| existing_base == base)
            && SecretVault::new(app_data, existing)
                .and_then(|vault| vault.load())
                .is_ok_and(|stored| stored == credential)
    })
}

/// Writes a transferred snapshot into the local cache under the assigned
/// instance id, relabeling the online identity fields so the row renders
/// exactly as it did on the source machine. Best effort: a failed restore
/// never fails the credential import — the next sync rebuilds the cache.
fn restore_snapshot(assigned: &str, snapshot: &TransferSnapshot, app_data: &Path) {
    if (snapshot.kind != "glm" && snapshot.kind != "online") || !snapshot.snapshot.is_object() {
        return;
    }
    let mut value = snapshot.snapshot.clone();
    if snapshot.kind == "online" {
        relabel_online_snapshot(&mut value, assigned);
    }
    let _ = SnapshotCache::new(app_data).restore(
        assigned,
        &snapshot.kind,
        value,
        snapshot.saved_at_ms,
    );
}

/// Stamps an online snapshot with the assigned instance identity, mirroring
/// `apply_instance_identity` in app.rs: `providerId` follows the assigned id
/// and the label's instance suffix is rewritten for the new index.
fn relabel_online_snapshot(snapshot: &mut Value, assigned_id: &str) {
    let (_, index) = split_instance_suffix(assigned_id).unwrap_or((assigned_id, 1));
    let Some(object) = snapshot.as_object_mut() else {
        return;
    };
    object.insert("providerId".to_string(), Value::String(assigned_id.to_string()));
    if let Some(Value::String(label)) = object.get("label") {
        let relabeled = if index >= 2 {
            format!("{} · 实例 {index}", strip_instance_suffix_label(label))
        } else {
            strip_instance_suffix_label(label)
        };
        object.insert("label".to_string(), Value::String(relabeled));
    }
}

/// Drops a trailing "· 实例 N" from a source label so the assigned index can
/// be stamped cleanly.
fn strip_instance_suffix_label(label: &str) -> String {
    match label.rsplit_once(" · 实例 ") {
        Some((prefix, suffix)) if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) => {
            prefix.to_string()
        }
        _ => label.to_string(),
    }
}

/// Remaps the transfer history onto the assigned instance ids and merges it
/// into the local daily history. Records for sources that were not imported
/// (skipped, invalid, or plain unknown ids) are dropped so ghost instances
/// never pollute the trend charts. Best effort: a failed merge never fails
/// the import — the credentials are already saved at this point.
fn merge_import_history(
    payload: &TransferPayload,
    app_data: &Path,
    assigned_by_source: &BTreeMap<String, String>,
) -> usize {
    if payload.mode != TransferMode::Full
        || payload.history.is_empty()
        || assigned_by_source.is_empty()
    {
        return 0;
    }
    let remapped: Vec<DailyUsageRecord> = payload
        .history
        .iter()
        .filter_map(|record| {
            let assigned = assigned_by_source.get(&record.provider_id)?;
            let mut record = record.clone();
            record.provider_id = assigned.clone();
            Some(record)
        })
        .collect();
    DailyUsageHistory::new(app_data)
        .merge(remapped)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cached(provider_id: &str, kind: &str, snapshot: Value) -> CachedSnapshot {
        CachedSnapshot {
            provider_id: provider_id.to_string(),
            kind: kind.to_string(),
            saved_at_ms: 1_787_194_023_645,
            snapshot,
        }
    }

    fn online_snapshot() -> Value {
        json!({
            "label": "Kimi Code · 实例 2",
            "primaryLabel": "5 小时用量",
            "primaryValue": "82.0%",
            "secondaryValue": "5 小时剩余 18%",
            "balanceCny": 64.2,
            "quotaUsedPercent": 82.0,
            "cooldownEndsAtMs": 1_787_194_023_645i64
        })
    }

    fn daily(date: &str, minute: Option<i16>, provider_id: &str, tokens: u64) -> DailyUsageRecord {
        DailyUsageRecord {
            date: date.to_string(),
            slot: None,
            minute,
            provider_id: provider_id.to_string(),
            requests: Some(1),
            total_tokens: Some(tokens),
            estimated_cost_cny: None,
            balance_cny: None,
        }
    }

    #[test]
    fn assembles_full_backup_with_credentials_remarks_and_history() {
        let remarks = BTreeMap::from([("kimi_cn_2".to_string(), "工作账号".to_string())]);
        let snapshots = vec![cached("kimi_cn_2", "online", online_snapshot())];
        let credentials = BTreeMap::from([("kimi_cn_2".to_string(), "sk-kimi-key".to_string())]);
        let history = vec![
            daily("2026-10-07", Some(10), "kimi_cn_2", 1_000),
            // History for an instance that is not exported must not travel.
            daily("2026-10-07", Some(10), "glm", 5_000),
        ];

        let payload = assemble_payload(
            TransferMode::Full,
            &remarks,
            &["kimi_cn_2".to_string()],
            &snapshots,
            &credentials,
            &history,
            1_755_648_000_000,
        );

        assert_eq!(payload.version, 1);
        assert_eq!(payload.instances.len(), 1);
        let entry = &payload.instances[0];
        assert_eq!(entry.remark.as_deref(), Some("工作账号"));
        assert_eq!(entry.credential.as_deref(), Some("sk-kimi-key"));
        // Full mode embeds the verbatim snapshot, not the normalized block.
        let snapshot = entry.snapshot.as_ref().expect("verbatim snapshot");
        assert_eq!(snapshot.kind, "online");
        assert_eq!(snapshot.saved_at_ms, 1_787_194_023_645);
        assert_eq!(snapshot.snapshot["primaryValue"], "82.0%");
        assert_eq!(snapshot.snapshot["balanceCny"], 64.2);
        assert!(entry.status.is_none());
        assert_eq!(payload.history.len(), 1);
        assert_eq!(payload.history[0].provider_id, "kimi_cn_2");
    }

    #[test]
    fn status_mode_never_serializes_a_credential_key() {
        let credentials = BTreeMap::from([("deepseek".to_string(), "sk-deep".to_string())]);
        let history = vec![daily("2026-10-07", Some(10), "deepseek", 1_000)];

        let payload = assemble_payload(
            TransferMode::Status,
            &BTreeMap::new(),
            &["deepseek".to_string()],
            &[],
            &credentials,
            &history,
            0,
        );
        let text = serde_json::to_string(&payload).expect("serialize");

        assert!(!text.contains("credential"));
        assert!(!text.contains("sk-deep"));
        assert!(payload.instances[0].credential.is_none());
        assert!(payload.instances[0].snapshot.is_none());
        // Status reports are shareable: no usage history travels either.
        assert!(payload.history.is_empty());
    }

    #[test]
    fn status_mode_keeps_the_normalized_snapshot_block() {
        let snapshots = vec![cached("kimi_cn", "online", online_snapshot())];

        let payload = assemble_payload(
            TransferMode::Status,
            &BTreeMap::new(),
            &["kimi_cn".to_string()],
            &snapshots,
            &BTreeMap::new(),
            &[],
            0,
        );

        let entry = &payload.instances[0];
        assert!(entry.snapshot.is_none());
        let status = entry.status.as_ref().expect("normalized status present");
        assert_eq!(status.kind, "online");
        assert_eq!(status.primary_value.as_deref(), Some("82.0%"));
        assert_eq!(status.balance_cny, Some(64.2));
    }

    #[test]
    fn instances_without_a_snapshot_export_null_status() {
        let payload = assemble_payload(
            TransferMode::Status,
            &BTreeMap::new(),
            &["deepseek".to_string()],
            &[],
            &BTreeMap::new(),
            &[],
            0,
        );

        assert!(payload.instances[0].status.is_none());
    }

    #[test]
    fn encodes_full_backups_compact_and_status_reports_pretty() {
        let full = assemble_payload(
            TransferMode::Full,
            &BTreeMap::new(),
            &["glm".to_string()],
            &[],
            &BTreeMap::new(),
            &[daily("2026-10-07", Some(10), "glm", 1_000)],
            0,
        );
        let bytes = encode_transfer_file(&full).expect("encode full");
        assert!(!String::from_utf8_lossy(&bytes).contains("\n  "));
        let parsed = parse_transfer_file(&bytes).expect("round-trip full");
        assert_eq!(parsed.mode, TransferMode::Full);
        assert_eq!(parsed.history.len(), 1);

        let status = assemble_payload(
            TransferMode::Status,
            &BTreeMap::new(),
            &["glm".to_string()],
            &[],
            &BTreeMap::new(),
            &[],
            0,
        );
        let bytes = encode_transfer_file(&status).expect("encode status");
        assert!(String::from_utf8_lossy(&bytes).contains("\n  "));
    }

    #[test]
    fn normalizes_glm_snapshots_without_synthesizing_display_strings() {
        let snapshot = json!({
            "planLevel": "pro",
            "usedPercent": 44.0,
            "requests": 1328,
            "totalTokens": 112_688_866,
            "cooldownEndsAtMs": 1_787_183_355_359i64
        });

        let status = normalize_status("glm", &snapshot, 42);

        assert_eq!(status.quota_used_percent, Some(44.0));
        assert_eq!(status.requests, Some(1328));
        assert_eq!(status.total_tokens, Some(112_688_866));
        assert_eq!(status.cooldown_ends_at_ms, Some(1_787_183_355_359));
        assert_eq!(status.label, None);
        assert_eq!(status.primary_value, None);
    }

    #[test]
    fn normalizing_an_unknown_snapshot_shape_degrades_to_none() {
        let status = normalize_status("online", &json!("not an object"), 7);

        assert_eq!(status.kind, "online");
        assert_eq!(status.saved_at_ms, 7);
        assert_eq!(status.primary_value, None);
        assert_eq!(status.balance_cny, None);
    }

    #[test]
    fn parses_a_full_transfer_file() {
        let json = r#"{
          "version": 1,
          "mode": "full",
          "exportedAtMs": 1755648000000,
          "instances": [
            {"providerId": "glm", "credential": "sk-glm-key"}
          ]
        }"#;

        let payload = parse_transfer_file(json.as_bytes()).expect("parse");

        assert_eq!(payload.mode, TransferMode::Full);
        assert_eq!(payload.instances[0].provider_id, "glm");
    }

    #[test]
    fn parses_files_with_a_utf8_bom() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(
            br#"{"version":1,"mode":"status","exportedAtMs":0,"instances":[]}"#,
        );

        let payload = parse_transfer_file(&bytes).expect("parse");

        assert_eq!(payload.mode, TransferMode::Status);
    }

    #[test]
    fn rejects_unsupported_versions_and_invalid_json() {
        let v2 = br#"{"version":2,"mode":"full","exportedAtMs":0,"instances":[]}"#;
        assert_eq!(
            parse_transfer_file(v2).unwrap_err(),
            TransferFileError::UnsupportedVersion
        );

        assert_eq!(
            parse_transfer_file(b"not json").unwrap_err(),
            TransferFileError::InvalidJson
        );
    }

    #[test]
    fn rejects_oversized_files_and_instance_floods() {
        let oversized = vec![b' '; MAX_TRANSFER_FILE_BYTES + 1];
        assert_eq!(
            parse_transfer_file(&oversized).unwrap_err(),
            TransferFileError::TooLarge
        );

        let mut instances = String::new();
        for _ in 0..=MAX_TRANSFER_INSTANCES {
            instances.push_str(r#"{"providerId":"deepseek"},"#);
        }
        let flood = format!(
            r#"{{"version":1,"mode":"full","exportedAtMs":0,"instances":[{instances}{}]}}"#,
            r#"{"providerId":"glm"}"#
        );
        assert_eq!(
            parse_transfer_file(flood.as_bytes()).unwrap_err(),
            TransferFileError::Malformed
        );
    }

    #[test]
    fn assigns_free_ids_verbatim_and_suffixes_collisions() {
        let mut taken: HashSet<String> = HashSet::new();

        // A free base id lands verbatim and joins the taken set.
        assert_eq!(
            assign_instance_id("deepseek", &mut taken).as_deref(),
            Some("deepseek")
        );
        // The same source again must not overwrite it.
        assert_eq!(
            assign_instance_id("deepseek", &mut taken).as_deref(),
            Some("deepseek_2")
        );
        // A later batch entry with an explicit suffix targets the next slot
        // beyond every taken index, not the literal source suffix.
        assert_eq!(
            assign_instance_id("deepseek_2", &mut taken).as_deref(),
            Some("deepseek_3")
        );
        assert_eq!(
            assign_instance_id("deepseek", &mut taken).as_deref(),
            Some("deepseek_4")
        );
    }

    #[test]
    fn assigned_ids_stay_within_the_vault_id_budget() {
        // A 10-digit u32 index on the longest base id still fits 32 chars.
        let mut taken: HashSet<String> =
            HashSet::from(["siliconflow_global_4294967295".to_string()]);

        let assigned =
            assign_instance_id("siliconflow_global_4294967295", &mut taken).expect("assigned");

        assert!(assigned.len() <= 32, "assigned id too long: {assigned}");
        assert!(
            assigned
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        );
    }

    #[test]
    fn rejects_unknown_source_bases() {
        let mut taken = HashSet::new();

        assert_eq!(assign_instance_id("mistral", &mut taken), None);
        assert_eq!(assign_instance_id("", &mut taken), None);
    }

    #[test]
    fn sanitizes_remarks_like_the_frontend() {
        assert_eq!(sanitize_remark("  工作   账号  ").as_deref(), Some("工作 账号"));
        let long: String = "备".repeat(30);
        assert_eq!(sanitize_remark(&long).map(|r| r.chars().count()), Some(24));
        assert_eq!(sanitize_remark("   "), None);
        assert_eq!(sanitize_remark(""), None);
    }

    #[test]
    fn validates_credential_formats_offline() {
        assert!(validate_credential("glm", "sk-some-glm-key"));
        assert!(!validate_credential("glm", ""));
        // Control characters cannot form a header value, so they fail offline.
        assert!(!validate_credential("glm", "bad\nglm"));
        assert!(validate_credential("kimi_cn", "sk-kimi-o21Abc"));
        // Multi-field providers store camelCase JSON and reject anything else.
        assert!(validate_credential(
            "xai",
            r#"{"managementKey":"mk-123","teamId":"team-1"}"#
        ));
        assert!(!validate_credential("xai", "not-json"));
        assert!(!validate_credential("mistral", "whatever"));
    }

    #[test]
    fn applies_imports_without_overwriting_existing_instances() {
        let dir = std::env::temp_dir().join(format!("llm-usage-transfer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("credentials")).expect("create temp dir");

        // Seed one existing kimi_cn instance so the imported one must suffix.
        SecretVault::new(&dir, "kimi_cn")
            .expect("vault")
            .save("sk-kimi-existing")
            .expect("seed vault");

        let payload: TransferPayload = serde_json::from_str(
            r#"{
              "version": 1,
              "mode": "full",
              "exportedAtMs": 0,
              "instances": [
                {"providerId": "kimi_cn", "remark": " 工作账号 ",
                 "credential": "sk-kimi-imported"},
                {"providerId": "unknown_provider", "credential": "sk-x"},
                {"providerId": "glm", "credential": "bad\nglm"}
              ]
            }"#,
        )
        .expect("fixture");

        let results = &apply_import(&payload, &dir, &enumerate_instances(&dir)).entries;

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].outcome, "saved");
        assert_eq!(results[0].assigned_instance_id.as_deref(), Some("kimi_cn_2"));
        assert_eq!(results[0].remark.as_deref(), Some("工作账号"));
        assert_eq!(results[1].outcome, "invalid");
        assert_eq!(results[1].reason, Some("供应商不受支持或已下线"));
        assert_eq!(results[2].outcome, "invalid");
        assert_eq!(results[2].reason, Some("凭据格式无效"));

        // The existing instance is untouched; the import landed beside it.
        assert_eq!(
            SecretVault::new(&dir, "kimi_cn")
                .expect("vault")
                .load()
                .expect("existing key"),
            "sk-kimi-existing"
        );
        assert_eq!(
            SecretVault::new(&dir, "kimi_cn_2")
                .expect("vault")
                .load()
                .expect("imported key"),
            "sk-kimi-imported"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn status_reports_are_skipped_with_a_dedicated_reason() {
        let dir = std::env::temp_dir().join(format!("llm-usage-transfer-status-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir.join("credentials")).expect("create temp dir");

        let payload: TransferPayload = serde_json::from_str(
            r#"{"version":1,"mode":"status","exportedAtMs":0,
                "instances":[{"providerId":"glm","status":{"kind":"glm","savedAtMs":1}}]}"#,
        )
        .expect("fixture");

        let results = &apply_import(&payload, &dir, &[]).entries;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].outcome, "skipped");
        assert_eq!(results[0].reason, Some("状态报告不含凭据"));
        assert_eq!(results[0].assigned_instance_id, None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn skips_credentials_that_already_exist_locally() {
        let dir = std::env::temp_dir().join(format!(
            "llm-usage-transfer-duplicate-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("credentials")).expect("create temp dir");
        // The same key is already configured under the bare instance id.
        SecretVault::new(&dir, "kimi_cn")
            .expect("vault")
            .save("sk-kimi-same")
            .expect("seed vault");

        let payload: TransferPayload = serde_json::from_str(
            r#"{
              "version": 1,
              "mode": "full",
              "exportedAtMs": 0,
              "instances": [
                {"providerId": "kimi_cn_3", "credential": "sk-kimi-same"}
              ],
              "history": [
                {"date": "2026-10-07", "minute": 10, "providerId": "kimi_cn_3",
                 "requests": 1, "totalTokens": 100}
              ]
            }"#,
        )
        .expect("fixture");

        let outcome = apply_import(&payload, &dir, &enumerate_instances(&dir));

        assert_eq!(outcome.entries.len(), 1);
        assert_eq!(outcome.entries[0].outcome, "skipped");
        assert_eq!(outcome.entries[0].reason, Some("本机已存在相同密钥的实例"));
        assert_eq!(outcome.entries[0].assigned_instance_id, None);
        // No duplicate instance was created and its history was not merged,
        // so re-importing a backup can never double-count usage.
        assert_eq!(outcome.history_merged, 0);
        assert!(enumerate_instances(&dir) == vec!["kimi_cn"]);
        assert!(DailyUsageHistory::new(&dir)
            .load()
            .expect("history")
            .is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restores_snapshots_under_the_assigned_id_with_relabeling() {
        let dir = std::env::temp_dir().join(format!(
            "llm-usage-transfer-restore-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("credentials")).expect("create temp dir");
        // Seed the bare id with a different key so the import lands on _2.
        SecretVault::new(&dir, "kimi_cn")
            .expect("vault")
            .save("sk-kimi-existing")
            .expect("seed vault");

        let payload: TransferPayload = serde_json::from_str(
            r#"{
              "version": 1,
              "mode": "full",
              "exportedAtMs": 0,
              "instances": [
                {"providerId": "kimi_cn", "credential": "sk-kimi-o21Abc",
                 "snapshot": {
                   "kind": "online",
                   "savedAtMs": 1234,
                   "snapshot": {
                     "providerId": "kimi_cn",
                     "label": "Kimi Code · 实例 3",
                     "primaryValue": "82.0%"
                   }
                 }}
              ]
            }"#,
        )
        .expect("fixture");

        let outcome = apply_import(&payload, &dir, &enumerate_instances(&dir));

        assert_eq!(outcome.entries[0].outcome, "saved");
        assert_eq!(
            outcome.entries[0].assigned_instance_id.as_deref(),
            Some("kimi_cn_2")
        );
        let snapshots = SnapshotCache::new(&dir).load_all().expect("restored cache");
        assert_eq!(snapshots.len(), 1);
        let restored = &snapshots[0];
        assert_eq!(restored.provider_id, "kimi_cn_2");
        assert_eq!(restored.kind, "online");
        // The capture time travels with the snapshot, not the import time.
        assert_eq!(restored.saved_at_ms, 1234);
        assert_eq!(restored.snapshot["providerId"], "kimi_cn_2");
        // The source instance suffix is rewritten for the assigned index.
        assert_eq!(restored.snapshot["label"], "Kimi Code · 实例 2");
        assert_eq!(restored.snapshot["primaryValue"], "82.0%");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn merges_history_onto_assigned_ids_and_drops_ghost_records() {
        let dir = std::env::temp_dir().join(format!(
            "llm-usage-transfer-history-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("credentials")).expect("create temp dir");

        let payload: TransferPayload = serde_json::from_str(
            r#"{
              "version": 1,
              "mode": "full",
              "exportedAtMs": 0,
              "instances": [
                {"providerId": "kimi_cn", "credential": "sk-kimi-o21Abc"}
              ],
              "history": [
                {"date": "2026-10-07", "minute": 10, "providerId": "kimi_cn",
                 "requests": 1, "totalTokens": 100},
                {"date": "2026-10-07", "minute": 11, "providerId": "kimi_cn",
                 "requests": 2, "totalTokens": 200},
                {"date": "2026-10-07", "minute": 10, "providerId": "kimi_cn",
                 "requests": 3, "totalTokens": 300},
                {"date": "2026-10-07", "minute": 10, "providerId": "glm",
                 "requests": 9, "totalTokens": 900},
                {"date": "2026-10-07", "minute": 10, "providerId": "unknown_provider",
                 "requests": 9, "totalTokens": 900}
              ]
            }"#,
        )
        .expect("fixture");

        let outcome = apply_import(&payload, &dir, &[]);

        assert_eq!(outcome.entries[0].outcome, "saved");
        // The within-batch duplicate key (minute 10) keeps the last record;
        // records for sources that were not imported are dropped.
        assert_eq!(outcome.history_merged, 2);
        let records = DailyUsageHistory::new(&dir).load().expect("merged history");
        assert_eq!(records.len(), 2);
        assert!(records.iter().all(|record| record.provider_id == "kimi_cn"));
        assert_eq!(
            records
                .iter()
                .find(|record| record.minute == Some(10))
                .and_then(|record| record.total_tokens),
            Some(300)
        );

        let _ = std::fs::remove_dir_all(&dir);
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
    fn enumerates_only_valid_dpapi_stems_sorted_by_instance() {
        let dir = std::env::temp_dir().join(format!("llm-usage-transfer-list-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let credentials = dir.join("credentials");
        std::fs::create_dir_all(&credentials).expect("create temp dir");
        for name in ["kimi_cn_2.dpapi", "kimi_cn.dpapi", "junk.dpapi", "glm.dpapi", "notes.txt"] {
            std::fs::write(credentials.join(name), b"x").expect("seed file");
        }

        let instances = enumerate_instances(&dir);

        assert_eq!(instances, vec!["glm", "kimi_cn", "kimi_cn_2"]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn enumerates_registry_instances_alongside_dpapi_files() {
        let dir = std::env::temp_dir().join(format!(
            "llm-usage-transfer-registry-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let credentials = dir.join("credentials");
        std::fs::create_dir_all(&credentials).expect("create temp dir");
        std::fs::write(credentials.join("glm.dpapi"), b"x").expect("seed dpapi file");
        // Keychain-backed instances only exist in the registry file; unknown
        // ids are filtered and a dpapi duplicate does not double-list.
        std::fs::write(
            dir.join("instances.json"),
            br#"["kimi_cn","glm","not_a_provider"]"#,
        )
        .expect("seed registry");

        let instances = enumerate_instances(&dir);

        assert_eq!(instances, vec!["glm", "kimi_cn"]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
