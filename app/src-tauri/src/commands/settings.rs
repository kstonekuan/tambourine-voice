use crate::settings::{
    check_hotkey_conflict, AppSettings, CleanupPromptSections, HotkeyConfig, HotkeyType,
    SettingsError, StoreKey, DEFAULT_SERVER_URL,
};
use crate::state::{AppState, ShortcutErrors, ShortcutRegistrationResult};
use tauri::{AppHandle, Manager};

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
    key: StoreKey,
    default: T,
) -> T {
    app.store("settings.json")
        .ok()
        .and_then(|store| store.get(key.as_str()))
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or(default)
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
    let store_key = hotkey_type.store_key();
    let mut hotkey: HotkeyConfig =
        get_setting_from_store(&app, store_key, hotkey_type.default_hotkey());
    hotkey.enabled = enabled;

    crate::save_setting_to_store(&app, store_key, &hotkey)?;
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
            StoreKey::ToggleHotkey,
            HotkeyConfig::default_toggle(),
        ),
        hold_hotkey: get_setting_from_store(
            &app,
            StoreKey::HoldHotkey,
            HotkeyConfig::default_hold(),
        ),
        paste_last_hotkey: get_setting_from_store(
            &app,
            StoreKey::PasteLastHotkey,
            HotkeyConfig::default_paste_last(),
        ),
        selected_mic_id: get_setting_from_store(&app, StoreKey::SelectedMicId, None),
        sound_enabled: get_setting_from_store(&app, StoreKey::SoundEnabled, true),
        cleanup_prompt_sections: get_setting_from_store(
            &app,
            StoreKey::CleanupPromptSections,
            None,
        ),
        stt_provider: get_setting_from_store(&app, StoreKey::SttProvider, "auto".to_string()),
        llm_provider: get_setting_from_store(&app, StoreKey::LlmProvider, "auto".to_string()),
        auto_mute_audio: get_setting_from_store(&app, StoreKey::AutoMuteAudio, false),
        stt_timeout_seconds: get_setting_from_store(&app, StoreKey::SttTimeoutSeconds, None),
        server_url: get_setting_from_store(
            &app,
            StoreKey::ServerUrl,
            DEFAULT_SERVER_URL.to_string(),
        ),
        llm_formatting_enabled: get_setting_from_store(&app, StoreKey::LlmFormattingEnabled, true),
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
    crate::save_setting_to_store(&app, hotkey_type.store_key(), &config)
        .map_err(SettingsError::StoreError)?;

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
    crate::save_setting_to_store(&app, StoreKey::SelectedMicId, &mic_id)?;
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
    crate::save_setting_to_store(&app, StoreKey::SoundEnabled, &enabled)?;
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
    crate::save_setting_to_store(&app, StoreKey::CleanupPromptSections, &sections)?;
    log::info!("Updated cleanup prompt sections");

    // Sync to server
    if let Some(ref s) = sections {
        if let Err(e) = config_sync.read().await.sync_prompt_sections(s).await {
            log::warn!("Failed to sync prompt sections to server: {e}");
            return Err(e);
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
    crate::save_setting_to_store(&app, StoreKey::SttProvider, &provider)?;
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
    crate::save_setting_to_store(&app, StoreKey::LlmProvider, &provider)?;
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
    crate::save_setting_to_store(&app, StoreKey::AutoMuteAudio, &enabled)?;
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
    crate::save_setting_to_store(&app, StoreKey::SttTimeoutSeconds, &timeout_seconds)?;
    log::info!("Updated STT timeout: {timeout_seconds:?}");

    // Sync to server
    if let Some(timeout) = timeout_seconds {
        if let Err(e) = config_sync.read().await.sync_stt_timeout(timeout).await {
            log::warn!("Failed to sync STT timeout to server: {e}");
            return Err(e);
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
    crate::save_setting_to_store(&app, StoreKey::ServerUrl, &url)?;
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
    crate::save_setting_to_store(&app, StoreKey::LlmFormattingEnabled, &enabled)?;

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
        return Err(e);
    }

    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn update_llm_formatting_enabled(_app: AppHandle, _enabled: bool) -> Result<(), String> {
    Ok(())
}

/// Reset all hotkeys to their default values
#[cfg(desktop)]
#[tauri::command]
pub async fn reset_hotkeys_to_defaults(app: AppHandle) -> Result<(), String> {
    crate::save_setting_to_store(
        &app,
        StoreKey::ToggleHotkey,
        &HotkeyConfig::default_toggle(),
    )?;
    crate::save_setting_to_store(&app, StoreKey::HoldHotkey, &HotkeyConfig::default_hold())?;
    crate::save_setting_to_store(
        &app,
        StoreKey::PasteLastHotkey,
        &HotkeyConfig::default_paste_last(),
    )?;
    log::info!("Reset all hotkeys to defaults");
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn reset_hotkeys_to_defaults(_app: AppHandle) -> Result<(), String> {
    Ok(())
}
