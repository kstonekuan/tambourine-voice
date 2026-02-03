use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::{AppHandle, Manager};

use crate::config_sync::{ConfigSync, DEFAULT_STT_TIMEOUT_SECONDS};
use crate::history::{HistoryEntry, HistoryImportResult, HistoryImportStrategy, HistoryStorage};
use crate::settings::{
    AppSettings, CleanupPromptSections, PromptMode, PromptSection, PromptSectionType, StoreKey,
};

#[cfg(desktop)]
use tauri_plugin_store::StoreExt;

// ============================================================================
// EXPORT FILE STRUCTURES
// ============================================================================

/// Current export format version - increment when format changes
const EXPORT_VERSION: u32 = 1;

/// Type identifier for settings export files
const SETTINGS_EXPORT_TYPE: &str = "tambourine-settings";

/// Type identifier for history export files
const HISTORY_EXPORT_TYPE: &str = "tambourine-history";

/// HTML comment prefix for prompt files
const PROMPT_COMMENT_PREFIX: &str = "<!-- tambourine-prompt: ";
const PROMPT_COMMENT_SUFFIX: &str = " -->";

/// Settings data for export (excludes prompts - they're exported as .md files)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsExportData {
    pub toggle_hotkey: crate::settings::HotkeyConfig,
    pub hold_hotkey: crate::settings::HotkeyConfig,
    pub paste_last_hotkey: crate::settings::HotkeyConfig,
    pub selected_mic_id: Option<String>,
    pub sound_enabled: bool,
    // cleanup_prompt_sections is intentionally excluded - exported as .md files
    pub stt_provider: String,
    pub llm_provider: String,
    pub auto_mute_audio: bool,
    pub stt_timeout_seconds: Option<f64>,
    pub server_url: String,
}

impl From<AppSettings> for SettingsExportData {
    fn from(settings: AppSettings) -> Self {
        Self {
            toggle_hotkey: settings.toggle_hotkey,
            hold_hotkey: settings.hold_hotkey,
            paste_last_hotkey: settings.paste_last_hotkey,
            selected_mic_id: settings.selected_mic_id,
            sound_enabled: settings.sound_enabled,
            stt_provider: settings.stt_provider,
            llm_provider: settings.llm_provider,
            auto_mute_audio: settings.auto_mute_audio,
            stt_timeout_seconds: settings.stt_timeout_seconds,
            server_url: settings.server_url,
        }
    }
}

/// Settings export file format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsExportFile {
    #[serde(rename = "type")]
    pub file_type: String,
    pub version: u32,
    pub exported_at: DateTime<Utc>,
    pub data: SettingsExportData,
}

/// History export file format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryExportFile {
    #[serde(rename = "type")]
    pub file_type: String,
    pub version: u32,
    pub exported_at: DateTime<Utc>,
    pub entry_count: usize,
    pub data: Vec<HistoryEntry>,
}

// ============================================================================
// IMPORT RESULT TYPES
// ============================================================================

/// Detected file type from import
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectedFileType {
    Settings,
    History,
    Unknown,
}

// ============================================================================
// HELPER FOR FILE TYPE DETECTION
// ============================================================================

/// Minimal struct to detect file type without full parsing
#[derive(Debug, Deserialize)]
struct FileTypeProbe {
    #[serde(rename = "type")]
    file_type: Option<String>,
    version: Option<u32>,
}

// ============================================================================
// COMMANDS
// ============================================================================

/// Generate settings export JSON string (excludes prompts - they're exported as .md files)
#[cfg(desktop)]
#[tauri::command]
pub fn generate_settings_export(app: AppHandle) -> Result<String, String> {
    use super::settings::get_settings;

    let settings = get_settings(app)?;
    let export_data: SettingsExportData = settings.into();

    let export = SettingsExportFile {
        file_type: SETTINGS_EXPORT_TYPE.to_string(),
        version: EXPORT_VERSION,
        exported_at: Utc::now(),
        data: export_data,
    };

    serde_json::to_string_pretty(&export).map_err(|e| format!("Failed to serialize settings: {e}"))
}

#[cfg(not(desktop))]
#[tauri::command]
pub fn generate_settings_export(_app: AppHandle) -> Result<String, String> {
    Err("Not supported on this platform".to_string())
}

/// Generate history export JSON string
#[tauri::command]
pub fn generate_history_export(app: AppHandle) -> Result<String, String> {
    let history_storage = app.state::<HistoryStorage>();
    let entries = history_storage
        .get_all(None)
        .map_err(|e| format!("Failed to get history: {e}"))?;

    let export = HistoryExportFile {
        file_type: HISTORY_EXPORT_TYPE.to_string(),
        version: EXPORT_VERSION,
        exported_at: Utc::now(),
        entry_count: entries.len(),
        data: entries,
    };

    serde_json::to_string_pretty(&export).map_err(|e| format!("Failed to serialize history: {e}"))
}

