//! Per-agent live state, assembled from sidecar events.
//!
//! Before MON-14 this assembly lived in `AgentView.svelte`'s event handler;
//! each browser receiving `agent-event-{id}` did the stitching. Now Rust owns
//! the assembled snapshot and publishes it on `agent-state-{id}`. The legacy
//! raw channel is still emitted during Phase 1 so the existing frontend keeps
//! working; Phase 2 flips the consumer.
//!
//! The wire shape intentionally mirrors the hand-written `LiveAgentState` in
//! `src/lib/toolbox/types.ts` so Phase 2's swap to the specta-generated types
//! is mechanical. A `TurnState` enum was considered for invariant safety
//! (parent plan §1) but rejected in favor of a flat shape that matches the
//! frontend contract — the invariants are enforced inside `apply_event`.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;

// ---- Content blocks ----------------------------------------------------
//
// Assistant content blocks and tool args/results are kept as `serde_json::Value`.
// They are extensibility points owned by the Pi SDK and the maintenance cost of
// mirroring their full schema in Rust outweighs the specta benefit. The
// frontend already treats these as opaque in most paths.

pub type ContentBlocks = Vec<serde_json::Value>;
pub type ToolArgs = serde_json::Value;
pub type ToolResult = serde_json::Value;

// ---- Usage -------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase")]
pub struct Cost {
    pub input: f64,
    pub output: f64,
    pub total: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub total_tokens: i64,
    pub cost: Cost,
}

// ---- Tool execution ----------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ToolStatus {
    Running,
    Done,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecution {
    pub tool_call_id: String,
    pub tool_name: String,
    pub args: Option<ToolArgs>,
    pub result: Option<ToolResult>,
    pub is_error: Option<bool>,
    pub status: ToolStatus,
}

// ---- Streaming message -------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase")]
pub struct StreamingMessage {
    pub content: ContentBlocks,
    pub model: Option<String>,
    pub usage: Option<Usage>,
    pub timestamp: Option<i64>,
}

// ---- Display items -----------------------------------------------------
//
// Mirrors the `DisplayItem` union in src/lib/types.ts. The serde tag `kind`
// matches what the frontend switches on.

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum DisplayItem {
    User {
        content: String,
        timestamp: Option<i64>,
    },
    Assistant {
        content: ContentBlocks,
        model: Option<String>,
        usage: Option<Usage>,
        timestamp: Option<i64>,
    },
    #[serde(rename = "tool-group")]
    ToolGroup {
        executions: Vec<ToolExecution>,
        turn_complete: bool,
    },
    Status {
        text: String,
    },
    Notification {
        text: String,
        level: NotificationLevel,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
}

// ---- LiveAgentState ----------------------------------------------------
//
// The wire type emitted on `agent-state-{id}`. Field names use camelCase
// via serde(rename_all) so the generated TS matches the existing frontend
// shape exactly. `state_version` and `desynced` are the only new fields
// vs. the hand-written TS interface.

#[derive(Debug, Clone, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase")]
pub struct LiveAgentState {
    pub items: Vec<DisplayItem>,
    /// Flat map from tool call id → execution. Serializes as a JS object.
    /// Phase 2's store adapter converts to a `Map` before handing to tool
    /// components so the `AgentContext.live` shape stays frozen.
    pub tool_executions: HashMap<String, ToolExecution>,
    pub streaming_message: Option<StreamingMessage>,
    pub last_usage: Option<Usage>,
    /// Index into `items` pointing at the currently open tool-group, if any.
    /// Kept as an index rather than a ref to keep the struct `Clone`-cheap and
    /// serialization-safe. `None` when no tool group is open for this turn.
    #[serde(skip)]
    pub current_tool_group_idx: Option<usize>,
    pub activity_status: String,
    pub event_count: u64,
    /// Set to true when the reader hit a parse failure or an out-of-order
    /// event it could not reconcile. Reset to false on the next `message_start`.
    pub desynced: bool,
    /// Monotonically increasing per-agent. The frontend reconciles by dropping
    /// any incoming snapshot whose version is <= its current entry version.
    pub state_version: u64,
}

// ---- Event application -------------------------------------------------

/// Whether the caller should emit a snapshot immediately, debounce it,
/// or skip emission entirely for this event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyOutcome {
    /// Flush a snapshot now. Used for terminal events (message_end,
    /// tool_execution_end, agent_end, etc.) where latency matters.
    EmitNow,
    /// Mark the entry dirty and rely on the caller's coalescing timer.
    /// Used for message_update during streaming — token-rate events are
    /// coalesced into ~60fps snapshots (see DEBOUNCE_MILLIS in agent.rs).
    Debounce,
    /// No state change; caller does not need to emit.
    NoOp,
}

