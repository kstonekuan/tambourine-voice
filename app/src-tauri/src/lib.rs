use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};
use tauri_utils::config::BackgroundThrottlingPolicy;

mod audio;
mod audio_mute;
mod commands;
mod config_sync;
pub mod events;
mod history;

use events::EventName;
mod mic_capture;
mod settings;
mod state;

#[cfg(test)]
mod tests;

use audio_mute::AudioMuteManager;
use history::HistoryStorage;
use mic_capture::{AudioDeviceInfo, MicCapture, MicCaptureManager};
use settings::{HotkeyConfig, HotkeyType, StoreKey};
use state::{AppState, ShortcutState};

#[cfg(desktop)]
use tauri_plugin_store::StoreExt;

#[cfg(desktop)]
use tauri_plugin_global_shortcut::{
    Shortcut, ShortcutEvent as TauriShortcutEvent, ShortcutState as TauriShortcutState,
};

#[cfg(desktop)]
use commands::settings::get_setting_from_store;

/// Events that can trigger state transitions in the shortcut state machine
#[cfg(desktop)]
#[derive(Debug, Clone, Copy)]
pub enum ShortcutEvent {
    TogglePressed,
    ToggleReleased,
    HoldPressed,
    HoldReleased,
    PastePressed,
    PasteReleased,
}

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
/// Also handles Tauri's "keyX" format for letter keys (e.g., "keyv" -> "v")
#[cfg(desktop)]
pub(crate) fn normalize_shortcut_string(s: &str) -> String {
    let normalized = s
        .to_lowercase()
        .replace("ctrl", "control")
        .replace("cmd", "super")
        .replace("meta", "super")
        .replace("win", "super");

    // Handle Tauri's "keyX" format for letter keys (e.g., "control+alt+keyv" -> "control+alt+v")
    // Split by '+', normalize each part, rejoin
    normalized
        .split('+')
        .map(|part| {
            // If part starts with "key" and is followed by a single letter, strip the "key" prefix
            if part.starts_with("key") && part.len() == 4 {
                &part[3..]
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// Get the normalized shortcut string for a hotkey config, falling back to default if invalid
#[cfg(desktop)]
fn get_normalized_shortcut_string(
    hotkey: &HotkeyConfig,
    default_fn: fn() -> HotkeyConfig,
) -> String {
    let shortcut_str = hotkey.to_shortcut().map_or_else(
        |_| default_fn().to_shortcut_string(),
        |_| hotkey.to_shortcut_string(),
    );
    normalize_shortcut_string(&shortcut_str)
}

/// Match a shortcut string against configured hotkeys
#[cfg(desktop)]
fn match_hotkey(app: &AppHandle, shortcut_str: &str) -> Option<HotkeyType> {
    let toggle_hotkey: HotkeyConfig =
        get_setting_from_store(app, StoreKey::ToggleHotkey, HotkeyConfig::default_toggle());
    let hold_hotkey: HotkeyConfig =
        get_setting_from_store(app, StoreKey::HoldHotkey, HotkeyConfig::default_hold());
    let paste_last_hotkey: HotkeyConfig = get_setting_from_store(
        app,
        StoreKey::PasteLastHotkey,
        HotkeyConfig::default_paste_last(),
    );

    if shortcut_str == get_normalized_shortcut_string(&toggle_hotkey, HotkeyConfig::default_toggle)
    {
        Some(HotkeyType::Toggle)
    } else if shortcut_str
        == get_normalized_shortcut_string(&hold_hotkey, HotkeyConfig::default_hold)
    {
        Some(HotkeyType::Hold)
    } else if shortcut_str
        == get_normalized_shortcut_string(&paste_last_hotkey, HotkeyConfig::default_paste_last)
    {
        Some(HotkeyType::PasteLast)
    } else {
        None
    }
}

/// Save a setting to the store
#[cfg(desktop)]
pub(crate) fn save_setting_to_store<T: serde::Serialize>(
    app: &AppHandle,
    key: StoreKey,
    value: &T,
) -> Result<(), String> {
    let store = app
        .store("settings.json")
        .map_err(|e| format!("Failed to get store: {e}"))?;
    let json_value = serde_json::to_value(value).map_err(|e| e.to_string())?;
    store.set(key.as_str(), json_value); // set() returns ()
    store
        .save()
        .map_err(|e| format!("Failed to save store: {e}"))?;
    Ok(())
}

/// Start recording with sound and audio mute handling
#[cfg(desktop)]
fn start_recording(
    app: &AppHandle,
    sound_enabled: bool,
    audio_mute_manager: Option<&AudioMuteManager>,
    auto_mute_audio: bool,
    source: &str,
) {
    log::info!("{source}: starting recording");
    // Play sound BEFORE muting so it's audible
    if sound_enabled {
        audio::play_sound(audio::SoundType::RecordingStart);
        // Brief delay to let sound play before muting
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
    if auto_mute_audio {
        if let Some(manager) = audio_mute_manager {
            if let Err(e) = manager.mute() {
                log::warn!("Failed to mute audio: {e}");
            }
        }
    }
    let _ = app.emit(EventName::RecordingStart.as_str(), ());
}

/// Stop recording with sound and audio unmute handling
#[cfg(desktop)]
fn stop_recording(
    app: &AppHandle,
    sound_enabled: bool,
    audio_mute_manager: Option<&AudioMuteManager>,
    auto_mute_audio: bool,
    source: &str,
) {
    log::info!("{source}: stopping recording");
    // Unmute system audio if it was muted
    if auto_mute_audio {
        if let Some(manager) = audio_mute_manager {
            if let Err(e) = manager.unmute() {
                log::warn!("Failed to unmute audio: {e}");
            }
        }
    }
    if sound_enabled {
        audio::play_sound(audio::SoundType::RecordingStop);
    }
    let _ = app.emit(EventName::RecordingStop.as_str(), ());
}

/// Paste the last transcription from history
#[cfg(desktop)]
fn paste_last_transcription(app: &AppHandle) {
    log::info!("PasteLast: pasting last transcription");
    let history_storage = app.state::<HistoryStorage>();

    if let Ok(entries) = history_storage.get_all(Some(1)) {
        if let Some(entry) = entries.first() {
            if let Err(e) = commands::text::type_text_blocking(&entry.text) {
                log::error!("Failed to paste last transcription: {e}");
            }
        } else {
            log::info!("PasteLast: no history entries available");
        }
    }
}

/// Map a Tauri shortcut event to our internal `ShortcutEvent` type.
/// Returns None if the shortcut doesn't match any configured hotkey.
#[cfg(desktop)]
fn map_to_shortcut_event(
    app: &AppHandle,
    shortcut: &Shortcut,
    event: TauriShortcutEvent,
) -> Option<ShortcutEvent> {
    let shortcut_str = normalize_shortcut_string(&shortcut.to_string());

    let Some(matched) = match_hotkey(app, &shortcut_str) else {
        log::warn!("Unknown shortcut: {shortcut_str}");
        return None;
    };

    Some(match (matched, event.state) {
        (HotkeyType::Toggle, TauriShortcutState::Pressed) => ShortcutEvent::TogglePressed,
        (HotkeyType::Toggle, TauriShortcutState::Released) => ShortcutEvent::ToggleReleased,
        (HotkeyType::Hold, TauriShortcutState::Pressed) => ShortcutEvent::HoldPressed,
        (HotkeyType::Hold, TauriShortcutState::Released) => ShortcutEvent::HoldReleased,
        (HotkeyType::PasteLast, TauriShortcutState::Pressed) => ShortcutEvent::PastePressed,
        (HotkeyType::PasteLast, TauriShortcutState::Released) => ShortcutEvent::PasteReleased,
    })
}

/// Handle a shortcut event using a state machine.
///
/// This function implements clean state transitions based on the current state
/// and the incoming event. Invalid states are unrepresentable by design.
#[cfg(desktop)]
pub fn handle_shortcut_event(app: &AppHandle, shortcut: &Shortcut, event: TauriShortcutEvent) {
    // Map the Tauri event to our internal event type
    let Some(shortcut_event) = map_to_shortcut_event(app, shortcut, event) else {
        return;
    };

    // Get application state and settings
    let state = app.state::<AppState>();
    let sound_enabled: bool = get_setting_from_store(app, StoreKey::SoundEnabled, true);
    let auto_mute_audio: bool = get_setting_from_store(app, StoreKey::AutoMuteAudio, false);
    let audio_mute_manager = app.try_state::<AudioMuteManager>();

    // Lock the state for the duration of the transition
    let mut current_state = state.shortcut_state.lock().unwrap();

    *current_state = match (&*current_state, shortcut_event) {
        (ShortcutState::Idle, ShortcutEvent::TogglePressed) => {
            let _ = app.emit(EventName::PrepareRecording.as_str(), ());
            ShortcutState::PreparingToRecordViaToggle
        }
        (ShortcutState::PreparingToRecordViaToggle, ShortcutEvent::ToggleReleased) => {
            start_recording(
                app,
                sound_enabled,
                audio_mute_manager.as_deref(),
                auto_mute_audio,
                "Toggle",
            );
            ShortcutState::RecordingViaToggle
        }
        (ShortcutState::RecordingViaToggle, ShortcutEvent::TogglePressed) => {
            ShortcutState::RecordingViaToggle
        }
        (ShortcutState::RecordingViaToggle, ShortcutEvent::ToggleReleased) => {
            stop_recording(
                app,
                sound_enabled,
                audio_mute_manager.as_deref(),
                auto_mute_audio,
                "Toggle",
            );
            ShortcutState::Idle
        }
        (ShortcutState::Idle, ShortcutEvent::HoldPressed) => {
            start_recording(
                app,
                sound_enabled,
                audio_mute_manager.as_deref(),
                auto_mute_audio,
                "Hold",
            );
            ShortcutState::RecordingViaHold
        }
        (ShortcutState::RecordingViaHold, ShortcutEvent::HoldReleased) => {
            stop_recording(
                app,
                sound_enabled,
                audio_mute_manager.as_deref(),
                auto_mute_audio,
                "Hold",
            );
            ShortcutState::Idle
        }
        (ShortcutState::RecordingViaHold, ShortcutEvent::HoldPressed) => {
            ShortcutState::RecordingViaHold
        }
        (
            ShortcutState::Idle | ShortcutState::WaitingForPasteKeyRelease,
            ShortcutEvent::PastePressed,
        ) => ShortcutState::WaitingForPasteKeyRelease,
        (ShortcutState::WaitingForPasteKeyRelease, ShortcutEvent::PasteReleased) => {
            paste_last_transcription(app);
            ShortcutState::Idle
        }
        (ShortcutState::PreparingToRecordViaToggle, ShortcutEvent::TogglePressed) => {
            ShortcutState::PreparingToRecordViaToggle
        }
        (current, event) => {
            log::trace!("Ignoring event {event:?} in state {current:?}");
            *current
        }
    };
}

/// Check if audio mute is supported on this platform
#[tauri::command]
fn is_audio_mute_supported() -> bool {
    audio_mute::is_supported()
}

/// Start native microphone capture
#[tauri::command]
fn start_native_mic(
    state: tauri::State<'_, MicCaptureManager>,
    device_id: Option<String>,
) -> Result<(), String> {
    state
        .capture()
        .start(device_id.as_deref())
        .map_err(|e| e.to_string())
}

/// Stop native microphone capture
#[tauri::command]
fn stop_native_mic(state: tauri::State<'_, MicCaptureManager>) {
    state.capture().stop();
}

/// Pause native microphone capture (stream stays alive for fast resume)
#[tauri::command]
fn pause_native_mic(state: tauri::State<'_, MicCaptureManager>) {
    state.capture().pause();
}

/// Resume native microphone capture after pause
#[tauri::command]
fn resume_native_mic(state: tauri::State<'_, MicCaptureManager>) {
    state.capture().resume();
}

/// List available native audio input devices with ID and name
#[tauri::command]
fn list_native_mic_devices(state: tauri::State<'_, MicCaptureManager>) -> Vec<AudioDeviceInfo> {
    state.capture().list_devices()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
#[allow(clippy::too_many_lines)]
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
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(AppState::default())
        .manage(config_sync::new_config_sync())
        .invoke_handler(tauri::generate_handler![
            commands::text::type_text,
            commands::text::get_server_url,
            commands::settings::register_shortcuts,
            commands::settings::unregister_shortcuts,
            commands::settings::get_shortcut_errors,
            commands::settings::set_hotkey_enabled,
            commands::settings::get_settings,
            commands::settings::update_hotkey,
            commands::settings::update_selected_mic,
            commands::settings::update_sound_enabled,
            commands::settings::update_cleanup_prompt_sections,
            commands::settings::update_stt_provider,
            commands::settings::update_llm_provider,
            commands::settings::update_auto_mute_audio,
            commands::settings::update_stt_timeout,
            commands::settings::update_server_url,
            commands::settings::update_llm_formatting_enabled,
            commands::settings::reset_hotkeys_to_defaults,
            is_audio_mute_supported,
            commands::history::add_history_entry,
            commands::history::get_history,
            commands::history::delete_history_entry,
            commands::history::clear_history,
            commands::export_import::generate_settings_export,
            commands::export_import::generate_history_export,
            commands::export_import::generate_prompt_exports,
            commands::export_import::parse_prompt_file,
            commands::export_import::import_prompt,
            commands::export_import::detect_export_file_type,
            commands::export_import::import_settings,
            commands::export_import::import_history,
            commands::export_import::factory_reset,
            commands::overlay::resize_overlay,
            commands::config_sync::set_server_connected,
            commands::config_sync::set_server_disconnected,
            start_native_mic,
            stop_native_mic,
            pause_native_mic,
            resume_native_mic,
            list_native_mic_devices,
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

            // Initialize native mic capture manager
            // Audio data is streamed to frontend via "native-audio-data" events
            let app_handle = app.handle().clone();
            let mic_capture_manager = MicCaptureManager::new(move |audio_data| {
                let _ = app_handle.emit(EventName::NativeAudioData.as_str(), audio_data);
            });
            app.manage(mic_capture_manager);

            // Register shortcuts from store (now that store plugin is available)
            // This function handles errors gracefully - it never fails the app startup
            #[cfg(desktop)]
            {
                register_initial_shortcuts(app.handle());
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
                        // - can_join_all_spaces: appears on all Spaces (virtual desktops)
                        // - full_screen_auxiliary: works alongside fullscreen apps
                        // - ignores_cycle: excluded from Cmd+Tab app cycling
                        let behavior = CollectionBehavior::new()
                            .can_join_all_spaces()
                            .full_screen_auxiliary()
                            .ignores_cycle();
                        panel.set_collection_behavior(behavior.value());

                        // Set style mask to non-activating panel
                        let style = tauri_nspanel::StyleMask::empty().nonactivating_panel();
                        panel.set_style_mask(style.value());

                        // Force the panel to re-register with the window server after setting behaviors
                        // A hide/show cycle is more reliable than order_front_regardless alone
                        // This mimics what happens when dragging the window - the window server
                        // re-evaluates and properly applies the collection behavior
                        panel.hide();
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        panel.show();
                        panel.order_front_regardless();

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
                // Truncation is intentional: pixel coordinates don't need sub-pixel precision
                #[allow(clippy::cast_possible_truncation)]
                let x = (f64::from(size.width) / scale) as i32 - 150;
                #[allow(clippy::cast_possible_truncation)]
                let y = (f64::from(size.height) / scale) as i32 - 100;
                let _ = overlay.set_position(tauri::Position::Logical(tauri::LogicalPosition {
                    x: f64::from(x),
                    y: f64::from(y),
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
                // Emit disconnect request to frontend before exiting
                if let Some(window) = app.get_webview_window("overlay") {
                    let _ = window.emit(EventName::RequestDisconnect.as_str(), ());
                }
                // Give frontend time to disconnect gracefully
                std::thread::sleep(std::time::Duration::from_millis(500));
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
    // Just initialize the plugin - shortcuts will be registered in setup() after store is available
    tauri_plugin_global_shortcut::Builder::new().build()
}

/// Core shortcut registration logic - used by both initial startup and re-registration command
#[cfg(desktop)]
pub(crate) fn do_register_shortcuts(app: &AppHandle) -> state::ShortcutRegistrationResult {
    use state::{ShortcutErrors, ShortcutRegistrationResult};
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    // Read hotkeys from store with defaults
    let mut toggle_hotkey: HotkeyConfig =
        get_setting_from_store(app, StoreKey::ToggleHotkey, HotkeyConfig::default_toggle());
    let mut hold_hotkey: HotkeyConfig =
        get_setting_from_store(app, StoreKey::HoldHotkey, HotkeyConfig::default_hold());
    let mut paste_last_hotkey: HotkeyConfig = get_setting_from_store(
        app,
        StoreKey::PasteLastHotkey,
        HotkeyConfig::default_paste_last(),
    );

    log::info!(
        "Registering shortcuts - Toggle: {} (enabled: {}), Hold: {} (enabled: {}), PasteLast: {} (enabled: {})",
        toggle_hotkey.to_shortcut_string(),
        toggle_hotkey.enabled,
        hold_hotkey.to_shortcut_string(),
        hold_hotkey.enabled,
        paste_last_hotkey.to_shortcut_string(),
        paste_last_hotkey.enabled
    );

    let shortcut_manager = app.global_shortcut();
    let _ = shortcut_manager.unregister_all();

    let mut result = ShortcutRegistrationResult {
        toggle_registered: false,
        hold_registered: false,
        paste_last_registered: false,
        errors: ShortcutErrors::default(),
    };

    // Helper to try registering a single shortcut
    let try_register = |hotkey: &mut HotkeyConfig,
                        name: &str,
                        store_key: StoreKey,
                        default_fn: fn() -> HotkeyConfig,
                        registered: &mut bool,
                        error: &mut Option<String>| {
        if !hotkey.enabled {
            log::info!("{name} shortcut is disabled, skipping");
            return;
        }

        let shortcut = hotkey.to_shortcut_or_default(default_fn);
        match shortcut_manager.on_shortcut(shortcut, |app_handle, shortcut, event| {
            handle_shortcut_event(app_handle, shortcut, event);
        }) {
            Ok(()) => {
                *registered = true;
                log::info!("{name} shortcut registered");
            }
            Err(e) => {
                *error = Some(format!("Hotkey conflict: {e}"));
                log::warn!("Failed to register {name} shortcut: {e}. Auto-disabling.");
                hotkey.enabled = false;
                let _ = save_setting_to_store(app, store_key, hotkey);
            }
        }
    };

    try_register(
        &mut toggle_hotkey,
        "Toggle",
        StoreKey::ToggleHotkey,
        HotkeyConfig::default_toggle,
        &mut result.toggle_registered,
        &mut result.errors.toggle_error,
    );
    try_register(
        &mut hold_hotkey,
        "Hold",
        StoreKey::HoldHotkey,
        HotkeyConfig::default_hold,
        &mut result.hold_registered,
        &mut result.errors.hold_error,
    );
    try_register(
        &mut paste_last_hotkey,
        "PasteLast",
        StoreKey::PasteLastHotkey,
        HotkeyConfig::default_paste_last,
        &mut result.paste_last_registered,
        &mut result.errors.paste_last_error,
    );

    // Store errors in app state
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut shortcut_errors) = state.shortcut_errors.write() {
            *shortcut_errors = result.errors.clone();
        }
    }

    result
}

/// Register shortcuts from store settings (called from `setup()` after store plugin is available)
#[cfg(desktop)]
fn register_initial_shortcuts(app: &AppHandle) {
    let result = do_register_shortcuts(app);
    if result.errors.has_any_error() {
        log::warn!("Some shortcuts failed to register. Check settings to resolve conflicts.");
    } else {
        log::info!("All shortcuts registered successfully");
    }
}
