use std::sync::atomic::Ordering;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};
use tauri_utils::config::BackgroundThrottlingPolicy;

mod audio;
mod audio_mute;
mod commands;
mod history;
mod settings;
mod state;

#[cfg(test)]
mod tests;

use audio_mute::AudioMuteManager;
use history::HistoryStorage;
use settings::HotkeyConfig;
use state::AppState;

#[cfg(desktop)]
use tauri_plugin_store::StoreExt;

#[cfg(desktop)]
use tauri_plugin_global_shortcut::{Shortcut, ShortcutEvent, ShortcutState};

// Define NSPanel type for overlay on macOS
#[cfg(target_os = "macos")]
tauri_nspanel::tauri_panel! {
    panel!(OverlayPanel {
        config: {
            can_become_key_window: false,
            is_floating_panel: true
        }
    })
}

/// Normalize a shortcut string for comparison (handles "ctrl" vs "control" differences)
#[cfg(desktop)]
pub(crate) fn normalize_shortcut_string(s: &str) -> String {
    s.to_lowercase()
        .replace("ctrl", "control")
        .replace("cmd", "super")
        .replace("meta", "super")
        .replace("win", "super")
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

/// Start recording with sound and audio mute handling
#[cfg(desktop)]
fn start_recording(
    app: &AppHandle,
    state: &AppState,
    sound_enabled: bool,
    audio_mute_manager: &Option<tauri::State<'_, AudioMuteManager>>,
    auto_mute_audio: bool,
    source: &str,
) {
    state.is_recording.store(true, Ordering::SeqCst);
    log::info!("{}: starting recording", source);
    // Play sound BEFORE muting so it's audible
    if sound_enabled {
        audio::play_sound(audio::SoundType::RecordingStart);
        // Brief delay to let sound play before muting
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
    // Mute system audio if enabled
    if auto_mute_audio {
        if let Some(manager) = audio_mute_manager {
            if let Err(e) = manager.mute() {
                log::warn!("Failed to mute audio: {}", e);
            }
        }
    }
    let _ = app.emit("recording-start", ());
}

/// Stop recording with sound and audio unmute handling
#[cfg(desktop)]
fn stop_recording(
    app: &AppHandle,
    state: &AppState,
    sound_enabled: bool,
    audio_mute_manager: &Option<tauri::State<'_, AudioMuteManager>>,
    auto_mute_audio: bool,
    source: &str,
) {
    state.is_recording.store(false, Ordering::SeqCst);
    log::info!("{}: stopping recording", source);
    // Unmute system audio if it was muted
    if auto_mute_audio {
        if let Some(manager) = audio_mute_manager {
            if let Err(e) = manager.unmute() {
                log::warn!("Failed to unmute audio: {}", e);
            }
        }
    }
    if sound_enabled {
        audio::play_sound(audio::SoundType::RecordingStop);
    }
    let _ = app.emit("recording-stop", ());
}

/// Handle a shortcut event - public so it can be called from commands/settings.rs
#[cfg(desktop)]
pub fn handle_shortcut_event(app: &AppHandle, shortcut: &Shortcut, event: &ShortcutEvent) {
    let state = app.state::<AppState>();

    // Get current settings from store
    let sound_enabled: bool = get_setting_from_store(app, "sound_enabled", true);
    let auto_mute_audio: bool = get_setting_from_store(app, "auto_mute_audio", false);

    // Get shortcut string for comparison (normalized to handle "ctrl" vs "control" differences)
    let shortcut_str = normalize_shortcut_string(&shortcut.to_string());

    // Get configured shortcut strings from store (normalized), with validation fallback
    let toggle_hotkey: HotkeyConfig =
        get_setting_from_store(app, "toggle_hotkey", HotkeyConfig::default_toggle());
    let hold_hotkey: HotkeyConfig =
        get_setting_from_store(app, "hold_hotkey", HotkeyConfig::default_hold());
    let paste_last_hotkey: HotkeyConfig =
        get_setting_from_store(app, "paste_last_hotkey", HotkeyConfig::default_paste_last());

    // Validate hotkeys - if they can't be parsed as shortcuts, use defaults
    let toggle_shortcut_str = normalize_shortcut_string(
        &toggle_hotkey
            .to_shortcut()
            .map(|_| toggle_hotkey.to_shortcut_string())
            .unwrap_or_else(|_| HotkeyConfig::default_toggle().to_shortcut_string()),
    );
    let hold_shortcut_str = normalize_shortcut_string(
        &hold_hotkey
            .to_shortcut()
            .map(|_| hold_hotkey.to_shortcut_string())
            .unwrap_or_else(|_| HotkeyConfig::default_hold().to_shortcut_string()),
    );
    let paste_last_shortcut_str = normalize_shortcut_string(
        &paste_last_hotkey
            .to_shortcut()
            .map(|_| paste_last_hotkey.to_shortcut_string())
            .unwrap_or_else(|_| HotkeyConfig::default_paste_last().to_shortcut_string()),
    );

    // Get audio mute manager if available
    let audio_mute_manager = app.try_state::<AudioMuteManager>();

    // Compare normalized strings directly
    let is_toggle = shortcut_str == toggle_shortcut_str;
    let is_hold = shortcut_str == hold_shortcut_str;
    let is_paste_last = shortcut_str == paste_last_shortcut_str;

    if is_toggle {
        // Toggle mode: action happens on key release (debounced)
        match event.state {
            ShortcutState::Pressed => {
                state.toggle_key_held.swap(true, Ordering::SeqCst);
            }
            ShortcutState::Released => {
                if state.toggle_key_held.swap(false, Ordering::SeqCst) {
                    if state.is_recording.load(Ordering::SeqCst) {
                        stop_recording(
                            app,
                            &state,
                            sound_enabled,
                            &audio_mute_manager,
                            auto_mute_audio,
                            "Toggle",
                        );
                    } else {
                        start_recording(
                            app,
                            &state,
                            sound_enabled,
                            &audio_mute_manager,
                            auto_mute_audio,
                            "Toggle",
                        );
                    }
                }
            }
        }
    } else if is_hold {
        // Hold-to-Record: start on press, stop on release
        match event.state {
            ShortcutState::Pressed => {
                if !state.ptt_key_held.swap(true, Ordering::SeqCst) {
                    start_recording(
                        app,
                        &state,
                        sound_enabled,
                        &audio_mute_manager,
                        auto_mute_audio,
                        "Hold",
                    );
                }
            }
            ShortcutState::Released => {
                if state.ptt_key_held.swap(false, Ordering::SeqCst) {
                    stop_recording(
                        app,
                        &state,
                        sound_enabled,
                        &audio_mute_manager,
                        auto_mute_audio,
                        "Hold",
                    );
                }
            }
        }
    } else if is_paste_last {
        // Paste last transcription: hold-to-paste (paste happens on release)
        match event.state {
            ShortcutState::Pressed => {
                // Mark key as held (ignore OS key repeat)
                state.paste_key_held.swap(true, Ordering::SeqCst);
            }
            ShortcutState::Released => {
                if state.paste_key_held.swap(false, Ordering::SeqCst) {
                    // Key released - do the paste
                    log::info!("PasteLast: pasting last transcription");
                    let history_storage = app.state::<HistoryStorage>();

                    if let Ok(entries) = history_storage.get_all(Some(1)) {
                        if let Some(entry) = entries.first() {
                            if let Err(e) = commands::text::type_text_blocking(&entry.text) {
                                log::error!("Failed to paste last transcription: {}", e);
                            }
                        } else {
                            log::info!("PasteLast: no history entries available");
                        }
                    }
                }
            }
        }
    } else {
        log::warn!("Unknown shortcut: {}", shortcut_str);
    }
}

/// Initial settings for shortcut registration (before store plugin is available)
#[cfg(desktop)]
struct InitialShortcutSettings {
    toggle_hotkey: HotkeyConfig,
    hold_hotkey: HotkeyConfig,
    paste_last_hotkey: HotkeyConfig,
}

/// Load initial shortcut settings from the store file (used before app is fully set up)
#[cfg(desktop)]
fn load_initial_settings() -> InitialShortcutSettings {
    let app_data_dir = dirs::data_dir()
        .map(|p| p.join("com.tambourine.voice-dictation"))
        .unwrap_or_default();
    let settings_path = app_data_dir.join("settings.json");

    // The store plugin uses a JSON object with keys at the top level
    if settings_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&settings_path) {
            if let Ok(store_data) = serde_json::from_str::<serde_json::Value>(&content) {
                let toggle_hotkey = store_data
                    .get("toggle_hotkey")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_else(HotkeyConfig::default_toggle);
                let hold_hotkey = store_data
                    .get("hold_hotkey")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_else(HotkeyConfig::default_hold);
                let paste_last_hotkey = store_data
                    .get("paste_last_hotkey")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_else(HotkeyConfig::default_paste_last);

                return InitialShortcutSettings {
                    toggle_hotkey,
                    hold_hotkey,
                    paste_last_hotkey,
                };
            }
        }
    }

    InitialShortcutSettings {
        toggle_hotkey: HotkeyConfig::default_toggle(),
        hold_hotkey: HotkeyConfig::default_hold(),
        paste_last_hotkey: HotkeyConfig::default_paste_last(),
    }
}

