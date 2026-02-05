use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[cfg(desktop)]
use tauri_plugin_global_shortcut::Shortcut;

// ============================================================================
// DEFAULT SETTINGS CONSTANTS - Single source of truth for all defaults
// ============================================================================

/// Default server URL
pub const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:8765";

// ============================================================================
// DEFAULT HOTKEY CONSTANTS - Single source of truth for all default hotkeys
// ============================================================================

/// Default modifiers for all hotkeys
pub const DEFAULT_HOTKEY_MODIFIERS: &[&str] = &["ctrl", "alt"];

/// Default key for toggle recording (Ctrl+Alt+Space)
pub const DEFAULT_TOGGLE_KEY: &str = "Space";

/// Default key for hold-to-record (Ctrl+Alt+Backquote)
pub const DEFAULT_HOLD_KEY: &str = "Backquote";

/// Default key for paste last transcription (Ctrl+Alt+.)
pub const DEFAULT_PASTE_LAST_KEY: &str = "Period";

// ============================================================================
// STORE KEY ENUM - Type-safe access to settings.json keys
// ============================================================================

/// Store keys for settings.json - provides type-safe access to settings
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreKey {
    /// Toggle hotkey configuration
    ToggleHotkey,
    /// Hold hotkey configuration
    HoldHotkey,
    /// Paste-last hotkey configuration
    PasteLastHotkey,
    /// Selected microphone ID
    SelectedMicId,
    /// Sound enabled setting
    SoundEnabled,
    /// Cleanup prompt sections
    CleanupPromptSections,
    /// STT provider selection
    SttProvider,
    /// LLM provider selection
    LlmProvider,
    /// Auto-mute audio setting
    AutoMuteAudio,
    /// STT timeout in seconds
    SttTimeoutSeconds,
    /// Server URL
    ServerUrl,
    /// LLM formatting enabled (true = format with LLM, false = raw transcription)
    LlmFormattingEnabled,
}

impl StoreKey {
    /// Returns the string key used in settings.json
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ToggleHotkey => "toggle_hotkey",
            Self::HoldHotkey => "hold_hotkey",
            Self::PasteLastHotkey => "paste_last_hotkey",
            Self::SelectedMicId => "selected_mic_id",
            Self::SoundEnabled => "sound_enabled",
            Self::CleanupPromptSections => "cleanup_prompt_sections",
            Self::SttProvider => "stt_provider",
            Self::LlmProvider => "llm_provider",
            Self::AutoMuteAudio => "auto_mute_audio",
            Self::SttTimeoutSeconds => "stt_timeout_seconds",
            Self::ServerUrl => "server_url",
            Self::LlmFormattingEnabled => "llm_formatting_enabled",
        }
    }
}

// ============================================================================

/// Enable boolean field by default (needed for serde)
fn default_enabled() -> bool {
    true
}
/// Configuration for a hotkey combination
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HotkeyConfig {
    /// Modifier keys (e.g., `["ctrl", "alt"]`)
    pub modifiers: Vec<String>,
    /// The main key (e.g., "Space")
    pub key: String,
    /// Whether the hotkey is enabled (default: true)
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self::default_with_key(DEFAULT_TOGGLE_KEY)
    }
}

impl HotkeyConfig {
    /// Internal helper to create a default hotkey config with a specific key
    fn default_with_key(key: &str) -> Self {
        Self {
            modifiers: DEFAULT_HOTKEY_MODIFIERS
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
            key: key.to_string(),
            enabled: true,
        }
    }

    /// Create default toggle hotkey config
    pub fn default_toggle() -> Self {
        Self::default_with_key(DEFAULT_TOGGLE_KEY)
    }

    /// Create default hold hotkey config
    pub fn default_hold() -> Self {
        Self::default_with_key(DEFAULT_HOLD_KEY)
    }

    /// Create default paste-last hotkey config
    pub fn default_paste_last() -> Self {
        Self::default_with_key(DEFAULT_PASTE_LAST_KEY)
    }

    /// Convert to shortcut string format like "ctrl+alt+Space"
    /// Note: modifiers must be lowercase for the parser to recognize them
    pub fn to_shortcut_string(&self) -> String {
        let mut parts: Vec<String> = self.modifiers.iter().map(|m| m.to_lowercase()).collect();
        parts.push(self.key.clone());
        parts.join("+")
    }

    /// Convert to a tauri Shortcut using `FromStr` parsing
    #[cfg(desktop)]
    pub fn to_shortcut(&self) -> Result<Shortcut, String> {
        let shortcut_str = self.to_shortcut_string();
        Shortcut::from_str(&shortcut_str)
            .map_err(|e| format!("Failed to parse shortcut '{shortcut_str}': {e:?}"))
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

    /// Check if two hotkey configs are equivalent (case-insensitive comparison)
    pub fn is_same_as(&self, other: &HotkeyConfig) -> bool {
        if self.key.to_lowercase() != other.key.to_lowercase() {
            return false;
        }
        if self.modifiers.len() != other.modifiers.len() {
            return false;
        }
        self.modifiers.iter().all(|mod_a| {
            other
                .modifiers
                .iter()
                .any(|mod_b| mod_a.to_lowercase() == mod_b.to_lowercase())
        })
    }
}

// ============================================================================
// PROMPT SECTION TYPES
// ============================================================================

/// Mode of prompt: auto (let server optimize) or manual (custom content)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "mode")]
pub enum PromptMode {
    /// Let the server optimize the prompt
    #[serde(rename = "auto")]
    Auto,
    /// Use custom content provided by the user
    #[serde(rename = "manual")]
    Manual { content: String },
}

/// Configuration for a single prompt section.
/// Two-layer structure: enabled status + mode (auto/manual)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PromptSection {
    pub enabled: bool,
    #[serde(rename = "mode")]
    pub prompt_mode: PromptMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptSectionType {
    Main,
    Advanced,
    Dictionary,
}

impl PromptSectionType {
    pub const ALL: [Self; 3] = [Self::Main, Self::Advanced, Self::Dictionary];

