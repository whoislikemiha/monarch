use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
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

fn lmstudio_base_url() -> String {
    std::env::var("LMSTUDIO_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:1234/v1".to_string())
}

async fn fetch_lmstudio_models() -> Result<Vec<ModelInfo>, String> {
    let base_url = lmstudio_base_url();
    let url = format!("{}/models", base_url.trim_end_matches('/'));

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("LM Studio not reachable at {}: {}", base_url, e))?;

    if !resp.status().is_success() {
        return Err(format!(
            "LM Studio returned HTTP {} at {}",
            resp.status(),
            base_url
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
        })
        .collect())
}

#[tauri::command]
pub async fn get_models(
    cache: tauri::State<'_, Arc<ModelCache>>,
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

// ---- WebSocket wrappers ----

pub async fn ws_get_models(cache: &ModelCache, provider: String) -> Result<Vec<ModelInfo>, String> {
    match provider.as_str() {
        "anthropic" => Ok(anthropic_models()),
        "openai-codex" => Ok(openai_codex_models()),
        "openrouter" => {
            {
                let cached = cache.openrouter.lock().map_err(|e| e.to_string())?;
                if let Some((ref models, ref fetched_at)) = *cached {
                    if fetched_at.elapsed() < CACHE_TTL {
                        return Ok(models.clone());
                    }
                }
            }
            let models = fetch_openrouter_models().await?;
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
    get_provider_auth_status_inner(provider)
}

pub fn ws_get_provider_auth_status(provider: String) -> Result<ProviderAuthStatus, String> {
    get_provider_auth_status_inner(provider)
}

fn get_provider_auth_status_inner(provider: String) -> Result<ProviderAuthStatus, String> {
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
