use serde::{Deserialize, Serialize};

use crate::memory::search::MemorySearchResult;

use super::config::{ClassifierInvocation, KeeperConfig, LoadSessionMessage, ShadowConfig};

/// Commands the Rust backend sends to the Node sidecar over stdin. One
/// variant per TS interface in `sidecar/src/protocol.ts`. Serialized via
/// `serde_json::to_string` at the send site; the `?` on the resulting
/// `serde_json::Error` hits `MonarchError::Serde` via the existing
/// `From<serde_json::Error>` impl.
///
/// `Deserialize` is also derived to support the `send_command` /
/// `ws_send_command` narrow typed passthrough: the frontend posts a
/// JSON payload, Rust injects `agentId` into the raw `Value`, then
/// `from_value::<SidecarCommand>` validates the shape against the
/// canonical wire contract before reserializing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum SidecarCommand {
    CreateSession {
        agent_id: String,
        cwd: String,
        provider: String,
        model: String,
        thinking_level: String,
        shadow: Option<ShadowConfig>,
        custom_prompt: Option<String>,
        project_instructions: Option<String>,
        context_window: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        captain_identity_payload: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shadow_identity_payload: Option<String>,
    },
    DestroySession {
        agent_id: String,
    },
    Prompt {
        agent_id: String,
        /// Either a plain string or an array of content parts (text + image).
        /// Kept as `Value` so both shapes serialize transparently to the sidecar
        /// without Rust needing to mirror the full multimodal union.
        message: serde_json::Value,
        /// MON-82: classifier invocation for this turn. Rust mints the id
        /// and resolves the config from `classifier.toml` before sending.
        /// `None` when the classifier is disabled.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        classifier: Option<ClassifierInvocation>,
    },
    Abort {
        agent_id: String,
    },
    SetModel {
        agent_id: String,
        provider: String,
        model_id: String,
        context_window: Option<i32>,
    },
    SetThinkingLevel {
        agent_id: String,
        level: String,
    },
    NewSession {
        agent_id: String,
    },
    Compact {
        agent_id: String,
    },
    LoadSession {
        agent_id: String,
        messages: Vec<LoadSessionMessage>,
    },
    ExtensionUiResponse {
        agent_id: String,
        request_id: String,
        value: serde_json::Value,
    },
    SetCustomPrompt {
        agent_id: String,
        prompt: Option<String>,
        project_instructions: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        captain_identity_payload: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shadow_identity_payload: Option<String>,
    },
    /// MON-100: continuous compaction. Rust dispatches one `KeeperRun` when
    /// the per-agent token counter crosses a threshold; sidecar makes a
    /// one-shot LLM call against `config.{provider, model}` with `slice` as
    /// the user message + `config.system_prompt` as the system prompt, then
    /// emits a `keeper_result` event AND rewrites Pi's `state.messages`
    /// in-place with a synthesized scaffold (deferred to next `agent_end`
    /// when streaming).
    KeeperRun {
        agent_id: String,
        run_id: i64,
        trigger: String,
        slice: String,
        config: KeeperConfig,
    },
    /// MON-101: Rust response to a sidecar `memory_search_request`. The
    /// sidecar blocks the user turn briefly waiting for this, then proceeds
    /// without injection on timeout or error.
    MemorySearchResponse {
        agent_id: String,
        request_id: String,
        results: Vec<MemorySearchResult>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}
