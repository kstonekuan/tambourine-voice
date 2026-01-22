use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use tauri_plugin_http::reqwest::Client;
use tokio::sync::RwLock;

use crate::settings::CleanupPromptSections;

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
        log::info!(
            "Config sync connected: {} (uuid: {})",
            server_url,
            client_uuid
        );
        self.server_url = Some(server_url);
        self.client_uuid = Some(client_uuid);
    }

    /// Clear connection info when disconnected
    pub fn set_disconnected(&mut self) {
        self.server_url = None;
        self.client_uuid = None;
        log::info!("Config sync disconnected");
    }

    /// Sync prompt sections to server (best-effort, logs errors)
    pub async fn sync_prompt_sections(
        &self,
        sections: &CleanupPromptSections,
    ) -> Result<(), String> {
        let (url, uuid) = match (&self.server_url, &self.client_uuid) {
            (Some(u), Some(id)) => (u, id),
            _ => return Ok(()), // Not connected, skip silently
        };

        self.client
            .put(format!("{}/api/config/prompts", url))
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
        let (url, uuid) = match (&self.server_url, &self.client_uuid) {
            (Some(u), Some(id)) => (u, id),
            _ => return Ok(()), // Not connected, skip silently
        };

        #[derive(Serialize)]
        struct TimeoutBody {
            timeout_seconds: f64,
        }

        self.client
            .put(format!("{}/api/config/stt-timeout", url))
            .header("X-Client-UUID", uuid)
            .json(&TimeoutBody { timeout_seconds })
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;

        log::debug!("Synced STT timeout ({}) to server", timeout_seconds);
        Ok(())
    }
}

pub type ConfigSync = Arc<RwLock<ConfigSyncState>>;

pub fn new_config_sync() -> ConfigSync {
    Arc::new(RwLock::new(ConfigSyncState::new()))
}
