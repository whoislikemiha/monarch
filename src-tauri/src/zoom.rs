use crate::MonarchError;

const ZOOM_MIN: f64 = 0.5;
const ZOOM_MAX: f64 = 2.0;

#[tauri::command]
#[specta::specta]
pub fn set_zoom(
    window: tauri::WebviewWindow,
    level: f64,
) -> Result<f64, MonarchError> {
    let clamped = level.clamp(ZOOM_MIN, ZOOM_MAX);
    window.set_zoom(clamped)
        .map_err(|e| MonarchError::persistence(format!("Failed to set zoom: {}", e)))?;
    Ok(clamped)
}
