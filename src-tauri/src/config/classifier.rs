//! MON-82: global classifier configuration, loaded from
//! `~/.config/monarch/classifier.toml`.
//!
//! Scope is global for Slice 1 — one classifier config drives every agent's
//! per-turn classifier. The file is optional; when absent the built-in
//! defaults below apply (Haiku 4.5 primary, no fallback, 3s timeout,
//! enabled).
//!
//! Example `classifier.toml`:
//!
//! ```toml
//! enabled = true
//! timeout_ms = 3000
//!
//! [primary]
//! provider = "anthropic"
//! model = "claude-haiku-4-5"
//!
//! [fallback]
//! provider = "lmstudio"
//! model = "qwen3-4b-instruct"
//! ```
//!
//! The system prompt is not stored in the file; it lives in code so it
//! evolves with the product. It is exposed to the frontend (read-only) so
//! the user can see what the classifier is being asked to do.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::MonarchError;

pub const DEFAULT_TIMEOUT_MS: u32 = 3000;
pub const DEFAULT_PRIMARY_PROVIDER: &str = "anthropic";
pub const DEFAULT_PRIMARY_MODEL: &str = "claude-haiku-4-5";

/// System prompt shipped with the classifier. Kept in sync with
/// `DEFAULT_CLASSIFIER_SYSTEM_PROMPT` in `sidecar/src/classifier.ts`; the
/// Rust side is the canonical value so Tauri can expose it to the settings
/// UI without crossing the sidecar.
pub const DEFAULT_CLASSIFIER_SYSTEM_PROMPT: &str = "You are a complexity classifier for incoming user prompts in a multi-agent coding assistant.

Label the user's message with exactly one of:

- chitchat: greetings, social/small-talk, or meta-questions that need no task execution
- simple: a direct request solvable in a single focused turn (e.g. a one-line fix, a factual question, a small rename)
- decomposable: work that benefits from an explicit plan — several files, several decisions, or sequenced steps
- delegate: work that benefits from parallel subtasks or exploration across unrelated areas, where multiple agents should run simultaneously

Bias toward escalation on ambiguity — prefer 'decomposable' over 'simple' when it's a close call. A misclassified simple prompt is cheap; a missed decomposable task is expensive.

Output ONLY a single JSON object, no prose, no code fences:

{\"complexity\": \"<label>\", \"confidence\": <number between 0 and 1>, \"rationale\": \"<one short sentence>\"}";

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClassifierProvider {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ClassifierConfig {
    pub enabled: Option<bool>,
    pub primary: Option<ClassifierProvider>,
    pub fallback: Option<ClassifierProvider>,
    pub timeout_ms: Option<u32>,
}

/// Resolved view — all fields filled in, ready to ship to the sidecar.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedClassifierConfig {
    pub enabled: bool,
    pub primary: ClassifierProvider,
    pub fallback: Option<ClassifierProvider>,
    pub timeout_ms: u32,
    pub system_prompt: String,
}

fn default_primary() -> ClassifierProvider {
    ClassifierProvider {
        provider: DEFAULT_PRIMARY_PROVIDER.to_string(),
        model: DEFAULT_PRIMARY_MODEL.to_string(),
    }
}

pub fn resolve(raw: ClassifierConfig) -> ResolvedClassifierConfig {
    ResolvedClassifierConfig {
        enabled: raw.enabled.unwrap_or(true),
        primary: raw.primary.unwrap_or_else(default_primary),
        fallback: raw.fallback,
        timeout_ms: raw.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS),
        system_prompt: DEFAULT_CLASSIFIER_SYSTEM_PROMPT.to_string(),
    }
}

pub(crate) fn config_path() -> Result<PathBuf, MonarchError> {
    let dir = dirs::config_dir()
        .ok_or_else(|| MonarchError::persistence("config_dir unavailable"))?
        .join("monarch");
    Ok(dir.join("classifier.toml"))
}

pub async fn load_raw() -> ClassifierConfig {
    let Ok(path) = config_path() else {
        return ClassifierConfig::default();
    };
    let Ok(contents) = tokio::fs::read_to_string(&path).await else {
        return ClassifierConfig::default();
    };
    toml::from_str(&contents).unwrap_or_default()
}

/// Resolved config as the sidecar should see it. Reads the TOML once per
/// call; cheap enough (microseconds) that caching isn't worth the
/// invalidation story for Slice 1.
pub async fn resolved() -> ResolvedClassifierConfig {
    resolve(load_raw().await)
}

pub(crate) async fn write_raw(cfg: &ClassifierConfig) -> Result<(), MonarchError> {
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
pub async fn classifier_get_config() -> Result<ResolvedClassifierConfig, MonarchError> {
    Ok(resolved().await)
}

#[tauri::command]
#[specta::specta]
pub async fn classifier_set_config(
    config: ClassifierConfig,
) -> Result<ResolvedClassifierConfig, MonarchError> {
    write_raw(&config).await?;
    Ok(resolve(config))
}

#[tauri::command]
#[specta::specta]
pub async fn classifier_get_config_path() -> Result<String, MonarchError> {
    Ok(config_path()?.to_string_lossy().to_string())
}
