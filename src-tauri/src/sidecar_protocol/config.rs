use serde::{Deserialize, Serialize};

/// Mirror of `ShadowConfig` in `sidecar/src/protocol.ts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShadowConfig {
    pub name: String,
    pub title: String,
    pub grade: String,
    pub id: String,
}

/// Message row shape carried by `load_session`. Mirrors the inline interface
/// in `LoadSessionCommand.messages[]` — role is left as a free-form string
/// because the DB already stores arbitrary role strings and we don't want to
/// gate sidecar replay on a validation layer that isn't in scope for MON-32.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadSessionMessage {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub model: Option<String>,
}

/// MON-82: per-turn classifier invocation mirrored on the sidecar side.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifierProvider {
    pub provider: String,
    pub model: String,
}

/// MON-100: Curator invocation config mirrored on the sidecar side. Rust resolves
/// provider/model/systemPrompt from `~/.config/monarch/memory.toml` and ships
/// it per call so the sidecar stays stateless WRT Curator config (same shape
/// pattern as `ClassifierInvocationConfig`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeeperConfig {
    pub provider: String,
    pub model: String,
    pub system_prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifierInvocationConfig {
    pub enabled: bool,
    pub primary: ClassifierProvider,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<ClassifierProvider>,
    pub timeout_ms: u32,
    pub system_prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifierInvocation {
    pub id: String,
    pub config: ClassifierInvocationConfig,
}
