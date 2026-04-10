//! Placeholder tool — proves end-to-end wiring of the toolbox registry.
//! Future tools follow the same shape: inner impl + #[tauri::command] wrapper
//! + ws_* wrapper consumed by `ws::dispatch_command`.

use std::time::{SystemTime, UNIX_EPOCH};

fn ping_impl() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("pong @ unix:{}", secs)
}

#[tauri::command]
#[specta::specta]
pub fn toolbox_placeholder_ping() -> Result<String, String> {
    Ok(ping_impl())
}

pub fn ws_toolbox_placeholder_ping() -> Result<String, String> {
    Ok(ping_impl())
}
