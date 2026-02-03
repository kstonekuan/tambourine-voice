use serde::{Deserialize, Serialize};
use std::sync::{Mutex, RwLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShortcutState {
    #[default]
    Idle,
    PreparingToRecordViaToggle,
    RecordingViaToggle,
    RecordingViaHold,
    WaitingForPasteKeyRelease,
}

#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShortcutErrors {
    pub toggle_error: Option<String>,
    pub hold_error: Option<String>,
    pub paste_last_error: Option<String>,
}

impl ShortcutErrors {
    pub fn has_any_error(&self) -> bool {
        self.toggle_error.is_some() || self.hold_error.is_some() || self.paste_last_error.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutRegistrationResult {
    pub toggle_registered: bool,
    pub hold_registered: bool,
    pub paste_last_registered: bool,
    pub errors: ShortcutErrors,
}

#[derive(Default)]
pub struct AppState {
    pub shortcut_state: Mutex<ShortcutState>,
    pub shortcut_errors: RwLock<ShortcutErrors>,
}
