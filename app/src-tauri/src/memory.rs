use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::RwLock;
use tempfile::NamedTempFile;
use tokio::sync::mpsc;

const MEMORY_FILE_NAME: &str = "user-memory.md";
const MEMORY_BACKUP_FILE_NAME: &str = "user-memory.backup.md";

const REQUIRED_MEMORY_SECTION_HEADERS: [&str; 7] = [
    "## Long-Term Signals",
    "## Ongoing Context",
    "## Active Threads",
    "## Recent Entities",
    "## Observed Recurring Phrases",
    "## Do Not Store",
    "## Metadata",
];

const DEFAULT_MEMORY_CONTENT: &str = r"# User Memory

## Long-Term Signals
- _None yet._

## Ongoing Context
- _No active context yet._

## Active Threads
- _No active threads yet._

## Recent Entities
- _No recurring entities yet._

## Observed Recurring Phrases
- _No recurring phrases yet._

## Do Not Store
- Passwords, API keys, one-time codes, personal identifiers, protected health information.

## Metadata
- Version: 1
- Last Updated: _Never_
";

/// Manages the local markdown memory file and backup lifecycle.
pub struct MemoryStorage {
    memory_file_path: PathBuf,
    memory_backup_file_path: PathBuf,
    latest_memory_markdown: RwLock<String>,
}

/// Shared sender used by commands to queue memory sync attempts.
pub struct MemorySyncTriggerQueue {
    memory_sync_trigger_sender: mpsc::UnboundedSender<()>,
}

impl MemorySyncTriggerQueue {
    fn new(memory_sync_trigger_sender: mpsc::UnboundedSender<()>) -> Self {
        Self {
            memory_sync_trigger_sender,
        }
    }

    pub fn enqueue_memory_sync_trigger(
        &self,
    ) -> std::result::Result<(), mpsc::error::SendError<()>> {
        self.memory_sync_trigger_sender.send(())
    }
}

pub fn new_memory_sync_trigger_queue() -> (MemorySyncTriggerQueue, mpsc::UnboundedReceiver<()>) {
    let (memory_sync_trigger_sender, memory_sync_trigger_receiver) = mpsc::unbounded_channel();
    (
        MemorySyncTriggerQueue::new(memory_sync_trigger_sender),
        memory_sync_trigger_receiver,
    )
}

impl MemoryStorage {
    /// Create storage under the app data directory.
    pub fn new(app_data_dir: PathBuf) -> Self {
        let memory_file_path = app_data_dir.join(MEMORY_FILE_NAME);
        let memory_backup_file_path = app_data_dir.join(MEMORY_BACKUP_FILE_NAME);
        Self {
            memory_file_path,
            memory_backup_file_path,
            latest_memory_markdown: RwLock::new(String::new()),
        }
    }

    /// Ensure the memory file exists and in-memory cache is initialized.
    pub fn init(&self) -> Result<()> {
        self.ensure_parent_directory_exists()?;

        if self.memory_file_path.exists() {
            let existing_memory_markdown = fs::read_to_string(&self.memory_file_path)
                .with_context(|| {
                    format!(
                        "Failed to read memory file {}",
                        self.memory_file_path.display()
                    )
                })?;
            self.replace_cached_markdown(existing_memory_markdown)?;
            return Ok(());
        }

        self.write_memory_markdown(DEFAULT_MEMORY_CONTENT.to_string())
    }

    /// Load existing memory content into cache if the file already exists.
    pub fn hydrate_cache_from_disk_if_present(&self) -> Result<()> {
        self.ensure_parent_directory_exists()?;

        if !self.memory_file_path.exists() {
            self.replace_cached_markdown(String::new())?;
            return Ok(());
        }

        let existing_memory_markdown =
            fs::read_to_string(&self.memory_file_path).with_context(|| {
                format!(
                    "Failed to read memory file {}",
                    self.memory_file_path.display()
                )
            })?;
        self.replace_cached_markdown(existing_memory_markdown)
    }

    /// Read the most recent memory markdown from cache.
    pub fn read_memory_markdown(&self) -> Result<String> {
        self.hydrate_cache_from_disk_if_present()?;

        if !self.memory_file_path.exists() {
            return Err(anyhow::anyhow!(
                "Memory file is not initialized. Enable memory to create it."
            ));
        }

        let cached_memory_markdown = self.latest_memory_markdown.read().map_err(|error| {
            anyhow::anyhow!("Failed to acquire memory read lock while reading cache: {error}")
        })?;
        Ok(cached_memory_markdown.clone())
    }

