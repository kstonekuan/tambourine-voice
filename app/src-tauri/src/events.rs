//! Type-safe event system for inter-window communication.
//!
//! Events are broadcast to all windows via Tauri's event system.
//! This module provides constants and types for event names and payloads.
//!
//! IMPORTANT: Event names and payload types must match the TypeScript side.
//! See: src/lib/events.ts

use serde::Serialize;

// =============================================================================
// Event Names - Must match src/lib/events.ts
// =============================================================================

/// Type-safe event names for Tauri event emission.
/// Use `EventName::*.as_str()` when calling `app.emit()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventName {
    /// Rust → All: Recording started
    RecordingStart,
    /// Rust → All: Recording stopped
    RecordingStop,
    /// Rust → All: Prepare for recording (mic warmup)
    PrepareRecording,
    /// Rust → All: Config sync response
    ConfigResponse,
    /// Rust → Overlay: Disconnect request on app quit
    RequestDisconnect,
    /// Main → Overlay: Settings changed, refetch needed
    SettingsChanged,
    /// Main → Overlay: Request reconnection
    ReconnectRequest,
    /// Overlay → Main: Connection state updates
    ConnectionState,
    /// Overlay → Main: Reconnection started
    ReconnectStarted,
    /// Overlay → Main: Reconnection result
    ReconnectResult,
    /// Rust → All: History changed
    HistoryChanged,
    /// Rust → Overlay: Native audio data from mic capture
    NativeAudioData,
}

impl EventName {
    /// Returns the string representation for Tauri event emission.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RecordingStart => "recording-start",
            Self::RecordingStop => "recording-stop",
            Self::PrepareRecording => "prepare-recording",
            Self::ConfigResponse => "config-response",
            Self::RequestDisconnect => "request-disconnect",
            Self::SettingsChanged => "settings-changed",
            Self::ReconnectRequest => "request-reconnect",
            Self::ConnectionState => "connection-state-changed",
            Self::ReconnectStarted => "reconnect-started",
            Self::ReconnectResult => "reconnect-result",
            Self::HistoryChanged => "history-changed",
            Self::NativeAudioData => "native-audio-data",
        }
    }
}

// =============================================================================
// Config Setting Names - Must match src/lib/events.ts
// =============================================================================

/// Type-safe config setting names for config sync responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSetting {
    PromptSections,
    SttTimeout,
    SttProvider,
    LlmProvider,
}

impl ConfigSetting {
    /// Returns the string representation for config response payloads.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PromptSections => "prompt-sections",
            Self::SttTimeout => "stt-timeout",
            Self::SttProvider => "stt-provider",
            Self::LlmProvider => "llm-provider",
        }
    }
}

// =============================================================================
// Event Payloads
// =============================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ConfigResponse<T: Serialize> {
    #[serde(rename = "config-updated")]
    Updated { setting: String, value: T },
    #[serde(rename = "config-error")]
    Error { setting: String, error: String },
}

impl<T: Serialize> ConfigResponse<T> {
    pub fn updated(setting: ConfigSetting, value: T) -> Self {
        Self::Updated {
            setting: setting.as_str().to_string(),
            value,
        }
    }

    pub fn error(setting: ConfigSetting, error: impl ToString) -> ConfigResponse<()> {
        ConfigResponse::Error {
            setting: setting.as_str().to_string(),
            error: error.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ConnectionStatePayload {
    pub state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReconnectResultPayload {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
