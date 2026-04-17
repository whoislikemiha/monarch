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
    anthropic: Mutex<Option<(Vec<ModelInfo>, Instant)>>,
    openai_codex: Mutex<Option<(Vec<ModelInfo>, Instant)>>,
    openrouter: Mutex<Option<(Vec<ModelInfo>, Instant)>>,
}

const CACHE_TTL: Duration = Duration::from_secs(3600); // 1 hour

impl ModelCache {
    pub fn new() -> Self {
        Self {
            anthropic: Mutex::new(None),
            openai_codex: Mutex::new(None),
            openrouter: Mutex::new(None),
        }
    }
}

fn pi_auth_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".pi").join("agent").join("auth.json"))
}

fn pi_auth_entry(provider: &str) -> Result<Option<Value>, MonarchError> {
    let path = match pi_auth_path() {
        Some(path) => path,
        None => return Ok(None),
    };

    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path)?;
    let parsed: Value = serde_json::from_str(&content)?;

    Ok(parsed
        .as_object()
        .and_then(|obj| obj.get(provider))
        .cloned())
}

fn pi_auth_entry_exists(provider: &str) -> Result<bool, MonarchError> {
    Ok(pi_auth_entry(provider)?.is_some())
}

/// Extract the OAuth `access` token Pi stores for a given provider.
/// Returns `None` if `auth.json` is missing, the provider isn't configured,
/// or the entry lacks an `access` field.
fn pi_auth_access_token(provider: &str) -> Result<Option<String>, MonarchError> {
    Ok(pi_auth_entry(provider)?
        .and_then(|entry| entry.get("access").cloned())
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .filter(|s| !s.is_empty()))
}

fn env_var_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

/// Source of a provider credential — used both to pick the right auth header
/// at fetch time and to tell the user in the UI which credential we're on.
#[derive(Debug, Clone, Copy)]
enum AuthSource {
    PiSubscription,
    EnvApiKey,
}

struct ProviderCreds {
    token: String,
    source: AuthSource,
}

fn anthropic_creds() -> Result<Option<ProviderCreds>, MonarchError> {
    if let Some(token) = pi_auth_access_token("anthropic")? {
        return Ok(Some(ProviderCreds {
            token,
            source: AuthSource::PiSubscription,
        }));
    }
    if let Some(token) = env_var_nonempty("ANTHROPIC_API_KEY") {
        return Ok(Some(ProviderCreds {
            token,
            source: AuthSource::EnvApiKey,
        }));
    }
    Ok(None)
}

fn openai_codex_creds() -> Result<Option<ProviderCreds>, MonarchError> {
    if let Some(token) = pi_auth_access_token("openai-codex")? {
        return Ok(Some(ProviderCreds {
            token,
            source: AuthSource::PiSubscription,
        }));
    }
    if let Some(token) = env_var_nonempty("OPENAI_API_KEY") {
        return Ok(Some(ProviderCreds {
            token,
            source: AuthSource::EnvApiKey,
        }));
    }
    Ok(None)
}

/// Shared cache-or-fetch pattern. Returns the cached value when fresh,
/// otherwise calls `fetcher` and stores the result. `force_refresh=true`
/// skips the cache read — used by the UI's Retry button so transient
/// provider failures can be re-attempted without waiting out the TTL.
async fn cached_or_fetch<F, Fut>(
    slot: &Mutex<Option<(Vec<ModelInfo>, Instant)>>,
    lock_label: &'static str,
    force_refresh: bool,
    fetcher: F,
) -> Result<Vec<ModelInfo>, MonarchError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<Vec<ModelInfo>, MonarchError>>,
{
    if !force_refresh {
        let cached = slot.lock().map_err(lock_poisoned(lock_label))?;
        if let Some((ref models, ref fetched_at)) = *cached {
            if fetched_at.elapsed() < CACHE_TTL {
                return Ok(models.clone());
            }
        }
    }

    let models = fetcher().await?;

    {
        let mut cached = slot.lock().map_err(lock_poisoned(lock_label))?;
        *cached = Some((models.clone(), Instant::now()));
    }

    Ok(models)
}

#[derive(Deserialize)]
struct AnthropicListResponse {
    data: Vec<AnthropicListModel>,
}

#[derive(Deserialize)]
struct AnthropicListModel {
    id: String,
    #[serde(default)]
    display_name: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiListResponse {
    data: Vec<OpenAiListModel>,
}

#[derive(Deserialize)]
struct OpenAiListModel {
    id: String,
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

async fn fetch_anthropic_models() -> Result<Vec<ModelInfo>, MonarchError> {
    let creds = anthropic_creds()?.ok_or_else(|| {
        MonarchError::persistence(
            "No Anthropic credentials found. Log in via Pi or set ANTHROPIC_API_KEY.",
        )
    })?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let mut req = client
        .get("https://api.anthropic.com/v1/models")
        .header("anthropic-version", "2023-06-01");

    // Pi's Anthropic access token is an OAuth `sk-ant-oat01-...` — goes as
    // a Bearer token with the OAuth beta header. The env-var fallback is a
    // classic `sk-ant-api03-...` API key, which takes `x-api-key`.
    req = match creds.source {
        AuthSource::PiSubscription => req
            .header("Authorization", format!("Bearer {}", creds.token))
            .header("anthropic-beta", "oauth-2025-04-20"),
        AuthSource::EnvApiKey => req.header("x-api-key", creds.token),
    };

    let resp = req.send().await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(MonarchError::persistence(format!(
            "Anthropic /v1/models returned HTTP {status}: {}",
            body.chars().take(200).collect::<String>()
        )));
    }

