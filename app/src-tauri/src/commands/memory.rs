use crate::config_sync::ConfigSync;
use crate::history::{HistoryEntry, HistoryStorage};
use crate::memory::MemoryStorage;
#[cfg(desktop)]
use crate::settings::{LocalOnlySetting, RtviSyncedSetting};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

#[cfg(desktop)]
use crate::commands::settings::get_setting_from_store;

#[cfg(desktop)]
use tauri_plugin_store::StoreExt;

const MEMORY_SYNC_HISTORY_LIMIT: usize = 30;
const MEMORY_SYNC_SESSION_INTERVAL: u64 = 3;
const MEMORY_SYNC_COMPLETED_SESSION_COUNTER_KEY: &str = "memory_sync_completed_session_counter";

fn calculate_completed_session_counter_after_successful_sync(
    completed_session_counter: u64,
) -> u64 {
    completed_session_counter % MEMORY_SYNC_SESSION_INTERVAL
}

#[derive(Serialize)]
struct MemorySyncHistoryEntryPayload {
    timestamp: String,
    text: String,
    raw_text: String,
}

#[derive(Serialize)]
struct MemorySyncRequestPayload {
    llm_provider: String,
    history_entries: Vec<MemorySyncHistoryEntryPayload>,
    existing_memory_markdown: Option<String>,
}

#[derive(Deserialize)]
struct MemorySyncResponsePayload {
    memory_markdown: String,
}

#[cfg(desktop)]
fn read_memory_sync_completed_session_counter(app: &AppHandle) -> anyhow::Result<u64> {
    let store = app
        .store("settings.json")
        .context("Failed to open settings store while reading memory sync counter")?;

    let stored_counter_value = store.get(MEMORY_SYNC_COMPLETED_SESSION_COUNTER_KEY);
    let normalized_completed_session_counter = stored_counter_value
        .and_then(|value| {
            value.as_u64().or_else(|| {
                value
                    .as_i64()
                    .and_then(|signed_value| signed_value.try_into().ok())
            })
        })
        .unwrap_or(0);

    Ok(normalized_completed_session_counter)
}

#[cfg(desktop)]
pub(crate) fn set_memory_sync_completed_session_counter(
    app: &AppHandle,
    completed_session_counter: u64,
) -> anyhow::Result<()> {
    let store = app
        .store("settings.json")
        .context("Failed to open settings store while writing memory sync counter")?;
    store.set(
        MEMORY_SYNC_COMPLETED_SESSION_COUNTER_KEY,
        serde_json::json!(completed_session_counter),
    );
    store
        .save()
        .context("Failed to persist memory sync counter in settings store")?;
    Ok(())
}

#[cfg(desktop)]
fn increment_memory_sync_completed_session_counter(app: &AppHandle) -> anyhow::Result<u64> {
    let current_completed_session_counter = read_memory_sync_completed_session_counter(app)?;
    let next_completed_session_counter = current_completed_session_counter.saturating_add(1);
    set_memory_sync_completed_session_counter(app, next_completed_session_counter)?;
    Ok(next_completed_session_counter)
}

fn to_memory_sync_history_entry_payload(
    history_entry: HistoryEntry,
) -> MemorySyncHistoryEntryPayload {
    MemorySyncHistoryEntryPayload {
        timestamp: history_entry.timestamp.to_rfc3339(),
        text: history_entry.text,
        raw_text: history_entry.raw_text,
    }
}

#[cfg(desktop)]
async fn request_memory_markdown_generation(
    config_sync: &ConfigSync,
    llm_provider: String,
    history_entries: Vec<MemorySyncHistoryEntryPayload>,
    existing_memory_markdown: Option<String>,
) -> anyhow::Result<String> {
    let (memory_sync_http_client, server_url, client_uuid) = {
        let config_sync_state = config_sync.read().await;
        config_sync_state
            .clone_memory_sync_http_request_context()
            .ok_or_else(|| anyhow::anyhow!("Server URL or client UUID missing for memory sync"))?
    };

    let memory_sync_endpoint_url = format!("{server_url}/api/config/memory-sync");
    let memory_sync_response = memory_sync_http_client
        .post(&memory_sync_endpoint_url)
        .header("X-Client-UUID", client_uuid)
        .json(&MemorySyncRequestPayload {
            llm_provider,
            history_entries,
            existing_memory_markdown,
        })
        .send()
        .await
        .with_context(|| {
            format!("Failed to send memory sync request to {memory_sync_endpoint_url}")
        })?
        .error_for_status()
        .with_context(|| {
            format!(
                "Server returned an error for memory sync request to {memory_sync_endpoint_url}"
            )
        })?;

    let memory_sync_response_payload = memory_sync_response
        .json::<MemorySyncResponsePayload>()
        .await
        .context("Failed to parse memory sync response payload from server")?;

    Ok(memory_sync_response_payload.memory_markdown)
}

/// Ensure the user memory file exists with the default template.
#[tauri::command]
pub async fn initialize_memory_file(
    memory_storage: State<'_, MemoryStorage>,
) -> Result<(), String> {
    memory_storage.init().map_err(|error| format!("{error:#}"))
}