/// Check if audio mute is supported on this platform
#[tauri::command]
fn is_audio_mute_supported() -> bool {
    audio_mute::is_supported()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder.plugin(build_global_shortcut_plugin());
    }

    #[cfg(target_os = "macos")]
    {
        builder = builder.plugin(tauri_nspanel::init());
    }

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::text::type_text,
            commands::text::get_server_url,
            commands::settings::register_shortcuts,
            commands::settings::unregister_shortcuts,
            is_audio_mute_supported,
            commands::history::add_history_entry,
            commands::history::get_history,
            commands::history::delete_history_entry,
            commands::history::clear_history,
            commands::overlay::resize_overlay,
        ])
        .setup(|app| {
            // Initialize history storage
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data directory");

            let history_storage = HistoryStorage::new(app_data_dir);
            app.manage(history_storage);

            // Initialize audio mute manager (may be None on unsupported platforms)
            if let Some(audio_mute_manager) = AudioMuteManager::new() {
                app.manage(audio_mute_manager);
            }
            // Create overlay window
            let overlay = tauri::WebviewWindowBuilder::new(
                app,
                "overlay",
                tauri::WebviewUrl::App("overlay.html".into()),
            )
            .title("Voice Overlay")
            .inner_size(48.0, 48.0)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .focused(false)
            .focusable(false)
            .accept_first_mouse(true)
            .visible(true)
            .visible_on_all_workspaces(true)
            .background_throttling(BackgroundThrottlingPolicy::Disabled)
            .build()?;

            // On macOS, convert to NSPanel for better fullscreen app behavior
            #[cfg(target_os = "macos")]
            {
                use tauri_nspanel::{CollectionBehavior, PanelLevel, WebviewWindowExt};
                match overlay.to_panel::<OverlayPanel>() {
                    Ok(panel) => {
                        // Configure panel to float above fullscreen apps
                        panel.set_level(PanelLevel::ScreenSaver.value());
                        panel.set_floating_panel(true);

                        // Set collection behavior to appear on all spaces including fullscreen
                        let behavior = CollectionBehavior::new()
                            .can_join_all_spaces()
                            .full_screen_auxiliary();
                        panel.set_collection_behavior(behavior.value());

                        // Set style mask to non-activating panel
                        let style = tauri_nspanel::StyleMask::empty().nonactivating_panel();
                        panel.set_style_mask(style.value());

                        log::info!("[NSPanel] Successfully converted overlay to NSPanel");
                    }
                    Err(e) => {
                        log::error!("[NSPanel] Failed to convert overlay to NSPanel: {:?}", e);
                    }
                }
            }

            // Position bottom-right
            if let Ok(Some(monitor)) = overlay.current_monitor() {
                let size = monitor.size();
                let scale = monitor.scale_factor();
                let x = (size.width as f64 / scale) as i32 - 150;
                let y = (size.height as f64 / scale) as i32 - 100;
                let _ = overlay.set_position(tauri::Position::Logical(tauri::LogicalPosition {
                    x: x as f64,
                    y: y as f64,
                }));
            }

            // Setup system tray
            setup_tray(app.handle())?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show_item = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

    // Load the template icon for macOS menu bar
    // The @2x version is automatically used for retina displays
    let icon_bytes = include_bytes!("../icons/tray-iconTemplate@2x.png");
    let icon = tauri::image::Image::from_bytes(icon_bytes)?;

    let _tray = TrayIconBuilder::new()
        .icon(icon)
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    if window.is_visible().unwrap_or(false) {
                        let _ = window.hide();
                    } else {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
            }
        })
        .build(app)?;

    Ok(())
}

