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

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::agent_state::{
    ApplyOutcome, ContentBlocks, DisplayItem, LiveAgentState, StreamingMessage, ToolExecution,
    ToolStatus, Usage,
};

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

// ========================================================================
// Inbound: SidecarEvent + InnerEvent
// ========================================================================

/// Typed `message` field carried by `message_start` / `message_update` /
/// `message_end` inner events. `content` is kept as an opaque
/// `serde_json::Value` because the per-block shape is owned by the Pi SDK
/// (same reasoning as `ContentBlocks` in `agent_state.rs` — the maintenance
/// cost of mirroring the SDK's union outweighs the type safety benefit).
///
/// `role` defaults to empty string to preserve the pre-MON-32 fall-through:
/// an absent or unknown role lets `apply_event` emit a `NoOp` instead of
/// flipping desync. This matches the current `unwrap_or("")` behavior
/// exactly — the ticket's "no silent defaulting" rule is about numeric /
/// id-like fields that must not silently become `0` or `""`, not about
/// best-effort fall-throughs on well-known enum-like strings.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub content: Option<Value>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub usage: Option<Usage>,
    #[serde(default)]
    pub timestamp: Option<i64>,
}

/// The inner event carried by `SidecarEvent::Event { event }`. One variant
/// per `case "xxx":` arm in the pre-MON-32 `apply_event` switch, plus an
/// `Unknown { raw }` fallback for forward-compat with sidecar versions that
/// ship new event kinds — Unknown flips `desynced` via `apply_event` the
/// same way the old catch-all did, but keeps the raw payload around for
/// debugging.
#[derive(Debug, Clone)]
pub enum InnerEvent {
    AgentStart,
    AgentEnd,
    TurnStart,
    TurnEnd,
    MessageStart {
        message: Message,
    },
    MessageUpdate {
        message: Message,
    },
    MessageEnd {
        message: Message,
    },
    ToolExecutionStart {
        tool_call_id: String,
        tool_name: String,
        args: Option<Value>,
    },
    ToolExecutionEnd {
        tool_call_id: String,
        tool_name: Option<String>,
        result: Option<Value>,
        is_error: bool,
    },
    CompactionStart {
        reason: Option<String>,
    },
    CompactionEnd {
        aborted: Option<bool>,
    },
    AutoRetryStart {
        attempt: i64,
    },
    AutoRetryEnd,
    QueueUpdate,
    ToolExecutionUpdate,
    Unknown {
        raw: Value,
    },
}

/// Private Deserialize helper. `InnerEvent::deserialize` first peeks at the
/// `type` tag; known tags are routed through this helper (which has a
/// plain derived `Deserialize`), unknown tags fall through to
/// `InnerEvent::Unknown { raw }`. Keeping the helper distinct from the
/// public enum lets `Unknown` carry a `serde_json::Value` payload —
/// `#[serde(other)]` only supports unit variants.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
enum KnownInnerEvent {
    AgentStart,
    AgentEnd,
    TurnStart,
    TurnEnd,
    MessageStart {
        message: Message,
    },
    MessageUpdate {
        message: Message,
    },
    MessageEnd {
        message: Message,
    },
    ToolExecutionStart {
        tool_call_id: String,
        tool_name: String,
        #[serde(default)]
        args: Option<Value>,
    },
    ToolExecutionEnd {
        tool_call_id: String,
        #[serde(default)]
        tool_name: Option<String>,
        #[serde(default)]
        result: Option<Value>,
        is_error: bool,
    },
    CompactionStart {
        #[serde(default)]
        reason: Option<String>,
    },
    CompactionEnd {
        #[serde(default)]
        aborted: Option<bool>,
    },
    AutoRetryStart {
        attempt: i64,
    },
    AutoRetryEnd,
    QueueUpdate,
    ToolExecutionUpdate,
}