/// Generate prompt exports as markdown content with HTML comment headers.
/// Returns a `HashMap` of section name -> markdown content (always 3 files with state markers).
#[cfg(desktop)]
#[tauri::command]
pub fn generate_prompt_exports(app: AppHandle) -> Result<HashMap<String, String>, String> {
    use super::settings::get_settings;

    let settings = get_settings(app)?;
    let mut prompts = HashMap::new();

    if let Some(sections) = settings.cleanup_prompt_sections {
        let format_prompt = |section_name: &str, section: &PromptSection| -> String {
            let mode_str = match &section.prompt_mode {
                PromptMode::Auto => "auto",
                PromptMode::Manual { .. } => "manual",
            };

            let content = match &section.prompt_mode {
                PromptMode::Auto => String::new(),
                PromptMode::Manual { content } => content.clone(),
            };

            format!(
                "{}{}{}\nenabled: {}\nmode: {}\n---\n{}",
                PROMPT_COMMENT_PREFIX,
                section_name,
                PROMPT_COMMENT_SUFFIX,
                section.enabled,
                mode_str,
                content
            )
        };

        for section_type in PromptSectionType::ALL {
            prompts.insert(
                section_type.as_str().to_string(),
                format_prompt(section_type.as_str(), sections.get(section_type)),
            );
        }
    }

    Ok(prompts)
}

#[cfg(not(desktop))]
#[tauri::command]
pub fn generate_prompt_exports(_app: AppHandle) -> Result<HashMap<String, String>, String> {
    Ok(HashMap::new())
}

/// Parse a prompt file content and extract the section name from the HTML comment.
/// Returns (`section_name`, content) if valid, or an error message.
#[tauri::command]
pub fn parse_prompt_file(content: String) -> Result<(String, String), String> {
    let trimmed = content.trim();

    // Check for HTML comment header
    if !trimmed.starts_with(PROMPT_COMMENT_PREFIX) {
        return Err("Not a valid prompt file: missing header comment".to_string());
    }

    // Find the end of the comment
    let after_prefix = &trimmed[PROMPT_COMMENT_PREFIX.len()..];
    let suffix_pos = after_prefix
        .find(PROMPT_COMMENT_SUFFIX)
        .ok_or("Not a valid prompt file: malformed header comment")?;

    let section_name = after_prefix[..suffix_pos].trim();

    // Validate section name by parsing as PromptSectionType
    section_name.parse::<PromptSectionType>()?;
    let section_name = section_name.to_string();

    // Extract content after the comment
    let content_start = PROMPT_COMMENT_PREFIX.len() + suffix_pos + PROMPT_COMMENT_SUFFIX.len();
    let prompt_content = trimmed[content_start..].trim().to_string();

    Ok((section_name, prompt_content))
}