impl LiveAgentState {
    /// Apply one parsed inner event from the sidecar to this state.
    ///
    /// Port of the `handleEvent` switch in `src/lib/AgentView.svelte` pre-MON-14.
    /// `state_version` is bumped whenever state actually changed (i.e. not on
    /// `NoOp` outcomes).
    pub fn apply_event(&mut self, event: &serde_json::Value) -> ApplyOutcome {
        let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");
        self.event_count = self.event_count.saturating_add(1);

        let outcome = match event_type {
            "agent_start" => {
                self.activity_status = "Agent processing...".to_string();
                self.items.push(DisplayItem::Status {
                    text: "Agent started".to_string(),
                });
                ApplyOutcome::EmitNow
            }
            "agent_end" => {
                self.activity_status = String::new();
                self.commit_streaming_message();
                self.items.push(DisplayItem::Status {
                    text: "Agent finished".to_string(),
                });
                ApplyOutcome::EmitNow
            }
            "turn_start" => {
                self.activity_status = "LLM call in progress...".to_string();
                self.current_tool_group_idx = None;
                ApplyOutcome::EmitNow
            }
            "turn_end" => {
                self.activity_status = "Processing response...".to_string();
                if let Some(idx) = self.current_tool_group_idx {
                    if let Some(DisplayItem::ToolGroup { turn_complete, .. }) =
                        self.items.get_mut(idx)
                    {
                        *turn_complete = true;
                    }
                }
                self.current_tool_group_idx = None;
                ApplyOutcome::EmitNow
            }
            "message_start" => {
                self.desynced = false;
                let message = event.get("message");
                let role = message
                    .and_then(|m| m.get("role"))
                    .and_then(|r| r.as_str())
                    .unwrap_or("");
                match role {
                    "user" => {
                        let content = message
                            .and_then(|m| m.get("content"))
                            .map(extract_user_text)
                            .unwrap_or_default();
                        let timestamp = message
                            .and_then(|m| m.get("timestamp"))
                            .and_then(|t| t.as_i64());
                        self.items.push(DisplayItem::User { content, timestamp });
                        ApplyOutcome::EmitNow
                    }
                    "assistant" => {
                        self.streaming_message = message.map(streaming_from_json);
                        self.activity_status = "Receiving response...".to_string();
                        ApplyOutcome::EmitNow
                    }
                    _ => ApplyOutcome::NoOp,
                }
            }
            "message_update" => {
                if let Some(message) = event.get("message") {
                    if message.get("role").and_then(|r| r.as_str()) == Some("assistant") {
                        let sm = streaming_from_json(message);
                        if let Some(usage) = sm.usage.clone() {
                            self.last_usage = Some(usage);
                        }
                        self.streaming_message = Some(sm);
                        return ApplyOutcome::Debounce;
                    }
                }
                ApplyOutcome::NoOp
            }
            "message_end" => {
                if let Some(message) = event.get("message") {
                    if message.get("role").and_then(|r| r.as_str()) == Some("assistant") {
                        let sm = streaming_from_json(message);
                        if let Some(usage) = sm.usage.clone() {
                            self.last_usage = Some(usage);
                        }
                        self.items.push(DisplayItem::Assistant {
                            content: sm.content,
                            model: sm.model,
                            usage: sm.usage,
                            timestamp: sm.timestamp,
                        });
                        self.streaming_message = None;
                    }
                }
                ApplyOutcome::EmitNow
            }
            "tool_execution_start" => {
                let tool_call_id = event
                    .get("toolCallId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let tool_name = event
                    .get("toolName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let args = event.get("args").cloned();
                self.activity_status = format!("Running tool: {}", tool_name);
                let exec = ToolExecution {
                    tool_call_id: tool_call_id.clone(),
                    tool_name,
                    args,
                    result: None,
                    is_error: None,
                    status: ToolStatus::Running,
                };
                self.tool_executions
                    .insert(tool_call_id.clone(), exec.clone());

                match self.current_tool_group_idx {
                    Some(idx) => {
                        if let Some(DisplayItem::ToolGroup { executions, .. }) =
                            self.items.get_mut(idx)
                        {
                            executions.push(exec);
                        }
                    }
                    None => {
                        self.items.push(DisplayItem::ToolGroup {
                            executions: vec![exec],
                            turn_complete: false,
                        });
                        self.current_tool_group_idx = Some(self.items.len() - 1);
                    }
                }
                ApplyOutcome::EmitNow
            }
            "tool_execution_end" => {
                let tool_call_id = event
                    .get("toolCallId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let result = event.get("result").cloned();
                let is_error = event
                    .get("isError")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let status = if is_error {
                    ToolStatus::Error
                } else {
                    ToolStatus::Done
                };

                self.activity_status = String::new();

                if let Some(existing) = self.tool_executions.get_mut(&tool_call_id) {
                    existing.result = result.clone();
                    existing.is_error = Some(is_error);
                    existing.status = status;
                }

                if let Some(idx) = self.current_tool_group_idx {
                    if let Some(DisplayItem::ToolGroup { executions, .. }) =
                        self.items.get_mut(idx)
                    {
                        if let Some(exec) =
                            executions.iter_mut().find(|e| e.tool_call_id == tool_call_id)
                        {
                            exec.result = result;
                            exec.is_error = Some(is_error);
                            exec.status = status;
                        }
                    }
                }
                ApplyOutcome::EmitNow
            }
            "compaction_start" => {
                let reason = event
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                self.activity_status = "Compacting context...".to_string();
                self.items.push(DisplayItem::Status {
                    text: format!("Context compaction started ({})", reason),
                });
                ApplyOutcome::EmitNow
            }
            "compaction_end" => {
                let aborted = event
                    .get("aborted")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                self.activity_status = String::new();
                self.items.push(DisplayItem::Status {
                    text: if aborted {
                        "Compaction aborted".to_string()
                    } else {
                        "Context compacted".to_string()
                    },
                });
                ApplyOutcome::EmitNow
            }
            "auto_retry_start" => {
                let attempt = event
                    .get("attempt")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                self.activity_status = format!("Auto-retry attempt {}...", attempt);
                self.items.push(DisplayItem::Status {
                    text: format!("Auto-retry attempt {}", attempt),
                });
                ApplyOutcome::EmitNow
            }
            "auto_retry_end" => ApplyOutcome::NoOp,
            "queue_update" => ApplyOutcome::NoOp,
            "tool_execution_update" => ApplyOutcome::NoOp,
            // Truly-unknown event type: the reader saw something our state
            // machine doesn't know how to fold in. Flip the desync flag so
            // the dev-only frontend indicator can surface it.
            _ => {
                self.desynced = true;
                ApplyOutcome::EmitNow
            }
        };

        if !matches!(outcome, ApplyOutcome::NoOp) {
            self.state_version = self.state_version.saturating_add(1);
        }
        outcome
    }

    /// Replace `items` with a fresh list rebuilt from persisted messages on
    /// recovery. Resets the streaming/tool-group tracking and bumps the
    /// version. Caller is responsible for emitting one snapshot after this.
    ///
    /// Mid-stream assembly (partial streaming_message, in-flight tool group)
    /// is intentionally dropped on recovery — we cannot reconstruct it from
    /// SQLite, and showing a frozen partial state is worse than a clean reset.
    pub fn reset_with_items(&mut self, items: Vec<DisplayItem>) {
        self.items = items;
        self.tool_executions.clear();
        self.streaming_message = None;
        self.current_tool_group_idx = None;
        self.activity_status = String::new();
        self.desynced = false;
        self.state_version = self.state_version.saturating_add(1);
    }

    /// Commit a live streaming message to `items` and clear it. Used by
    /// `agent_end` when the sidecar never sent a distinct `message_end`.
    fn commit_streaming_message(&mut self) {
        if let Some(sm) = self.streaming_message.take() {
            self.items.push(DisplayItem::Assistant {
                content: sm.content,
                model: sm.model,
                usage: sm.usage,
                timestamp: sm.timestamp,
            });
        }
    }

    /// Mark the entry desynced after a parse failure or unexpected state.
    /// Called from the reader task's malformed-line branch; surfaced to the
    /// dev-only indicator (`VITE_MONARCH_DEBUG_DESYNC`). Reset on the next
    /// `message_start`.
    pub fn mark_desynced(&mut self) {
        self.desynced = true;
        self.state_version = self.state_version.saturating_add(1);
    }
}

// ---- Helpers ----

/// Extract plain-text content from a Pi message.content field which may be
/// either a string or an array of content blocks. Mirrors the inline logic in
/// AgentView.svelte's `message_start` case for user messages.
fn extract_user_text(content: &serde_json::Value) -> String {
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

fn streaming_from_json(message: &serde_json::Value) -> StreamingMessage {
    let content = message
        .get("content")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();
    let model = message
        .get("model")
        .and_then(|m| m.as_str())
        .map(String::from);
    let usage = message.get("usage").and_then(parse_usage);
    let timestamp = message.get("timestamp").and_then(|t| t.as_i64());
    StreamingMessage {
        content,
        model,
        usage,
        timestamp,
    }
}

fn parse_usage(value: &serde_json::Value) -> Option<Usage> {
    let obj = value.as_object()?;
    let get_i64 = |k: &str| obj.get(k).and_then(|v| v.as_i64()).unwrap_or(0);
    let cost = obj
        .get("cost")
        .and_then(|c| c.as_object())
        .map(|c| Cost {
            input: c.get("input").and_then(|v| v.as_f64()).unwrap_or(0.0),
            output: c.get("output").and_then(|v| v.as_f64()).unwrap_or(0.0),
            total: c.get("total").and_then(|v| v.as_f64()).unwrap_or(0.0),
        })
        .unwrap_or_default();
    Some(Usage {
        input: get_i64("input"),
        output: get_i64("output"),
        cache_read: get_i64("cacheRead"),
        cache_write: get_i64("cacheWrite"),
        total_tokens: get_i64("totalTokens"),
        cost,
    })
}

// ---- Recovery helpers --------------------------------------------------

/// Rebuild `items` from persisted messages following the same layout rules
/// as `restoreDisplayItemsFromMessages` in AgentView.svelte. The `messages`
/// list is the output of `db.get_messages_with_ancestry(session_id)`.
pub fn display_items_from_messages(
    messages: &[crate::db::MessageRow],
    status_text: &str,
) -> Vec<DisplayItem> {
    let mut restored: Vec<DisplayItem> = Vec::new();
    let mut pending_tool_results: Vec<ToolExecution> = Vec::new();

    let flush_pending = |restored: &mut Vec<DisplayItem>, pending: &mut Vec<ToolExecution>| {
        if pending.is_empty() {
            return;
        }
        let drained = std::mem::take(pending);
        restored.push(DisplayItem::ToolGroup {
            executions: drained,
            turn_complete: true,
        });
    };

    for (index, msg) in messages.iter().enumerate() {
        match msg.role.as_str() {
            "user" => {
                flush_pending(&mut restored, &mut pending_tool_results);
                let content = parse_stored_content(&msg.content);
                let text = extract_user_text(&content);
                restored.push(DisplayItem::User {
                    content: text,
                    timestamp: parse_timestamp(&msg.timestamp),
                });
            }
            "toolResult" => {
                if let Some(exec) =
                    parse_stored_tool_result(&msg.content, &format!("restored-tool-{}", index))
                {
                    pending_tool_results.push(exec);
                }
            }
            "assistant" => {
                let content_value = parse_stored_content(&msg.content);
                let content_blocks: ContentBlocks = content_value
                    .as_array()
                    .cloned()
                    .unwrap_or_else(|| vec![content_value.clone()]);

                let tool_calls: Vec<&serde_json::Value> = content_blocks
                    .iter()
                    .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("toolCall"))
                    .collect();

                if !tool_calls.is_empty() || !pending_tool_results.is_empty() {
                    let pending_by_id: HashMap<String, ToolExecution> = pending_tool_results
                        .drain(..)
                        .map(|e| (e.tool_call_id.clone(), e))
                        .collect();
                    let mut merged: Vec<ToolExecution> = Vec::new();
                    let mut handled_ids: std::collections::HashSet<String> =
                        std::collections::HashSet::new();
                    for tc in &tool_calls {
                        let id = tc
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let name = tc
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("tool")
                            .to_string();
                        let args = tc.get("arguments").cloned();
                        handled_ids.insert(id.clone());
                        let result_entry = pending_by_id.get(&id);
                        merged.push(ToolExecution {
                            tool_call_id: id,
                            tool_name: name,
                            args,
                            result: result_entry.and_then(|e| e.result.clone()),
                            is_error: result_entry.and_then(|e| e.is_error),
                            status: result_entry
                                .map(|e| e.status)
                                .unwrap_or(ToolStatus::Done),
                        });
                    }
                    for (id, exec) in pending_by_id {
                        if !handled_ids.contains(&id) {
                            merged.push(exec);
                        }
                    }
                    if !merged.is_empty() {
                        restored.push(DisplayItem::ToolGroup {
                            executions: merged,
                            turn_complete: true,
                        });
                    }
                }

                if has_visible_assistant_content(&content_blocks) {
                    restored.push(DisplayItem::Assistant {
                        content: content_blocks,
                        model: msg.model.clone(),
                        usage: None,
                        timestamp: parse_timestamp(&msg.timestamp),
                    });
                }
            }
            _ => {}
        }
    }

    flush_pending(&mut restored, &mut pending_tool_results);

    if restored.is_empty() {
        return vec![DisplayItem::Status {
            text: format!("{} (no stored messages)", status_text),
        }];
    }

    let mut with_header = Vec::with_capacity(restored.len() + 1);
    with_header.push(DisplayItem::Status {
        text: status_text.to_string(),
    });
    with_header.extend(restored);
    with_header
}

fn parse_stored_content(stored: &str) -> serde_json::Value {
    let trimmed = stored.trim();
    if trimmed.is_empty() {
        return serde_json::Value::String(String::new());
    }
    let looks_serialized =
        trimmed.starts_with('[') || trimmed.starts_with('{') || trimmed.starts_with('"');
    if !looks_serialized {
        return serde_json::Value::String(stored.to_string());
    }
    serde_json::from_str(stored).unwrap_or_else(|_| serde_json::Value::String(stored.to_string()))
}

fn parse_stored_tool_result(content: &str, fallback_id: &str) -> Option<ToolExecution> {
    let parsed = parse_stored_content(content);
    let obj = parsed.as_object()?;
    let tool_call_id = obj
        .get("toolCallId")
        .and_then(|v| v.as_str())
        .unwrap_or(fallback_id)
        .to_string();
    let tool_name = obj
        .get("toolName")
        .and_then(|v| v.as_str())
        .unwrap_or("tool")
        .to_string();
    let is_error = obj.get("isError").and_then(|v| v.as_bool()).unwrap_or(false);
    let result_raw = obj.get("result").cloned();
    let result = result_raw.map(|r| {
        if let Some(s) = r.as_str() {
            parse_stored_content(s)
        } else {
            r
        }
    });
    // Skip rows that look empty (no id, no name, no result) — same as the TS version.
    if obj.get("toolCallId").is_none()
        && obj.get("toolName").is_none()
        && result.is_none()
    {
        return None;
    }
    Some(ToolExecution {
        tool_call_id,
        tool_name,
        args: None,
        result,
        is_error: Some(is_error),
        status: if is_error {
            ToolStatus::Error
        } else {
            ToolStatus::Done
        },
    })
}

fn has_visible_assistant_content(blocks: &[serde_json::Value]) -> bool {
    blocks.iter().any(|b| {
        match b.get("type").and_then(|t| t.as_str()) {
            Some("text") => b
                .get("text")
                .and_then(|t| t.as_str())
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false),
            Some("thinking") => b
                .get("thinking")
                .and_then(|t| t.as_str())
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false),
            Some("image") => true,
            _ => false,
        }
    })
}

fn parse_timestamp(value: &str) -> Option<i64> {
    value.parse::<i64>().ok()
}