#[cfg(desktop)]
fn build_global_shortcut_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    // Load settings to get configured hotkeys
    let initial_settings = load_initial_settings();

    // Create shortcuts from settings (with fallbacks to defaults)
    let toggle_shortcut = initial_settings
        .toggle_hotkey
        .to_shortcut_or_default(HotkeyConfig::default_toggle);
    let hold_shortcut = initial_settings
        .hold_hotkey
        .to_shortcut_or_default(HotkeyConfig::default_hold);
    let paste_last_shortcut = initial_settings
        .paste_last_hotkey
        .to_shortcut_or_default(HotkeyConfig::default_paste_last);

    log::info!(
        "Registering shortcuts - Toggle: {}, Hold: {}, PasteLast: {}",
        initial_settings.toggle_hotkey.to_shortcut_string(),
        initial_settings.hold_hotkey.to_shortcut_string(),
        initial_settings.paste_last_hotkey.to_shortcut_string()
    );

    tauri_plugin_global_shortcut::Builder::new()
        .with_shortcuts([toggle_shortcut, hold_shortcut, paste_last_shortcut])
        .expect("Failed to register global shortcuts - check if another instance is running")
        .with_handler(|app, shortcut, event| {
            handle_shortcut_event(app, shortcut, &event);
        })
        .build()
}
