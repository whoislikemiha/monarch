use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
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

// Known OpenAI models
fn openai_models() -> Vec<ModelInfo> {
    [
        ("gpt-4.1", "GPT-4.1"),
        ("gpt-4.1-mini", "GPT-4.1 Mini"),
        ("gpt-4.1-nano", "GPT-4.1 Nano"),
        ("o3", "o3"),
        ("o3-mini", "o3 Mini"),
        ("o4-mini", "o4 Mini"),
        ("gpt-4o", "GPT-4o"),
        ("gpt-4o-mini", "GPT-4o Mini"),
    ]
    .into_iter()
    .map(|(id, name)| ModelInfo {
        id: id.to_string(),
        name: name.to_string(),
        provider: "openai".to_string(),
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
    cache: tauri::State<'_, ModelCache>,
    provider: String,
) -> Result<Vec<ModelInfo>, String> {
    match provider.as_str() {
        "anthropic" => Ok(anthropic_models()),
        "openai" => Ok(openai_models()),
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
        _ => Ok(vec![]),
    }
}
