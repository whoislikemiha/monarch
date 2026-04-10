use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::error::{lock_poisoned, MonarchError};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    /// Optional pre-detected context window in tokens. Only populated for
    /// LM Studio entries discovered via the native `/api/v0/models` endpoint,
    /// and only for models LM Studio reports as currently loaded.
    #[serde(rename = "contextWindow")]
    pub context_window: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ProviderAuthStatus {
    pub provider: String,
    pub checked: bool,
    pub configured: bool,
    pub source: Option<String>,
    pub message: String,
}

// Cache for fetched models
pub struct ModelCache {
    openrouter: Mutex<Option<(Vec<ModelInfo>, Instant)>>,
}

const CACHE_TTL: Duration = Duration::from_secs(3600); // 1 hour

impl ModelCache {
    pub fn new() -> Self {
        Self {
            openrouter: Mutex::new(None),
        }
    }
}

fn pi_auth_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".pi").join("agent").join("auth.json"))
}

fn pi_auth_entry_exists(provider: &str) -> Result<bool, MonarchError> {
    let path = match pi_auth_path() {
        Some(path) => path,
        None => return Ok(false),
    };

    if !path.exists() {
        return Ok(false);
    }

    let content = fs::read_to_string(&path)?;
    let parsed: Value = serde_json::from_str(&content)?;

    Ok(parsed
        .as_object()
        .and_then(|obj| obj.get(provider))
        .is_some())
}

// Known Anthropic models
fn anthropic_models() -> Vec<ModelInfo> {
    [
        ("claude-opus-4-6", "Claude Opus 4.6"),
        ("claude-sonnet-4-5", "Claude Sonnet 4.5"),
        ("claude-haiku-4-5", "Claude Haiku 4.5"),
    ]
    .into_iter()
    .map(|(id, name)| ModelInfo {
        id: id.to_string(),
        name: name.to_string(),
        provider: "anthropic".to_string(),
        context_window: None,
    })
    .collect()
}

// Subscription-backed OpenAI Codex models via Pi auth
fn openai_codex_models() -> Vec<ModelInfo> {
    [("gpt-5.4", "GPT-5.4")]
    .into_iter()
    .map(|(id, name)| ModelInfo {
        id: id.to_string(),
        name: name.to_string(),
        provider: "openai-codex".to_string(),
        context_window: None,
    })
    .collect()
}

#[derive(Deserialize)]
struct OpenRouterResponse {
    data: Vec<OpenRouterModel>,
}

#[derive(Deserialize)]
struct OpenRouterModel {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct LmStudioResponse {
    data: Vec<LmStudioModel>,
}

#[derive(Deserialize)]
struct LmStudioModel {
    id: String,
}

/// Subset of the `/api/v0/models` payload we care about. LM Studio exposes
/// more fields (type, arch, quantization, publisher, etc.) — we only need
/// identity and context sizing.
#[derive(Deserialize)]
struct LmStudioNativeResponse {
    data: Vec<LmStudioNativeModel>,
}

#[derive(Deserialize)]
struct LmStudioNativeModel {
    id: String,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    loaded_context_length: Option<u32>,
}

/// Host root for LM Studio's REST API (e.g. `http://127.0.0.1:1234`).
/// `LMSTUDIO_BASE_URL` may point at either the host root or the legacy
/// OpenAI-compatible path — strip a trailing `/v1` so callers can append
/// either `/v1/models` or `/api/v0/models`.
fn lmstudio_host_root() -> String {
    let raw = std::env::var("LMSTUDIO_BASE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:1234".to_string());
    let trimmed = raw.trim_end_matches('/');
    trimmed
        .strip_suffix("/v1")
        .map(|s| s.to_string())
        .unwrap_or_else(|| trimmed.to_string())
}

async fn fetch_lmstudio_models() -> Result<Vec<ModelInfo>, MonarchError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?;

    // Prefer the native endpoint — it exposes per-model context sizing.
    // Fall back to the OpenAI-compatible endpoint if the user is running
    // an older LM Studio build that doesn't expose `/api/v0`.
    match fetch_lmstudio_models_native(&client).await {
        Ok(models) => Ok(models),
        Err(native_err) => match fetch_lmstudio_models_openai(&client).await {
            Ok(models) => Ok(models),
            Err(openai_err) => Err(MonarchError::persistence(format!(
                "LM Studio unavailable: native={native_err} / openai={openai_err}"
            ))),
        },
    }
}

async fn fetch_lmstudio_models_native(
    client: &reqwest::Client,
) -> Result<Vec<ModelInfo>, MonarchError> {
    let host = lmstudio_host_root();
    let url = format!("{}/api/v0/models", host);

    let resp = client.get(&url).send().await?;

    if !resp.status().is_success() {
        return Err(MonarchError::persistence(format!(
            "LM Studio native API returned HTTP {} at {}",
            resp.status(),
            url
        )));
    }

    let parsed: LmStudioNativeResponse = resp.json().await?;

    // Only surface models LM Studio reports as currently loaded — matches
    // the scope of the OpenAI-compatible /v1/models endpoint, so the picker
    // only lists things the user can actually talk to right now.
    Ok(parsed
        .data
        .into_iter()
        .filter(|m| m.state.as_deref() == Some("loaded"))
        .map(|m| ModelInfo {
            id: m.id.clone(),
            name: m.id,
            provider: "lmstudio".to_string(),
            context_window: m.loaded_context_length,
        })
        .collect())
}

