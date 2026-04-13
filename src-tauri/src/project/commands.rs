//! Tauri command wrappers for the project module.
//!
//! Thin adapters over the free functions in `super` so the IPC and
//! WebSocket transports can both call into the same underlying logic.

use std::sync::Arc;

use crate::db::Database;
use crate::error::MonarchError;

#[tauri::command]
#[specta::specta]
pub async fn detect_project(
    db: tauri::State<'_, Arc<Database>>,
    cwd: String,
) -> Result<Option<serde_json::Value>, MonarchError> {
    super::detect_project(&db, &cwd).await
}

#[tauri::command]
#[specta::specta]
pub fn read_project_instructions(cwd: String) -> Result<Option<String>, MonarchError> {
    Ok(super::read_project_instructions(&cwd))
}
