use crate::settings::{
    check_hotkey_conflict, AppSettings, CleanupPromptSections, HotkeyConfig, HotkeyType,
    HttpSyncedSetting, LocalOnlySetting, RtviSyncedSetting, SettingClass, SettingsError,
    DEFAULT_SERVER_URL,
};
use crate::state::{AppState, ShortcutErrors, ShortcutRegistrationResult};
use anyhow::{anyhow, Context};
use tauri::{AppHandle, Manager};

#[cfg(desktop)]
use crate::active_app_context::sync_focus_watcher_enabled;

#[cfg(desktop)]
use tauri_plugin_global_shortcut::GlobalShortcutExt;

#[cfg(desktop)]
use tauri_plugin_store::StoreExt;

/// Temporarily unregister all global shortcuts.
/// Call this before capturing a new hotkey to prevent the shortcuts from intercepting key presses.
#[cfg(desktop)]
#[tauri::command]
pub async fn unregister_shortcuts(app: AppHandle) -> Result<(), String> {
    log::info!("Temporarily unregistering all shortcuts for hotkey capture");
    let shortcut_manager = app.global_shortcut();
    shortcut_manager
        .unregister_all()
        .map_err(|e| format!("Failed to unregister shortcuts: {e}"))?;
    Ok(())
}

// Stub for non-desktop platforms
#[cfg(not(desktop))]
#[tauri::command]
pub async fn unregister_shortcuts(_app: AppHandle) -> Result<(), String> {
    Ok(())
}

/// Helper to read a setting from the store with a default fallback
#[cfg(desktop)]
pub(crate) fn get_setting_from_store<T: serde::de::DeserializeOwned>(
    app: &AppHandle,
    setting_class: impl Into<SettingClass>,
    default: T,
) -> T {
    let storage_key_name = setting_class.into().storage_key_name();
    app.store("settings.json")
        .ok()
        .and_then(|store| store.get(storage_key_name))
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or(default)
}

fn persist_local_only_setting<T: serde::Serialize>(
    app: &AppHandle,
    local_only_setting: LocalOnlySetting,
    value: &T,
) -> anyhow::Result<()> {
    crate::save_setting_to_store(app, local_only_setting.into(), value).with_context(|| {
        format!(
            "Failed to persist local-only setting '{}'",
            local_only_setting.storage_key_name()
        )
    })
}

fn persist_http_synced_setting<T: serde::Serialize>(
    app: &AppHandle,
    http_synced_setting: HttpSyncedSetting,
    value: &T,
) -> anyhow::Result<()> {
    crate::save_setting_to_store(app, http_synced_setting.into(), value).with_context(|| {
        format!(
            "Failed to persist HTTP-synced setting '{}'",
            http_synced_setting.storage_key_name()
        )
    })
}

fn persist_rtvi_synced_setting<T: serde::Serialize>(
    app: &AppHandle,
    rtvi_synced_setting: RtviSyncedSetting,
    value: &T,
) -> anyhow::Result<()> {
    crate::save_setting_to_store(app, rtvi_synced_setting.into(), value).with_context(|| {
        format!(
            "Failed to persist RTVI-synced setting '{}'",
            rtvi_synced_setting.storage_key_name()
        )
    })
}

#[cfg(desktop)]
pub(crate) fn reconcile_focus_watcher_enabled_state(
    app: &AppHandle,
    send_active_app_context_enabled: bool,
) -> anyhow::Result<()> {
    let app_state = app
        .try_state::<AppState>()
        .context("AppState unavailable while reconciling focus watcher lifecycle")?;

    let mut focus_watcher_guard = app_state.focus_watcher.lock().map_err(|lock_error| {
        anyhow!("Failed to lock focus watcher state for reconciliation: {lock_error}")
    })?;

    sync_focus_watcher_enabled(
        app,
        &mut focus_watcher_guard,
        send_active_app_context_enabled,
    );
    Ok(())
}

/// Re-register global shortcuts with the current settings from the store.
/// Called from frontend after hotkey settings are changed.
#[cfg(desktop)]
#[tauri::command]
pub async fn register_shortcuts(app: AppHandle) -> Result<ShortcutRegistrationResult, String> {
    Ok(crate::do_register_shortcuts(&app))
}

