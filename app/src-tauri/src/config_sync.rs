use anyhow::{Context, Result};
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
    pub async fn sync_prompt_sections(&self, sections: &CleanupPromptSections) -> Result<()> {
        let (Some(server_url), Some(client_uuid)) = (&self.server_url, &self.client_uuid) else {
            return Ok(()); // Not connected, skip silently
        };

        let endpoint_url = format!("{server_url}/api/config/prompts");
        self.client
            .put(&endpoint_url)
            .header("X-Client-UUID", client_uuid)
            .json(sections)
            .send()
            .await
            .with_context(|| {
                format!("Failed to send prompt sections sync request to {endpoint_url}")
            })?
            .error_for_status()
            .with_context(|| {
                format!(
                    "Server returned an error for prompt sections sync request to {endpoint_url}"
                )
            })?;

        log::debug!("Synced prompt sections to server");
        Ok(())
    }

    /// Sync STT timeout to server
    pub async fn sync_stt_timeout(&self, timeout_seconds: f64) -> Result<()> {
        #[derive(Serialize)]
        struct TimeoutBody {
            timeout_seconds: f64,
        }

        let (Some(server_url), Some(client_uuid)) = (&self.server_url, &self.client_uuid) else {
            return Ok(()); // Not connected, skip silently
        };

        let endpoint_url = format!("{server_url}/api/config/stt-timeout");
        self.client
            .put(&endpoint_url)
            .header("X-Client-UUID", client_uuid)
            .json(&TimeoutBody { timeout_seconds })
            .send()
            .await
            .with_context(|| format!("Failed to send STT timeout sync request to {endpoint_url}"))?
            .error_for_status()
            .with_context(|| {
                format!("Server returned an error for STT timeout sync request to {endpoint_url}")
            })?;

        log::debug!("Synced STT timeout ({timeout_seconds}) to server");
        Ok(())
    }

    /// Sync LLM formatting enabled setting to server
    pub async fn sync_llm_formatting_enabled(&self, enabled: bool) -> Result<()> {
        #[derive(Serialize)]
        struct LlmFormattingBody {
            enabled: bool,
        }

        let (Some(server_url), Some(client_uuid)) = (&self.server_url, &self.client_uuid) else {
            return Ok(()); // Not connected, skip silently
        };

        let endpoint_url = format!("{server_url}/api/config/llm-formatting");
        self.client
            .put(&endpoint_url)
            .header("X-Client-UUID", client_uuid)
            .json(&LlmFormattingBody { enabled })
            .send()
            .await
            .with_context(|| {
                format!("Failed to send LLM formatting sync request to {endpoint_url}")
            })?
            .error_for_status()
            .with_context(|| {
                format!(
                    "Server returned an error for LLM formatting sync request to {endpoint_url}"
                )
            })?;

        log::debug!("Synced LLM formatting enabled={enabled} to server");
        Ok(())
    }
}

pub type ConfigSync = Arc<RwLock<ConfigSyncState>>;

pub fn new_config_sync() -> ConfigSync {
    Arc::new(RwLock::new(ConfigSyncState::new()))
}
