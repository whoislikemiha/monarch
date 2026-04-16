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
        /// Either a plain string or an array of content parts (text + image).
        /// Kept as `Value` so both shapes serialize transparently to the sidecar
        /// without Rust needing to mirror the full multimodal union.
        message: serde_json::Value,
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
            // MON-71: stamp agent-span start so AgentEnd can decorate the
            // "finished" status line with elapsed time.
            state.agent_started_at_ms = Some(now_ms());
            state.items.push(DisplayItem::Status {
                text: "Agent started".to_string(),
            });
            ApplyOutcome::EmitNow
        }
        InnerEvent::AgentEnd => {
            state.activity_status = String::new();
            state.is_streaming = false;
            state.commit_streaming_message();
            // MON-71: "Agent finished in 2 min 14 sec". Omit the suffix
            // (and show plain "Agent finished") when we never saw an
            // AgentStart (replayed session) or when the span was
            // sub-1-second — matches the TS formatter's null behaviour.
            let text = match state
                .agent_started_at_ms
                .take()
                .map(|start| now_ms().saturating_sub(start))
                .and_then(crate::agent_state::format_duration_ms)
            {
                Some(d) => format!("Agent finished in {}", d),
                None => "Agent finished".to_string(),
            };
            state.items.push(DisplayItem::Status { text });
            ApplyOutcome::EmitNow
        }
        InnerEvent::TurnStart => {
            state.activity_status = "LLM call in progress...".to_string();
            state.current_tool_group_idx = None;
            // MON-71: stamp the turn's start time. Used at MessageStart to seed
            // `StreamingMessage.turn_started_at_ms` (so the frontend ticker has
            // an anchor) and at MessageEnd to compute the final duration.
            state.turn_started_at_ms = Some(now_ms());
            state.thinking_block_starts.clear();
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
                        .map(crate::agent_state::extract_user_text)
                        .unwrap_or_default();
                    state.items.push(DisplayItem::User {
                        content,
                        timestamp: message.timestamp,
                    });
                    ApplyOutcome::EmitNow
                }
                "assistant" => {
                    let mut sm = streaming_from(message);
                    // MON-71: if no TurnStart preceded this (compaction, retry),
                    // stamp now so the ticker still has something to anchor to.
                    let anchor = state.turn_started_at_ms.unwrap_or_else(now_ms);
                    state.turn_started_at_ms = Some(anchor);
                    sm.turn_started_at_ms = Some(anchor);
                    record_new_thinking_blocks(&mut state.thinking_block_starts, &sm.content);
                    state.streaming_message = Some(sm);
                    state.activity_status = "Receiving response...".to_string();
                    state.is_streaming = true;
                    ApplyOutcome::EmitNow
                }
                _ => ApplyOutcome::NoOp,
            }
        }
        InnerEvent::MessageUpdate { message } => {
            if message.role == "assistant" {
                let mut sm = streaming_from(message);
                if let Some(usage) = sm.usage.clone() {
                    state.last_usage = Some(usage);
                }
                // MON-71: preserve the turn anchor set at MessageStart so the
                // frontend's live ticker doesn't jitter between updates.
                sm.turn_started_at_ms = state.turn_started_at_ms;
                record_new_thinking_blocks(&mut state.thinking_block_starts, &sm.content);
                state.streaming_message = Some(sm);
                // MON-70: fall through so `state_version` gets bumped at the
                // end of the match. Previously this early-returned, meaning
                // every debounced snapshot during a streaming turn carried
                // the same stateVersion and the frontend's `<=` stale-drop
                // check discarded all but the first one — visually manifesting
                // as "nothing streams, everything dumps at the end."
                ApplyOutcome::Debounce
            } else {
                ApplyOutcome::NoOp
            }
        }
        InnerEvent::MessageEnd { message } => {
            if message.role == "assistant" {
                let sm = streaming_from(message);
                if let Some(usage) = sm.usage.clone() {
                    state.last_usage = Some(usage);
                }
                // MON-71: seal remaining thinking blocks and compute turn
                // duration before pushing the finalized assistant item.
                let now = now_ms();
                let mut content = sm.content;
                record_new_thinking_blocks(&mut state.thinking_block_starts, &content);
                finalize_thinking_durations(&state.thinking_block_starts, &mut content, now);
                state.thinking_block_starts.clear();
                let duration_ms = state
                    .turn_started_at_ms
                    .map(|start| now.saturating_sub(start));
                state.turn_started_at_ms = None;
                state.items.push(DisplayItem::Assistant {
                    content,
                    model: sm.model,
                    usage: sm.usage,
                    timestamp: sm.timestamp,
                    duration_ms,
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
            state.is_streaming = true;
            // MON-71: stamp start time. Drives live ticker while the tool is
            // running; subtracted from the end stamp to compute duration.
            let started_at_ms = now_ms();
            let exec = ToolExecution {
                tool_call_id: tool_call_id.clone(),
                tool_name: tool_name.clone(),
                args: args.clone(),
                result: None,
                is_error: None,
                status: ToolStatus::Running,
                started_at_ms: Some(started_at_ms),
                duration_ms: None,
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

            // MON-71: compute duration once from the stamped start time on
            // the canonical `tool_executions` entry, then mirror it onto the
            // tool group's execution below so both views stay in sync.
            let duration_ms = state
                .tool_executions
                .get(tool_call_id)
                .and_then(|e| e.started_at_ms)
                .map(|start| now_ms().saturating_sub(start));

            if let Some(existing) = state.tool_executions.get_mut(tool_call_id) {
                existing.result = result.clone();
                existing.is_error = Some(*is_error);
                existing.status = status;
                existing.duration_ms = duration_ms;
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
                        exec.duration_ms = duration_ms;
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
        // MON-39 item 9: unknown events return NoOp so `state_version`
        // does not bump per event. The reader-side path in
        // `handle_sidecar_event` is the canonical entry that flips
        // `desynced` via `mark_desynced`, bumping the version once per
        // desync transition. This arm is defense-in-depth for unknown
        // events that bypass the early-return (e.g. empty agent_id).
        InnerEvent::Unknown { .. } => ApplyOutcome::NoOp,
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
        turn_started_at_ms: None,
    }
}

/// MON-71: current wall-clock in milliseconds since epoch. Thin wrapper so
/// test code can stub it if needed; production uses `chrono::Utc::now()`.
fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// MON-71: record start times for any thinking blocks that have appeared
/// in the streaming content since the last call. Uses content-block index
/// as the key — Pi SDK appends blocks in order, so index is stable for the
/// lifetime of a turn. Existing entries are never overwritten so we keep
/// the earliest-observed start (the block's real start moment, not the
/// latest update that carried more thinking text).
fn record_new_thinking_blocks(starts: &mut std::collections::HashMap<usize, i64>, content: &[serde_json::Value]) {
    let now = now_ms();
    for (i, block) in content.iter().enumerate() {
        if block.get("type").and_then(|t| t.as_str()) == Some("thinking") {
            starts.entry(i).or_insert(now);
        }
    }
}

/// MON-71: on `MessageEnd`, inject `_monarch.durationMs` into each thinking
/// block's JSON so the duration survives the round trip through SQLite
/// (content is persisted as opaque JSON). The injection key is namespaced
/// to avoid colliding with any Pi SDK field that may land on these blocks.
///
/// End time uses `message_end_ms` for blocks that are immediately followed
/// by another block (text or tool call) and for the final block — the
/// tightest sensible boundary given we only observe deltas, not explicit
/// thinking_end events at this layer. Rounding error is ≤ one debounce
/// window (16 ms) which is invisible at the second-granularity display.
fn finalize_thinking_durations(
    starts: &std::collections::HashMap<usize, i64>,
    content: &mut [serde_json::Value],
    message_end_ms: i64,
) {
    for (i, block) in content.iter_mut().enumerate() {
        if block.get("type").and_then(|t| t.as_str()) != Some("thinking") {
            continue;
        }
        let Some(start) = starts.get(&i).copied() else {
            continue;
        };
        let duration = message_end_ms.saturating_sub(start);
        if let Some(obj) = block.as_object_mut() {
            let meta = obj
                .entry("_monarch".to_string())
                .or_insert_with(|| serde_json::json!({}));
            if let Some(meta_obj) = meta.as_object_mut() {
                meta_obj.insert(
                    "durationMs".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(duration)),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ---- helpers -----------------------------------------------------------

    fn fresh_state() -> LiveAgentState {
        LiveAgentState::default()
    }

    fn msg(role: &str, content: Option<Value>) -> Message {
        Message {
            role: role.to_string(),
            content,
            model: Some("test-model".to_string()),
            usage: None,
            timestamp: Some(1000),
        }
    }

    fn msg_with_usage(role: &str, content: Option<Value>, usage: Usage) -> Message {
        Message {
            role: role.to_string(),
            content,
            model: Some("test-model".to_string()),
            usage: Some(usage),
            timestamp: Some(1000),
        }
    }

    fn assistant_content() -> Option<Value> {
        Some(json!([{"type": "text", "text": "hello"}]))
    }

    fn tool_start(id: &str, name: &str) -> InnerEvent {
        InnerEvent::ToolExecutionStart {
            tool_call_id: id.to_string(),
            tool_name: name.to_string(),
            args: Some(json!({"path": "/tmp"})),
        }
    }

    fn tool_end(id: &str, is_error: bool) -> InnerEvent {
        InnerEvent::ToolExecutionEnd {
            tool_call_id: id.to_string(),
            tool_name: Some("read_file".to_string()),
            result: Some(json!("ok")),
            is_error,
        }
    }

    /// Count items of a specific kind.
    fn count_items(state: &LiveAgentState, kind: &str) -> usize {
        state
            .items
            .iter()
            .filter(|item| match (item, kind) {
                (DisplayItem::User { .. }, "user") => true,
                (DisplayItem::Assistant { .. }, "assistant") => true,
                (DisplayItem::ToolGroup { .. }, "tool-group") => true,
                (DisplayItem::Status { .. }, "status") => true,
                _ => false,
            })
            .count()
    }

    // ---- invariant: event_count always increments --------------------------

    #[test]
    fn event_count_always_increments() {
        let mut s = fresh_state();
        let events: Vec<InnerEvent> = vec![
            InnerEvent::AgentStart,
            InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
            InnerEvent::MessageUpdate {
                message: msg("assistant", assistant_content()),
            },
            InnerEvent::MessageEnd {
                message: msg("assistant", assistant_content()),
            },
            InnerEvent::Unknown {
                raw: json!({"type": "future_event"}),
            },
            InnerEvent::QueueUpdate,
            InnerEvent::AgentEnd,
        ];
        for (i, ev) in events.iter().enumerate() {
            apply_event(&mut s, ev);
            assert_eq!(s.event_count, (i + 1) as u64, "after event {}", i);
        }
    }

    // ---- invariant: state_version -----------------------------------------

    #[test]
    fn state_version_bumps_on_non_noop() {
        let mut s = fresh_state();
        let v_before = s.state_version;
        apply_event(&mut s, &InnerEvent::AgentStart);
        assert_eq!(s.state_version, v_before + 1);
    }

    #[test]
    fn state_version_unchanged_on_noop() {
        let mut s = fresh_state();
        let v_before = s.state_version;
        let outcome = apply_event(&mut s, &InnerEvent::QueueUpdate);
        assert_eq!(outcome, ApplyOutcome::NoOp);
        assert_eq!(s.state_version, v_before);
    }

    #[test]
    fn state_version_bumps_on_debounce() {
        // MON-70: MessageUpdate(assistant) returns Debounce *and* bumps
        // state_version so each debounced snapshot carries a monotonically
        // increasing version. The frontend's stale-drop check (`incoming
        // <= existing`) otherwise discards every snapshot after the first
        // during a streaming turn.
        let mut s = fresh_state();
        // Set up streaming state so the update is meaningful.
        apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
        );
        let v_before = s.state_version;
        let outcome = apply_event(
            &mut s,
            &InnerEvent::MessageUpdate {
                message: msg("assistant", assistant_content()),
            },
        );
        assert_eq!(outcome, ApplyOutcome::Debounce);
        assert_eq!(s.state_version, v_before + 1);
    }

    // ---- AgentStart / AgentEnd --------------------------------------------

    #[test]
    fn agent_start_sets_activity_and_pushes_status() {
        let mut s = fresh_state();
        let outcome = apply_event(&mut s, &InnerEvent::AgentStart);
        assert_eq!(outcome, ApplyOutcome::EmitNow);
        assert_eq!(s.activity_status, "Agent processing...");
        assert_eq!(count_items(&s, "status"), 1);
    }

    #[test]
    fn agent_end_clears_streaming_and_pushes_status() {
        let mut s = fresh_state();
        s.is_streaming = true;
        s.activity_status = "something".to_string();
        let outcome = apply_event(&mut s, &InnerEvent::AgentEnd);
        assert_eq!(outcome, ApplyOutcome::EmitNow);
        assert!(!s.is_streaming);
        assert!(s.activity_status.is_empty());
        assert_eq!(count_items(&s, "status"), 1);
    }

    #[test]
    fn agent_end_commits_streaming_message() {
        let mut s = fresh_state();
        // Simulate assistant streaming in progress without a MessageEnd.
        apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
        );
        assert!(s.streaming_message.is_some());

        apply_event(&mut s, &InnerEvent::AgentEnd);
        assert!(s.streaming_message.is_none());
        assert_eq!(count_items(&s, "assistant"), 1);
        assert!(!s.is_streaming);
    }

    // ---- TurnStart / TurnEnd ----------------------------------------------

    #[test]
    fn turn_start_resets_tool_group_idx() {
        let mut s = fresh_state();
        s.current_tool_group_idx = Some(3);
        let outcome = apply_event(&mut s, &InnerEvent::TurnStart);
        assert_eq!(outcome, ApplyOutcome::EmitNow);
        assert!(s.current_tool_group_idx.is_none());
        assert_eq!(s.activity_status, "LLM call in progress...");
    }

    #[test]
    fn turn_end_marks_tool_group_complete() {
        let mut s = fresh_state();
        // Create a tool group first.
        apply_event(&mut s, &tool_start("t1", "read_file"));
        assert!(s.current_tool_group_idx.is_some());

        let outcome = apply_event(&mut s, &InnerEvent::TurnEnd);
        assert_eq!(outcome, ApplyOutcome::EmitNow);
        assert!(s.current_tool_group_idx.is_none());

        // The tool group should be marked turn_complete.
        match &s.items.last() {
            Some(DisplayItem::ToolGroup { turn_complete, .. }) => {
                assert!(turn_complete);
            }
            other => panic!("expected ToolGroup, got {:?}", other),
        }
    }

    // ---- MessageStart -----------------------------------------------------

    #[test]
    fn message_start_user_pushes_display_item() {
        let mut s = fresh_state();
        let outcome = apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("user", Some(json!("hello world"))),
            },
        );
        assert_eq!(outcome, ApplyOutcome::EmitNow);
        assert_eq!(count_items(&s, "user"), 1);
        match &s.items[0] {
            DisplayItem::User { content, timestamp } => {
                assert_eq!(content, "hello world");
                assert_eq!(*timestamp, Some(1000));
            }
            other => panic!("expected User, got {:?}", other),
        }
    }

    #[test]
    fn message_start_user_extracts_text_from_content_blocks() {
        let mut s = fresh_state();
        apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg(
                    "user",
                    Some(json!([
                        {"type": "text", "text": "hello "},
                        {"type": "image", "source": "..."},
                        {"type": "text", "text": "world"}
                    ])),
                ),
            },
        );
        match &s.items[0] {
            DisplayItem::User { content, .. } => {
                assert_eq!(content, "hello world");
            }
            other => panic!("expected User, got {:?}", other),
        }
    }

    #[test]
    fn message_start_assistant_begins_streaming() {
        let mut s = fresh_state();
        let outcome = apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
        );
        assert_eq!(outcome, ApplyOutcome::EmitNow);
        assert!(s.is_streaming);
        assert!(s.streaming_message.is_some());
        assert_eq!(s.activity_status, "Receiving response...");
    }

    #[test]
    fn message_start_resets_desynced() {
        let mut s = fresh_state();
        s.desynced = true;
        apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
        );
        assert!(!s.desynced);
    }

    #[test]
    fn message_start_unknown_role_is_noop() {
        let mut s = fresh_state();
        let v_before = s.state_version;
        let outcome = apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("system", None),
            },
        );
        // NoOp from the match arm, but desynced is still reset — and then
        // the version bump guard sees NoOp and skips the bump.
        assert_eq!(outcome, ApplyOutcome::NoOp);
        assert_eq!(s.state_version, v_before);
    }

    // ---- MessageUpdate ----------------------------------------------------

    #[test]
    fn message_update_assistant_returns_debounce() {
        let mut s = fresh_state();
        apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
        );
        let outcome = apply_event(
            &mut s,
            &InnerEvent::MessageUpdate {
                message: msg("assistant", Some(json!([{"type": "text", "text": "hello world"}]))),
            },
        );
        assert_eq!(outcome, ApplyOutcome::Debounce);
        assert!(s.streaming_message.is_some());
        let sm = s.streaming_message.as_ref().unwrap();
        assert_eq!(sm.content.len(), 1);
    }

    #[test]
    fn message_update_non_assistant_is_noop() {
        let mut s = fresh_state();
        let outcome = apply_event(
            &mut s,
            &InnerEvent::MessageUpdate {
                message: msg("user", None),
            },
        );
        assert_eq!(outcome, ApplyOutcome::NoOp);
    }

    #[test]
    fn message_update_captures_usage() {
        let mut s = fresh_state();
        apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
        );
        let usage = Usage {
            input: 100,
            output: 50,
            total_tokens: 150,
            ..Default::default()
        };
        apply_event(
            &mut s,
            &InnerEvent::MessageUpdate {
                message: msg_with_usage("assistant", assistant_content(), usage),
            },
        );
        assert!(s.last_usage.is_some());
        assert_eq!(s.last_usage.as_ref().unwrap().input, 100);
    }

    // ---- MessageEnd -------------------------------------------------------

    #[test]
    fn message_end_assistant_commits_to_items() {
        let mut s = fresh_state();
        apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
        );
        apply_event(
            &mut s,
            &InnerEvent::MessageUpdate {
                message: msg("assistant", assistant_content()),
            },
        );
        let outcome = apply_event(
            &mut s,
            &InnerEvent::MessageEnd {
                message: msg("assistant", assistant_content()),
            },
        );
        assert_eq!(outcome, ApplyOutcome::EmitNow);
        assert!(s.streaming_message.is_none());
        assert_eq!(count_items(&s, "assistant"), 1);
    }

    #[test]
    fn message_end_non_assistant_still_emits() {
        // MessageEnd always returns EmitNow regardless of role, but only
        // pushes a DisplayItem for assistant.
        let mut s = fresh_state();
        let outcome = apply_event(
            &mut s,
            &InnerEvent::MessageEnd {
                message: msg("user", None),
            },
        );
        assert_eq!(outcome, ApplyOutcome::EmitNow);
        assert_eq!(count_items(&s, "assistant"), 0);
    }

    // ---- Full assistant message lifecycle ----------------------------------

    #[test]
    fn full_assistant_message_lifecycle() {
        let mut s = fresh_state();

        // Start
        apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
        );
        assert!(s.is_streaming);
        assert!(s.streaming_message.is_some());

        // Multiple updates
        for _ in 0..5 {
            let outcome = apply_event(
                &mut s,
                &InnerEvent::MessageUpdate {
                    message: msg("assistant", assistant_content()),
                },
            );
            assert_eq!(outcome, ApplyOutcome::Debounce);
        }
        assert!(s.streaming_message.is_some());

        // End
        apply_event(
            &mut s,
            &InnerEvent::MessageEnd {
                message: msg("assistant", assistant_content()),
            },
        );
        assert!(s.streaming_message.is_none());
        assert_eq!(count_items(&s, "assistant"), 1);
        // is_streaming is NOT cleared by MessageEnd — only by AgentEnd.
        assert!(s.is_streaming);
    }

    // ---- ToolExecution lifecycle -------------------------------------------

    #[test]
    fn tool_execution_start_creates_group_and_map_entry() {
        let mut s = fresh_state();
        let outcome = apply_event(&mut s, &tool_start("tc1", "read_file"));
        assert_eq!(outcome, ApplyOutcome::EmitNow);
        assert!(s.is_streaming);
        assert_eq!(s.activity_status, "Running tool: read_file");
        assert!(s.tool_executions.contains_key("tc1"));
        assert_eq!(s.tool_executions["tc1"].status, ToolStatus::Running);
        assert_eq!(count_items(&s, "tool-group"), 1);
        assert!(s.current_tool_group_idx.is_some());
    }

    #[test]
    fn tool_execution_end_updates_status_to_done() {
        let mut s = fresh_state();
        apply_event(&mut s, &tool_start("tc1", "read_file"));
        let outcome = apply_event(&mut s, &tool_end("tc1", false));
        assert_eq!(outcome, ApplyOutcome::EmitNow);
        assert!(s.activity_status.is_empty());
        assert_eq!(s.tool_executions["tc1"].status, ToolStatus::Done);
        assert_eq!(s.tool_executions["tc1"].is_error, Some(false));
        assert!(s.tool_executions["tc1"].result.is_some());
    }

    #[test]
    fn tool_execution_end_error_sets_error_status() {
        let mut s = fresh_state();
        apply_event(&mut s, &tool_start("tc1", "write_file"));
        apply_event(&mut s, &tool_end("tc1", true));
        assert_eq!(s.tool_executions["tc1"].status, ToolStatus::Error);
        assert_eq!(s.tool_executions["tc1"].is_error, Some(true));
    }

    #[test]
    fn tool_execution_end_updates_display_item_too() {
        let mut s = fresh_state();
        apply_event(&mut s, &tool_start("tc1", "read_file"));
        apply_event(&mut s, &tool_end("tc1", false));

        // The ToolGroup's execution should also be updated.
        match &s.items.last() {
            Some(DisplayItem::ToolGroup { executions, .. }) => {
                assert_eq!(executions[0].status, ToolStatus::Done);
                assert!(executions[0].result.is_some());
            }
            other => panic!("expected ToolGroup, got {:?}", other),
        }
    }

    // ---- Tool grouping ----------------------------------------------------

    #[test]
    fn consecutive_tools_share_one_group() {
        let mut s = fresh_state();
        apply_event(&mut s, &tool_start("tc1", "read_file"));
        apply_event(&mut s, &tool_start("tc2", "write_file"));

        assert_eq!(count_items(&s, "tool-group"), 1);
        match &s.items.last() {
            Some(DisplayItem::ToolGroup { executions, .. }) => {
                assert_eq!(executions.len(), 2);
                assert_eq!(executions[0].tool_call_id, "tc1");
                assert_eq!(executions[1].tool_call_id, "tc2");
            }
            other => panic!("expected ToolGroup with 2 execs, got {:?}", other),
        }
    }

    #[test]
    fn new_turn_starts_new_tool_group() {
        let mut s = fresh_state();
        // First turn: one tool.
        apply_event(&mut s, &tool_start("tc1", "read_file"));
        apply_event(&mut s, &tool_end("tc1", false));
        apply_event(&mut s, &InnerEvent::TurnEnd);

        // Second turn.
        apply_event(&mut s, &InnerEvent::TurnStart);
        apply_event(&mut s, &tool_start("tc2", "write_file"));

        assert_eq!(count_items(&s, "tool-group"), 2);
    }

    // ---- Interleaved tool + message events ---------------------------------

    #[test]
    fn interleaved_message_and_tool_events() {
        let mut s = fresh_state();

        // Assistant message.
        apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
        );
        apply_event(
            &mut s,
            &InnerEvent::MessageEnd {
                message: msg("assistant", assistant_content()),
            },
        );

        // Tool execution.
        apply_event(&mut s, &tool_start("tc1", "read_file"));
        apply_event(&mut s, &tool_end("tc1", false));

        // Another assistant message.
        apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
        );
        apply_event(
            &mut s,
            &InnerEvent::MessageEnd {
                message: msg("assistant", assistant_content()),
            },
        );

        assert_eq!(count_items(&s, "assistant"), 2);
        assert_eq!(count_items(&s, "tool-group"), 1);

        // Order: Assistant, ToolGroup, Assistant.
        assert!(matches!(&s.items[0], DisplayItem::Assistant { .. }));
        assert!(matches!(&s.items[1], DisplayItem::ToolGroup { .. }));
        assert!(matches!(&s.items[2], DisplayItem::Assistant { .. }));
    }

    // ---- Compaction -------------------------------------------------------

    #[test]
    fn compaction_start_with_reason() {
        let mut s = fresh_state();
        let outcome = apply_event(
            &mut s,
            &InnerEvent::CompactionStart {
                reason: Some("token limit".to_string()),
            },
        );
        assert_eq!(outcome, ApplyOutcome::EmitNow);
        assert_eq!(s.activity_status, "Compacting context...");
        match &s.items[0] {
            DisplayItem::Status { text } => {
                assert!(text.contains("token limit"));
            }
            other => panic!("expected Status, got {:?}", other),
        }
    }

    #[test]
    fn compaction_start_without_reason() {
        let mut s = fresh_state();
        apply_event(
            &mut s,
            &InnerEvent::CompactionStart { reason: None },
        );
        match &s.items[0] {
            DisplayItem::Status { text } => {
                assert!(text.contains("unknown"));
            }
            other => panic!("expected Status, got {:?}", other),
        }
    }

    #[test]
    fn compaction_end_normal() {
        let mut s = fresh_state();
        apply_event(
            &mut s,
            &InnerEvent::CompactionEnd { aborted: Some(false) },
        );
        assert!(s.activity_status.is_empty());
        match &s.items[0] {
            DisplayItem::Status { text } => assert_eq!(text, "Context compacted"),
            other => panic!("expected Status, got {:?}", other),
        }
    }

    #[test]
    fn compaction_end_aborted() {
        let mut s = fresh_state();
        apply_event(
            &mut s,
            &InnerEvent::CompactionEnd { aborted: Some(true) },
        );
        match &s.items[0] {
            DisplayItem::Status { text } => assert_eq!(text, "Compaction aborted"),
            other => panic!("expected Status, got {:?}", other),
        }
    }

    // ---- AutoRetry --------------------------------------------------------

    #[test]
    fn auto_retry_start() {
        let mut s = fresh_state();
        let outcome = apply_event(&mut s, &InnerEvent::AutoRetryStart { attempt: 3 });
        assert_eq!(outcome, ApplyOutcome::EmitNow);
        assert!(s.activity_status.contains("3"));
    }

    #[test]
    fn auto_retry_end_is_noop() {
        let mut s = fresh_state();
        let outcome = apply_event(&mut s, &InnerEvent::AutoRetryEnd);
        assert_eq!(outcome, ApplyOutcome::NoOp);
    }

    // ---- NoOp events ------------------------------------------------------

    #[test]
    fn noop_events_do_not_bump_version() {
        let noop_events = vec![
            InnerEvent::AutoRetryEnd,
            InnerEvent::QueueUpdate,
            InnerEvent::ToolExecutionUpdate,
            InnerEvent::Unknown {
                raw: json!({"type": "from_the_future"}),
            },
        ];
        for ev in &noop_events {
            let mut s = fresh_state();
            let v = s.state_version;
            let outcome = apply_event(&mut s, ev);
            assert_eq!(outcome, ApplyOutcome::NoOp);
            assert_eq!(s.state_version, v, "version bumped on {:?}", ev);
            assert_eq!(s.event_count, 1, "event_count wrong on {:?}", ev);
        }
    }

    // ---- Full agent turn scenario -----------------------------------------

    #[test]
    fn full_agent_turn_scenario() {
        let mut s = fresh_state();

        // Agent starts.
        apply_event(&mut s, &InnerEvent::AgentStart);
        apply_event(&mut s, &InnerEvent::TurnStart);

        // Assistant thinks, then calls a tool.
        apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
        );
        apply_event(
            &mut s,
            &InnerEvent::MessageEnd {
                message: msg("assistant", assistant_content()),
            },
        );
        apply_event(&mut s, &tool_start("tc1", "read_file"));
        apply_event(&mut s, &tool_end("tc1", false));

        // Second LLM call with response.
        apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
        );
        apply_event(
            &mut s,
            &InnerEvent::MessageUpdate {
                message: msg("assistant", assistant_content()),
            },
        );
        apply_event(
            &mut s,
            &InnerEvent::MessageEnd {
                message: msg("assistant", assistant_content()),
            },
        );

        apply_event(&mut s, &InnerEvent::TurnEnd);
        apply_event(&mut s, &InnerEvent::AgentEnd);

        // Final state assertions.
        assert!(!s.is_streaming);
        assert!(s.activity_status.is_empty());
        assert!(s.streaming_message.is_none());
        assert_eq!(count_items(&s, "assistant"), 2);
        assert_eq!(count_items(&s, "tool-group"), 1);
        assert_eq!(count_items(&s, "status"), 2); // AgentStart, AgentEnd
        assert_eq!(s.event_count, 11);
        assert!(s.state_version > 0);
    }
}
