//! MON-99: global memory / Keeper configuration, loaded from
//! `~/.config/monarch/memory.toml`.
//!
//! The Keeper is disabled (no-op) when no keeper model is configured.
//! Embedding defaults to `bge-small-en-v1.5` with lazy model download to
//! `~/.config/monarch/models/`.
//!
//! Example `memory.toml`:
//!
//! ```toml
//! top_k = 5
//!
//! [keeper]
//! provider = "anthropic"
//! model = "claude-haiku-4-5"
//!
//! [embedding]
//! model_id = "bge-small-en-v1.5"
//! models_dir = "~/.config/monarch/models"
//! ```

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::MonarchError;

pub const DEFAULT_EMBEDDING_MODEL_ID: &str = "bge-small-en-v1.5";
pub const DEFAULT_TOP_K: u32 = 5;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KeeperModelConfig {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct EmbeddingConfig {
    pub model_id: Option<String>,
    pub models_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct MemoryConfig {
    pub keeper: Option<KeeperModelConfig>,
    pub embedding: Option<EmbeddingConfig>,
    pub top_k: Option<u32>,
}

/// Resolved view — all fields filled in, ready to ship to the sidecar / memory index.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedMemoryConfig {
    /// If None, the Keeper is disabled.
    pub keeper: Option<KeeperModelConfig>,
    pub embedding_model_id: String,
    pub models_dir: String,
    pub top_k: u32,
    pub enabled: bool,
}

pub fn resolve(raw: MemoryConfig) -> ResolvedMemoryConfig {
    let enabled = raw.keeper.is_some();
    let models_dir = raw
        .embedding
        .as_ref()
        .and_then(|e| e.models_dir.clone())
        .unwrap_or_else(default_models_dir);
    let embedding_model_id = raw
        .embedding
        .as_ref()
        .and_then(|e| e.model_id.clone())
        .unwrap_or_else(|| DEFAULT_EMBEDDING_MODEL_ID.to_string());
    ResolvedMemoryConfig {
        keeper: raw.keeper,
        embedding_model_id,
        models_dir,
        top_k: raw.top_k.unwrap_or(DEFAULT_TOP_K),
        enabled,
    }
}

fn default_models_dir() -> String {
    dirs::config_dir()
        .map(|d| d.join("monarch").join("models").to_string_lossy().to_string())
        .unwrap_or_else(|| "~/.config/monarch/models".to_string())
}

pub fn config_path_ws() -> Result<String, MonarchError> {
    Ok(config_path()?.to_string_lossy().to_string())
}

pub async fn write_raw_ws(cfg: &MemoryConfig) -> Result<(), MonarchError> {
    write_raw(cfg).await
}

fn config_path() -> Result<PathBuf, MonarchError> {
    let dir = dirs::config_dir()
        .ok_or_else(|| MonarchError::persistence("config_dir unavailable"))?
        .join("monarch");
    Ok(dir.join("memory.toml"))
}

pub async fn load_raw() -> MemoryConfig {
    let Ok(path) = config_path() else {
        return MemoryConfig::default();
    };
    let Ok(contents) = tokio::fs::read_to_string(&path).await else {
        return MemoryConfig::default();
    };
    toml::from_str(&contents).unwrap_or_default()
}

pub async fn resolved() -> ResolvedMemoryConfig {
    resolve(load_raw().await)
}

async fn write_raw(cfg: &MemoryConfig) -> Result<(), MonarchError> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(MonarchError::from)?;
    }
    let toml_str =
        toml::to_string_pretty(cfg).map_err(|e| MonarchError::persistence(e.to_string()))?;
    tokio::fs::write(&path, toml_str)
        .await
        .map_err(MonarchError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn memory_get_config() -> Result<ResolvedMemoryConfig, MonarchError> {
    Ok(resolved().await)
}

#[tauri::command]
#[specta::specta]
pub async fn memory_set_config(
    config: MemoryConfig,
) -> Result<ResolvedMemoryConfig, MonarchError> {
    write_raw(&config).await?;
    Ok(resolve(config))
}

#[tauri::command]
#[specta::specta]
pub async fn memory_get_config_path() -> Result<String, MonarchError> {
    Ok(config_path()?.to_string_lossy().to_string())
}
