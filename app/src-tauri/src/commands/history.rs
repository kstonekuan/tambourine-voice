use crate::active_app_context::ActiveAppContextSnapshot;
use crate::history::{HistoryEntry, HistoryStorage};
use tauri::State;

/// Add a new entry to the dictation history
#[tauri::command]
pub async fn add_history_entry(
    text: String,
    raw_text: String,
    active_app_context: Option<ActiveAppContextSnapshot>,
    history: State<'_, HistoryStorage>,
) -> Result<HistoryEntry, String> {
    history
        .add_entry(text, raw_text, active_app_context)
        .map_err(|error| error.to_string())
}

/// Get dictation history entries
#[tauri::command]
pub async fn get_history(
    limit: Option<usize>,
    history: State<'_, HistoryStorage>,
) -> Result<Vec<HistoryEntry>, String> {
    history.get_all(limit).map_err(|error| error.to_string())
}

/// Delete a history entry by ID
#[tauri::command]
pub async fn delete_history_entry(
    id: String,
    history: State<'_, HistoryStorage>,
) -> Result<bool, String> {
    history.delete(&id).map_err(|error| error.to_string())
}

/// Clear all history entries
#[tauri::command]
pub async fn clear_history(history: State<'_, HistoryStorage>) -> Result<(), String> {
    history.clear().map_err(|error| error.to_string())
}
