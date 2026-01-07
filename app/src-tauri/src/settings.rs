use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[cfg(desktop)]
use tauri_plugin_global_shortcut::Shortcut;

// ============================================================================
// DEFAULT HOTKEY CONSTANTS - Single source of truth for all default hotkeys
// ============================================================================

/// Default modifiers for all hotkeys
pub const DEFAULT_HOTKEY_MODIFIERS: &[&str] = &["ctrl", "alt"];

/// Default key for toggle recording (Ctrl+Alt+Space)
pub const DEFAULT_TOGGLE_KEY: &str = "Space";

/// Default key for hold-to-record (Ctrl+Alt+`)
pub const DEFAULT_HOLD_KEY: &str = "Backquote";

/// Default key for paste last transcription (Ctrl+Alt+.)
pub const DEFAULT_PASTE_LAST_KEY: &str = "Period";

// ============================================================================

/// Configuration for a hotkey combination
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HotkeyConfig {
    /// Modifier keys (e.g., ["ctrl", "alt"])
    pub modifiers: Vec<String>,
    /// The main key (e.g., "Space")
    pub key: String,
    /// Whether the hotkey is enabled (default: true)
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// Default value for enabled field (used by serde)
fn default_enabled() -> bool {
    true
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            modifiers: DEFAULT_HOTKEY_MODIFIERS
                .iter()
                .map(|s| s.to_string())
                .collect(),
            key: DEFAULT_TOGGLE_KEY.to_string(),
            enabled: true,
        }
    }
}

impl HotkeyConfig {
    /// Create default toggle hotkey config
    pub fn default_toggle() -> Self {
        Self {
            modifiers: DEFAULT_HOTKEY_MODIFIERS
                .iter()
                .map(|s| s.to_string())
                .collect(),
            key: DEFAULT_TOGGLE_KEY.to_string(),
            enabled: true,
        }
    }

    /// Create default hold hotkey config
    pub fn default_hold() -> Self {
        Self {
            modifiers: DEFAULT_HOTKEY_MODIFIERS
                .iter()
                .map(|s| s.to_string())
                .collect(),
            key: DEFAULT_HOLD_KEY.to_string(),
            enabled: true,
        }
    }

    /// Create default paste-last hotkey config
    pub fn default_paste_last() -> Self {
        Self {
            modifiers: DEFAULT_HOTKEY_MODIFIERS
                .iter()
                .map(|s| s.to_string())
                .collect(),
            key: DEFAULT_PASTE_LAST_KEY.to_string(),
            enabled: true,
        }
    }

    /// Convert to shortcut string format like "ctrl+alt+Space"
    /// Note: modifiers must be lowercase for the parser to recognize them
    pub fn to_shortcut_string(&self) -> String {
        let mut parts: Vec<String> = self.modifiers.iter().map(|m| m.to_lowercase()).collect();
        parts.push(self.key.clone());
        parts.join("+")
    }

    /// Convert to a tauri Shortcut using FromStr parsing
    #[cfg(desktop)]
    pub fn to_shortcut(&self) -> Result<Shortcut, String> {
        let shortcut_str = self.to_shortcut_string();
        Shortcut::from_str(&shortcut_str)
            .map_err(|e| format!("Failed to parse shortcut '{}': {:?}", shortcut_str, e))
    }

    /// Convert to a tauri Shortcut, falling back to a default if parsing fails
    #[cfg(desktop)]
    pub fn to_shortcut_or_default(&self, default_fn: fn() -> Self) -> Shortcut {
        self.to_shortcut().unwrap_or_else(|_| {
            default_fn()
                .to_shortcut()
                .expect("Default hotkey must be valid")
        })
    }
}
