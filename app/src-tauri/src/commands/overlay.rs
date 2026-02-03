use tauri::{AppHandle, Manager};

#[tauri::command]
pub async fn resize_overlay(app: AppHandle, width: f64, height: f64) -> Result<(), String> {
    // Enforce minimum dimensions to prevent invisible window
    let min_size = 48.0;
    let width = width.max(min_size);
    let height = height.max(min_size);

    if let Some(window) = app.get_webview_window("overlay") {
        // Get current center point from current position and size
        // This allows the overlay to be dragged and maintain its new position
        let center = if let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size()) {
            let scale = window.scale_factor().unwrap_or(1.0);
            let x = f64::from(pos.x) / scale;
            let y = f64::from(pos.y) / scale;
            let w = f64::from(size.width) / scale;
            let h = f64::from(size.height) / scale;
            Some((x + w / 2.0, y + h / 2.0))
        } else {
            None
        };

        // Set the new size
        window
            .set_size(tauri::Size::Logical(tauri::LogicalSize { width, height }))
            .map_err(|e| e.to_string())?;

        // Reposition to keep center fixed
        if let Some((cx, cy)) = center {
            let x = cx - width / 2.0;
            let y = cy - height / 2.0;
            window
                .set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }))
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
