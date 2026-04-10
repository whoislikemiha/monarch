//! MON-32: typed Rust ↔ sidecar JSONL protocol.
//!
//! Mirrors the canonical contract in `sidecar/src/protocol.ts`. Outbound
//! commands are constructed as `SidecarCommand` values and serialized once at
//! the send site; inbound events are parsed once at the reader boundary into
//! `SidecarEvent`, with the `event` envelope carrying a typed `InnerEvent`
//! that replaces the `get("type").and_then(as_str).unwrap_or("")` dispatch
//! the per-agent `LiveAgentState` used to do against `serde_json::Value`.
//!
//! Unknown event types (envelope or inner) are represented as explicit
//! `Unknown { raw }` variants carrying the original payload, so the reader
//! can flip the dev-only desync indicator without losing the debugging
//! context. Parse failures on *known* tags propagate as
//! `serde_json::Error` → `MonarchError::Serde` via the existing `From` impl.

use serde::Serialize;

// ========================================================================
// Outbound: SidecarCommand
// ========================================================================

/// Mirror of `ShadowConfig` in `sidecar/src/protocol.ts`.
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
pub struct LoadSessionMessage {
    pub role: String,
    pub content: String,
    pub model: Option<String>,
}

/// Commands the Rust backend sends to the Node sidecar over stdin. One
/// variant per TS interface in `sidecar/src/protocol.ts`. Serialized via
/// `serde_json::to_string` at the send site; the `?` on the resulting
/// `serde_json::Error` hits `MonarchError::Serde` via the existing
/// `From<serde_json::Error>` impl.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
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
    },
    DestroySession {
        agent_id: String,
    },
    Prompt {
        agent_id: String,
        message: String,
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
    },
}

impl SidecarCommand {
    /// Replace the `agentId` field in any variant. Used by the
    /// `send_command` passthrough so the frontend can omit the id and the
    /// Rust side can inject it from the Tauri command parameter.
    #[allow(dead_code)]
    pub fn set_agent_id(&mut self, id: String) {
        match self {
            Self::CreateSession { agent_id, .. }
            | Self::DestroySession { agent_id }
            | Self::Prompt { agent_id, .. }
            | Self::Abort { agent_id }
            | Self::SetModel { agent_id, .. }
            | Self::SetThinkingLevel { agent_id, .. }
            | Self::NewSession { agent_id }
            | Self::Compact { agent_id }
            | Self::LoadSession { agent_id, .. }
            | Self::ExtensionUiResponse { agent_id, .. }
            | Self::SetCustomPrompt { agent_id, .. } => {
                *agent_id = id;
            }
        }
    }
}
