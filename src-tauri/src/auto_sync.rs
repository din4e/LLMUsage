use std::{sync::Mutex, time::Duration};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::task::JoinHandle;

use crate::app::{self, CommandError};

const AUTO_SYNC_COMPLETED_EVENT: &str = "auto-sync-completed";
/// Floor for the background cadence so a corrupted stored value cannot turn
/// into a request flood against the provider APIs.
const MIN_INTERVAL_SECONDS: i64 = 30;

/// Owns the running auto-sync task. Replacing the interval aborts the old
/// task (a round cut mid-flight is safe: caches and history use atomic writes
/// and a serialized upsert).
#[derive(Default)]
pub struct AutoSyncState(Mutex<Option<JoinHandle<()>>>);

/// Per-instance outcome of one background round, forwarded to the frontend so
/// the dashboard can refresh rows and flag failures without doing the sync.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncRoundResult {
    synced: Vec<String>,
    failed: Vec<SyncFailure>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncFailure {
    instance_id: String,
    message: String,
}

/// Starts, replaces, or stops the Rust-side auto-sync task. `seconds <= 0`
/// cancels it. The interval itself stays persisted in the frontend's local
/// storage; the frontend re-invokes this on boot.
#[tauri::command(rename_all = "camelCase")]
pub fn set_auto_sync_interval(app: AppHandle, seconds: i64) {
    let state = app.state::<AutoSyncState>();
    let Ok(mut slot) = state.0.lock() else {
        // Only reachable if a previous holder panicked; skip the swap.
        return;
    };
    if let Some(running) = slot.take() {
        running.abort();
    }
    if seconds > 0 {
        // Clone out of the borrowed handle: the lock guard lives to the end
        // of the function but the task owns its own AppHandle.
        *slot = Some(spawn_rounds(app.clone(), seconds.max(MIN_INTERVAL_SECONDS)));
    }
}

fn spawn_rounds(app: AppHandle, seconds: i64) -> JoinHandle<()> {
    let interval = Duration::from_secs(seconds as u64);
    tokio::spawn(async move {
        // Sleep-first: the frontend runs its own opening sync round on boot,
        // so the background cadence only starts ticking after one interval.
        loop {
            tokio::time::sleep(interval).await;
            run_sync_round(&app).await;
        }
    })
}

/// Syncs every configured instance serially (one 15 s request timeout per
/// instance; serial keeps the worst case under the 60 s UI interval while
/// avoiding provider rate limits) and reports the round to the frontend.
async fn run_sync_round(app: &AppHandle) {
    let Some(window) = app::current_day_window(chrono::Utc::now()) else {
        return;
    };
    let Some(app_data) = app.path().app_data_dir().ok() else {
        return;
    };
    let instances = llm_usage_core::transfer::enumerate_instances(&app_data);
    let mut synced = Vec::new();
    let mut failed = Vec::new();
    for instance_id in instances {
        match sync_instance(app, &instance_id, &window).await {
            Ok(()) => synced.push(instance_id),
            Err(error) => failed.push(SyncFailure {
                instance_id,
                message: error.message().to_string(),
            }),
        }
    }
    // A hidden/frozen WebView queues the event and drains it on show; a
    // missing listener (browser preview) is not an error.
    let _ = app.emit(
        AUTO_SYNC_COMPLETED_EVENT,
        SyncRoundResult { synced, failed },
    );
}

async fn sync_instance(
    app: &AppHandle,
    instance_id: &str,
    window: &app::LocalDayWindow,
) -> Result<(), CommandError> {
    if instance_id == "glm" || instance_id.starts_with("glm_") {
        app::sync_glm_instance(
            app,
            instance_id,
            &window.date_key,
            Some(window.quarter_slot),
            &window.beijing_start,
            &window.beijing_end,
        )
        .await
        .map(|_| ())
    } else {
        let instance = llm_usage_core::providers::online::OnlineProvider::parse_instance(instance_id)
            .ok_or_else(CommandError::invalid_provider)?;
        app::sync_online_instance(
            app,
            &instance,
            &window.date_key,
            Some(window.quarter_slot),
            window.local_start_ms,
            window.local_end_ms,
        )
        .await
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_the_round_report_in_camel_case_for_the_frontend() {
        let payload = SyncRoundResult {
            synced: vec!["glm".to_string()],
            failed: vec![SyncFailure {
                instance_id: "kimi_cn".to_string(),
                message: "同步失败".to_string(),
            }],
        };
        let json = serde_json::to_string(&payload).expect("serialize");
        assert!(json.contains("\"synced\":[\"glm\"]"));
        assert!(json.contains("\"failed\":[{\"instanceId\":\"kimi_cn\""));
    }

    #[test]
    fn keeps_the_floor_below_the_shortest_ui_interval() {
        // The UI's shortest option is 60 s; the floor only guards corrupted
        // stored values, so it must never override a legit user choice.
        assert!(MIN_INTERVAL_SECONDS < 60);
    }
}