impl From<KnownInnerEvent> for InnerEvent {
    fn from(k: KnownInnerEvent) -> Self {
        match k {
            KnownInnerEvent::AgentStart => Self::AgentStart,
            KnownInnerEvent::AgentEnd => Self::AgentEnd,
            KnownInnerEvent::TurnStart => Self::TurnStart,
            KnownInnerEvent::TurnEnd => Self::TurnEnd,
            KnownInnerEvent::MessageStart { message } => Self::MessageStart { message },
            KnownInnerEvent::MessageUpdate { message } => Self::MessageUpdate { message },
            KnownInnerEvent::MessageEnd { message } => Self::MessageEnd { message },
            KnownInnerEvent::ToolExecutionStart {
                tool_call_id,
                tool_name,
                args,
            } => Self::ToolExecutionStart {
                tool_call_id,
                tool_name,
                args,
            },
            KnownInnerEvent::ToolExecutionEnd {
                tool_call_id,
                tool_name,
                result,
                is_error,
            } => Self::ToolExecutionEnd {
                tool_call_id,
                tool_name,
                result,
                is_error,
            },
            KnownInnerEvent::CompactionStart { reason } => Self::CompactionStart { reason },
            KnownInnerEvent::CompactionEnd { aborted } => Self::CompactionEnd { aborted },
            KnownInnerEvent::AutoRetryStart { attempt } => Self::AutoRetryStart { attempt },
            KnownInnerEvent::AutoRetryEnd => Self::AutoRetryEnd,
            KnownInnerEvent::QueueUpdate => Self::QueueUpdate,
            KnownInnerEvent::ToolExecutionUpdate => Self::ToolExecutionUpdate,
        }
    }
}

/// Tags the sidecar ships today. Keep in sync with `KnownInnerEvent`
/// variants. Drift risk is bounded — a missing tag just routes the event
/// through `Unknown` (benign: desync indicator flips), and a stale tag
/// here can't cause typed deserialization to misfire because the second
/// pass does a strict decode anyway.
const KNOWN_INNER_TAGS: &[&str] = &[
    "agent_start",
    "agent_end",
    "turn_start",
    "turn_end",
    "message_start",
    "message_update",
    "message_end",
    "tool_execution_start",
    "tool_execution_end",
    "compaction_start",
    "compaction_end",
    "auto_retry_start",
    "auto_retry_end",
    "queue_update",
    "tool_execution_update",
];

impl<'de> Deserialize<'de> for InnerEvent {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let raw = Value::deserialize(d)?;
        let tag = raw
            .get("type")
            .and_then(|t| t.as_str())
            .ok_or_else(|| D::Error::missing_field("type"))?;
        if !KNOWN_INNER_TAGS.contains(&tag) {
            return Ok(Self::Unknown { raw });
        }
        let known: KnownInnerEvent = serde_json::from_value(raw).map_err(D::Error::custom)?;
        Ok(known.into())
    }
}

/// Top-level sidecar → Rust envelope. Mirrors the `SidecarEvent` union in
/// `sidecar/src/protocol.ts`.
#[derive(Debug, Clone)]
pub enum SidecarEvent {
    SessionReady {
        agent_id: String,
        context_window: Option<i64>,
    },
    SessionDestroyed {
        agent_id: String,
    },
    Event {
        agent_id: String,
        event: InnerEvent,
    },
    ExtensionUiRequest {
        agent_id: String,
    },
    Error {
        agent_id: String,
        error: String,
    },
    Unknown {
        raw: Value,
    },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
enum KnownSidecarEvent {
    SessionReady {
        agent_id: String,
        #[serde(default)]
        context_window: Option<i64>,
    },
    SessionDestroyed {
        agent_id: String,
    },
    Event {
        agent_id: String,
        event: InnerEvent,
    },
    ExtensionUiRequest {
        agent_id: String,
    },
    Error {
        agent_id: String,
        error: String,
    },
}

impl From<KnownSidecarEvent> for SidecarEvent {
    fn from(k: KnownSidecarEvent) -> Self {
        match k {
            KnownSidecarEvent::SessionReady {
                agent_id,
                context_window,
            } => Self::SessionReady {
                agent_id,
                context_window,
            },
            KnownSidecarEvent::SessionDestroyed { agent_id } => Self::SessionDestroyed { agent_id },
            KnownSidecarEvent::Event { agent_id, event } => Self::Event { agent_id, event },
            KnownSidecarEvent::ExtensionUiRequest { agent_id } => {
                Self::ExtensionUiRequest { agent_id }
            }
            KnownSidecarEvent::Error { agent_id, error } => Self::Error { agent_id, error },
        }
    }
}

const KNOWN_SIDECAR_TAGS: &[&str] = &[
    "session_ready",
    "session_destroyed",
    "event",
    "extension_ui_request",
    "error",
];

impl<'de> Deserialize<'de> for SidecarEvent {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let raw = Value::deserialize(d)?;
        let tag = raw
            .get("type")
            .and_then(|t| t.as_str())
            .ok_or_else(|| D::Error::missing_field("type"))?;
        if !KNOWN_SIDECAR_TAGS.contains(&tag) {
            return Ok(Self::Unknown { raw });
        }
        let known: KnownSidecarEvent = serde_json::from_value(raw).map_err(D::Error::custom)?;
        Ok(known.into())
    }
}