// Stub for non-desktop platforms
#[cfg(not(desktop))]
#[tauri::command]
pub async fn register_shortcuts(_app: AppHandle) -> Result<ShortcutRegistrationResult, String> {
    Ok(ShortcutRegistrationResult {
        toggle_registered: true,
        hold_registered: true,
        paste_last_registered: true,
        errors: ShortcutErrors::default(),
    })
}

/// Get the current shortcut registration errors
#[tauri::command]
pub fn get_shortcut_errors(app: AppHandle) -> ShortcutErrors {
    app.try_state::<AppState>()
        .and_then(|state| state.shortcut_errors.read().ok().map(|e| e.clone()))
        .unwrap_or_default()
}

/// Set a hotkey's enabled state
#[cfg(desktop)]
#[tauri::command]
pub async fn set_hotkey_enabled(
    app: AppHandle,
    hotkey_type: HotkeyType,
    enabled: bool,
) -> Result<(), String> {
    let local_only_setting = hotkey_type.local_only_setting();
    let mut hotkey: HotkeyConfig =
        get_setting_from_store(&app, local_only_setting, hotkey_type.default_hotkey());
    hotkey.enabled = enabled;

    persist_local_only_setting(&app, local_only_setting, &hotkey)
        .map_err(|error| format!("{error:#}"))?;
    log::info!(
        "Set {} hotkey enabled: {}",
        hotkey_type.display_name(),
        enabled
    );
    Ok(())
}

// Stub for non-desktop platforms
#[cfg(not(desktop))]
#[tauri::command]
pub async fn set_hotkey_enabled(
    _app: AppHandle,
    _hotkey_type: HotkeyType,
    _enabled: bool,
) -> Result<(), String> {
    Ok(())
}

// ============================================================================
// SETTINGS CRUD COMMANDS
// ============================================================================

/// Get all application settings with defaults applied
/// This is the single source of truth for reading settings
#[cfg(desktop)]
#[tauri::command]
pub fn get_settings(app: AppHandle) -> Result<AppSettings, String> {
    Ok(AppSettings {
        toggle_hotkey: get_setting_from_store(
            &app,
            LocalOnlySetting::ToggleHotkey,
            HotkeyConfig::default_toggle(),
        ),
        hold_hotkey: get_setting_from_store(
            &app,
            LocalOnlySetting::HoldHotkey,
            HotkeyConfig::default_hold(),
        ),
        paste_last_hotkey: get_setting_from_store(
            &app,
            LocalOnlySetting::PasteLastHotkey,
            HotkeyConfig::default_paste_last(),
        ),
        selected_mic_id: get_setting_from_store(&app, LocalOnlySetting::SelectedMicId, None),
        sound_enabled: get_setting_from_store(&app, LocalOnlySetting::SoundEnabled, true),
        cleanup_prompt_sections: get_setting_from_store(
            &app,
            HttpSyncedSetting::CleanupPromptSections,
            None,
        ),
        stt_provider: get_setting_from_store(
            &app,
            RtviSyncedSetting::SttProvider,
            "auto".to_string(),
        ),
        llm_provider: get_setting_from_store(
            &app,
            RtviSyncedSetting::LlmProvider,
            "auto".to_string(),
        ),
        auto_mute_audio: get_setting_from_store(&app, LocalOnlySetting::AutoMuteAudio, false),
        stt_timeout_seconds: get_setting_from_store(
            &app,
            HttpSyncedSetting::SttTimeoutSeconds,
            None,
        ),
        server_url: get_setting_from_store(
            &app,
            LocalOnlySetting::ServerUrl,
            DEFAULT_SERVER_URL.to_string(),
        ),
        llm_formatting_enabled: get_setting_from_store(
            &app,
            HttpSyncedSetting::LlmFormattingEnabled,
            true,
        ),
        send_active_app_context_enabled: get_setting_from_store(
            &app,
            LocalOnlySetting::SendActiveAppContextEnabled,
            false,
        ),
    })
}

// Stub for non-desktop platforms
#[cfg(not(desktop))]
#[tauri::command]
pub fn get_settings(_app: AppHandle) -> Result<AppSettings, String> {
    Ok(AppSettings::default())
}

