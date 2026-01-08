use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicBool;
use std::sync::RwLock;

/// Tracks errors from shortcut registration attempts
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShortcutErrors {
    /// Error message if toggle shortcut failed to register
    pub toggle_error: Option<String>,
    /// Error message if hold shortcut failed to register
    pub hold_error: Option<String>,
    /// Error message if paste_last shortcut failed to register
    pub paste_last_error: Option<String>,
}

impl ShortcutErrors {
    /// Check if any shortcut has an error
    pub fn has_any_error(&self) -> bool {
        self.toggle_error.is_some() || self.hold_error.is_some() || self.paste_last_error.is_some()
    }
}

/// Result of shortcut registration attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutRegistrationResult {
    pub toggle_registered: bool,
    pub hold_registered: bool,
    pub paste_last_registered: bool,
    pub errors: ShortcutErrors,
}

#[derive(Default)]
pub struct AppState {
    /// Tracks if currently recording (for both toggle and hold modes)
    pub is_recording: AtomicBool,
    /// Tracks if PTT key is currently held down (for hold-to-record mode)
    pub ptt_key_held: AtomicBool,
    /// Tracks if paste-last key is currently held down
    pub paste_key_held: AtomicBool,
    /// Tracks if toggle key is currently held down (for debouncing - action happens on release)
    pub toggle_key_held: AtomicBool,
    /// Tracks errors from shortcut registration attempts
    pub shortcut_errors: RwLock<ShortcutErrors>,
}
