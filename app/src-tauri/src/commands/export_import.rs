use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::{AppHandle, Manager};

use crate::config_sync::{ConfigSync, DEFAULT_STT_TIMEOUT_SECONDS};
use crate::history::{HistoryEntry, HistoryImportResult, HistoryImportStrategy, HistoryStorage};
use crate::settings::{
    AppSettings, CleanupPromptSections, HttpSyncedSetting, LocalOnlySetting, PromptMode,
    PromptSection, PromptSectionType, RtviSyncedSetting, SettingClass,
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
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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
    pub llm_formatting_enabled: bool,
    pub server_url: String,
    pub send_active_app_context_enabled: bool,
}

impl Default for SettingsExportData {
    fn default() -> Self {
        AppSettings::default().into()
    }
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
            llm_formatting_enabled: settings.llm_formatting_enabled,
            server_url: settings.server_url,
            send_active_app_context_enabled: settings.send_active_app_context_enabled,
        }
    }
}

impl From<SettingsExportData> for AppSettings {
    fn from(exported_settings: SettingsExportData) -> Self {
        Self {
            toggle_hotkey: exported_settings.toggle_hotkey,
            hold_hotkey: exported_settings.hold_hotkey,
            paste_last_hotkey: exported_settings.paste_last_hotkey,
            selected_mic_id: exported_settings.selected_mic_id,
            sound_enabled: exported_settings.sound_enabled,
            // Prompts are imported from prompt markdown files, not the JSON settings file.
            cleanup_prompt_sections: None,
            stt_provider: exported_settings.stt_provider,
            llm_provider: exported_settings.llm_provider,
            auto_mute_audio: exported_settings.auto_mute_audio,
            stt_timeout_seconds: exported_settings.stt_timeout_seconds,
            llm_formatting_enabled: exported_settings.llm_formatting_enabled,
            server_url: exported_settings.server_url,
            send_active_app_context_enabled: exported_settings.send_active_app_context_enabled,
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

/// Warning from best-effort runtime setting application.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeApplyWarningCode {
    #[serde(rename = "focus_watcher_reconcile_failed")]
    FocusWatcherReconcile,
    #[serde(rename = "prompt_sections_sync_failed")]
    PromptSectionsSync,
    #[serde(rename = "stt_timeout_sync_failed")]
    SttTimeoutSync,
    #[serde(rename = "llm_formatting_sync_failed")]
    LlmFormattingSync,
}

/// Runtime action that was successfully applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeApplyAction {
    FocusWatcherEnabled,
    FocusWatcherDisabled,
    PromptSectionsSynced,
    SttTimeoutSynced,
    LlmFormattingSynced,
}

/// Warning from best-effort runtime setting application.
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeApplyWarning {
    pub code: RuntimeApplyWarningCode,
    pub message: String,
    #[serde(serialize_with = "serialize_setting_class_as_storage_key_name")]
    pub setting_key: SettingClass,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeActionApplied {
    pub action: RuntimeApplyAction,
    #[serde(serialize_with = "serialize_setting_class_as_storage_key_name")]
    pub setting_key: SettingClass,
}

/// Runtime application summary returned by import/reset commands.
#[derive(Debug, Clone, Serialize, Default)]
pub struct RuntimeApplyOutcome {
    pub warnings: Vec<RuntimeApplyWarning>,
    pub runtime_actions_applied: Vec<RuntimeActionApplied>,
}

pub type ImportSettingsOutcome = RuntimeApplyOutcome;
pub type FactoryResetOutcome = RuntimeApplyOutcome;

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
pub fn generate_prompt_exports(
    app: AppHandle,
) -> Result<HashMap<PromptSectionType, String>, String> {
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
                section_type,
                format_prompt(section_type.as_str(), sections.get(section_type)),
            );
        }
    }

    Ok(prompts)
}

#[cfg(not(desktop))]
#[tauri::command]
pub fn generate_prompt_exports(
    _app: AppHandle,
) -> Result<HashMap<PromptSectionType, String>, String> {
    Ok(HashMap::new())
}

/// Parse a prompt file content and extract the section name from the HTML comment.
/// Returns (`section_name`, content) if valid, or an error message.
#[tauri::command]
pub fn parse_prompt_file(content: String) -> Result<(PromptSectionType, String), String> {
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
    let section_type = section_name.parse::<PromptSectionType>()?;

    // Extract content after the comment
    let content_start = PROMPT_COMMENT_PREFIX.len() + suffix_pos + PROMPT_COMMENT_SUFFIX.len();
    let prompt_content = trimmed[content_start..].trim().to_string();

    Ok((section_type, prompt_content))
}

