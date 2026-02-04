use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use tauri_plugin_http::reqwest::Client;
use tokio::sync::RwLock;

use crate::settings::CleanupPromptSections;

/// Default STT timeout in seconds (matches server's `DEFAULT_TRANSCRIPTION_WAIT_TIMEOUT_SECONDS`)
pub const DEFAULT_STT_TIMEOUT_SECONDS: f64 = 0.5;

/// Tracks server connection state for config syncing
pub struct ConfigSyncState {
    client: Client,
    server_url: Option<String>,
    client_uuid: Option<String>,
}

impl Default for ConfigSyncState {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigSyncState {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("Failed to create HTTP client"),
            server_url: None,
            client_uuid: None,
        }
    }

    /// Set connection info when connected to server
    pub fn set_connected(&mut self, server_url: String, client_uuid: String) {
        log::info!("Config sync connected: {server_url} (uuid: {client_uuid})");
        self.server_url = Some(server_url);
        self.client_uuid = Some(client_uuid);
    }

    /// Clear connection info when disconnected
    pub fn set_disconnected(&mut self) {
        self.server_url = None;
        self.client_uuid = None;
        log::info!("Config sync disconnected");
    }

    /// Check if connected to a server
    pub fn is_connected(&self) -> bool {
        self.server_url.is_some() && self.client_uuid.is_some()
    }

    /// Sync prompt sections to server (best-effort, logs errors)
    pub async fn sync_prompt_sections(
        &self,
        sections: &CleanupPromptSections,
    ) -> Result<(), String> {
        let (Some(url), Some(uuid)) = (&self.server_url, &self.client_uuid) else {
            return Ok(()); // Not connected, skip silently
        };

        self.client
            .put(format!("{url}/api/config/prompts"))
            .header("X-Client-UUID", uuid)
            .json(sections)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;

        log::debug!("Synced prompt sections to server");
        Ok(())
    }

    /// Sync STT timeout to server
    pub async fn sync_stt_timeout(&self, timeout_seconds: f64) -> Result<(), String> {
        #[derive(Serialize)]
        struct TimeoutBody {
            timeout_seconds: f64,
        }

        let (Some(url), Some(uuid)) = (&self.server_url, &self.client_uuid) else {
            return Ok(()); // Not connected, skip silently
        };

        self.client
            .put(format!("{url}/api/config/stt-timeout"))
            .header("X-Client-UUID", uuid)
            .json(&TimeoutBody { timeout_seconds })
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;

        log::debug!("Synced STT timeout ({timeout_seconds}) to server");
        Ok(())
    }

    /// Sync LLM formatting enabled setting to server
    pub async fn sync_llm_formatting_enabled(&self, enabled: bool) -> Result<(), String> {
        #[derive(Serialize)]
        struct LlmFormattingBody {
            enabled: bool,
        }

        let (Some(url), Some(uuid)) = (&self.server_url, &self.client_uuid) else {
            return Ok(()); // Not connected, skip silently
        };

        self.client
            .put(format!("{url}/api/config/llm-formatting"))
            .header("X-Client-UUID", uuid)
            .json(&LlmFormattingBody { enabled })
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;

        log::debug!("Synced LLM formatting enabled={enabled} to server");
        Ok(())
    }
}

pub type ConfigSync = Arc<RwLock<ConfigSyncState>>;

pub fn new_config_sync() -> ConfigSync {
    Arc::new(RwLock::new(ConfigSyncState::new()))
}
