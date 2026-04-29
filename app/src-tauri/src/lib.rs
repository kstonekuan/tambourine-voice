use anyhow::Context;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};
use tauri_utils::config::BackgroundThrottlingPolicy;

mod active_app_context;
mod audio;
mod audio_mute;
mod commands;
mod config_sync;
pub mod events;
mod history;

use active_app_context::get_current_active_app_context;
use events::{EventName, RecordingStartFailedPayload};
mod mic_capture;
mod settings;
mod state;

#[cfg(test)]
mod tests;

use audio_mute::AudioMuteManager;
use history::HistoryStorage;
use mic_capture::{AudioDeviceInfo, MicCapture, MicCaptureManager};
use settings::{HotkeyConfig, HotkeyType, LocalOnlySetting, SettingClass};
use state::{AppState, ShortcutState};

#[cfg(desktop)]
enum StartRecordingError {
    UnavailableWhileConnecting,
    Other(String),
}

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

/// Returns whether Rust currently has confirmed server connectivity.
///
/// Shortcut handling runs in a sync context, so we use `try_read` to avoid
/// blocking if another task currently holds the config-sync lock.
#[cfg(desktop)]
fn is_server_connected_for_shortcuts(app: &AppHandle) -> bool {
    let Some(config_sync_state) = app.try_state::<config_sync::ConfigSync>() else {
        return false;
    };

    let Ok(config_sync_guard) = config_sync_state.try_read() else {
        log::debug!(
            "Unable to read config sync state during shortcut handling; treating as disconnected"
        );
        return false;
    };

    config_sync_guard.is_connected()
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
    let toggle_hotkey: HotkeyConfig = get_setting_from_store(
        app,
        LocalOnlySetting::ToggleHotkey,
        HotkeyConfig::default_toggle(),
    );
    let hold_hotkey: HotkeyConfig = get_setting_from_store(
        app,
        LocalOnlySetting::HoldHotkey,
        HotkeyConfig::default_hold(),
    );
    let paste_last_hotkey: HotkeyConfig = get_setting_from_store(
        app,
        LocalOnlySetting::PasteLastHotkey,
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
    setting_class: SettingClass,
    value: &T,
) -> anyhow::Result<()> {
    let storage_key_name = setting_class.storage_key_name();
    let store = app
        .store("settings.json")
        .with_context(|| format!("Failed to get settings store for '{storage_key_name}'"))?;
    let json_value = serde_json::to_value(value)
        .with_context(|| format!("Failed to serialize setting value for '{storage_key_name}'"))?;
    store.set(storage_key_name, json_value); // set() returns ()
    store
        .save()
        .with_context(|| format!("Failed to save settings store for '{storage_key_name}'"))?;
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
) -> Result<(), StartRecordingError> {
    log::info!("{source}: starting recording");

    // If we're still connecting/reconnecting, don't play start sound or emit
    // recording-start. Play an explicit unavailable sound instead.
    if !is_server_connected_for_shortcuts(app) {
        if sound_enabled {
            audio::play_sound(audio::SoundType::Unavailable);
        }
        return Err(StartRecordingError::UnavailableWhileConnecting);
    }

    let recording_start_committed = auto_mute_audio.then(|| {
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false))
    });

    // Play start sound without blocking event emission. If auto-mute is
    // enabled, defer the mute work to a background thread so the start event
    // can be emitted immediately while still giving the sound a short head
    // start before muting the system output.
    if sound_enabled {
        if auto_mute_audio {
            use std::sync::{mpsc, Arc};
            use std::time::Duration as StdDuration;

            let (tx, rx) = mpsc::channel();
            // Request playback with notify (audio module spawns its own thread)
            audio::play_sound_with_notify(audio::SoundType::Start, Some(tx));

            let app_for_mute = app.clone();
            let source_for_mute = source.to_string();
            let recording_start_committed_for_thread = recording_start_committed
                .as_ref()
                .map(Arc::clone)
                .expect("auto_mute_audio implies a commit flag exists");
            std::thread::spawn(move || {
                use std::io::Write;

                let playback_started = wait_for_recording_start_commit(
                    rx,
                    recording_start_committed_for_thread,
                    StdDuration::from_millis(250),
                    StdDuration::from_millis(500),
                    None,
                );

                if !playback_started {
                    return;
                }

                let should_mute = if let Some(app_state) = app_for_mute.try_state::<AppState>() {
                    let shortcut_state = app_state.shortcut_state.lock().unwrap_or_else(|error| {
                        panic!("Failed to lock shortcut state while deferring mute: {error}")
                    });
                    matches!(
                        *shortcut_state,
                        ShortcutState::PreparingToRecordViaToggle
                            | ShortcutState::RecordingViaToggle
                            | ShortcutState::RecordingViaHold
                    )
                } else {
                    false
                };

                if should_mute {
                    if let Some(audio_mute_manager) = app_for_mute.try_state::<AudioMuteManager>() {
                        if let Err(mute_error) = audio_mute_manager.mute() {
                            log::warn!("Failed to mute system audio for recording start: {mute_error}");
                        }
                    }
                }

                if let Ok(app_data_dir) = app_for_mute.path().app_data_dir() {
                    let _ = std::fs::create_dir_all(&app_data_dir);
                    let log_path = app_data_dir.join("e2e_playback_timestamps.log");
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&log_path)
                    {
                        let _ = writeln!(
                            f,
                            "{},{},{}",
                            chrono::Utc::now().to_rfc3339(),
                            if playback_started { "playback_started" } else { "playback_timeout" },
                            source_for_mute
                        );
                        if should_mute {
                            let _ = writeln!(
                                f,
                                "{},{},{}",
                                chrono::Utc::now().to_rfc3339(),
                                "muted",
                                source_for_mute
                            );
                        }
                    }
                }
            });
        } else {
            // Non-auto-mute path: just play without waiting
            audio::play_sound(audio::SoundType::Start);
        }
    }

    let mut mute_manager_used_for_start_attempt: Option<&AudioMuteManager> = None;
    if auto_mute_audio {
        // The actual mute happens asynchronously after the start sound begins.
        // We still keep this block so the start path can fail early when mute
        // support is unavailable on the current platform.
        let required_audio_mute_manager = audio_mute_manager.ok_or_else(|| {
            StartRecordingError::Other(
                "Mute-audio setting is enabled, but audio mute is unavailable on this system"
                    .to_string(),
            )
        })?;
        mute_manager_used_for_start_attempt = Some(required_audio_mute_manager);
    }

    if let Err(emit_error) = app.emit(EventName::RecordingStart.as_str(), ()) {
        if let Some(mute_manager) = mute_manager_used_for_start_attempt {
            if let Err(unmute_error) = mute_manager.unmute() {
                return Err(StartRecordingError::Other(format!(
                    "Failed to emit recording-start event: {emit_error}. \
                     Additionally failed to restore system audio mute state: {unmute_error}"
                )));
            }
        }
        return Err(StartRecordingError::Other(format!(
            "Failed to emit recording-start event: {emit_error}"
        )));
    }

    if let Some(recording_start_committed) = &recording_start_committed {
        recording_start_committed.store(true, std::sync::atomic::Ordering::Release);
    }

    Ok(())
}

