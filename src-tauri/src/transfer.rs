//! Batch import/export of provider credentials with remaining-status info.
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

use crate::cache::CachedSnapshot;
use crate::providers::glm::GlmClient;
use crate::providers::online::{split_instance_suffix, OnlineClient, OnlineProvider};
use crate::secret::SecretVault;

pub const TRANSFER_FORMAT_VERSION: u32 = 1;
const MAX_TRANSFER_FILE_BYTES: usize = 2 * 1024 * 1024;
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

/// Lists configured instance ids (GLM and online) from a credentials
/// directory, sorted by base id then instance index.
pub fn enumerate_instances(app_data: &Path) -> Vec<String> {
    let entries = match std::fs::read_dir(app_data.join("credentials")) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    let mut instances: Vec<(String, u32, String)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("dpapi") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if let Some((base, index)) = credential_instance(stem) {
            instances.push((base, index, stem.to_string()));
        }
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
/// rather than failing the whole backup.
pub fn assemble_payload(
    mode: TransferMode,
    remarks: &BTreeMap<String, String>,
    instances: &[String],
    snapshots: &[CachedSnapshot],
    credentials: &BTreeMap<String, String>,
    exported_at_ms: i64,
) -> TransferPayload {
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
            let status = snapshots
                .iter()
                .find(|cached| cached.provider_id == *instance_id)
                .map(|cached| normalize_status(&cached.kind, &cached.snapshot, cached.saved_at_ms));
            TransferInstance {
                provider_id: instance_id.clone(),
                remark,
                credential,
                status,
            }
        })
        .collect();
    TransferPayload {
        version: TRANSFER_FORMAT_VERSION,
        mode,
        exported_at_ms,
        instances: entries,
    }
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
/// vault under a non-colliding instance id and reports the per-entry outcome.
/// Never overwrites an existing instance and never touches the snapshot
/// cache, daily history, or any log. The returned results carry no
/// credential bytes.
pub fn apply_import(
    payload: &TransferPayload,
    app_data: &Path,
    existing: &[String],
) -> Vec<ImportEntryResult> {
    let mut taken: HashSet<String> = existing.iter().cloned().collect();
    payload
        .instances
        .iter()
        .map(|entry| import_entry(payload.mode, entry, app_data, &mut taken))
        .collect()
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
        Ok(()) => ImportEntryResult {
            source_provider_id: entry.provider_id.clone(),
            assigned_instance_id: Some(assigned),
            remark,
            outcome: "saved",
            reason: None,
        },
        Err(_) => ImportEntryResult {
            source_provider_id: entry.provider_id.clone(),
            assigned_instance_id: None,
            remark,
            outcome: "invalid",
            reason: Some("本机凭据存储不可用"),
        },
    }
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

    #[test]
    fn assembles_full_backup_with_credentials_and_remarks() {
        let remarks = BTreeMap::from([("kimi_cn_2".to_string(), "工作账号".to_string())]);
        let snapshots = vec![cached("kimi_cn_2", "online", online_snapshot())];
        let credentials = BTreeMap::from([("kimi_cn_2".to_string(), "sk-kimi-key".to_string())]);

        let payload = assemble_payload(
            TransferMode::Full,
            &remarks,
            &["kimi_cn_2".to_string()],
            &snapshots,
            &credentials,
            1_755_648_000_000,
        );

        assert_eq!(payload.version, 1);
        assert_eq!(payload.instances.len(), 1);
        let entry = &payload.instances[0];
        assert_eq!(entry.remark.as_deref(), Some("工作账号"));
        assert_eq!(entry.credential.as_deref(), Some("sk-kimi-key"));
        let status = entry.status.as_ref().expect("status present");
        assert_eq!(status.kind, "online");
        assert_eq!(status.primary_value.as_deref(), Some("82.0%"));
        assert_eq!(status.balance_cny, Some(64.2));
        assert_eq!(status.saved_at_ms, 1_787_194_023_645);
    }

    #[test]
    fn status_mode_never_serializes_a_credential_key() {
        let credentials = BTreeMap::from([("deepseek".to_string(), "sk-deep".to_string())]);

        let payload = assemble_payload(
            TransferMode::Status,
            &BTreeMap::new(),
            &["deepseek".to_string()],
            &[],
            &credentials,
            0,
        );
        let text = serde_json::to_string(&payload).expect("serialize");

        assert!(!text.contains("credential"));
        assert!(!text.contains("sk-deep"));
        assert!(payload.instances[0].credential.is_none());
    }

    #[test]
    fn instances_without_a_snapshot_export_null_status() {
        let payload = assemble_payload(
            TransferMode::Status,
            &BTreeMap::new(),
            &["deepseek".to_string()],
            &[],
            &BTreeMap::new(),
            0,
        );

        assert!(payload.instances[0].status.is_none());
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

        let results = apply_import(&payload, &dir, &enumerate_instances(&dir));

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

        let results = apply_import(&payload, &dir, &[]);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].outcome, "skipped");
        assert_eq!(results[0].reason, Some("状态报告不含凭据"));
        assert_eq!(results[0].assigned_instance_id, None);

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
}
