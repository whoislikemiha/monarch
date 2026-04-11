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
#[serde(rename_all = "camelCase", default)]
pub struct Cost {
    pub input: f64,
    pub output: f64,
    pub total: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase", default)]
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
    pub(crate) fn commit_streaming_message(&mut self) {
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

/// Parse a canonical RFC3339 UTC timestamp into Unix seconds. MON-39
/// item 4 unified all timestamp columns on the `%Y-%m-%dT%H:%M:%SZ`
/// format — both Rust writers (`chrono_now`) and SQLite DEFAULTs
/// (`strftime('%Y-%m-%dT%H:%M:%SZ','now')`) emit this shape. Returns
/// `None` on a format we can't parse so the caller can fall through to
/// `timestamp: None` instead of panicking.
fn parse_timestamp(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.timestamp())
}
