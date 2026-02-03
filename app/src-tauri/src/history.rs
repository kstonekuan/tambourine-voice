use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;
use uuid::Uuid;

const MAX_HISTORY_ENTRIES: usize = 500;

/// Strategy for importing history entries
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryImportStrategy {
    /// Replace all existing entries with imported ones
    Replace,
    /// Append imported entries to existing ones (imported entries first/newer)
    MergeAppend,
    /// Merge but skip entries with matching IDs
    MergeDeduplicate,
}

/// Result of a history import operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryImportResult {
    pub success: bool,
    pub entries_imported: Option<usize>,
    pub entries_skipped: Option<usize>,
}

/// A single dictation history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub text: String,
    #[serde(default)]
    pub raw_text: String,
}

impl HistoryEntry {
    pub fn new(text: String, raw_text: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            text,
            raw_text,
        }
    }
}

/// Storage for dictation history entries
#[derive(Debug, Serialize, Deserialize, Default)]
struct HistoryData {
    entries: Vec<HistoryEntry>,
}

/// Manages loading and saving of dictation history
pub struct HistoryStorage {
    data: RwLock<HistoryData>,
    file_path: PathBuf,
}

impl HistoryStorage {
    /// Create a new history storage with the given app data directory
    pub fn new(app_data_dir: PathBuf) -> Self {
        let file_path = app_data_dir.join("history.json");

        if let Some(parent) = file_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let data = Self::load_from_file(&file_path).unwrap_or_default();

        Self {
            data: RwLock::new(data),
            file_path,
        }
    }

    /// Load history from the JSON file
    fn load_from_file(file_path: &PathBuf) -> Option<HistoryData> {
        let content = fs::read_to_string(file_path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Save current history to disk
    fn save(&self) -> Result<(), String> {
        let data = self
            .data
            .read()
            .map_err(|e| format!("Failed to read history: {e}"))?;

        let content = serde_json::to_string_pretty(&*data)
            .map_err(|e| format!("Failed to serialize history: {e}"))?;

        fs::write(&self.file_path, content)
            .map_err(|e| format!("Failed to write history file: {e}"))?;

        Ok(())
    }

    /// Add a new entry to the history
    pub fn add_entry(&self, text: String, raw_text: String) -> Result<HistoryEntry, String> {
        let entry = HistoryEntry::new(text, raw_text);
        {
            let mut data = self
                .data
                .write()
                .map_err(|e| format!("Failed to write history: {e}"))?;

            data.entries.insert(0, entry.clone());

            if data.entries.len() > MAX_HISTORY_ENTRIES {
                data.entries.truncate(MAX_HISTORY_ENTRIES);
            }
        }
        self.save()?;
        Ok(entry)
    }

    /// Get all history entries (newest first), optionally limited
    pub fn get_all(&self, limit: Option<usize>) -> Result<Vec<HistoryEntry>, String> {
        let data = self
            .data
            .read()
            .map_err(|e| format!("Failed to read history: {e}"))?;

        let entries = match limit {
            Some(n) => data.entries.iter().take(n).cloned().collect(),
            None => data.entries.clone(),
        };

        Ok(entries)
    }

    /// Delete an entry by ID
    pub fn delete(&self, id: &str) -> Result<bool, String> {
        let deleted = {
            let mut data = self
                .data
                .write()
                .map_err(|e| format!("Failed to write history: {e}"))?;

            let initial_len = data.entries.len();
            data.entries.retain(|e| e.id != id);
            data.entries.len() < initial_len
        };

        if deleted {
            self.save()?;
        }

        Ok(deleted)
    }

    /// Clear all history
    pub fn clear(&self) -> Result<(), String> {
        {
            let mut data = self
                .data
                .write()
                .map_err(|e| format!("Failed to write history: {e}"))?;
            data.entries.clear();
        }
        self.save()
    }

    /// Import entries with the specified strategy
    pub fn import_entries(
        &self,
        mut entries: Vec<HistoryEntry>,
        strategy: HistoryImportStrategy,
    ) -> Result<HistoryImportResult, String> {
        let imported_count;
        let skipped_count;

        {
            let mut data = self
                .data
                .write()
                .map_err(|e| format!("Failed to write history: {e}"))?;

            match strategy {
                HistoryImportStrategy::Replace => {
                    // Sort imported entries by timestamp (newest first)
                    entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
                    imported_count = entries.len();
                    skipped_count = 0;
                    data.entries = entries;
                }
                HistoryImportStrategy::MergeAppend => {
                    // Prepend imported entries (imported are considered newer)
                    // Sort imported entries by timestamp (newest first)
                    entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
                    imported_count = entries.len();
                    skipped_count = 0;

                    // Prepend imported entries to existing
                    let mut combined = entries;
                    combined.append(&mut data.entries);
                    data.entries = combined;
                }
                HistoryImportStrategy::MergeDeduplicate => {
                    // Collect existing IDs
                    let existing_ids: HashSet<String> =
                        data.entries.iter().map(|e| e.id.clone()).collect();

                    // Filter out entries that already exist
                    let new_entries: Vec<HistoryEntry> = entries
                        .into_iter()
                        .filter(|e| !existing_ids.contains(&e.id))
                        .collect();

                    imported_count = new_entries.len();
                    skipped_count = 0; // We'll calculate this from the original count

                    // Prepend new entries
                    let mut combined = new_entries;
                    combined.append(&mut data.entries);

                    // Sort by timestamp (newest first)
                    combined.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
                    data.entries = combined;
                }
            }

            // Truncate to max entries
            if data.entries.len() > MAX_HISTORY_ENTRIES {
                data.entries.truncate(MAX_HISTORY_ENTRIES);
            }
        }

        self.save()?;

        Ok(HistoryImportResult {
            success: true,
            entries_imported: Some(imported_count),
            entries_skipped: Some(skipped_count),
        })
    }
}
