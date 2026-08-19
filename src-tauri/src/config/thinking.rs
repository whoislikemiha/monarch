//! Per-model default thinking levels, loaded from `~/.config/monarch/thinking.toml`.
//!
//! Each entry keys on `(provider, model_pattern)`. `model_pattern` is a case-
//! insensitive substring match against the model id; `"*"` / `""` matches any
//! model on that provider. The first matching entry wins, so put specific
//! patterns before wildcards.
//!
//! Example `thinking.toml`:
//!
//! ```toml
//! [[model]]
//! provider = "anthropic"
//! pattern = "opus-4-6"
//! default = "xhigh"
//!
//! [[model]]
//! provider = "anthropic"
//! pattern = "*"
//! default = "medium"
//!
//! [[model]]
//! provider = "openai-codex"
//! pattern = "*"
//! default = "medium"
//! ```
//!
//! When the file is missing, the built-in defaults below apply. When the file
//! exists but a `(provider, model)` has no matching entry, the built-in
//! default for that provider is used. When no built-in exists, we return
//! `"off"` — we never silently enable reasoning without an explicit opt-in.

use serde::Deserialize;
use std::path::PathBuf;

use crate::error::MonarchError;

#[derive(Debug, Clone, Deserialize)]
struct RawEntry {
    provider: String,
    #[serde(default)]
    pattern: Option<String>,
    default: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawConfig {
    #[serde(default, rename = "model")]
    models: Vec<RawEntry>,
}

/// Built-in fallback table. Kept conservative — providers with reasoning-
/// capable models default to `medium`; everything else stays `off`.
fn builtin_default(provider: &str, model: &str) -> &'static str {
    let m = model.to_lowercase();
    match provider {
        "anthropic" => {
            if m.contains("fable-5") || m.contains("opus-5") || m.contains("opus-4-6") || m.contains("opus-4.6") || m.contains("opus-4-7") || m.contains("opus-4.7") {
                "high"
            } else if m.contains("sonnet-5") || m.contains("sonnet-4-6") || m.contains("sonnet-4.6") {
                "medium"
            } else {
                "off"
            }
        }
        "openai-codex" => "medium",
        "openrouter" => {
            if m.starts_with("anthropic/") && (m.contains("opus-4-6") || m.contains("opus-4.6")) {
                "high"
            } else {
                "off"
            }
        }
        _ => "off",
    }
}

fn config_path() -> Result<PathBuf, MonarchError> {
    let dir = dirs::config_dir()
        .ok_or_else(|| MonarchError::persistence("config_dir unavailable"))?
        .join("monarch");
    Ok(dir.join("thinking.toml"))
}

async fn load_raw() -> RawConfig {
    let Ok(path) = config_path() else {
        return RawConfig::default();
    };
    let Ok(contents) = tokio::fs::read_to_string(&path).await else {
        return RawConfig::default();
    };
    toml::from_str(&contents).unwrap_or_default()
}

fn pattern_matches(pattern: &Option<String>, model: &str) -> bool {
    match pattern.as_deref() {
        None | Some("") | Some("*") => true,
        Some(p) => model.to_lowercase().contains(&p.to_lowercase()),
    }
}

/// Resolve the default thinking level for a given `(provider, model)` pair.
/// User config takes precedence over built-in defaults.
pub async fn default_for(provider: &str, model: &str) -> String {
    let cfg = load_raw().await;
    for entry in &cfg.models {
        if entry.provider == provider && pattern_matches(&entry.pattern, model) {
            return entry.default.clone();
        }
    }
    builtin_default(provider, model).to_string()
}

#[tauri::command]
#[specta::specta]
pub async fn get_thinking_default(provider: String, model: String) -> Result<String, MonarchError> {
    Ok(default_for(&provider, &model).await)
}

#[tauri::command]
#[specta::specta]
pub async fn get_thinking_config_path() -> Result<String, MonarchError> {
    Ok(config_path()?.to_string_lossy().to_string())
}