    /// String representation used in file exports/imports
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Advanced => "advanced",
            Self::Dictionary => "dictionary",
        }
    }
}

impl FromStr for PromptSectionType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "main" => Ok(Self::Main),
            "advanced" => Ok(Self::Advanced),
            "dictionary" => Ok(Self::Dictionary),
            _ => Err(format!("Unknown prompt section: {s}")),
        }
    }
}

/// Configuration for all cleanup prompt sections
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CleanupPromptSections {
    pub main: PromptSection,
    pub advanced: PromptSection,
    pub dictionary: PromptSection,
}

impl Default for CleanupPromptSections {
    fn default() -> Self {
        Self {
            main: PromptSection {
                enabled: true,
                prompt_mode: PromptMode::Auto,
            },
            advanced: PromptSection {
                enabled: true,
                prompt_mode: PromptMode::Auto,
            },
            dictionary: PromptSection {
                enabled: true,
                prompt_mode: PromptMode::Auto,
            },
        }
    }
}

impl CleanupPromptSections {
    pub fn get(&self, section_type: PromptSectionType) -> &PromptSection {
        match section_type {
            PromptSectionType::Main => &self.main,
            PromptSectionType::Advanced => &self.advanced,
            PromptSectionType::Dictionary => &self.dictionary,
        }
    }

    pub fn set(&mut self, section_type: PromptSectionType, section: PromptSection) {
        match section_type {
            PromptSectionType::Main => self.main = section,
            PromptSectionType::Advanced => self.advanced = section,
            PromptSectionType::Dictionary => self.dictionary = section,
        }
    }
}

// ============================================================================
// APP SETTINGS - Complete settings structure
// ============================================================================

/// Complete application settings matching the TypeScript `AppSettings` interface
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub toggle_hotkey: HotkeyConfig,
    pub hold_hotkey: HotkeyConfig,
    pub paste_last_hotkey: HotkeyConfig,
    pub selected_mic_id: Option<String>,
    pub sound_enabled: bool,
    #[serde(default)]
    pub cleanup_prompt_sections: Option<CleanupPromptSections>,
    pub stt_provider: String,
    pub llm_provider: String,
    pub auto_mute_audio: bool,
    pub stt_timeout_seconds: Option<f64>,
    pub server_url: String,
    /// LLM formatting enabled (true = format with LLM, false = raw transcription)
    #[serde(default = "default_enabled")]
    pub llm_formatting_enabled: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            toggle_hotkey: HotkeyConfig::default_toggle(),
            hold_hotkey: HotkeyConfig::default_hold(),
            paste_last_hotkey: HotkeyConfig::default_paste_last(),
            selected_mic_id: None,
            sound_enabled: true,
            cleanup_prompt_sections: None,
            stt_provider: "auto".to_string(),
            llm_provider: "auto".to_string(),
            auto_mute_audio: false,
            stt_timeout_seconds: None,
            server_url: DEFAULT_SERVER_URL.to_string(),
            llm_formatting_enabled: true,
        }
    }
}

// ============================================================================
// SETTINGS ERRORS
// ============================================================================

/// Type of hotkey for error reporting
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyType {
    Toggle,
    Hold,
    PasteLast,
}

impl HotkeyType {
    pub fn store_key(self) -> StoreKey {
        match self {
            Self::Toggle => StoreKey::ToggleHotkey,
            Self::Hold => StoreKey::HoldHotkey,
            Self::PasteLast => StoreKey::PasteLastHotkey,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Toggle => "toggle",
            Self::Hold => "hold",
            Self::PasteLast => "paste last",
        }
    }

    pub fn default_hotkey(self) -> HotkeyConfig {
        match self {
            Self::Toggle => HotkeyConfig::default_toggle(),
            Self::Hold => HotkeyConfig::default_hold(),
            Self::PasteLast => HotkeyConfig::default_paste_last(),
        }
    }
}

/// Errors that can occur during settings operations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum SettingsError {
    /// Hotkey conflicts with another existing hotkey
    HotkeyConflict {
        message: String,
        conflicting_type: HotkeyType,
    },
    /// Invalid value for a field
    InvalidValue { field: String, message: String },
    /// Error accessing the store
    StoreError(String),
}

impl std::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SettingsError::HotkeyConflict { message, .. } => write!(f, "{message}"),
            SettingsError::InvalidValue { field, message } => {
                write!(f, "Invalid value for {field}: {message}")
            }
            SettingsError::StoreError(msg) => write!(f, "Store error: {msg}"),
        }
    }
}

impl std::error::Error for SettingsError {}

/// Check if a hotkey conflicts with any existing hotkeys (excluding the one being updated)
pub fn check_hotkey_conflict(
    new_hotkey: &HotkeyConfig,
    settings: &AppSettings,
    exclude_type: HotkeyType,
) -> Option<SettingsError> {
    let hotkeys_to_check = [
        (HotkeyType::Toggle, &settings.toggle_hotkey),
        (HotkeyType::Hold, &settings.hold_hotkey),
        (HotkeyType::PasteLast, &settings.paste_last_hotkey),
    ];

    for (hotkey_type, existing) in hotkeys_to_check {
        if hotkey_type != exclude_type && new_hotkey.is_same_as(existing) {
            return Some(SettingsError::HotkeyConflict {
                message: format!(
                    "This shortcut is already used for the {} hotkey",
                    hotkey_type.display_name()
                ),
                conflicting_type: hotkey_type,
            });
        }
    }
    None
}
