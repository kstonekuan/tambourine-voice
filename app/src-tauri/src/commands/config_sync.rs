use crate::config_sync::ConfigSync;

/// Notify Rust that we've connected to the server
/// This enables config syncing via HTTP API
#[tauri::command]
pub async fn set_server_connected(
    server_url: String,
    client_uuid: String,
    config_sync: tauri::State<'_, ConfigSync>,
) -> Result<(), String> {
    let mut sync = config_sync.write().await;
    sync.set_connected(server_url, client_uuid);
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
