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

pub mod names {
    // Rust → All: Hotkey triggers
    pub const RECORDING_START: &str = "recording-start";
    pub const RECORDING_STOP: &str = "recording-stop";
    pub const PREPARE_RECORDING: &str = "prepare-recording";

    // Rust → All: Config sync notifications
    pub const CONFIG_RESPONSE: &str = "config-response";

    // Rust → Overlay: Disconnect request on app quit
    pub const REQUEST_DISCONNECT: &str = "request-disconnect";

    // Main → Overlay: Settings changed, refetch needed
    pub const SETTINGS_CHANGED: &str = "settings-changed";

    // Main → Overlay: Request reconnection
    pub const RECONNECT_REQUEST: &str = "request-reconnect";

    // Overlay → Main: Connection state updates
    pub const CONNECTION_STATE: &str = "connection-state-changed";

    // Overlay → Main: Reconnection progress
    pub const RECONNECT_STARTED: &str = "reconnect-started";
    pub const RECONNECT_RESULT: &str = "reconnect-result";

    // Rust → All: History changed
    pub const HISTORY_CHANGED: &str = "history-changed";
}

// =============================================================================
// Config Setting Names - Must match src/lib/events.ts
// =============================================================================

pub mod config_settings {
    pub const PROMPT_SECTIONS: &str = "prompt-sections";
    pub const STT_TIMEOUT: &str = "stt-timeout";
    pub const STT_PROVIDER: &str = "stt-provider";
    pub const LLM_PROVIDER: &str = "llm-provider";
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
    pub fn updated(setting: &str, value: T) -> Self {
        Self::Updated {
            setting: setting.to_string(),
            value,
        }
    }

    pub fn error(setting: &str, error: impl ToString) -> ConfigResponse<()> {
        ConfigResponse::Error {
            setting: setting.to_string(),
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
