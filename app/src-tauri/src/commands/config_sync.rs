use crate::config_sync::ConfigSync;
use crate::events::{ConfigResponse, ConfigSetting, EventName};
use tauri::{AppHandle, Emitter};

/// Notify Rust that we've connected to the server
/// This stores connection info and syncs current settings
#[tauri::command]
pub async fn set_server_connected(
    app: AppHandle,
    server_url: String,
    client_uuid: String,
    config_sync: tauri::State<'_, ConfigSync>,
) -> Result<(), String> {
    // Store connection info
    {
        let mut sync = config_sync.write().await;
        sync.set_connected(server_url, client_uuid);
    }

    // Sync current settings to server
    let settings = super::settings::get_settings(app.clone())?;
    let sync = config_sync.read().await;

    if let Some(ref sections) = settings.cleanup_prompt_sections {
        match sync.sync_prompt_sections(sections).await {
            Ok(()) => {
                let _ = app.emit(
                    EventName::ConfigResponse.as_str(),
                    ConfigResponse::updated(ConfigSetting::PromptSections, sections),
                );
            }
            Err(e) => {
                log::warn!("Failed to sync prompt sections on connect: {e}");
                let _ = app.emit(
                    EventName::ConfigResponse.as_str(),
                    ConfigResponse::<()>::error(ConfigSetting::PromptSections, e),
                );
            }
        }
    }

    if let Some(timeout) = settings.stt_timeout_seconds {
        match sync.sync_stt_timeout(timeout).await {
            Ok(()) => {
                let _ = app.emit(
                    EventName::ConfigResponse.as_str(),
                    ConfigResponse::updated(ConfigSetting::SttTimeout, timeout),
                );
            }
            Err(e) => {
                log::warn!("Failed to sync STT timeout on connect: {e}");
                let _ = app.emit(
                    EventName::ConfigResponse.as_str(),
                    ConfigResponse::<()>::error(ConfigSetting::SttTimeout, e),
                );
            }
        }
    }

    // Sync LLM formatting enabled setting on connect
    match sync
        .sync_llm_formatting_enabled(settings.llm_formatting_enabled)
        .await
    {
        Ok(()) => {
            log::info!(
                "Synced LLM formatting enabled={} on connect",
                settings.llm_formatting_enabled
            );
        }
        Err(e) => {
            log::warn!("Failed to sync LLM formatting on connect: {e}");
        }
    }

    Ok(())
}

/// Notify Rust that we've disconnected from the server
/// This disables config syncing
#[tauri::command]
pub async fn set_server_disconnected(
    config_sync: tauri::State<'_, ConfigSync>,
) -> Result<(), String> {
    let mut sync = config_sync.write().await;
    sync.set_disconnected();
    Ok(())
}