/// Import a prompt into the specified section.
#[cfg(desktop)]
#[tauri::command]
pub async fn import_prompt(
    app: AppHandle,
    section: PromptSectionType,
    content: String,
    config_sync: tauri::State<'_, ConfigSync>,
) -> Result<(), String> {
    use super::settings::get_setting_from_store;

    // Get current prompt sections or use default
    let mut sections: CleanupPromptSections = get_setting_from_store(
        &app,
        HttpSyncedSetting::CleanupPromptSections,
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

    sections.set(section, new_section);

    // Save updated sections
    crate::save_setting_to_store(
        &app,
        HttpSyncedSetting::CleanupPromptSections.into(),
        &sections,
    )
    .map_err(|error| format!("Failed to save imported prompt section: {error:#}"))?;

    // Sync to server if connected
    let sync = config_sync.read().await;
    if sync.is_connected() {
        if let Err(e) = sync.sync_prompt_sections(&sections).await {
            log::warn!("Failed to sync prompt after import: {e}");
        }
    }

    log::info!("Imported prompt for section: {}", section.as_str());
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn import_prompt(
    _app: AppHandle,
    _section: PromptSectionType,
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

// ============================================================================
// SETTINGS IMPORT/RESET STORE MAPPING
// ============================================================================

const IMPORT_EXPORT_SETTING_CLASSES: [SettingClass; 12] = [
    SettingClass::LocalOnly(LocalOnlySetting::ToggleHotkey),
    SettingClass::LocalOnly(LocalOnlySetting::HoldHotkey),
    SettingClass::LocalOnly(LocalOnlySetting::PasteLastHotkey),
    SettingClass::LocalOnly(LocalOnlySetting::SelectedMicId),
    SettingClass::LocalOnly(LocalOnlySetting::SoundEnabled),
    SettingClass::ServerSyncedRtvi(RtviSyncedSetting::SttProvider),
    SettingClass::ServerSyncedRtvi(RtviSyncedSetting::LlmProvider),
    SettingClass::LocalOnly(LocalOnlySetting::AutoMuteAudio),
    SettingClass::ServerSyncedHttp(HttpSyncedSetting::SttTimeoutSeconds),
    SettingClass::ServerSyncedHttp(HttpSyncedSetting::LlmFormattingEnabled),
    SettingClass::LocalOnly(LocalOnlySetting::ServerUrl),
    SettingClass::LocalOnly(LocalOnlySetting::SendActiveAppContextEnabled),
];

const FACTORY_RESET_SETTING_CLASSES: [SettingClass; 9] = [
    SettingClass::LocalOnly(LocalOnlySetting::ToggleHotkey),
    SettingClass::LocalOnly(LocalOnlySetting::HoldHotkey),
    SettingClass::LocalOnly(LocalOnlySetting::PasteLastHotkey),
    SettingClass::LocalOnly(LocalOnlySetting::SoundEnabled),
    SettingClass::ServerSyncedRtvi(RtviSyncedSetting::SttProvider),
    SettingClass::ServerSyncedRtvi(RtviSyncedSetting::LlmProvider),
    SettingClass::LocalOnly(LocalOnlySetting::AutoMuteAudio),
    SettingClass::LocalOnly(LocalOnlySetting::ServerUrl),
    SettingClass::LocalOnly(LocalOnlySetting::SendActiveAppContextEnabled),
];

fn serialized_value_for_setting_class(
    app_settings: &AppSettings,
    setting_class: SettingClass,
) -> anyhow::Result<serde_json::Value> {
    let setting_value = match setting_class {
        SettingClass::LocalOnly(local_only_setting) => match local_only_setting {
            LocalOnlySetting::ToggleHotkey => serde_json::to_value(&app_settings.toggle_hotkey),
            LocalOnlySetting::HoldHotkey => serde_json::to_value(&app_settings.hold_hotkey),
            LocalOnlySetting::PasteLastHotkey => {
                serde_json::to_value(&app_settings.paste_last_hotkey)
            }
            LocalOnlySetting::SelectedMicId => serde_json::to_value(&app_settings.selected_mic_id),
            LocalOnlySetting::SoundEnabled => serde_json::to_value(app_settings.sound_enabled),
            LocalOnlySetting::AutoMuteAudio => serde_json::to_value(app_settings.auto_mute_audio),
            LocalOnlySetting::ServerUrl => serde_json::to_value(&app_settings.server_url),
            LocalOnlySetting::SendActiveAppContextEnabled => {
                serde_json::to_value(app_settings.send_active_app_context_enabled)
            }
        },
        SettingClass::ServerSyncedHttp(http_synced_setting) => match http_synced_setting {
            HttpSyncedSetting::CleanupPromptSections => {
                serde_json::to_value(&app_settings.cleanup_prompt_sections)
            }
            HttpSyncedSetting::SttTimeoutSeconds => {
                serde_json::to_value(app_settings.stt_timeout_seconds)
            }
            HttpSyncedSetting::LlmFormattingEnabled => {
                serde_json::to_value(app_settings.llm_formatting_enabled)
            }
        },
        SettingClass::ServerSyncedRtvi(rtvi_synced_setting) => match rtvi_synced_setting {
            RtviSyncedSetting::SttProvider => serde_json::to_value(&app_settings.stt_provider),
            RtviSyncedSetting::LlmProvider => serde_json::to_value(&app_settings.llm_provider),
        },
    };

    setting_value.with_context(|| {
        format!(
            "Failed to serialize setting value for key '{}'",
            setting_class.storage_key_name()
        )
    })
}

fn write_setting_classes_to_store(
    app_settings: &AppSettings,
    setting_classes: &[SettingClass],
    mut write_setting_entry: impl FnMut(SettingClass, serde_json::Value),
) -> anyhow::Result<()> {
    for setting_class in setting_classes {
        let setting_value = serialized_value_for_setting_class(app_settings, *setting_class)?;
        write_setting_entry(*setting_class, setting_value);
    }
    Ok(())
}

#[cfg(desktop)]
fn apply_runtime_warning(
    code: RuntimeApplyWarningCode,
    setting_key: SettingClass,
    message: String,
) -> RuntimeApplyWarning {
    RuntimeApplyWarning {
        code,
        message,
        setting_key,
    }
}

#[cfg(desktop)]
fn apply_runtime_action(
    action: RuntimeApplyAction,
    setting_key: SettingClass,
) -> RuntimeActionApplied {
    RuntimeActionApplied {
        action,
        setting_key,
    }
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn serialize_setting_class_as_storage_key_name<S>(
    setting_class: &SettingClass,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(setting_class.storage_key_name())
}

#[cfg(desktop)]
async fn apply_runtime_side_effects(
    app: &AppHandle,
    send_active_app_context_enabled: bool,
    llm_formatting_enabled: bool,
    stt_timeout_seconds_to_sync: Option<f64>,
    prompt_sections_to_sync: Option<&CleanupPromptSections>,
    config_sync: &ConfigSync,
) -> RuntimeApplyOutcome {
    let mut runtime_apply_outcome = RuntimeApplyOutcome::default();

    match super::settings::reconcile_focus_watcher_enabled_state(
        app,
        send_active_app_context_enabled,
    ) {
        Ok(()) => {
            let focus_watcher_action = if send_active_app_context_enabled {
                RuntimeApplyAction::FocusWatcherEnabled
            } else {
                RuntimeApplyAction::FocusWatcherDisabled
            };
            runtime_apply_outcome
                .runtime_actions_applied
                .push(apply_runtime_action(
                    focus_watcher_action,
                    LocalOnlySetting::SendActiveAppContextEnabled.into(),
                ));
        }
        Err(error) => {
            runtime_apply_outcome.warnings.push(apply_runtime_warning(
                RuntimeApplyWarningCode::FocusWatcherReconcile,
                LocalOnlySetting::SendActiveAppContextEnabled.into(),
                format!(
                    "Failed to sync active app context watcher: {error:#}. Toggle 'Send active app context' to retry sync."
                ),
            ));
        }
    }

    let sync = config_sync.read().await;
    if !sync.is_connected() {
        return runtime_apply_outcome;
    }

    if let Some(prompt_sections) = prompt_sections_to_sync {
        match sync.sync_prompt_sections(prompt_sections).await {
            Ok(()) => {
                runtime_apply_outcome
                    .runtime_actions_applied
                    .push(apply_runtime_action(
                        RuntimeApplyAction::PromptSectionsSynced,
                        HttpSyncedSetting::CleanupPromptSections.into(),
                    ));
            }
            Err(error) => {
                runtime_apply_outcome.warnings.push(apply_runtime_warning(
                    RuntimeApplyWarningCode::PromptSectionsSync,
                    HttpSyncedSetting::CleanupPromptSections.into(),
                    format!(
                        "Failed to sync prompt sections to server: {error:#}. Toggle a prompt section to retry sync."
                    ),
                ));
            }
        }
    }

    if let Some(timeout_seconds) = stt_timeout_seconds_to_sync {
        match sync.sync_stt_timeout(timeout_seconds).await {
            Ok(()) => {
                runtime_apply_outcome
                    .runtime_actions_applied
                    .push(apply_runtime_action(
                        RuntimeApplyAction::SttTimeoutSynced,
                        HttpSyncedSetting::SttTimeoutSeconds.into(),
                    ));
            }
            Err(error) => {
                runtime_apply_outcome.warnings.push(apply_runtime_warning(
                    RuntimeApplyWarningCode::SttTimeoutSync,
                    HttpSyncedSetting::SttTimeoutSeconds.into(),
                    format!(
                        "Failed to sync STT timeout to server: {error:#}. Change 'STT timeout' to retry sync."
                    ),
                ));
            }
        }
    }

    match sync
        .sync_llm_formatting_enabled(llm_formatting_enabled)
        .await
    {
        Ok(()) => {
            runtime_apply_outcome
                .runtime_actions_applied
                .push(apply_runtime_action(
                    RuntimeApplyAction::LlmFormattingSynced,
                    HttpSyncedSetting::LlmFormattingEnabled.into(),
                ));
        }
        Err(error) => {
            runtime_apply_outcome.warnings.push(apply_runtime_warning(
                RuntimeApplyWarningCode::LlmFormattingSync,
                HttpSyncedSetting::LlmFormattingEnabled.into(),
                format!(
                    "Failed to sync LLM formatting mode to server: {error:#}. Toggle 'LLM formatting' to retry sync."
                ),
            ));
        }
    }

    runtime_apply_outcome
}

/// Import settings from a JSON string
#[cfg(desktop)]
#[tauri::command]
pub async fn import_settings(
    app: AppHandle,
    content: String,
    config_sync: tauri::State<'_, ConfigSync>,
) -> Result<ImportSettingsOutcome, String> {
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
    let imported_settings: AppSettings = export.data.into();

    // Save each setting individually so we can handle defaults properly.
    // Note: cleanup_prompt_sections is not imported here - prompts come from .md files.
    write_setting_classes_to_store(
        &imported_settings,
        &IMPORT_EXPORT_SETTING_CLASSES,
        |setting_class, setting_value| {
            store.set(setting_class.storage_key_name(), setting_value);
        },
    )
    .map_err(|error| format!("Failed to serialize setting for import: {error:#}"))?;

    store
        .save()
        .map_err(|e| format!("Failed to save settings: {e}"))?;

    let runtime_apply_outcome = apply_runtime_side_effects(
        &app,
        imported_settings.send_active_app_context_enabled,
        imported_settings.llm_formatting_enabled,
        imported_settings.stt_timeout_seconds,
        None,
        &config_sync,
    )
    .await;

    if runtime_apply_outcome.warnings.is_empty() {
        log::info!("Successfully imported settings from export file");
    } else {
        log::warn!(
            "Settings imported with {} runtime warnings",
            runtime_apply_outcome.warnings.len()
        );
    }

    Ok(runtime_apply_outcome)
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn import_settings(
    _app: AppHandle,
    _content: String,
    _config_sync: tauri::State<'_, ConfigSync>,
) -> Result<ImportSettingsOutcome, String> {
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
    let result = history_storage
        .import_entries(export.data, strategy)
        .map_err(|error| error.to_string())?;

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
) -> Result<FactoryResetOutcome, String> {
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
    history_storage.clear().map_err(|error| error.to_string())?;

    // Re-initialize with default settings
    let default_settings = AppSettings::default();

    write_setting_classes_to_store(
        &default_settings,
        &FACTORY_RESET_SETTING_CLASSES,
        |setting_class, setting_value| {
            store.set(setting_class.storage_key_name(), setting_value);
        },
    )
    .map_err(|error| format!("Failed to serialize setting for factory reset: {error:#}"))?;

    store
        .save()
        .map_err(|e| format!("Failed to save default settings: {e}"))?;

    let default_sections = CleanupPromptSections::default();
    let runtime_apply_outcome = apply_runtime_side_effects(
        &app,
        default_settings.send_active_app_context_enabled,
        default_settings.llm_formatting_enabled,
        Some(DEFAULT_STT_TIMEOUT_SECONDS),
        Some(&default_sections),
        &config_sync,
    )
    .await;

    if runtime_apply_outcome.warnings.is_empty() {
        log::info!("Factory reset completed: settings and history cleared");
    } else {
        log::warn!(
            "Factory reset completed with {} runtime warnings",
            runtime_apply_outcome.warnings.len()
        );
    }

    Ok(runtime_apply_outcome)
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn factory_reset(
    _app: AppHandle,
    _config_sync: tauri::State<'_, ConfigSync>,
) -> Result<FactoryResetOutcome, String> {
    Err("Not supported on this platform".to_string())
}