#[cfg(desktop)]
fn wait_for_recording_start_commit(
    playback_started_rx: std::sync::mpsc::Receiver<()>,
    recording_start_committed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    playback_started_wait: std::time::Duration,
    commit_wait: std::time::Duration,
    wait_started_notify: Option<std::sync::mpsc::Sender<()>>,
) -> bool {
    use std::sync::atomic::Ordering;
    use std::time::Duration as StdDuration;

    if let Some(wait_started_notify) = wait_started_notify {
        let _ = wait_started_notify.send(());
    }

    let playback_started = playback_started_rx.recv_timeout(playback_started_wait).is_ok();

    let mut waited_for_commit = StdDuration::ZERO;
    while !recording_start_committed.load(Ordering::Acquire) && waited_for_commit < commit_wait {
        std::thread::sleep(StdDuration::from_millis(10));
        waited_for_commit += StdDuration::from_millis(10);
    }

    playback_started && recording_start_committed.load(Ordering::Acquire)
}

#[cfg(desktop)]
fn emit_recording_start_failed(app: &AppHandle, error: String, source: &str) {
    log::warn!("{source}: recording start aborted: {error}");
    if let Err(emit_error) = app.emit(
        EventName::RecordingStartFailed.as_str(),
        RecordingStartFailedPayload { error },
    ) {
        log::warn!("{source}: failed to emit recording-start-failed event: {emit_error}");
    }
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
        audio::play_sound(audio::SoundType::Stop);
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
    let sound_enabled: bool = get_setting_from_store(app, LocalOnlySetting::SoundEnabled, true);
    let auto_mute_audio: bool = get_setting_from_store(app, LocalOnlySetting::AutoMuteAudio, false);
    let audio_mute_manager = app.try_state::<AudioMuteManager>();

    // Lock the state for the duration of the transition
    let mut current_state = state.shortcut_state.lock().unwrap();

    *current_state = match (&*current_state, shortcut_event) {
        (ShortcutState::Idle, ShortcutEvent::TogglePressed) => {
            if is_server_connected_for_shortcuts(app) {
                let _ = app.emit(EventName::PrepareRecording.as_str(), ());
                ShortcutState::PreparingToRecordViaToggle
            } else {
                if sound_enabled {
                    audio::play_sound(audio::SoundType::Unavailable);
                }
                ShortcutState::Idle
            }
        }
        (ShortcutState::PreparingToRecordViaToggle, ShortcutEvent::ToggleReleased) => {
            let recording_start_result = start_recording(
                app,
                sound_enabled,
                audio_mute_manager.as_deref(),
                auto_mute_audio,
                "Toggle",
            );
            match recording_start_result {
                Ok(()) => ShortcutState::RecordingViaToggle,
                Err(StartRecordingError::UnavailableWhileConnecting) => ShortcutState::Idle,
                Err(StartRecordingError::Other(error)) => {
                    emit_recording_start_failed(app, error, "Toggle");
                    ShortcutState::Idle
                }
            }
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
            let recording_start_result = start_recording(
                app,
                sound_enabled,
                audio_mute_manager.as_deref(),
                auto_mute_audio,
                "Hold",
            );
            match recording_start_result {
                Ok(()) => ShortcutState::RecordingViaHold,
                Err(StartRecordingError::UnavailableWhileConnecting) => ShortcutState::Idle,
                Err(StartRecordingError::Other(error)) => {
                    emit_recording_start_failed(app, error, "Hold");
                    ShortcutState::Idle
                }
            }
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

/// Get current active app context snapshot
#[tauri::command]
fn active_app_get_current_context(app: AppHandle) -> active_app_context::ActiveAppContextSnapshot {
    #[cfg(target_os = "macos")]
    {
        use std::sync::mpsc;
        use std::time::Duration;

        let (snapshot_sender, snapshot_receiver) =
            mpsc::sync_channel::<active_app_context::ActiveAppContextSnapshot>(1);

        if let Err(error) = app.run_on_main_thread(move || {
            let snapshot = get_current_active_app_context();
            let _ = snapshot_sender.send(snapshot);
        }) {
            log::warn!(
                "Failed to dispatch focus snapshot to macOS main thread: {error}. Returning fallback active app context."
            );
            return fallback_active_app_context_snapshot();
        }

        snapshot_receiver
            .recv_timeout(Duration::from_millis(150))
            .unwrap_or_else(|error| {
                log::warn!(
                    "Timed out waiting for macOS focus snapshot on main thread: {error}. Returning fallback active app context."
                );
                fallback_active_app_context_snapshot()
            })
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        get_current_active_app_context()
    }
}

#[cfg(target_os = "macos")]
fn fallback_active_app_context_snapshot() -> active_app_context::ActiveAppContextSnapshot {
    active_app_context::ActiveAppContextSnapshot {
        focused_application: None,
        focused_window: None,
        focused_browser_tab: None,
        event_source: active_app_context::FocusEventSource::Unknown,
        confidence_level: active_app_context::FocusConfidenceLevel::Low,
        captured_at: chrono::Utc::now().to_rfc3339(),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
#[allow(clippy::too_many_lines)]
pub fn run() {
    // Initialize logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|_app, _argv, _cwd| {
            // Intentionally avoid showing/focusing windows on duplicate launch.
            // The second process should terminate without side effects.
            log::warn!("Ignoring duplicate app launch; primary instance remains active");
        }));
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
        .plugin(tauri_plugin_os::init())
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
            commands::settings::update_llm_timeout_raw_fallback_enabled,
            commands::settings::update_send_active_app_context_enabled,
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
            active_app_get_current_context,
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

            #[cfg(desktop)]
            {
                let send_active_app_context_enabled = get_setting_from_store(
                    app.handle(),
                    LocalOnlySetting::SendActiveAppContextEnabled,
                    false,
                );
                if let Err(error) = commands::settings::reconcile_focus_watcher_enabled_state(
                    app.handle(),
                    send_active_app_context_enabled,
                ) {
                    log::warn!(
                        "Failed to reconcile focus watcher lifecycle during startup: {error:#}"
                    );
                }
            }

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
                        log::error!("[NSPanel] Failed to convert overlay to NSPanel: {e:?}");
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
    let mut toggle_hotkey: HotkeyConfig = get_setting_from_store(
        app,
        LocalOnlySetting::ToggleHotkey,
        HotkeyConfig::default_toggle(),
    );
    let mut hold_hotkey: HotkeyConfig = get_setting_from_store(
        app,
        LocalOnlySetting::HoldHotkey,
        HotkeyConfig::default_hold(),
    );
    let mut paste_last_hotkey: HotkeyConfig = get_setting_from_store(
        app,
        LocalOnlySetting::PasteLastHotkey,
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
                        local_only_setting: LocalOnlySetting,
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
                let _ = save_setting_to_store(app, local_only_setting.into(), hotkey);
            }
        }
    };

    try_register(
        &mut toggle_hotkey,
        "Toggle",
        LocalOnlySetting::ToggleHotkey,
        HotkeyConfig::default_toggle,
        &mut result.toggle_registered,
        &mut result.errors.toggle_error,
    );
    try_register(
        &mut hold_hotkey,
        "Hold",
        LocalOnlySetting::HoldHotkey,
        HotkeyConfig::default_hold,
        &mut result.hold_registered,
        &mut result.errors.hold_error,
    );
    try_register(
        &mut paste_last_hotkey,
        "PasteLast",
        LocalOnlySetting::PasteLastHotkey,
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
