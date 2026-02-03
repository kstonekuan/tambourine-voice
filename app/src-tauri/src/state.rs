use serde::{Deserialize, Serialize};
use std::sync::{Mutex, RwLock};

/// State machine for shortcut handling.
///
/// This enum represents all valid states the shortcut system can be in,
/// preventing invalid state combinations that were possible with separate booleans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShortcutState {
    /// No keys pressed, not recording
    #[default]
    Idle,
    /// Toggle key held, mic warming up (preparing to record)
    PreparingToggle,
    /// Recording via toggle mode (press-release to start, press-release to stop)
    RecordingToggle,
    /// Recording via hold mode (hold to record, release to stop)
    RecordingHold,
    /// Paste key held, waiting for release to paste
    PastePending,
}

impl ShortcutState {
    /// Check if currently in a recording state (either toggle or hold mode)
    #[allow(dead_code)] // Provided for external use
    pub fn is_recording(self) -> bool {
        matches!(self, Self::RecordingToggle | Self::RecordingHold)
    }
}

/// Tracks errors from shortcut registration attempts
#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShortcutErrors {
    /// Error message if toggle shortcut failed to register
    pub toggle_error: Option<String>,
    /// Error message if hold shortcut failed to register
    pub hold_error: Option<String>,
    /// Error message if `paste_last` shortcut failed to register
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
    /// Current state of the shortcut state machine
    pub shortcut_state: Mutex<ShortcutState>,
    /// Tracks errors from shortcut registration attempts
    pub shortcut_errors: RwLock<ShortcutErrors>,
}