    let parsed: AnthropicListResponse = resp.json().await?;

    Ok(parsed
        .data
        .into_iter()
        .map(|m| ModelInfo {
            name: m.display_name.unwrap_or_else(|| m.id.clone()),
            id: m.id,
            provider: "anthropic".to_string(),
            context_window: None,
        })
        .collect())
}

// OpenAI's `/v1/models` returns the whole catalogue — embeddings, whisper,
// tts, dall-e, moderation, etc. Filter down to chat-capable families so the
// Codex picker doesn't list `text-embedding-3-large` as something you can
// have a conversation with. Prefixes chosen to cover current + near-future
// reasoning and codex models.
const CODEX_ID_PREFIXES: &[&str] = &["gpt-", "o1", "o3", "o4", "codex-"];

fn is_codex_chat_model(id: &str) -> bool {
    CODEX_ID_PREFIXES.iter().any(|p| id.starts_with(p))
}

async fn fetch_openai_codex_models() -> Result<Vec<ModelInfo>, MonarchError> {
    let creds = openai_codex_creds()?.ok_or_else(|| {
        MonarchError::persistence(
            "No OpenAI Codex credentials found. Log in via Pi or set OPENAI_API_KEY.",
        )
    })?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let resp = client
        .get("https://api.openai.com/v1/models")
        .header("Authorization", format!("Bearer {}", creds.token))
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(MonarchError::persistence(format!(
            "OpenAI /v1/models returned HTTP {status}: {}",
            body.chars().take(200).collect::<String>()
        )));
    }

    let parsed: OpenAiListResponse = resp.json().await?;

    Ok(parsed
        .data
        .into_iter()
        .filter(|m| is_codex_chat_model(&m.id))
        .map(|m| ModelInfo {
            name: m.id.clone(),
            id: m.id,
            provider: "openai-codex".to_string(),
            context_window: None,
        })
        .collect())
}

async fn get_models_inner(
    cache: &ModelCache,
    provider: &str,
    force_refresh: bool,
) -> Result<Vec<ModelInfo>, MonarchError> {
    match provider {
        "anthropic" => {
            cached_or_fetch(
                &cache.anthropic,
                "anthropic cache",
                force_refresh,
                fetch_anthropic_models,
            )
            .await
        }
        "openai-codex" => {
            cached_or_fetch(
                &cache.openai_codex,
                "openai-codex cache",
                force_refresh,
                fetch_openai_codex_models,
            )
            .await
        }
        "openrouter" => {
            cached_or_fetch(
                &cache.openrouter,
                "openrouter cache",
                force_refresh,
                fetch_openrouter_models,
            )
            .await
        }
        "lmstudio" => fetch_lmstudio_models().await,
        _ => Ok(vec![]),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn get_models(
    cache: tauri::State<'_, Arc<ModelCache>>,
    provider: String,
    force_refresh: Option<bool>,
) -> Result<Vec<ModelInfo>, MonarchError> {
    get_models_inner(&cache, &provider, force_refresh.unwrap_or(false)).await
}

// ---- WebSocket wrappers ----

pub async fn ws_get_models(
    cache: &ModelCache,
    provider: String,
    force_refresh: bool,
) -> Result<Vec<ModelInfo>, MonarchError> {
    get_models_inner(cache, &provider, force_refresh).await
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
        "anthropic" => Ok(auth_status_for(
            provider,
            pi_auth_entry_exists("anthropic")?,
            env_var_nonempty("ANTHROPIC_API_KEY").is_some(),
            "Pi Claude",
            "ANTHROPIC_API_KEY",
        )),
        "openai-codex" => Ok(auth_status_for(
            provider,
            pi_auth_entry_exists("openai-codex")?,
            env_var_nonempty("OPENAI_API_KEY").is_some(),
            "Pi Codex",
            "OPENAI_API_KEY",
        )),
        _ => Ok(ProviderAuthStatus {
            provider,
            checked: false,
            configured: false,
            source: None,
            message: "This provider does not use Pi subscription auth checks.".to_string(),
        }),
    }
}

/// Build a `ProviderAuthStatus` honouring the "Pi subscription takes precedence
/// over env API key" order the fetchers themselves use. Keeps the status the
/// UI sees and the credential the fetcher actually picks in lockstep.
fn auth_status_for(
    provider: String,
    pi_found: bool,
    env_found: bool,
    pi_label: &str,
    env_var_name: &str,
) -> ProviderAuthStatus {
    if pi_found {
        ProviderAuthStatus {
            provider,
            checked: true,
            configured: true,
            source: Some("~/.pi/agent/auth.json".to_string()),
            message: format!("{pi_label} subscription auth found."),
        }
    } else if env_found {
        ProviderAuthStatus {
            provider,
            checked: true,
            configured: true,
            source: Some(format!("${env_var_name}")),
            message: format!("Using {env_var_name} from environment."),
        }
    } else {
        ProviderAuthStatus {
            provider,
            checked: true,
            configured: false,
            source: None,
            message: format!(
                "No {pi_label} auth or {env_var_name} found — model list will be unavailable."
            ),
        }
    }
}
