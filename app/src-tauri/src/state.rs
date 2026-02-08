use serde::{Deserialize, Serialize};
use std::sync::{Mutex, RwLock};

use crate::active_app_context::FocusWatcherHandle;

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

pub struct AppState {
    pub shortcut_state: Mutex<ShortcutState>,
    pub shortcut_errors: RwLock<ShortcutErrors>,
    pub focus_watcher: Mutex<Option<FocusWatcherHandle>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            shortcut_state: Mutex::new(ShortcutState::default()),
            shortcut_errors: RwLock::new(ShortcutErrors::default()),
            focus_watcher: Mutex::new(None),
        }
    }
}
