use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::RwLock;

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
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            modifiers: DEFAULT_HOTKEY_MODIFIERS
                .iter()
                .map(|s| s.to_string())
                .collect(),
            key: DEFAULT_TOGGLE_KEY.to_string(),
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

    /// Check if this hotkey has the same key combination as another
    pub fn is_same_as(&self, other: &HotkeyConfig) -> bool {
        self.key.eq_ignore_ascii_case(&other.key)
            && self.modifiers.len() == other.modifiers.len()
            && self
                .modifiers
                .iter()
                .all(|m| other.modifiers.iter().any(|o| m.eq_ignore_ascii_case(o)))
    }
}

/// Configuration for a single prompt section
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PromptSection {
    /// Whether this section is enabled
    pub enabled: bool,
    /// Custom content (None = use default)
    pub content: Option<String>,
}

impl Default for PromptSection {
    fn default() -> Self {
        Self {
            enabled: true,
            content: None,
        }
    }
}

/// Configuration for all cleanup prompt sections
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CleanupPromptSections {
    /// Main prompt section (core rules, punctuation, new lines)
    pub main: PromptSection,
    /// Advanced features section (backtrack corrections, list formatting)
    pub advanced: PromptSection,
    /// Personal dictionary section (word mappings)
    pub dictionary: PromptSection,
}

impl Default for CleanupPromptSections {
    fn default() -> Self {
        Self {
            main: PromptSection {
                enabled: true,
                content: None,
            },
            advanced: PromptSection {
                enabled: true,
                content: None,
            },
            dictionary: PromptSection {
                enabled: false,
                content: None,
            },
        }
    }
}

/// Application settings that are persisted to disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// Hotkey for toggle recording mode
    #[serde(default = "default_toggle_hotkey")]
    pub toggle_hotkey: HotkeyConfig,

    /// Hotkey for hold-to-record mode
    #[serde(default = "default_hold_hotkey")]
    pub hold_hotkey: HotkeyConfig,

    /// Hotkey for paste last transcription
    #[serde(default = "default_paste_last_hotkey")]
    pub paste_last_hotkey: HotkeyConfig,

    /// Selected microphone device ID (None = system default)
    #[serde(default)]
    pub selected_mic_id: Option<String>,

    /// Whether sound feedback is enabled
    #[serde(default = "default_sound_enabled")]
    pub sound_enabled: bool,

    /// Cleanup prompt sections configuration
    #[serde(default)]
    pub cleanup_prompt_sections: Option<CleanupPromptSections>,

    /// Selected STT provider (None = use server default)
    #[serde(default)]
    pub stt_provider: Option<String>,

    /// Selected LLM provider (None = use server default)
    #[serde(default)]
    pub llm_provider: Option<String>,

    /// Whether to automatically mute system audio during recording
    #[serde(default)]
    pub auto_mute_audio: bool,

    /// STT timeout in seconds (None = use server default)
    #[serde(default)]
    pub stt_timeout_seconds: Option<f64>,
}

fn default_toggle_hotkey() -> HotkeyConfig {
    HotkeyConfig::default_toggle()
}

fn default_hold_hotkey() -> HotkeyConfig {
    HotkeyConfig::default_hold()
}

fn default_paste_last_hotkey() -> HotkeyConfig {
    HotkeyConfig::default_paste_last()
}

fn default_sound_enabled() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            toggle_hotkey: default_toggle_hotkey(),
            hold_hotkey: default_hold_hotkey(),
            paste_last_hotkey: default_paste_last_hotkey(),
            selected_mic_id: None,
            sound_enabled: true,
            cleanup_prompt_sections: None,
            stt_provider: None,
            llm_provider: None,
            auto_mute_audio: false,
            stt_timeout_seconds: None,
        }
    }
}

/// Manages loading and saving of application settings
pub struct SettingsManager {
    settings: RwLock<AppSettings>,
    file_path: PathBuf,
}

impl SettingsManager {
    /// Create a new settings manager with the given app data directory
    pub fn new(app_data_dir: PathBuf) -> Self {
        let file_path = app_data_dir.join("settings.json");

        // Ensure the directory exists
        if let Some(parent) = file_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        // Load existing settings or use defaults
        let settings = Self::load_from_file(&file_path).unwrap_or_default();

        Self {
            settings: RwLock::new(settings),
            file_path,
        }
    }

