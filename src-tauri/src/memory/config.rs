//! MON-99: global memory / Curator configuration, loaded from
//! `~/.config/monarch/memory.toml`.
//!
//! The Curator is ON by default (MON-130): a missing file or a missing
//! `[keeper]` section resolves to `anthropic/claude-haiku-4-5`, mirroring the
//! classifier's default. Opt out with a top-level `enabled = false`.
//! Embedding defaults to `bge-small-en-v1.5` with lazy model download to
//! `~/.config/monarch/models/`.
//!
//! Example `memory.toml`:
//!
//! ```toml
//! enabled = true
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
/// MON-100: defaults for the continuous-compaction trigger. Soft fires at the
/// next `turn_end` once the running token sum since the last successful
/// Curator run crosses this; hard fires at any `message_end` once it crosses
/// `DEFAULT_HARD_THRESHOLD_TOKENS`. Anthropic context windows are 1M for
/// Sonnet/Opus and 200k for Haiku, so 25k/30k stays comfortably below either.
pub const DEFAULT_SOFT_THRESHOLD_TOKENS: u32 = 25_000;
pub const DEFAULT_HARD_THRESHOLD_TOKENS: u32 = 30_000;
/// MON-130: Curator model used when `memory.toml` has no `[keeper]` section.
pub const DEFAULT_KEEPER_PROVIDER: &str = "anthropic";
pub const DEFAULT_KEEPER_MODEL: &str = "claude-haiku-4-5";

/// MON-100: system prompt shipped with every Curator run. Lives in code (not
/// `memory.toml`) so it evolves with the product the same way
/// `DEFAULT_CLASSIFIER_SYSTEM_PROMPT` does — the settings UI surfaces it
/// read-only via `ResolvedMemoryConfig.keeper_system_prompt`.
///
/// Anchored on `thoughts/design/shadow-cognition/distillation.md` § "What the
/// Keeper produces per tick", § "Atomic claims", and § "What NOT to capture".
/// P2 ships single-tier (no merge/supersede thresholds) — every claim inserts;
/// dedupe lands after MON-93/94 calibrate cosine boundaries.
pub const DEFAULT_KEEPER_SYSTEM_PROMPT: &str = "You are the Curator — the cognitive metabolism of an AI agent. You read a slice of the agent's recent activity (raw messages, tool calls, dialogue) and produce two outputs: a compaction summary that will replace that slice in the agent's working context, and a list of atomic claims worth remembering for the long term.

You will be given:

- RECENT ACTIVITY: messages and tool calls since the last compaction tick.
- PRIOR SUMMARY: the last compaction summary (may be empty on the first tick).
- RELATED MEMORIES: existing claims that look topically related to the recent activity. Use these to avoid duplicating what is already known.

OUTPUT — exactly one JSON object, no prose, no code fences:

{
  \"compaction_summary\": \"<2-6 sentences, third-person, capturing what the agent did, decided, learned, and what is still open. This text will be shown to the agent as its only memory of the slice — it must be enough to continue work coherently.>\",
  \"claims\": [
    {
      \"title\": \"<3-8 words, the claim's heading>\",
      \"summary\": \"<exactly one sentence, the assertion itself>\",
      \"content\": \"<1-3 sentences expanding the claim with context and rationale>\",
      \"kind\": \"<fact | decision | constraint | convention | preference | correction | landmark>\"
    }
  ]
}

ATOMIC CLAIM RULES — each claim must be:

- A single falsifiable assertion. \"Auth is important\" is not a claim; \"Session token TTL is 24 hours\" is.
- Self-contained. A future agent reading just this claim, with no neighbors, must understand it.
- One assertion. If you find yourself writing \"X and Y\", split into two claims.

WHAT TO CAPTURE:

- Decisions made, with the rationale (\"Chose 24h TTL over 1h because compliance requires ≥24h\").
- Conventions discovered (\"This codebase uses Vitest, not Jest\").
- Preferences expressed by the supervisor (\"The supervisor prefers terse responses\").
- Corrections received (\"Don't use tauri::invoke directly; route through src/lib/api.ts\").
- Constraints learned (\"Compliance requires ≥24h token TTL\").
- Landmarks of progress (\"Shipped MON-82 on 2026-04-22\").

WHAT NOT TO CAPTURE — most distillation failures are over-capture, not under-capture:

- Conversational chitchat: \"thanks\", \"ok\", greetings.
- Tool outputs verbatim — the raw stream already has them.
- Transient state: which file is currently open, current branch, in-flight test runs.
- Anything trivially derivable from the codebase (\"Function verify_token lives in auth.rs\" — just grep).
- Routine work observations (\"Read auth.rs\"). Routine ≠ learning.
- Anything already covered by RELATED MEMORIES — if a candidate matches an existing claim, omit it.

Negative test: would an agent benefit from finding this claim again in 6 months? If no, omit.

If the slice produced no durable learning, return an empty `claims` array — that is correct. The compaction summary still goes out so the agent has continuity.";

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
    /// MON-130: master switch. `None` means enabled — the Curator is on by
    /// default; only an explicit `enabled = false` opts out.
    pub enabled: Option<bool>,
    pub keeper: Option<KeeperModelConfig>,
    pub embedding: Option<EmbeddingConfig>,
    pub top_k: Option<u32>,
    /// MON-100: continuous-compaction soft trigger threshold (tokens).
    pub soft_threshold_tokens: Option<u32>,
    /// MON-100: continuous-compaction hard trigger threshold (tokens).
    pub hard_threshold_tokens: Option<u32>,
}

/// Resolved view — all fields filled in, ready to ship to the sidecar / memory index.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedMemoryConfig {
    /// Always populated after resolve (defaults to Haiku, MON-130); gate
    /// Curator work on `enabled`, not on this being `Some`.
    pub keeper: Option<KeeperModelConfig>,
    pub embedding_model_id: String,
    pub models_dir: String,
    pub top_k: u32,
    pub enabled: bool,
    /// MON-100: soft / hard compaction thresholds in tokens. Defaults from
    /// `DEFAULT_SOFT_THRESHOLD_TOKENS` / `DEFAULT_HARD_THRESHOLD_TOKENS`.
    pub soft_threshold_tokens: u32,
    pub hard_threshold_tokens: u32,
    /// MON-100: read-only Curator system prompt. Surfaced in MemorySettings UI;
    /// supervisor-edited prompt is out of scope for Slice B (see plan §
    /// "Out of scope").
    pub keeper_system_prompt: String,
}

pub fn resolve(raw: MemoryConfig) -> ResolvedMemoryConfig {
    // MON-130: on by default. A missing `[keeper]` section used to silently
    // disable the whole Curator; now it just means "use the default model".
    let enabled = raw.enabled.unwrap_or(true);
    let keeper = raw.keeper.or_else(|| {
        Some(KeeperModelConfig {
            provider: DEFAULT_KEEPER_PROVIDER.to_string(),
            model: DEFAULT_KEEPER_MODEL.to_string(),
        })
    });
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
        keeper,
        embedding_model_id,
        models_dir,
        top_k: raw.top_k.unwrap_or(DEFAULT_TOP_K),
        enabled,
        soft_threshold_tokens: raw
            .soft_threshold_tokens
            .unwrap_or(DEFAULT_SOFT_THRESHOLD_TOKENS),
        hard_threshold_tokens: raw
            .hard_threshold_tokens
            .unwrap_or(DEFAULT_HARD_THRESHOLD_TOKENS),
        keeper_system_prompt: DEFAULT_KEEPER_SYSTEM_PROMPT.to_string(),
    }
}

fn default_models_dir() -> String {
    dirs::config_dir()
        .map(|d| {
            d.join("monarch")
                .join("models")
                .to_string_lossy()
                .to_string()
        })
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
pub async fn memory_set_config(config: MemoryConfig) -> Result<ResolvedMemoryConfig, MonarchError> {
    write_raw(&config).await?;
    Ok(resolve(config))
}

#[tauri::command]
#[specta::specta]
pub async fn memory_get_config_path() -> Result<String, MonarchError> {
    Ok(config_path()?.to_string_lossy().to_string())
}