    /// Replace the memory markdown with backup+atomic write semantics.
    pub fn write_memory_markdown(&self, new_memory_markdown: String) -> Result<()> {
        validate_memory_markdown_content(&new_memory_markdown)?;
        self.ensure_parent_directory_exists()?;

        if self.memory_file_path.exists() {
            fs::copy(&self.memory_file_path, &self.memory_backup_file_path).with_context(|| {
                format!(
                    "Failed to backup memory file from {} to {}",
                    self.memory_file_path.display(),
                    self.memory_backup_file_path.display()
                )
            })?;
        }

        self.persist_memory_markdown_atomically(&new_memory_markdown)?;
        self.replace_cached_markdown(new_memory_markdown)?;
        Ok(())
    }

    /// Remove persisted memory artifacts and clear in-memory cache.
    pub fn clear_persisted_memory_artifacts(&self) -> Result<()> {
        let memory_artifact_file_paths = [&self.memory_file_path, &self.memory_backup_file_path];

        for memory_artifact_file_path in memory_artifact_file_paths {
            if memory_artifact_file_path.exists() {
                fs::remove_file(memory_artifact_file_path).with_context(|| {
                    format!(
                        "Failed to remove memory artifact file {}",
                        memory_artifact_file_path.display()
                    )
                })?;
            }
        }

        self.replace_cached_markdown(String::new())?;
        Ok(())
    }

    fn ensure_parent_directory_exists(&self) -> Result<()> {
        let memory_directory_path = self
            .memory_file_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Memory file path has no parent directory"))?;
        fs::create_dir_all(memory_directory_path).with_context(|| {
            format!(
                "Failed to ensure memory directory exists at {}",
                memory_directory_path.display()
            )
        })
    }

    fn persist_memory_markdown_atomically(&self, memory_markdown: &str) -> Result<()> {
        let memory_directory_path = self
            .memory_file_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Memory file path has no parent directory"))?;

        let mut temporary_memory_file =
            NamedTempFile::new_in(memory_directory_path).with_context(|| {
                format!(
                    "Failed to create temporary memory file in {}",
                    memory_directory_path.display()
                )
            })?;

        temporary_memory_file
            .write_all(memory_markdown.as_bytes())
            .with_context(|| {
                format!(
                    "Failed to write temporary memory file for {}",
                    self.memory_file_path.display()
                )
            })?;

        temporary_memory_file
            .as_file()
            .sync_all()
            .with_context(|| {
                format!(
                    "Failed to sync temporary memory file for {}",
                    self.memory_file_path.display()
                )
            })?;

        let persisted_memory_file = temporary_memory_file
            .persist(&self.memory_file_path)
            .map_err(|persist_error| persist_error.error)
            .with_context(|| {
                format!(
                    "Failed to atomically replace memory file {}",
                    self.memory_file_path.display()
                )
            })?;

        persisted_memory_file.sync_all().with_context(|| {
            format!(
                "Failed to sync persisted memory file {}",
                self.memory_file_path.display()
            )
        })?;

        Ok(())
    }

    fn replace_cached_markdown(&self, new_memory_markdown: String) -> Result<()> {
        let mut cached_memory_markdown = self.latest_memory_markdown.write().map_err(|error| {
            anyhow::anyhow!("Failed to acquire memory write lock while updating cache: {error}")
        })?;
        *cached_memory_markdown = new_memory_markdown;
        Ok(())
    }
}

fn validate_memory_markdown_content(memory_markdown: &str) -> Result<()> {
    let trimmed_memory_markdown = memory_markdown.trim();

    if trimmed_memory_markdown.is_empty() {
        return Err(anyhow::anyhow!(
            "Memory markdown cannot be empty; expected required sections"
        ));
    }

    if trimmed_memory_markdown.starts_with("---") {
        return Err(anyhow::anyhow!(
            "Memory markdown must not start with YAML front matter"
        ));
    }

    for required_section_header in REQUIRED_MEMORY_SECTION_HEADERS {
        if !trimmed_memory_markdown.contains(required_section_header) {
            return Err(anyhow::anyhow!(
                "Memory markdown is missing required section '{required_section_header}'"
            ));
        }
    }

    Ok(())
}
