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
}
