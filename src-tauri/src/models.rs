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
    /// Whether this model is reachable via Pi's subscription credentials.
    /// `Some(true)` for the curated subscription set, `Some(false)` for live
    /// API-only entries, `None` for providers where the distinction is moot
    /// (OpenRouter, LM Studio).
    pub subscription: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AuthMode {
    None,
    Subscription,
    ApiKey,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAuthStatus {
    pub provider: String,
    pub checked: bool,
    pub configured: bool,
    pub source: Option<String>,
    pub message: String,
    pub auth_mode: AuthMode,
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

fn env_var_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

/// Curated subscription-supported set for Anthropic. Used both as the
/// fallback list when no API key is configured AND as the membership
/// check that flags subscription support on the live API list.
const ANTHROPIC_SUBSCRIPTION_MODELS: &[(&str, &str)] = &[
    ("claude-opus-4-7", "Claude Opus 4.7"),
    ("claude-sonnet-4-6", "Claude Sonnet 4.6"),
    ("claude-haiku-4-5", "Claude Haiku 4.5"),
];

const OPENAI_CODEX_SUBSCRIPTION_MODELS: &[&str] = &[
    "gpt-5.4",
    "gpt-5",
    "gpt-5-mini",
    "o3",
    "o3-mini",
    "o4-mini",
    "codex-mini-latest",
];

fn anthropic_subscription_supports(id: &str) -> bool {
    ANTHROPIC_SUBSCRIPTION_MODELS
        .iter()
        .any(|(known, _)| *known == id)
}

fn openai_codex_subscription_supports(id: &str) -> bool {
    OPENAI_CODEX_SUBSCRIPTION_MODELS.contains(&id)
}

/// Curated fallback for Anthropic. Used when no `ANTHROPIC_API_KEY` env var
/// is set — Pi's subscription OAuth tokens cannot call `/v1/models`
/// (Anthropic returns "OAuth authentication is currently not supported"),
/// so OAuth-only users see this list. Bump these as new models ship.
fn anthropic_curated() -> Vec<ModelInfo> {
    ANTHROPIC_SUBSCRIPTION_MODELS
        .iter()
        .map(|(id, name)| ModelInfo {
            id: id.to_string(),
            name: name.to_string(),
            provider: "anthropic".to_string(),
            context_window: None,
            subscription: Some(true),
        })
        .collect()
}

/// Curated fallback for OpenAI Codex. Same story: Pi's ChatGPT subscription
/// JWT lacks the `api.model.read` scope and is rejected by `/v1/models`,
/// so OAuth-only users see this list.
fn openai_codex_curated() -> Vec<ModelInfo> {
    OPENAI_CODEX_SUBSCRIPTION_MODELS
        .iter()
        .map(|id| ModelInfo {
            id: id.to_string(),
            name: id.to_string(),
            provider: "openai-codex".to_string(),
            context_window: None,
            subscription: Some(true),
        })
        .collect()
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
    /// Unix epoch seconds. Used to sort newest-first so the dropdown's
    /// first 50 entries are the latest models, not whatever order OpenAI
    /// returns (which is roughly by id alphabetically).
    #[serde(default)]
    created: u64,
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
            subscription: None,
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
            subscription: None,
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
            subscription: None,
        })
        .collect())
}

async fn fetch_anthropic_models(api_key: String) -> Result<Vec<ModelInfo>, MonarchError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let resp = client
        .get("https://api.anthropic.com/v1/models")
        .header("anthropic-version", "2023-06-01")
        .header("x-api-key", api_key)
        .send()
        .await?;

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
            subscription: Some(anthropic_subscription_supports(&m.id)),
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

async fn fetch_openai_codex_models(api_key: String) -> Result<Vec<ModelInfo>, MonarchError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let resp = client
        .get("https://api.openai.com/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
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

    let mut filtered: Vec<OpenAiListModel> = parsed
        .data
        .into_iter()
        .filter(|m| is_codex_chat_model(&m.id))
        .collect();
    filtered.sort_by(|a, b| b.created.cmp(&a.created));

    Ok(filtered
        .into_iter()
        .map(|m| ModelInfo {
            name: m.id.clone(),
            subscription: Some(openai_codex_subscription_supports(&m.id)),
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
        // Anthropic + Codex listing endpoints don't accept Pi's subscription
        // OAuth tokens — Anthropic outright rejects OAuth on /v1/models, and
        // ChatGPT JWTs lack the `api.model.read` scope. We only attempt a
        // live fetch when an API-key env var is present; otherwise return
        // the curated fallback so subscription-only users still see a useful
        // list. If the API-key live fetch fails, we surface the error rather
        // than silently degrading — the user opted into live by setting the
        // key and should see why it broke.
        "anthropic" => {
            if let Some(api_key) = env_var_nonempty("ANTHROPIC_API_KEY") {
                cached_or_fetch(
                    &cache.anthropic,
                    "anthropic cache",
                    force_refresh,
                    || fetch_anthropic_models(api_key),
                )
                .await
            } else {
                Ok(anthropic_curated())
            }
        }
        "openai-codex" => {
            if let Some(api_key) = env_var_nonempty("OPENAI_API_KEY") {
                cached_or_fetch(
                    &cache.openai_codex,
                    "openai-codex cache",
                    force_refresh,
                    || fetch_openai_codex_models(api_key),
                )
                .await
            } else {
                Ok(openai_codex_curated())
            }
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
            auth_mode: AuthMode::None,
        }),
    }
}

/// Build a `ProviderAuthStatus` describing both spawn-auth (which credential
/// will be used to call the provider at session-spawn time, handled by Pi)
/// and listing-auth (whether the model picker shows a live or curated list).
/// Pi's subscription OAuth tokens cover spawn but cannot call `/v1/models`,
/// so the listing path keys off the env API key only.
fn auth_status_for(
    provider: String,
    pi_found: bool,
    env_found: bool,
    pi_label: &str,
    env_var_name: &str,
) -> ProviderAuthStatus {
    let (configured, source, message, auth_mode) = match (pi_found, env_found) {
        (true, true) => (
            true,
            Some(format!("~/.pi/agent/auth.json + ${env_var_name}")),
            format!("{pi_label} subscription for spawn, {env_var_name} for live model list."),
            AuthMode::Both,
        ),
        (true, false) => (
            true,
            Some("~/.pi/agent/auth.json".to_string()),
            format!(
                "{pi_label} subscription found. Showing curated model list — \
                 set {env_var_name} for live discovery."
            ),
            AuthMode::Subscription,
        ),
        (false, true) => (
            true,
            Some(format!("${env_var_name}")),
            format!("Using {env_var_name} from environment for spawn and live model list."),
            AuthMode::ApiKey,
        ),
        (false, false) => (
            false,
            None,
            format!(
                "No {pi_label} auth or {env_var_name} found. \
                 Showing curated fallback list; spawning will fail until you log in."
            ),
            AuthMode::None,
        ),
    };

    ProviderAuthStatus {
        provider,
        checked: true,
        configured,
        source,
        message,
        auth_mode,
    }
}
