use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use crate::settings::{LocalOnlySetting, DEFAULT_SERVER_URL};

/// Delay after clipboard operations to ensure system stability
const CLIPBOARD_STABILIZATION_DELAY_MS: u64 = 50;

/// Delay between keyboard key press and release events
const KEY_EVENT_DELAY_MS: u64 = 50;

/// Delay before restoring previous clipboard content
const CLIPBOARD_RESTORE_DELAY_MS: u64 = 100;

#[tauri::command]
pub async fn get_server_url(app: AppHandle) -> Result<String, String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    let url = store
        .get(LocalOnlySetting::ServerUrl.storage_key_name())
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| DEFAULT_SERVER_URL.to_string());
    Ok(url)
}

#[tauri::command]
pub async fn type_text(app: AppHandle, text: String) -> Result<(), String> {
    // macOS HIToolbox APIs (used by enigo) must run on the main thread
    // Use a channel to get the result back from the main thread
    let (tx, rx) = mpsc::channel::<Result<(), String>>();

    app.run_on_main_thread(move || {
        let result = type_text_blocking(&text);
        let _ = tx.send(result);
    })
    .map_err(|e| e.to_string())?;

    // Wait for result from main thread
    rx.recv().map_err(|e| e.to_string())?
}

/// Type text using clipboard and paste. Used internally by shortcut handlers.
pub fn type_text_blocking(text: &str) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;

    // Save previous clipboard content
    let previous = clipboard.get_text().unwrap_or_default();

    // Set new text
    clipboard.set_text(text).map_err(|e| e.to_string())?;

    // Small delay for clipboard to stabilize
    thread::sleep(Duration::from_millis(CLIPBOARD_STABILIZATION_DELAY_MS));

    // Simulate Ctrl+V / Cmd+V
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;

    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| e.to_string())?;
    thread::sleep(Duration::from_millis(KEY_EVENT_DELAY_MS));
    enigo
        .key(Key::Unicode('v'), Direction::Click)
        .map_err(|e| e.to_string())?;
    thread::sleep(Duration::from_millis(KEY_EVENT_DELAY_MS));
    enigo
        .key(modifier, Direction::Release)
        .map_err(|e| e.to_string())?;

    // Restore previous clipboard after a delay
    thread::sleep(Duration::from_millis(CLIPBOARD_RESTORE_DELAY_MS));
    let _ = clipboard.set_text(&previous);

    Ok(())
}
