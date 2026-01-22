use crate::settings::{
    check_hotkey_conflict, AppSettings, CleanupPromptSections, HotkeyConfig, HotkeyType,
    SettingsError, DEFAULT_SERVER_URL,
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
        .map_err(|e| format!("Failed to unregister shortcuts: {}", e))?;
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
fn get_setting_from_store<T: serde::de::DeserializeOwned>(
    app: &AppHandle,
    key: &str,
    default: T,
) -> T {
    app.store("settings.json")
        .ok()
        .and_then(|store| store.get(key))
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
    hotkey_type: String,
    enabled: bool,
) -> Result<(), String> {
    let (store_key, default_hotkey) = match hotkey_type.as_str() {
        "toggle" => ("toggle_hotkey", HotkeyConfig::default_toggle()),
        "hold" => ("hold_hotkey", HotkeyConfig::default_hold()),
        "paste_last" => ("paste_last_hotkey", HotkeyConfig::default_paste_last()),
        _ => return Err(format!("Unknown hotkey type: {}", hotkey_type)),
    };

    let mut hotkey: HotkeyConfig = get_setting_from_store(&app, store_key, default_hotkey);
    hotkey.enabled = enabled;

    crate::save_setting_to_store(&app, store_key, &hotkey)?;
    log::info!("Set {} hotkey enabled: {}", hotkey_type, enabled);
    Ok(())
}

// Stub for non-desktop platforms
#[cfg(not(desktop))]
#[tauri::command]
pub async fn set_hotkey_enabled(
    _app: AppHandle,
    _hotkey_type: String,
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
            "toggle_hotkey",
            HotkeyConfig::default_toggle(),
        ),
        hold_hotkey: get_setting_from_store(&app, "hold_hotkey", HotkeyConfig::default_hold()),
        paste_last_hotkey: get_setting_from_store(
            &app,
            "paste_last_hotkey",
            HotkeyConfig::default_paste_last(),
        ),
        selected_mic_id: get_setting_from_store(&app, "selected_mic_id", None),
        sound_enabled: get_setting_from_store(&app, "sound_enabled", true),
        cleanup_prompt_sections: get_setting_from_store(&app, "cleanup_prompt_sections", None),
        stt_provider: get_setting_from_store(&app, "stt_provider", "auto".to_string()),
        llm_provider: get_setting_from_store(&app, "llm_provider", "auto".to_string()),
        auto_mute_audio: get_setting_from_store(&app, "auto_mute_audio", false),
        stt_timeout_seconds: get_setting_from_store(&app, "stt_timeout_seconds", None),
        server_url: get_setting_from_store(&app, "server_url", DEFAULT_SERVER_URL.to_string()),
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
    let settings =
        get_settings(app.clone()).map_err(|e| SettingsError::StoreError(e.to_string()))?;

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
    crate::save_setting_to_store(&app, "selected_mic_id", &mic_id)?;
    log::info!("Updated selected microphone: {:?}", mic_id);
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
    crate::save_setting_to_store(&app, "sound_enabled", &enabled)?;
    log::info!("Updated sound enabled: {}", enabled);
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
    use crate::events::{config_settings, names, ConfigResponse};
    use tauri::Emitter;

    // Save locally
    crate::save_setting_to_store(&app, "cleanup_prompt_sections", &sections)?;
    log::info!("Updated cleanup prompt sections");

    // Sync to server and emit notification
    if let Some(ref s) = sections {
        match config_sync.read().await.sync_prompt_sections(s).await {
            Ok(()) => {
                let _ = app.emit(
                    names::CONFIG_RESPONSE,
                    ConfigResponse::updated(config_settings::PROMPT_SECTIONS, s),
                );
            }
            Err(e) => {
                log::warn!("Failed to sync prompt sections to server: {}", e);
                let _ = app.emit(
                    names::CONFIG_RESPONSE,
                    ConfigResponse::<()>::error(config_settings::PROMPT_SECTIONS, e),
                );
            }
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
    crate::save_setting_to_store(&app, "stt_provider", &provider)?;
    log::info!("Updated STT provider: {}", provider);
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
    crate::save_setting_to_store(&app, "llm_provider", &provider)?;
    log::info!("Updated LLM provider: {}", provider);
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
    crate::save_setting_to_store(&app, "auto_mute_audio", &enabled)?;
    log::info!("Updated auto mute audio: {}", enabled);
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
    use crate::events::{config_settings, names, ConfigResponse};
    use tauri::Emitter;

    // Save locally
    crate::save_setting_to_store(&app, "stt_timeout_seconds", &timeout_seconds)?;
    log::info!("Updated STT timeout: {:?}", timeout_seconds);

    // Sync to server and emit notification
    if let Some(timeout) = timeout_seconds {
        match config_sync.read().await.sync_stt_timeout(timeout).await {
            Ok(()) => {
                let _ = app.emit(
                    names::CONFIG_RESPONSE,
                    ConfigResponse::updated(config_settings::STT_TIMEOUT, timeout),
                );
            }
            Err(e) => {
                log::warn!("Failed to sync STT timeout to server: {}", e);
                let _ = app.emit(
                    names::CONFIG_RESPONSE,
                    ConfigResponse::<()>::error(config_settings::STT_TIMEOUT, e),
                );
            }
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
    crate::save_setting_to_store(&app, "server_url", &url)?;
    log::info!("Updated server URL: {}", url);
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn update_server_url(_app: AppHandle, _url: String) -> Result<(), String> {
    Ok(())
}

/// Reset all hotkeys to their default values
#[cfg(desktop)]
#[tauri::command]
pub async fn reset_hotkeys_to_defaults(app: AppHandle) -> Result<(), String> {
    crate::save_setting_to_store(&app, "toggle_hotkey", &HotkeyConfig::default_toggle())?;
    crate::save_setting_to_store(&app, "hold_hotkey", &HotkeyConfig::default_hold())?;
    crate::save_setting_to_store(
        &app,
        "paste_last_hotkey",
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