    /// Load settings from the JSON file
    fn load_from_file(file_path: &PathBuf) -> Option<AppSettings> {
        let content = fs::read_to_string(file_path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Save current settings to disk
    pub fn save(&self) -> Result<(), String> {
        let settings = self
            .settings
            .read()
            .map_err(|e| format!("Failed to read settings: {}", e))?;

        let content = serde_json::to_string_pretty(&*settings)
            .map_err(|e| format!("Failed to serialize settings: {}", e))?;

        fs::write(&self.file_path, content)
            .map_err(|e| format!("Failed to write settings file: {}", e))?;

        Ok(())
    }

    /// Get a copy of the current settings
    pub fn get(&self) -> Result<AppSettings, String> {
        self.settings
            .read()
            .map(|s| s.clone())
            .map_err(|e| format!("Failed to read settings: {}", e))
    }

    /// Update settings and save to disk
    pub fn update(&self, new_settings: AppSettings) -> Result<(), String> {
        {
            let mut settings = self
                .settings
                .write()
                .map_err(|e| format!("Failed to write settings: {}", e))?;
            *settings = new_settings;
        }
        self.save()
    }

    /// Update the toggle hotkey
    pub fn update_toggle_hotkey(&self, hotkey: HotkeyConfig) -> Result<(), String> {
        {
            let mut settings = self
                .settings
                .write()
                .map_err(|e| format!("Failed to write settings: {}", e))?;
            settings.toggle_hotkey = hotkey;
        }
        self.save()
    }

    /// Update the hold hotkey
    pub fn update_hold_hotkey(&self, hotkey: HotkeyConfig) -> Result<(), String> {
        {
            let mut settings = self
                .settings
                .write()
                .map_err(|e| format!("Failed to write settings: {}", e))?;
            settings.hold_hotkey = hotkey;
        }
        self.save()
    }

    /// Update the paste last hotkey
    pub fn update_paste_last_hotkey(&self, hotkey: HotkeyConfig) -> Result<(), String> {
        {
            let mut settings = self
                .settings
                .write()
                .map_err(|e| format!("Failed to write settings: {}", e))?;
            settings.paste_last_hotkey = hotkey;
        }
        self.save()
    }

    /// Update the selected microphone
    pub fn update_selected_mic(&self, mic_id: Option<String>) -> Result<(), String> {
        {
            let mut settings = self
                .settings
                .write()
                .map_err(|e| format!("Failed to write settings: {}", e))?;
            settings.selected_mic_id = mic_id;
        }
        self.save()
    }

    /// Update sound enabled setting
    pub fn update_sound_enabled(&self, enabled: bool) -> Result<(), String> {
        {
            let mut settings = self
                .settings
                .write()
                .map_err(|e| format!("Failed to write settings: {}", e))?;
            settings.sound_enabled = enabled;
        }
        self.save()
    }

    /// Update the cleanup prompt sections setting
    pub fn update_cleanup_prompt_sections(
        &self,
        sections: Option<CleanupPromptSections>,
    ) -> Result<(), String> {
        {
            let mut settings = self
                .settings
                .write()
                .map_err(|e| format!("Failed to write settings: {}", e))?;
            settings.cleanup_prompt_sections = sections;
        }
        self.save()
    }

    /// Update the STT provider setting
    pub fn update_stt_provider(&self, provider: Option<String>) -> Result<(), String> {
        {
            let mut settings = self
                .settings
                .write()
                .map_err(|e| format!("Failed to write settings: {}", e))?;
            settings.stt_provider = provider;
        }
        self.save()
    }

    /// Update the LLM provider setting
    pub fn update_llm_provider(&self, provider: Option<String>) -> Result<(), String> {
        {
            let mut settings = self
                .settings
                .write()
                .map_err(|e| format!("Failed to write settings: {}", e))?;
            settings.llm_provider = provider;
        }
        self.save()
    }

    /// Update the auto mute audio setting
    pub fn update_auto_mute_audio(&self, enabled: bool) -> Result<(), String> {
        {
            let mut settings = self
                .settings
                .write()
                .map_err(|e| format!("Failed to write settings: {}", e))?;
            settings.auto_mute_audio = enabled;
        }
        self.save()
    }

    /// Update the STT timeout setting
    pub fn update_stt_timeout(&self, timeout_seconds: Option<f64>) -> Result<(), String> {
        {
            let mut settings = self
                .settings
                .write()
                .map_err(|e| format!("Failed to write settings: {}", e))?;
            settings.stt_timeout_seconds = timeout_seconds;
        }
        self.save()
    }
}