/// Import a prompt into the specified section.
#[cfg(desktop)]
#[tauri::command]
pub async fn import_prompt(
    app: AppHandle,
    section: String,
    content: String,
    config_sync: tauri::State<'_, ConfigSync>,
) -> Result<(), String> {
    use super::settings::get_setting_from_store;

    // Validate and parse section name
    let section_type: PromptSectionType = section.parse()?;

    // Get current prompt sections or use default
    let mut sections: CleanupPromptSections = get_setting_from_store(
        &app,
        StoreKey::CleanupPromptSections,
        CleanupPromptSections::default(),
    );

    let lines: Vec<&str> = content.lines().collect();

    let enabled = lines
        .iter()
        .find(|line| line.starts_with("enabled:"))
        .and_then(|line| line.strip_prefix("enabled:"))
        .is_none_or(|s| s.trim() == "true");

    let mode = lines
        .iter()
        .find(|line| line.starts_with("mode:"))
        .and_then(|line| line.strip_prefix("mode:"))
        .map_or("auto", str::trim);

    let content_start = lines.iter().position(|line| line.trim() == "---");
    let prompt_content = if let Some(idx) = content_start {
        lines[idx + 1..].join("\n")
    } else {
        content.clone()
    };

    let prompt_mode = if mode == "manual" {
        PromptMode::Manual {
            content: prompt_content,
        }
    } else {
        PromptMode::Auto
    };

    let new_section = PromptSection {
        enabled,
        prompt_mode,
    };

    sections.set(section_type, new_section);

    // Save updated sections
    crate::save_setting_to_store(&app, StoreKey::CleanupPromptSections, &sections)?;

    // Sync to server if connected
    let sync = config_sync.read().await;
    if sync.is_connected() {
        if let Err(e) = sync.sync_prompt_sections(&sections).await {
            log::warn!("Failed to sync prompt after import: {e}");
        }
    }

    log::info!("Imported prompt for section: {}", section_type.as_str());
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn import_prompt(
    _app: AppHandle,
    _section: String,
    _content: String,
    _config_sync: tauri::State<'_, ConfigSync>,
) -> Result<(), String> {
    Err("Not supported on this platform".to_string())
}

/// Detect the type of an export file from its content
#[tauri::command]
pub fn detect_export_file_type(content: String) -> DetectedFileType {
    match serde_json::from_str::<FileTypeProbe>(&content) {
        Ok(probe) => match probe.file_type.as_deref() {
            Some(SETTINGS_EXPORT_TYPE) => {
                if probe.version.is_some_and(|v| v <= EXPORT_VERSION) {
                    DetectedFileType::Settings
                } else {
                    log::warn!(
                        "Settings file version {} is newer than supported version {}",
                        probe.version.unwrap_or(0),
                        EXPORT_VERSION
                    );
                    DetectedFileType::Unknown
                }
            }
            Some(HISTORY_EXPORT_TYPE) => {
                if probe.version.is_some_and(|v| v <= EXPORT_VERSION) {
                    DetectedFileType::History
                } else {
                    log::warn!(
                        "History file version {} is newer than supported version {}",
                        probe.version.unwrap_or(0),
                        EXPORT_VERSION
                    );
                    DetectedFileType::Unknown
                }
            }
            _ => {
                log::warn!("Unknown file type: {:?}", probe.file_type);
                DetectedFileType::Unknown
            }
        },
        Err(e) => {
            log::warn!("Failed to parse file type: {e}");
            DetectedFileType::Unknown
        }
    }
}

/// Import settings from a JSON string
#[cfg(desktop)]
#[tauri::command]
pub async fn import_settings(
    app: AppHandle,
    content: String,
    config_sync: tauri::State<'_, ConfigSync>,
) -> Result<(), String> {
    // Parse the export file
    let export: SettingsExportFile = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse settings file: {e}"))?;

    // Validate file type
    if export.file_type != SETTINGS_EXPORT_TYPE {
        return Err(format!(
            "Invalid file type: expected '{}', got '{}'",
            SETTINGS_EXPORT_TYPE, export.file_type
        ));
    }

    // Validate version
    if export.version > EXPORT_VERSION {
        return Err(format!(
            "Unsupported version: file is version {}, max supported is {}",
            export.version, EXPORT_VERSION
        ));
    }

    // Get store
    let store = app
        .store("settings.json")
        .map_err(|e| format!("Failed to get store: {e}"))?;

    // Import each setting
    let settings = export.data;

    // Save each setting individually so we can handle defaults properly
    store.set(
        StoreKey::ToggleHotkey.as_str(),
        serde_json::to_value(&settings.toggle_hotkey).map_err(|e| e.to_string())?,
    );
    store.set(
        StoreKey::HoldHotkey.as_str(),
        serde_json::to_value(&settings.hold_hotkey).map_err(|e| e.to_string())?,
    );
    store.set(
        StoreKey::PasteLastHotkey.as_str(),
        serde_json::to_value(&settings.paste_last_hotkey).map_err(|e| e.to_string())?,
    );
    store.set(
        StoreKey::SelectedMicId.as_str(),
        serde_json::to_value(&settings.selected_mic_id).map_err(|e| e.to_string())?,
    );
    store.set(
        StoreKey::SoundEnabled.as_str(),
        serde_json::to_value(settings.sound_enabled).map_err(|e| e.to_string())?,
    );
    // Note: cleanup_prompt_sections is not imported here - prompts come from .md files
    store.set(
        StoreKey::SttProvider.as_str(),
        serde_json::to_value(&settings.stt_provider).map_err(|e| e.to_string())?,
    );
    store.set(
        StoreKey::LlmProvider.as_str(),
        serde_json::to_value(&settings.llm_provider).map_err(|e| e.to_string())?,
    );
    store.set(
        StoreKey::AutoMuteAudio.as_str(),
        serde_json::to_value(settings.auto_mute_audio).map_err(|e| e.to_string())?,
    );
    store.set(
        StoreKey::SttTimeoutSeconds.as_str(),
        serde_json::to_value(settings.stt_timeout_seconds).map_err(|e| e.to_string())?,
    );
    store.set(
        StoreKey::ServerUrl.as_str(),
        serde_json::to_value(&settings.server_url).map_err(|e| e.to_string())?,
    );

    store
        .save()
        .map_err(|e| format!("Failed to save settings: {e}"))?;

    log::info!("Successfully imported settings from export file");

    let sync = config_sync.read().await;
    if sync.is_connected() {
        if let Some(timeout) = settings.stt_timeout_seconds {
            if let Err(e) = sync.sync_stt_timeout(timeout).await {
                log::warn!("Failed to sync STT timeout after import: {e}");
            }
        }
    }

    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn import_settings(
    _app: AppHandle,
    _content: String,
    _config_sync: tauri::State<'_, ConfigSync>,
) -> Result<(), String> {
    Err("Not supported on this platform".to_string())
}

/// Import history from a JSON string with the specified merge strategy
#[tauri::command]
pub fn import_history(
    app: AppHandle,
    content: String,
    strategy: HistoryImportStrategy,
) -> Result<HistoryImportResult, String> {
    // Parse the export file
    let export: HistoryExportFile =
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse history file: {e}"))?;

    // Validate file type
    if export.file_type != HISTORY_EXPORT_TYPE {
        return Err(format!(
            "Invalid file type: expected '{}', got '{}'",
            HISTORY_EXPORT_TYPE, export.file_type
        ));
    }

    // Validate version
    if export.version > EXPORT_VERSION {
        return Err(format!(
            "Unsupported version: file is version {}, max supported is {}",
            export.version, EXPORT_VERSION
        ));
    }

    let history_storage = app.state::<HistoryStorage>();
    let result = history_storage.import_entries(export.data, strategy)?;

    log::info!(
        "Imported history: {} entries imported, {} skipped (strategy: {:?})",
        result.entries_imported.unwrap_or(0),
        result.entries_skipped.unwrap_or(0),
        strategy
    );

    Ok(result)
}

/// Factory reset: clears all settings and history
#[cfg(desktop)]
#[tauri::command]
pub async fn factory_reset(
    app: AppHandle,
    config_sync: tauri::State<'_, ConfigSync>,
) -> Result<(), String> {
    // Clear the settings store completely
    let store = app
        .store("settings.json")
        .map_err(|e| format!("Failed to get store: {e}"))?;

    store.clear();
    store
        .save()
        .map_err(|e| format!("Failed to save cleared store: {e}"))?;

    // Clear history
    let history_storage = app.state::<HistoryStorage>();
    history_storage.clear()?;

    // Re-initialize with default settings
    let default_settings = AppSettings::default();

    store.set(
        StoreKey::ToggleHotkey.as_str(),
        serde_json::to_value(&default_settings.toggle_hotkey).map_err(|e| e.to_string())?,
    );
    store.set(
        StoreKey::HoldHotkey.as_str(),
        serde_json::to_value(&default_settings.hold_hotkey).map_err(|e| e.to_string())?,
    );
    store.set(
        StoreKey::PasteLastHotkey.as_str(),
        serde_json::to_value(&default_settings.paste_last_hotkey).map_err(|e| e.to_string())?,
    );
    store.set(
        StoreKey::SoundEnabled.as_str(),
        serde_json::to_value(default_settings.sound_enabled).map_err(|e| e.to_string())?,
    );
    store.set(
        StoreKey::SttProvider.as_str(),
        serde_json::to_value(&default_settings.stt_provider).map_err(|e| e.to_string())?,
    );
    store.set(
        StoreKey::LlmProvider.as_str(),
        serde_json::to_value(&default_settings.llm_provider).map_err(|e| e.to_string())?,
    );
    store.set(
        StoreKey::AutoMuteAudio.as_str(),
        serde_json::to_value(default_settings.auto_mute_audio).map_err(|e| e.to_string())?,
    );
    store.set(
        StoreKey::ServerUrl.as_str(),
        serde_json::to_value(&default_settings.server_url).map_err(|e| e.to_string())?,
    );

    store
        .save()
        .map_err(|e| format!("Failed to save default settings: {e}"))?;

    // Sync defaults to server if connected
    let sync = config_sync.read().await;
    if sync.is_connected() {
        // Reset prompts to default mode
        let default_sections = CleanupPromptSections::default();
        if let Err(e) = sync.sync_prompt_sections(&default_sections).await {
            log::warn!("Failed to sync prompts on factory reset: {e}");
        }

        // Reset STT timeout to default
        if let Err(e) = sync.sync_stt_timeout(DEFAULT_STT_TIMEOUT_SECONDS).await {
            log::warn!("Failed to sync STT timeout on factory reset: {e}");
        }
    }

    log::info!("Factory reset completed: settings and history cleared");

    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn factory_reset(
    _app: AppHandle,
    _config_sync: tauri::State<'_, ConfigSync>,
) -> Result<(), String> {
    Err("Not supported on this platform".to_string())
}
