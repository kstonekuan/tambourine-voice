use std::sync::atomic::Ordering;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};

mod audio;
mod audio_mute;
mod commands;
mod history;
mod settings;
mod state;

use audio_mute::AudioMuteManager;
use history::HistoryStorage;
use settings::SettingsManager;
use state::AppState;

#[cfg(desktop)]
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, ShortcutState};

/// Check if audio mute is supported on this platform
#[tauri::command]
fn is_audio_mute_supported() -> bool {
    audio_mute::is_supported()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder.plugin(build_global_shortcut_plugin());
    }

    builder
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::text::type_text,
            commands::text::get_server_url,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::update_toggle_hotkey,
            commands::settings::update_hold_hotkey,
            commands::settings::update_selected_mic,
            commands::settings::update_sound_enabled,
            commands::settings::update_cleanup_prompt,
            commands::settings::update_stt_provider,
            commands::settings::update_llm_provider,
            commands::settings::update_auto_mute_audio,
            is_audio_mute_supported,
            commands::history::add_history_entry,
            commands::history::get_history,
            commands::history::delete_history_entry,
            commands::history::clear_history,
            commands::overlay::resize_overlay,
        ])
        .setup(|app| {
            // Initialize settings manager and history storage
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data directory");

            let settings_manager = SettingsManager::new(app_data_dir.clone());
            app.manage(settings_manager);

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
            .build()?;

            // Position bottom-right
            if let Ok(Some(monitor)) = overlay.current_monitor() {
                let size = monitor.size();
                let scale = monitor.scale_factor();
                let x = (size.width as f64 / scale) as i32 - 100;
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

    let _tray = TrayIconBuilder::new()
        .icon(
            app.default_window_icon()
                .ok_or("No default window icon configured")?
                .clone(),
        )
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
    // Define shortcuts
    let toggle_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Space);
    let hold_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Period);

    tauri_plugin_global_shortcut::Builder::new()
        .with_shortcuts([toggle_shortcut, hold_shortcut])
        .expect("Failed to register global shortcuts - check if another instance is running")
        .with_handler(move |app, shortcut, event| {
            let state = app.state::<AppState>();
            let settings_manager = app.state::<SettingsManager>();

            // Check if sound is enabled
            let settings = settings_manager.get().ok();
            let sound_enabled = settings.as_ref().map(|s| s.sound_enabled).unwrap_or(true);
            let auto_mute_audio = settings
                .as_ref()
                .map(|s| s.auto_mute_audio)
                .unwrap_or(false);

            // Get audio mute manager if available
            let audio_mute_manager = app.try_state::<AudioMuteManager>();

            // Helper to mute audio
            let mute_audio = || {
                if auto_mute_audio {
                    if let Some(manager) = &audio_mute_manager {
                        if let Err(e) = manager.mute() {
                            log::warn!("Failed to mute audio: {}", e);
                        }
                    }
                }
            };

            // Helper to unmute audio
            let unmute_audio = || {
                if auto_mute_audio {
                    if let Some(manager) = &audio_mute_manager {
                        if let Err(e) = manager.unmute() {
                            log::warn!("Failed to unmute audio: {}", e);
                        }
                    }
                }
            };

            let toggle_shortcut =
                Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Space);
            let hold_shortcut =
                Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Period);

            if shortcut == &toggle_shortcut {
                // Toggle mode: only respond to Pressed, ignore Released
                if matches!(event.state, ShortcutState::Pressed) {
                    let is_recording = state.is_recording.load(Ordering::SeqCst);

                    if is_recording {
                        // Stop recording
                        state.is_recording.store(false, Ordering::SeqCst);
                        log::info!("Toggle: stopping recording");
                        unmute_audio();
                        if sound_enabled {
                            audio::play_sound(audio::SoundType::RecordingStop);
                        }
                        let _ = app.emit("recording-stop", ());
                    } else {
                        // Start recording
                        state.is_recording.store(true, Ordering::SeqCst);
                        log::info!("Toggle: starting recording");
                        // Play sound BEFORE muting so it's audible
                        if sound_enabled {
                            audio::play_sound(audio::SoundType::RecordingStart);
                            // Brief delay to let sound play before muting
                            std::thread::sleep(std::time::Duration::from_millis(150));
                        }
                        mute_audio();
                        let _ = app.emit("recording-start", ());
                    }
                }
            } else if shortcut == &hold_shortcut {
                // Hold-to-Record: respond to both Pressed and Released
                match event.state {
                    ShortcutState::Pressed => {
                        // Use swap to detect first press vs OS key repeat
                        if !state.ptt_key_held.swap(true, Ordering::SeqCst) {
                            // First press - start recording
                            state.is_recording.store(true, Ordering::SeqCst);
                            log::info!("Hold: starting recording");
                            // Play sound BEFORE muting so it's audible
                            if sound_enabled {
                                audio::play_sound(audio::SoundType::RecordingStart);
                                // Brief delay to let sound play before muting
                                std::thread::sleep(std::time::Duration::from_millis(150));
                            }
                            mute_audio();
                            let _ = app.emit("recording-start", ());
                        }
                    }
                    ShortcutState::Released => {
                        if state.ptt_key_held.swap(false, Ordering::SeqCst) {
                            // Key released - stop recording
                            state.is_recording.store(false, Ordering::SeqCst);
                            log::info!("Hold: stopping recording");
                            unmute_audio();
                            if sound_enabled {
                                audio::play_sound(audio::SoundType::RecordingStop);
                            }
                            let _ = app.emit("recording-stop", ());
                        }
                    }
                }
            }
        })
        .build()
}