async fn fetch_lmstudio_models_openai(
    client: &reqwest::Client,
) -> Result<Vec<ModelInfo>, MonarchError> {
    let host = lmstudio_host_root();
    let url = format!("{}/v1/models", host);

    let resp = client.get(&url).send().await?;

    if !resp.status().is_success() {
        return Err(MonarchError::persistence(format!(
            "LM Studio returned HTTP {} at {}",
            resp.status(),
            url
        )));
    }

    let parsed: LmStudioResponse = resp.json().await?;

    Ok(parsed
        .data
        .into_iter()
        .map(|m| ModelInfo {
            id: m.id.clone(),
            name: m.id,
            provider: "lmstudio".to_string(),
            context_window: None,
        })
        .collect())
}

async fn fetch_openrouter_models() -> Result<Vec<ModelInfo>, MonarchError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let resp: OpenRouterResponse = client
        .get("https://openrouter.ai/api/v1/models")
        .send()
        .await?
        .json()
        .await?;

    Ok(resp
        .data
        .into_iter()
        .map(|m| ModelInfo {
            id: m.id.clone(),
            name: m.name,
            provider: "openrouter".to_string(),
            context_window: None,
        })
        .collect())
}

#[tauri::command]
#[specta::specta]
pub async fn get_models(
    cache: tauri::State<'_, Arc<ModelCache>>,
    provider: String,
) -> Result<Vec<ModelInfo>, MonarchError> {
    match provider.as_str() {
        "anthropic" => Ok(anthropic_models()),
        "openai-codex" => Ok(openai_codex_models()),
        "openrouter" => {
            // Check cache
            {
                let cached = cache.openrouter.lock().map_err(lock_poisoned("openrouter cache"))?;
                if let Some((ref models, ref fetched_at)) = *cached {
                    if fetched_at.elapsed() < CACHE_TTL {
                        return Ok(models.clone());
                    }
                }
            }

            // Fetch fresh
            let models = fetch_openrouter_models().await?;

            // Update cache
            {
                let mut cached = cache.openrouter.lock().map_err(lock_poisoned("openrouter cache"))?;
                *cached = Some((models.clone(), Instant::now()));
            }

            Ok(models)
        }
        "lmstudio" => fetch_lmstudio_models().await,
        _ => Ok(vec![]),
    }
}

// ---- WebSocket wrappers ----

pub async fn ws_get_models(cache: &ModelCache, provider: String) -> Result<Vec<ModelInfo>, MonarchError> {
    match provider.as_str() {
        "anthropic" => Ok(anthropic_models()),
        "openai-codex" => Ok(openai_codex_models()),
        "openrouter" => {
            {
                let cached = cache.openrouter.lock().map_err(lock_poisoned("openrouter cache"))?;
                if let Some((ref models, ref fetched_at)) = *cached {
                    if fetched_at.elapsed() < CACHE_TTL {
                        return Ok(models.clone());
                    }
                }
            }
            let models = fetch_openrouter_models().await?;
            {
                let mut cached = cache.openrouter.lock().map_err(lock_poisoned("openrouter cache"))?;
                *cached = Some((models.clone(), Instant::now()));
            }
            Ok(models)
        }
        "lmstudio" => fetch_lmstudio_models().await,
        _ => Ok(vec![]),
    }
}

#[tauri::command]
#[specta::specta]
pub fn get_provider_auth_status(provider: String) -> Result<ProviderAuthStatus, MonarchError> {
    get_provider_auth_status_inner(provider)
}

pub fn ws_get_provider_auth_status(provider: String) -> Result<ProviderAuthStatus, MonarchError> {
    get_provider_auth_status_inner(provider)
}

fn get_provider_auth_status_inner(provider: String) -> Result<ProviderAuthStatus, MonarchError> {
    match provider.as_str() {
        "anthropic" => {
            let configured = pi_auth_entry_exists("anthropic")?;
            Ok(ProviderAuthStatus {
                provider,
                checked: true,
                configured,
                source: if configured {
                    Some("~/.pi/agent/auth.json".to_string())
                } else {
                    None
                },
                message: if configured {
                    "Pi Claude auth found.".to_string()
                } else {
                    "No Pi Claude auth found. Anthropic can still work via ANTHROPIC_API_KEY.".to_string()
                },
            })
        }
        "openai-codex" => {
            let configured = pi_auth_entry_exists("openai-codex")?;
            Ok(ProviderAuthStatus {
                provider,
                checked: true,
                configured,
                source: if configured {
                    Some("~/.pi/agent/auth.json".to_string())
                } else {
                    None
                },
                message: if configured {
                    "Pi Codex auth found.".to_string()
                } else {
                    "No Pi Codex auth found. Run Pi login for OpenAI Codex first.".to_string()
                },
            })
        }
        _ => Ok(ProviderAuthStatus {
            provider,
            checked: false,
            configured: false,
            source: None,
            message: "This provider does not use Pi subscription auth checks.".to_string(),
        }),
    }
}
