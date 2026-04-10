use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    /// Optional pre-detected context window in tokens. Only populated for
    /// LM Studio entries discovered via the native `/api/v0/models` endpoint.
    #[serde(rename = "contextWindow", skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u32>,
    /// True when the upstream provider reports the model is not currently
    /// loaded — `context_window` in that case reflects `max_context_length`,
    /// not a live runtime value.
    #[serde(rename = "contextWindowIsMax", skip_serializing_if = "Option::is_none")]
    pub context_window_is_max: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

fn pi_auth_entry_exists(provider: &str) -> Result<bool, String> {
    let path = match pi_auth_path() {
        Some(path) => path,
        None => return Ok(false),
    };

    if !path.exists() {
        return Ok(false);
    }

    let content = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    let parsed: Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))?;

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
        context_window_is_max: None,
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
        context_window_is_max: None,
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
    #[serde(default)]
    max_context_length: Option<u32>,
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

async fn fetch_lmstudio_models() -> Result<Vec<ModelInfo>, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| e.to_string())?;

    // Prefer the native endpoint — it exposes per-model context sizing.
    // Fall back to the OpenAI-compatible endpoint if the user is running
    // an older LM Studio build that doesn't expose `/api/v0`.
    match fetch_lmstudio_models_native(&client).await {
        Ok(models) => Ok(models),
        Err(native_err) => {
            match fetch_lmstudio_models_openai(&client).await {
                Ok(models) => Ok(models),
                Err(openai_err) => Err(format!("{} / {}", native_err, openai_err)),
            }
        }
    }
}

async fn fetch_lmstudio_models_native(
    client: &reqwest::Client,
) -> Result<Vec<ModelInfo>, String> {
    let host = lmstudio_host_root();
    let url = format!("{}/api/v0/models", host);

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("LM Studio not reachable at {}: {}", host, e))?;

    if !resp.status().is_success() {
        return Err(format!(
            "LM Studio native API returned HTTP {} at {}",
            resp.status(),
            url
        ));
    }

    let parsed: LmStudioNativeResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse LM Studio native response: {}", e))?;

    Ok(parsed
        .data
        .into_iter()
        .map(|m| {
            let is_loaded = m.state.as_deref() == Some("loaded");
            let (context_window, context_window_is_max) = if is_loaded && m.loaded_context_length.is_some() {
                (m.loaded_context_length, Some(false))
            } else if let Some(max) = m.max_context_length {
                // Not loaded (or no live size reported) — surface the ceiling
                // so the spawn dialog can pre-fill with a value that won't
                // under-report. UI flags this as a non-live fallback.
                (Some(max), Some(true))
            } else {
                (None, None)
            };
            ModelInfo {
                id: m.id.clone(),
                name: m.id,
                provider: "lmstudio".to_string(),
                context_window,
                context_window_is_max,
            }
        })
        .collect())
}

async fn fetch_lmstudio_models_openai(
    client: &reqwest::Client,
) -> Result<Vec<ModelInfo>, String> {
    let host = lmstudio_host_root();
    let url = format!("{}/v1/models", host);

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("LM Studio not reachable at {}: {}", host, e))?;

    if !resp.status().is_success() {
        return Err(format!(
            "LM Studio returned HTTP {} at {}",
            resp.status(),
            url
        ));
    }

    let parsed: LmStudioResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse LM Studio response: {}", e))?;

    Ok(parsed
        .data
        .into_iter()
        .map(|m| ModelInfo {
            id: m.id.clone(),
            name: m.id,
            provider: "lmstudio".to_string(),
            context_window: None,
            context_window_is_max: None,
        })
        .collect())
}

async fn fetch_openrouter_models() -> Result<Vec<ModelInfo>, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let resp: OpenRouterResponse = client
        .get("https://openrouter.ai/api/v1/models")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch OpenRouter models: {}", e))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse OpenRouter response: {}", e))?;

    Ok(resp
        .data
        .into_iter()
        .map(|m| ModelInfo {
            id: m.id.clone(),
            name: m.name,
            provider: "openrouter".to_string(),
            context_window: None,
            context_window_is_max: None,
        })
        .collect())
}

#[tauri::command]
pub async fn get_models(
    cache: tauri::State<'_, ModelCache>,
    provider: String,
) -> Result<Vec<ModelInfo>, String> {
    match provider.as_str() {
        "anthropic" => Ok(anthropic_models()),
        "openai-codex" => Ok(openai_codex_models()),
        "openrouter" => {
            // Check cache
            {
                let cached = cache.openrouter.lock().map_err(|e| e.to_string())?;
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
                let mut cached = cache.openrouter.lock().map_err(|e| e.to_string())?;
                *cached = Some((models.clone(), Instant::now()));
            }

            Ok(models)
        }
        "lmstudio" => fetch_lmstudio_models().await,
        _ => Ok(vec![]),
    }
}

#[tauri::command]
pub fn get_provider_auth_status(provider: String) -> Result<ProviderAuthStatus, String> {
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