// ========================================================================
// Event application: typed apply_event
// ========================================================================

/// Apply one typed inner event from the sidecar to the per-agent
/// `LiveAgentState`. Moved out of `LiveAgentState::apply_event` in MON-32
/// so the protocol knowledge (tag dispatch + field destructuring) lives
/// next to the enum definitions, leaving `agent_state.rs` focused on the
/// snapshot shape.
///
/// `state_version` is bumped whenever state actually changed (i.e. not on
/// `NoOp` outcomes).
pub fn apply_event(state: &mut LiveAgentState, event: &InnerEvent) -> ApplyOutcome {
    state.event_count = state.event_count.saturating_add(1);

    let outcome = match event {
        InnerEvent::AgentStart => {
            state.activity_status = "Agent processing...".to_string();
            state.items.push(DisplayItem::Status {
                text: "Agent started".to_string(),
            });
            ApplyOutcome::EmitNow
        }
        InnerEvent::AgentEnd => {
            state.activity_status = String::new();
            state.commit_streaming_message();
            state.items.push(DisplayItem::Status {
                text: "Agent finished".to_string(),
            });
            ApplyOutcome::EmitNow
        }
        InnerEvent::TurnStart => {
            state.activity_status = "LLM call in progress...".to_string();
            state.current_tool_group_idx = None;
            ApplyOutcome::EmitNow
        }
        InnerEvent::TurnEnd => {
            state.activity_status = "Processing response...".to_string();
            if let Some(idx) = state.current_tool_group_idx {
                if let Some(DisplayItem::ToolGroup { turn_complete, .. }) =
                    state.items.get_mut(idx)
                {
                    *turn_complete = true;
                }
            }
            state.current_tool_group_idx = None;
            ApplyOutcome::EmitNow
        }
        InnerEvent::MessageStart { message } => {
            state.desynced = false;
            match message.role.as_str() {
                "user" => {
                    let content = message
                        .content
                        .as_ref()
                        .map(extract_user_text)
                        .unwrap_or_default();
                    state.items.push(DisplayItem::User {
                        content,
                        timestamp: message.timestamp,
                    });
                    ApplyOutcome::EmitNow
                }
                "assistant" => {
                    state.streaming_message = Some(streaming_from(message));
                    state.activity_status = "Receiving response...".to_string();
                    ApplyOutcome::EmitNow
                }
                _ => ApplyOutcome::NoOp,
            }
        }
        InnerEvent::MessageUpdate { message } => {
            if message.role == "assistant" {
                let sm = streaming_from(message);
                if let Some(usage) = sm.usage.clone() {
                    state.last_usage = Some(usage);
                }
                state.streaming_message = Some(sm);
                return ApplyOutcome::Debounce;
            }
            ApplyOutcome::NoOp
        }
        InnerEvent::MessageEnd { message } => {
            if message.role == "assistant" {
                let sm = streaming_from(message);
                if let Some(usage) = sm.usage.clone() {
                    state.last_usage = Some(usage);
                }
                state.items.push(DisplayItem::Assistant {
                    content: sm.content,
                    model: sm.model,
                    usage: sm.usage,
                    timestamp: sm.timestamp,
                });
                state.streaming_message = None;
            }
            ApplyOutcome::EmitNow
        }
        InnerEvent::ToolExecutionStart {
            tool_call_id,
            tool_name,
            args,
        } => {
            state.activity_status = format!("Running tool: {}", tool_name);
            let exec = ToolExecution {
                tool_call_id: tool_call_id.clone(),
                tool_name: tool_name.clone(),
                args: args.clone(),
                result: None,
                is_error: None,
                status: ToolStatus::Running,
            };
            state
                .tool_executions
                .insert(tool_call_id.clone(), exec.clone());

            match state.current_tool_group_idx {
                Some(idx) => {
                    if let Some(DisplayItem::ToolGroup { executions, .. }) =
                        state.items.get_mut(idx)
                    {
                        executions.push(exec);
                    }
                }
                None => {
                    state.items.push(DisplayItem::ToolGroup {
                        executions: vec![exec],
                        turn_complete: false,
                    });
                    state.current_tool_group_idx = Some(state.items.len() - 1);
                }
            }
            ApplyOutcome::EmitNow
        }
        InnerEvent::ToolExecutionEnd {
            tool_call_id,
            tool_name: _,
            result,
            is_error,
        } => {
            let status = if *is_error {
                ToolStatus::Error
            } else {
                ToolStatus::Done
            };
            state.activity_status = String::new();

            if let Some(existing) = state.tool_executions.get_mut(tool_call_id) {
                existing.result = result.clone();
                existing.is_error = Some(*is_error);
                existing.status = status;
            }

            if let Some(idx) = state.current_tool_group_idx {
                if let Some(DisplayItem::ToolGroup { executions, .. }) =
                    state.items.get_mut(idx)
                {
                    if let Some(exec) = executions
                        .iter_mut()
                        .find(|e| &e.tool_call_id == tool_call_id)
                    {
                        exec.result = result.clone();
                        exec.is_error = Some(*is_error);
                        exec.status = status;
                    }
                }
            }
            ApplyOutcome::EmitNow
        }
        InnerEvent::CompactionStart { reason } => {
            let reason = reason.as_deref().unwrap_or("unknown");
            state.activity_status = "Compacting context...".to_string();
            state.items.push(DisplayItem::Status {
                text: format!("Context compaction started ({})", reason),
            });
            ApplyOutcome::EmitNow
        }
        InnerEvent::CompactionEnd { aborted } => {
            let aborted = aborted.unwrap_or(false);
            state.activity_status = String::new();
            state.items.push(DisplayItem::Status {
                text: if aborted {
                    "Compaction aborted".to_string()
                } else {
                    "Context compacted".to_string()
                },
            });
            ApplyOutcome::EmitNow
        }
        InnerEvent::AutoRetryStart { attempt } => {
            state.activity_status = format!("Auto-retry attempt {}...", attempt);
            state.items.push(DisplayItem::Status {
                text: format!("Auto-retry attempt {}", attempt),
            });
            ApplyOutcome::EmitNow
        }
        InnerEvent::AutoRetryEnd => ApplyOutcome::NoOp,
        InnerEvent::QueueUpdate => ApplyOutcome::NoOp,
        InnerEvent::ToolExecutionUpdate => ApplyOutcome::NoOp,
        InnerEvent::Unknown { .. } => {
            state.desynced = true;
            ApplyOutcome::EmitNow
        }
    };

    if !matches!(outcome, ApplyOutcome::NoOp) {
        state.state_version = state.state_version.saturating_add(1);
    }
    outcome
}

/// Build a `StreamingMessage` from the typed inner `Message`. Replaces the
/// pre-MON-32 `streaming_from_json` helper in `agent_state.rs` which had to
/// walk `serde_json::Value` by hand. `content` extraction keeps the same
/// semantics: take the array if present, empty vec otherwise.
fn streaming_from(message: &Message) -> StreamingMessage {
    let content: ContentBlocks = message
        .content
        .as_ref()
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();
    StreamingMessage {
        content,
        model: message.model.clone(),
        usage: message.usage.clone(),
        timestamp: message.timestamp,
    }
}

/// Extract plain text from a Pi SDK message.content field, which may be
/// either a string or an array of content blocks. Mirrors the inline logic
/// in the pre-MON-32 `agent_state.rs::extract_user_text` — kept here so the
/// typed `apply_event` path doesn't need to reach back into the agent_state
/// module for a helper, and the recovery-path copy in `agent_state.rs`
/// stays independent.
fn extract_user_text(content: &Value) -> String {
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    if let Some(blocks) = content.as_array() {
        return blocks
            .iter()
            .filter_map(|b| {
                if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                    b.get("text").and_then(|t| t.as_str()).map(String::from)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("");
    }
    String::new()
}