/// Read the latest memory markdown content.
#[tauri::command]
pub async fn read_memory_markdown(
    memory_storage: State<'_, MemoryStorage>,
) -> Result<String, String> {
    memory_storage
        .read_memory_markdown()
        .map_err(|error| format!("{error:#}"))
}

/// Replace the memory markdown body (with backup before overwrite).
#[tauri::command]
pub async fn replace_memory_markdown(
    memory_markdown: String,
    memory_storage: State<'_, MemoryStorage>,
) -> Result<(), String> {
    memory_storage
        .write_memory_markdown(memory_markdown)
        .map_err(|error| format!("{error:#}"))
}

#[cfg(desktop)]
async fn run_memory_sync_if_due_with_dependencies(
    app: &AppHandle,
    history_storage: &HistoryStorage,
    memory_storage: &MemoryStorage,
    config_sync: &ConfigSync,
) -> Result<(), String> {
    let memory_enabled: bool = get_setting_from_store(app, LocalOnlySetting::MemoryEnabled, false);
    if !memory_enabled {
        return Ok(());
    }

    let completed_session_counter = increment_memory_sync_completed_session_counter(app)
        .map_err(|error| format!("{error:#}"))?;
    if completed_session_counter < MEMORY_SYNC_SESSION_INTERVAL {
        return Ok(());
    }

    let is_connected_to_server_for_memory_sync = {
        let config_sync_state = config_sync.read().await;
        config_sync_state.is_connected()
    };
    if !is_connected_to_server_for_memory_sync {
        return Ok(());
    }

    let llm_provider: String =
        get_setting_from_store(app, RtviSyncedSetting::LlmProvider, "auto".to_string());
    let recent_history_entries = history_storage
        .get_all(Some(MEMORY_SYNC_HISTORY_LIMIT))
        .map_err(|error| format!("{error:#}"))?;
    if recent_history_entries.is_empty() {
        return Ok(());
    }

    let existing_memory_markdown = match memory_storage.read_memory_markdown() {
        Ok(memory_markdown) => Some(memory_markdown),
        Err(error) => {
            log::warn!(
                "Failed to read existing memory markdown before sync; continuing without it: {error:#}"
            );
            None
        }
    };

    let history_entry_payloads = recent_history_entries
        .into_iter()
        .map(to_memory_sync_history_entry_payload)
        .collect();

    let generated_memory_markdown = request_memory_markdown_generation(
        config_sync,
        llm_provider,
        history_entry_payloads,
        existing_memory_markdown,
    )
    .await
    .map_err(|error| format!("{error:#}"))?;

    let is_memory_still_enabled: bool =
        get_setting_from_store(app, LocalOnlySetting::MemoryEnabled, false);
    if !is_memory_still_enabled {
        return Ok(());
    }

    memory_storage
        .write_memory_markdown(generated_memory_markdown)
        .map_err(|error| format!("{error:#}"))?;

    let next_completed_session_counter =
        calculate_completed_session_counter_after_successful_sync(completed_session_counter);
    set_memory_sync_completed_session_counter(app, next_completed_session_counter)
        .map_err(|error| format!("{error:#}"))?;

    Ok(())
}

#[cfg(desktop)]
pub(crate) async fn run_memory_sync_if_due_from_app_handle(app: &AppHandle) -> Result<(), String> {
    let history_storage = app.state::<HistoryStorage>();
    let memory_storage = app.state::<MemoryStorage>();
    let config_sync = app.state::<ConfigSync>();

    run_memory_sync_if_due_with_dependencies(
        app,
        history_storage.inner(),
        memory_storage.inner(),
        config_sync.inner(),
    )
    .await
}

/// Run one serialized memory sync attempt after a completed dictation session.
#[cfg(desktop)]
#[tauri::command]
pub async fn run_memory_sync_if_due(app: AppHandle) -> Result<(), String> {
    run_memory_sync_if_due_from_app_handle(&app).await
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn run_memory_sync_if_due(_app: AppHandle) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        calculate_completed_session_counter_after_successful_sync, MEMORY_SYNC_SESSION_INTERVAL,
    };

    #[test]
    fn clears_counter_when_exactly_on_interval_boundary() {
        let completed_session_counter = MEMORY_SYNC_SESSION_INTERVAL;

        let next_completed_session_counter =
            calculate_completed_session_counter_after_successful_sync(completed_session_counter);

        assert_eq!(next_completed_session_counter, 0);
    }

    #[test]
    fn preserves_only_partial_interval_progress_after_success() {
        let completed_session_counter = MEMORY_SYNC_SESSION_INTERVAL * 4 + 1;

        let next_completed_session_counter =
            calculate_completed_session_counter_after_successful_sync(completed_session_counter);

        assert_eq!(next_completed_session_counter, 1);
    }

    #[test]
    fn next_counter_is_always_below_sync_interval() {
        let completed_session_counter = u64::MAX;

        let next_completed_session_counter =
            calculate_completed_session_counter_after_successful_sync(completed_session_counter);

        assert!(next_completed_session_counter < MEMORY_SYNC_SESSION_INTERVAL);
    }
}
