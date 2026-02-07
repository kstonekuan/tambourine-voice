use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
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

        let data = match Self::load_from_file(&file_path) {
            Ok(history_data) => history_data,
            Err(error) => {
                if file_path.exists() {
                    log::warn!(
                        "Failed to load history from {}: {error}",
                        file_path.display()
                    );
                }
                HistoryData::default()
            }
        };

        Self {
            data: RwLock::new(data),
            file_path,
        }
    }

    /// Load history from the JSON file
    fn load_from_file(file_path: &Path) -> Result<HistoryData> {
        let file_content = fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read history file {}", file_path.display()))?;

        serde_json::from_str(&file_content)
            .with_context(|| format!("Failed to parse history file {}", file_path.display()))
    }

    /// Save current history to disk
    fn save(&self) -> Result<()> {
        let history_data = self.data.read().map_err(|error| {
            anyhow::anyhow!("Failed to acquire history read lock for save: {error}")
        })?;

        let serialized_history_content = serde_json::to_string_pretty(&*history_data)
            .context("Failed to serialize history data to JSON")?;

        fs::write(&self.file_path, serialized_history_content).with_context(|| {
            format!("Failed to write history file {}", self.file_path.display())
        })?;

        Ok(())
    }

    /// Add a new entry to the history
    pub fn add_entry(&self, text: String, raw_text: String) -> Result<HistoryEntry> {
        let new_history_entry = HistoryEntry::new(text, raw_text);
        {
            let mut history_data = self.data.write().map_err(|error| {
                anyhow::anyhow!("Failed to acquire history write lock when adding entry: {error}")
            })?;

            history_data.entries.insert(0, new_history_entry.clone());

            if history_data.entries.len() > MAX_HISTORY_ENTRIES {
                history_data.entries.truncate(MAX_HISTORY_ENTRIES);
            }
        }
        self.save()?;
        Ok(new_history_entry)
    }

    /// Get all history entries (newest first), optionally limited
    pub fn get_all(&self, limit: Option<usize>) -> Result<Vec<HistoryEntry>> {
        let history_data = self.data.read().map_err(|error| {
            anyhow::anyhow!("Failed to acquire history read lock when getting entries: {error}")
        })?;

        let history_entries = match limit {
            Some(entry_limit) => history_data
                .entries
                .iter()
                .take(entry_limit)
                .cloned()
                .collect(),
            None => history_data.entries.clone(),
        };

        Ok(history_entries)
    }

    /// Delete an entry by ID
    pub fn delete(&self, id: &str) -> Result<bool> {
        let deleted = {
            let mut history_data = self.data.write().map_err(|error| {
                anyhow::anyhow!(
                    "Failed to acquire history write lock when deleting entry {id}: {error}"
                )
            })?;

            let initial_entry_count = history_data.entries.len();
            history_data.entries.retain(|entry| entry.id != id);
            history_data.entries.len() < initial_entry_count
        };

        if deleted {
            self.save()?;
        }

        Ok(deleted)
    }

    /// Clear all history
    pub fn clear(&self) -> Result<()> {
        {
            let mut history_data = self.data.write().map_err(|error| {
                anyhow::anyhow!(
                    "Failed to acquire history write lock when clearing history: {error}"
                )
            })?;
            history_data.entries.clear();
        }
        self.save()
    }

    /// Import entries with the specified strategy
    pub fn import_entries(
        &self,
        mut entries: Vec<HistoryEntry>,
        strategy: HistoryImportStrategy,
    ) -> Result<HistoryImportResult> {
        let imported_count;
        let skipped_count;

        {
            let mut history_data = self.data.write().map_err(|error| {
                anyhow::anyhow!(
                    "Failed to acquire history write lock when importing entries: {error}"
                )
            })?;

            match strategy {
                HistoryImportStrategy::Replace => {
                    // Sort imported entries by timestamp (newest first)
                    entries.sort_by(|left_entry, right_entry| {
                        right_entry.timestamp.cmp(&left_entry.timestamp)
                    });
                    imported_count = entries.len();
                    skipped_count = 0;
                    history_data.entries = entries;
                }
                HistoryImportStrategy::MergeAppend => {
                    // Prepend imported entries (imported are considered newer)
                    // Sort imported entries by timestamp (newest first)
                    entries.sort_by(|left_entry, right_entry| {
                        right_entry.timestamp.cmp(&left_entry.timestamp)
                    });
                    imported_count = entries.len();
                    skipped_count = 0;

                    // Prepend imported entries to existing
                    let mut combined_entries = entries;
                    combined_entries.append(&mut history_data.entries);
                    history_data.entries = combined_entries;
                }
                HistoryImportStrategy::MergeDeduplicate => {
                    // Collect existing IDs
                    let existing_entry_ids: HashSet<String> = history_data
                        .entries
                        .iter()
                        .map(|entry| entry.id.clone())
                        .collect();

                    // Filter out entries that already exist
                    let new_entries: Vec<HistoryEntry> = entries
                        .into_iter()
                        .filter(|entry| !existing_entry_ids.contains(&entry.id))
                        .collect();

                    imported_count = new_entries.len();
                    skipped_count = 0; // We'll calculate this from the original count

                    // Prepend new entries
                    let mut combined_entries = new_entries;
                    combined_entries.append(&mut history_data.entries);

                    // Sort by timestamp (newest first)
                    combined_entries.sort_by(|left_entry, right_entry| {
                        right_entry.timestamp.cmp(&left_entry.timestamp)
                    });
                    history_data.entries = combined_entries;
                }
            }

            // Truncate to max entries
            if history_data.entries.len() > MAX_HISTORY_ENTRIES {
                history_data.entries.truncate(MAX_HISTORY_ENTRIES);
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