/// Update a hotkey with validation
#[cfg(desktop)]
#[tauri::command]
pub async fn update_hotkey(
    app: AppHandle,
    hotkey_type: HotkeyType,
    config: HotkeyConfig,
) -> Result<(), SettingsError> {
    // Get current settings to check for conflicts
    let settings = get_settings(app.clone()).map_err(|e| SettingsError::StoreError(e.clone()))?;

    // Check for conflicts with other hotkeys
    if let Some(error) = check_hotkey_conflict(&config, &settings, hotkey_type) {
        return Err(error);
    }

    // Save the hotkey
    let local_only_setting = hotkey_type.local_only_setting();
    persist_local_only_setting(&app, local_only_setting, &config)
        .map_err(|error| SettingsError::StoreError(format!("{error:#}")))?;

    log::info!(
        "Updated {} hotkey to: {}",
        hotkey_type.display_name(),
        config.to_shortcut_string()
    );
    Ok(())
}

// Stub for non-desktop platforms
#[cfg(not(desktop))]
#[tauri::command]
pub async fn update_hotkey(
    _app: AppHandle,
    _hotkey_type: HotkeyType,
    _config: HotkeyConfig,
) -> Result<(), SettingsError> {
    Ok(())
}

/// Update selected microphone ID
#[cfg(desktop)]
#[tauri::command]
pub async fn update_selected_mic(app: AppHandle, mic_id: Option<String>) -> Result<(), String> {
    persist_local_only_setting(&app, LocalOnlySetting::SelectedMicId, &mic_id)
        .map_err(|error| format!("{error:#}"))?;
    log::info!("Updated selected microphone: {mic_id:?}");
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn update_selected_mic(_app: AppHandle, _mic_id: Option<String>) -> Result<(), String> {
    Ok(())
}

/// Update sound enabled setting
#[cfg(desktop)]
#[tauri::command]
pub async fn update_sound_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    persist_local_only_setting(&app, LocalOnlySetting::SoundEnabled, &enabled)
        .map_err(|error| format!("{error:#}"))?;
    log::info!("Updated sound enabled: {enabled}");
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn update_sound_enabled(_app: AppHandle, _enabled: bool) -> Result<(), String> {
    Ok(())
}

/// Update cleanup prompt sections
#[cfg(desktop)]
#[tauri::command]
pub async fn update_cleanup_prompt_sections(
    app: AppHandle,
    sections: Option<CleanupPromptSections>,
    config_sync: tauri::State<'_, crate::config_sync::ConfigSync>,
) -> Result<(), String> {
    // Save locally
    persist_http_synced_setting(&app, HttpSyncedSetting::CleanupPromptSections, &sections)
        .map_err(|error| format!("{error:#}"))?;
    log::info!("Updated cleanup prompt sections");

    // Sync to server
    if let Some(ref s) = sections {
        if let Err(e) = config_sync.read().await.sync_prompt_sections(s).await {
            log::warn!("Failed to sync prompt sections to server: {e}");
            return Err(e.to_string());
        }
    }

    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn update_cleanup_prompt_sections(
    _app: AppHandle,
    _sections: Option<CleanupPromptSections>,
) -> Result<(), String> {
    Ok(())
}

/// Update STT provider
#[cfg(desktop)]
#[tauri::command]
pub async fn update_stt_provider(app: AppHandle, provider: String) -> Result<(), String> {
    // Provider settings are server-synced through RTVI from TypeScript, not Rust HTTP config sync.
    persist_rtvi_synced_setting(&app, RtviSyncedSetting::SttProvider, &provider)
        .map_err(|error| format!("{error:#}"))?;
    log::info!("Updated STT provider: {provider}");
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn update_stt_provider(_app: AppHandle, _provider: String) -> Result<(), String> {
    Ok(())
}

/// Update LLM provider
#[cfg(desktop)]
#[tauri::command]
pub async fn update_llm_provider(app: AppHandle, provider: String) -> Result<(), String> {
    // Provider settings are server-synced through RTVI from TypeScript, not Rust HTTP config sync.
    persist_rtvi_synced_setting(&app, RtviSyncedSetting::LlmProvider, &provider)
        .map_err(|error| format!("{error:#}"))?;
    log::info!("Updated LLM provider: {provider}");
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn update_llm_provider(_app: AppHandle, _provider: String) -> Result<(), String> {
    Ok(())
}

/// Update auto mute audio setting
#[cfg(desktop)]
#[tauri::command]
pub async fn update_auto_mute_audio(app: AppHandle, enabled: bool) -> Result<(), String> {
    persist_local_only_setting(&app, LocalOnlySetting::AutoMuteAudio, &enabled)
        .map_err(|error| format!("{error:#}"))?;
    log::info!("Updated auto mute audio: {enabled}");
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn update_auto_mute_audio(_app: AppHandle, _enabled: bool) -> Result<(), String> {
    Ok(())
}

/// Update STT timeout
#[cfg(desktop)]
#[tauri::command]
pub async fn update_stt_timeout(
    app: AppHandle,
    timeout_seconds: Option<f64>,
    config_sync: tauri::State<'_, crate::config_sync::ConfigSync>,
) -> Result<(), String> {
    // Save locally
    persist_http_synced_setting(&app, HttpSyncedSetting::SttTimeoutSeconds, &timeout_seconds)
        .map_err(|error| format!("{error:#}"))?;
    log::info!("Updated STT timeout: {timeout_seconds:?}");

    // Sync to server
    if let Some(timeout) = timeout_seconds {
        if let Err(e) = config_sync.read().await.sync_stt_timeout(timeout).await {
            log::warn!("Failed to sync STT timeout to server: {e}");
            return Err(e.to_string());
        }
    }

    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn update_stt_timeout(
    _app: AppHandle,
    _timeout_seconds: Option<f64>,
) -> Result<(), String> {
    Ok(())
}

/// Update server URL
#[cfg(desktop)]
#[tauri::command]
pub async fn update_server_url(app: AppHandle, url: String) -> Result<(), String> {
    persist_local_only_setting(&app, LocalOnlySetting::ServerUrl, &url)
        .map_err(|error| format!("{error:#}"))?;
    log::info!("Updated server URL: {url}");
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn update_server_url(_app: AppHandle, _url: String) -> Result<(), String> {
    Ok(())
}

/// Update LLM formatting enabled setting
#[cfg(desktop)]
#[tauri::command]
pub async fn update_llm_formatting_enabled(
    app: AppHandle,
    enabled: bool,
    config_sync: tauri::State<'_, crate::config_sync::ConfigSync>,
) -> Result<(), String> {
    // Save locally
    persist_http_synced_setting(&app, HttpSyncedSetting::LlmFormattingEnabled, &enabled)
        .map_err(|error| format!("{error:#}"))?;

    // Log the change
    if enabled {
        log::info!("LLM formatting enabled");
    } else {
        log::info!("LLM formatting disabled");
    }

    // Sync to server
    if let Err(e) = config_sync
        .read()
        .await
        .sync_llm_formatting_enabled(enabled)
        .await
    {
        log::warn!("Failed to sync LLM formatting to server: {e}");
        return Err(e.to_string());
    }

    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn update_llm_formatting_enabled(_app: AppHandle, _enabled: bool) -> Result<(), String> {
    Ok(())
}

/// Update send active app context setting
#[cfg(desktop)]
#[tauri::command]
pub async fn update_send_active_app_context_enabled(
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    persist_local_only_setting(
        &app,
        LocalOnlySetting::SendActiveAppContextEnabled,
        &enabled,
    )
    .map_err(|error| format!("{error:#}"))?;

    if let Err(error) = reconcile_focus_watcher_enabled_state(&app, enabled) {
        log::warn!(
            "Failed to reconcile focus watcher while updating send_active_app_context_enabled: {error:#}"
        );
    }

    log::info!("Send active app context enabled: {enabled}");
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn update_send_active_app_context_enabled(
    _app: AppHandle,
    _enabled: bool,
) -> Result<(), String> {
    Ok(())
}

/// Reset all hotkeys to their default values
#[cfg(desktop)]
#[tauri::command]
pub async fn reset_hotkeys_to_defaults(app: AppHandle) -> Result<(), String> {
    persist_local_only_setting(
        &app,
        LocalOnlySetting::ToggleHotkey,
        &HotkeyConfig::default_toggle(),
    )
    .map_err(|error| format!("{error:#}"))?;
    persist_local_only_setting(
        &app,
        LocalOnlySetting::HoldHotkey,
        &HotkeyConfig::default_hold(),
    )
    .map_err(|error| format!("{error:#}"))?;
    persist_local_only_setting(
        &app,
        LocalOnlySetting::PasteLastHotkey,
        &HotkeyConfig::default_paste_last(),
    )
    .map_err(|error| format!("{error:#}"))?;
    log::info!("Reset all hotkeys to defaults");
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn reset_hotkeys_to_defaults(_app: AppHandle) -> Result<(), String> {
    Ok(())
}
