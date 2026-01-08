use crate::settings::HotkeyConfig;
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
