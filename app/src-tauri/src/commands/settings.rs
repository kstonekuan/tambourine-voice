use crate::settings::HotkeyConfig;
use crate::state::{AppState, ShortcutErrors};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[cfg(desktop)]
use tauri_plugin_global_shortcut::GlobalShortcutExt;

#[cfg(desktop)]
use tauri_plugin_store::StoreExt;

/// Result of shortcut registration attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutRegistrationResult {
    pub toggle_registered: bool,
    pub hold_registered: bool,
    pub paste_last_registered: bool,
    pub errors: ShortcutErrors,
}

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
/// Returns registration status for each shortcut.
#[cfg(desktop)]
#[tauri::command]
pub async fn register_shortcuts(app: AppHandle) -> Result<ShortcutRegistrationResult, String> {
    // Read hotkeys from store with defaults (mutable for auto-disable on conflict)
    let mut toggle_hotkey: HotkeyConfig =
        get_setting_from_store(&app, "toggle_hotkey", HotkeyConfig::default_toggle());
    let mut hold_hotkey: HotkeyConfig =
        get_setting_from_store(&app, "hold_hotkey", HotkeyConfig::default_hold());
    let mut paste_last_hotkey: HotkeyConfig = get_setting_from_store(
        &app,
        "paste_last_hotkey",
        HotkeyConfig::default_paste_last(),
    );

    log::info!(
        "Re-registering shortcuts - Toggle: {} (enabled: {}), Hold: {} (enabled: {}), PasteLast: {} (enabled: {})",
        toggle_hotkey.to_shortcut_string(),
        toggle_hotkey.enabled,
        hold_hotkey.to_shortcut_string(),
        hold_hotkey.enabled,
        paste_last_hotkey.to_shortcut_string(),
        paste_last_hotkey.enabled
    );

    // Get the global shortcut manager
    let shortcut_manager = app.global_shortcut();

    // Unregister all existing shortcuts first
    shortcut_manager
        .unregister_all()
        .map_err(|e| format!("Failed to unregister shortcuts: {}", e))?;

    let mut result = ShortcutRegistrationResult {
        toggle_registered: false,
        hold_registered: false,
        paste_last_registered: false,
        errors: ShortcutErrors::default(),
    };

    // Register toggle shortcut if enabled
    if toggle_hotkey.enabled {
        let shortcut = toggle_hotkey.to_shortcut_or_default(HotkeyConfig::default_toggle);
        match shortcut_manager.on_shortcut(shortcut, |app_handle, shortcut, event| {
            crate::handle_shortcut_event(app_handle, shortcut, &event);
        }) {
            Ok(_) => {
                result.toggle_registered = true;
                log::info!("Toggle shortcut registered successfully");
            }
            Err(e) => {
                result.errors.toggle_error = Some(format!("Hotkey conflict: {}", e));
                log::warn!("Failed to register toggle shortcut: {}. Auto-disabling.", e);
                // Auto-disable the hotkey on conflict
                toggle_hotkey.enabled = false;
                let _ = crate::save_setting_to_store(&app, "toggle_hotkey", &toggle_hotkey);
            }
        }
    }

    // Register hold shortcut if enabled
    if hold_hotkey.enabled {
        let shortcut = hold_hotkey.to_shortcut_or_default(HotkeyConfig::default_hold);
        match shortcut_manager.on_shortcut(shortcut, |app_handle, shortcut, event| {
            crate::handle_shortcut_event(app_handle, shortcut, &event);
        }) {
            Ok(_) => {
                result.hold_registered = true;
                log::info!("Hold shortcut registered successfully");
            }
            Err(e) => {
                result.errors.hold_error = Some(format!("Hotkey conflict: {}", e));
                log::warn!("Failed to register hold shortcut: {}. Auto-disabling.", e);
                // Auto-disable the hotkey on conflict
                hold_hotkey.enabled = false;
                let _ = crate::save_setting_to_store(&app, "hold_hotkey", &hold_hotkey);
            }
        }
    }

    // Register paste_last shortcut if enabled
    if paste_last_hotkey.enabled {
        let shortcut = paste_last_hotkey.to_shortcut_or_default(HotkeyConfig::default_paste_last);
        match shortcut_manager.on_shortcut(shortcut, |app_handle, shortcut, event| {
            crate::handle_shortcut_event(app_handle, shortcut, &event);
        }) {
            Ok(_) => {
                result.paste_last_registered = true;
                log::info!("PasteLast shortcut registered successfully");
            }
            Err(e) => {
                result.errors.paste_last_error = Some(format!("Hotkey conflict: {}", e));
                log::warn!(
                    "Failed to register paste_last shortcut: {}. Auto-disabling.",
                    e
                );
                // Auto-disable the hotkey on conflict
                paste_last_hotkey.enabled = false;
                let _ = crate::save_setting_to_store(&app, "paste_last_hotkey", &paste_last_hotkey);
            }
        }
    }

    // Update app state with errors
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut shortcut_errors) = state.shortcut_errors.write() {
            *shortcut_errors = result.errors.clone();
        }
    }

    log::info!("Shortcuts registration complete: {:?}", result);
    Ok(result)
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
    let store_key = match hotkey_type.as_str() {
        "toggle" => "toggle_hotkey",
        "hold" => "hold_hotkey",
        "paste_last" => "paste_last_hotkey",
        _ => return Err(format!("Unknown hotkey type: {}", hotkey_type)),
    };

    let default_hotkey = match hotkey_type.as_str() {
        "toggle" => HotkeyConfig::default_toggle(),
        "hold" => HotkeyConfig::default_hold(),
        "paste_last" => HotkeyConfig::default_paste_last(),
        _ => unreachable!(),
    };

    // Read current hotkey config
    let mut hotkey: HotkeyConfig = get_setting_from_store(&app, store_key, default_hotkey);

    // Update enabled state
    hotkey.enabled = enabled;

    // Save back to store
    let store = app
        .store("settings.json")
        .map_err(|e| format!("Failed to get store: {}", e))?;
    let json_value = serde_json::to_value(&hotkey).map_err(|e| e.to_string())?;
    store.set(store_key, json_value); // set() returns ()
    store
        .save()
        .map_err(|e| format!("Failed to save store: {}", e))?;

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
