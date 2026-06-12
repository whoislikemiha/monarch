use parking_lot::Mutex as PlMutex;
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::broadcast;

use crate::agent::WsBroadcast;
use crate::db::{Database, MessageRow, RecordObjectiveEventPayload, SetPlanPayload};
use crate::error::MonarchError;
use crate::memory::index::MemoryIndex;
use crate::persistence::write_attachment_bytes;
use crate::sidecar_protocol::InnerEvent;
use crate::util::chrono_now;

use super::util::{inner_event_tag, is_narration_tool};
use super::{PendingAttachment, PersistCommand, PersistContext};

/// MON-75: split a user-message `content` Value into (text-only content,
/// list of image attachments to persist). Handles both wire shapes the
/// sidecar forwards:
///   - array of blocks: `[{type:"text",text}, {type:"image",data,mimeType}]`
///   - single string (no images possible)
///   - anything else falls through unchanged with an empty attachment list.
/// The returned content has image blocks removed; if the resulting array
/// is empty the content falls back to an empty string so downstream code
/// that expects a content column never sees null.
pub(super) fn extract_image_attachments(
    content: Option<serde_json::Value>,
) -> (Option<serde_json::Value>, Vec<PendingAttachment>) {
    let Some(value) = content else {
        return (None, Vec::new());
    };
    let serde_json::Value::Array(blocks) = value else {
        return (Some(value), Vec::new());
    };

    let mut attachments = Vec::new();
    let mut kept = Vec::with_capacity(blocks.len());
    for block in blocks {
        if block
            .get("type")
            .and_then(|t| t.as_str())
            .map(|t| t == "image")
            .unwrap_or(false)
        {
            let data = block.get("data").and_then(|d| d.as_str()).unwrap_or("");
            let mime = block
                .get("mimeType")
                .and_then(|m| m.as_str())
                .unwrap_or("image/png");
            if !data.is_empty() {
                attachments.push(PendingAttachment {
                    data_base64: data.to_string(),
                    mime_type: mime.to_string(),
                });
                // Do not keep the image block; the bytes move to disk.
                continue;
            }
        }
        kept.push(block);
    }

    (Some(serde_json::Value::Array(kept)), attachments)
}

/// MON-124: auto-harvested narration. When an assistant message contains
/// process-talk text followed by real (non-meta) tool calls, that text IS
/// the narration — the agent saying "now I'll check the auth handler" before
/// acting, exactly how a person follows an agent's work. Returns the intent
/// headline, or `None` when the message has no world-mutating/reading tool
/// calls, no text, or narrates explicitly (`set_current_action` /
/// `complete_action` win over the harvest).
pub(crate) fn harvest_narration_intent(content: Option<&serde_json::Value>) -> Option<String> {
    let blocks = content?.as_array()?;
    let mut has_world_tool = false;
    let mut last_text: Option<&str> = None;
    for block in blocks {
        match block.get("type").and_then(|t| t.as_str()) {
            Some("text") => {
                if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                    if !t.trim().is_empty() {
                        last_text = Some(t);
                    }
                }
            }
            Some("toolCall") => {
                let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("");
                if matches!(name, "set_current_action" | "complete_action") {
                    return None;
                }
                if !is_narration_tool(name) {
                    has_world_tool = true;
                }
            }
            _ => {}
        }
    }
    if !has_world_tool {
        return None;
    }
    // The last non-empty line of the trailing text block sits closest to the
    // tool calls it introduces — that's the headline candidate. Questions are
    // dialogue, not narration (surface routing: question → chat), so a line
    // ending in "?" never becomes a headline; better no harvest than a chat
    // sentence on the timeline.
    let line = last_text?
        .lines()
        .rev()
        .map(|l| l.trim().trim_start_matches(['-', '*', '#', '>']).trim())
        .find(|l| !l.is_empty() && !l.ends_with('?'))?;
    // A headline is one sentence, not a paragraph.
    let sentence = match line.find(". ") {
        Some(i) => &line[..i + 1],
        None => line,
    };
    let sentence = sentence.trim_end_matches(':').trim();
    if sentence.is_empty() {
        return None;
    }
    Some(if sentence.chars().count() <= 120 {
        sentence.to_string()
    } else {
        format!("{}…", sentence.chars().take(119).collect::<String>())
    })
}

/// MON-71: wall-clock durations pre-computed against the live state snapshot
/// from before the event was applied. `turn_duration_ms` is the finalized
/// turn duration at `MessageEnd`; `tool_duration_ms` is the finalized tool
/// duration at `ToolExecutionEnd`. Both are `None` for any other event.
#[derive(Default, Clone, Copy)]
pub(crate) struct EventDurations {
    pub turn_duration_ms: Option<i64>,
    pub tool_duration_ms: Option<i64>,
}

/// Build zero-to-two `PersistCommand`s for one inbound sidecar event.
/// Always produces a `LogEvent`; additionally produces a save-message
/// command for `message_end` / `tool_execution_end` when a session id is
/// known. Session id is resolved on the producer side, so the command
/// carries its own `Option<String>` — ordering guarantees would be
/// meaningless if the consumer re-resolved after a later mutation.
///
/// `inner_raw` is the raw `Value` payload of the inner event envelope,
/// used for byte-fidelity storage in `LogEvent.data`. Taking the raw
/// alongside the typed `InnerEvent` sidesteps the need for `Serialize`
/// on `InnerEvent::Unknown { raw }` (which would be a custom impl) and
/// preserves exact wire bytes for debugging.
/// MON-71: wall-clock durations pre-computed against the live state snapshot
/// from before the event was applied. `turn_duration_ms` is the finalized
/// turn duration at `MessageEnd`; `tool_duration_ms` is the finalized tool
/// duration at `ToolExecutionEnd`. Both are `None` for any other event.
pub(crate) fn build_persist_commands(
    agent_id: &str,
    session_id: Option<String>,
    event: &InnerEvent,
    inner_raw: Option<&serde_json::Value>,
    durations: EventDurations,
    current_objective_id: Option<String>,
) -> Vec<PersistCommand> {
    let mut cmds: Vec<PersistCommand> = Vec::with_capacity(2);

    let event_type = inner_event_tag(event).to_string();
    let data = inner_raw.and_then(|v| serde_json::to_string(v).ok());
    cmds.push(PersistCommand::LogEvent {
        agent_id: agent_id.to_string(),
        session_id: session_id.clone(),
        event_type,
        data,
    });

    let Some(session_id) = session_id else {
        return cmds;
    };

    match event {
        InnerEvent::MessageEnd {
            message,
            classification_id,
        } => {
            let role = if message.role.is_empty() {
                "unknown".to_string()
            } else {
                message.role.clone()
            };
            // Pi also emits `message_end` for toolResult messages, but the
            // canonical toolResult row (with toolCallId/toolName) is written
            // by the ToolExecutionEnd arm below. Persisting this one too
            // created a duplicate, id-less row whose replay sent an empty
            // `call_id` to the Codex Responses API (400 invalid_request).
            if role == "toolResult" {
                return cmds;
            }
            // MON-75: for user messages, pull any inline base64 image
            // blocks out of the content value before serialization. The
            // stored content stays text-only; image bytes are written to
            // disk and linked via `message_attachments` by the consumer.
            let (content_value, attachments): (Option<serde_json::Value>, Vec<PendingAttachment>) =
                if role == "user" {
                    extract_image_attachments(message.content.clone())
                } else {
                    (message.content.clone(), Vec::new())
                };
            let content = content_value
                .as_ref()
                .map(|c| serde_json::to_string(c).unwrap_or_default())
                .unwrap_or_default();
            let model = message.model.clone();
            let (tokens, cost) = match &message.usage {
                Some(u) => (u.total_tokens as i32, u.cost.total),
                None => (0, 0.0),
            };

            // MON-82: if this is the user turn and the sidecar paired a
            // classification with it, attach the id so the apply body can
            // backfill `classifications.message_id` right after the insert
            // (no deferred UPDATE, no race with the pipeline).
            let pending_classification_id = if role == "user" {
                classification_id.clone()
            } else {
                None
            };

            // MON-124: harvest the trailing process sentence of a working
            // assistant turn as the current action's intent, so the timeline
            // narrates even when the model never calls set_current_action.
            // The objective was resolved upstream (current → auto-create →
            // scratch) by `current_objective_for_event`.
            if role == "assistant" {
                if let (Some(intent), Some(objective_id)) = (
                    harvest_narration_intent(content_value.as_ref()),
                    current_objective_id.clone(),
                ) {
                    cmds.push(PersistCommand::ActionTransition {
                        agent_id: agent_id.to_string(),
                        objective_id,
                        intent,
                        previous_outcome: None,
                    });
                }
            }

            cmds.push(PersistCommand::SaveAssistantMessage {
                agent_id: agent_id.to_string(),
                message: MessageRow {
                    id: 0,
                    session_id,
                    role,
                    content,
                    model,
                    tokens,
                    cost,
                    timestamp: chrono_now(),
                    duration_ms: durations.turn_duration_ms,
                    attachments: Vec::new(),
                },
                attachments,
                pending_classification_id,
            });

            // MON-63: increment per-agent lifetime stats
            let (input_tokens, output_tokens, msg_cost) = match &message.usage {
                Some(u) => (u.input, u.output, u.cost.total),
                None => (0, 0, 0.0),
            };
            cmds.push(PersistCommand::IncrementAgentStats {
                agent_id: agent_id.to_string(),
                input_tokens,
                output_tokens,
                cost: msg_cost,
            });
        }
        InnerEvent::ToolExecutionEnd {
            tool_call_id,
            tool_name,
            result,
            is_error,
        } => {
            // Preserve pre-MON-32 storage shape: `toolName` defaulted to
            // "unknown" when the sidecar didn't include it, and the stored
            // `result` field was the *stringified* value, not the raw
            // JSON value — the outer serde_json::json! call wrapped an
            // already-stringified payload. Keep that byte-for-byte to
            // avoid breaking historical row parsing.
            let tool_name = tool_name.clone().unwrap_or_else(|| "unknown".to_string());
            let result_str = result
                .as_ref()
                .map(|r| serde_json::to_string(r).unwrap_or_default())
                .unwrap_or_default();

            // MON-71: embed tool duration inside the toolResult JSON blob so
            // it survives the round trip through SQLite. The recovery path
            // in `parse_stored_tool_result` reads it back as `durationMs`.
            let mut content_obj = serde_json::json!({
                "toolCallId": tool_call_id,
                "toolName": tool_name,
                "result": result_str,
                "isError": *is_error,
            });
            if let (Some(d), Some(obj)) = (durations.tool_duration_ms, content_obj.as_object_mut())
            {
                obj.insert("durationMs".to_string(), serde_json::json!(d));
            }
            let content = content_obj.to_string();

            cmds.push(PersistCommand::SaveToolResult {
                agent_id: agent_id.to_string(),
                message: MessageRow {
                    id: 0,
                    session_id,
                    role: "toolResult".to_string(),
                    content,
                    model: None,
                    tokens: 0,
                    cost: 0.0,
                    timestamp: chrono_now(),
                    duration_ms: None,
                    attachments: Vec::new(),
                },
            });

            // MON-63: record tool usage for specialization tracking
            cmds.push(PersistCommand::RecordToolUsage {
                agent_id: agent_id.to_string(),
                tool_name: tool_name.clone(),
                is_error: *is_error,
            });
            if !is_narration_tool(&tool_name) {
                cmds.push(PersistCommand::ToolCallEnd {
                    agent_id: agent_id.to_string(),
                    tool_call_id: tool_call_id.clone(),
                    result: result.clone(),
                    is_error: *is_error,
                    duration_ms: durations.tool_duration_ms,
                });
            }
        }
        InnerEvent::ToolExecutionStart {
            tool_call_id,
            tool_name,
            args,
        } => {
            if let Some(objective_id) = current_objective_id {
                if !is_narration_tool(tool_name) {
                    cmds.push(PersistCommand::ToolCallStart {
                        agent_id: agent_id.to_string(),
                        objective_id,
                        tool_call_id: tool_call_id.clone(),
                        tool_name: tool_name.clone(),
                        args: args.clone(),
                    });
                }
            }
        }
        InnerEvent::ActionTransition {
            intent,
            previous_outcome,
        } => {
            if let Some(objective_id) = current_objective_id {
                cmds.push(PersistCommand::ActionTransition {
                    agent_id: agent_id.to_string(),
                    objective_id,
                    intent: intent.clone(),
                    previous_outcome: previous_outcome.clone(),
                });
            }
        }
        InnerEvent::ActionComplete { outcome } => {
            cmds.push(PersistCommand::ActionComplete {
                agent_id: agent_id.to_string(),
                outcome: outcome.clone(),
            });
        }
        InnerEvent::ExecutorDecision {
            decision,
            rationale,
        } => {
            if let Some(objective_id) = current_objective_id {
                cmds.push(PersistCommand::ExecutorDecision {
                    agent_id: agent_id.to_string(),
                    objective_id,
                    decision: decision.clone(),
                    rationale: rationale.clone(),
                });
            }
        }
        InnerEvent::PlanSet { items, rationale } => {
            if let Some(objective_id) = current_objective_id {
                cmds.push(PersistCommand::PlanSet {
                    agent_id: agent_id.to_string(),
                    payload: SetPlanPayload {
                        objective_id,
                        items: items.clone(),
                        created_by: Some("executor".to_string()),
                        rationale: rationale.clone(),
                    },
                });
            }
        }
        InnerEvent::PlanItemStart { item_id } => {
            cmds.push(PersistCommand::PlanItemStart {
                agent_id: agent_id.to_string(),
                item_id: item_id.clone(),
            });
        }
        InnerEvent::PlanItemComplete { outcome } => {
            cmds.push(PersistCommand::PlanItemComplete {
                agent_id: agent_id.to_string(),
                outcome: outcome.clone(),
            });
        }
        InnerEvent::PlanItemSkip { item_id, reason } => {
            cmds.push(PersistCommand::PlanItemSkip {
                agent_id: agent_id.to_string(),
                item_id: item_id.clone(),
                reason: reason.clone(),
            });
        }
        InnerEvent::PlanItemBlock { item_id, reason } => {
            cmds.push(PersistCommand::PlanItemBlock {
                agent_id: agent_id.to_string(),
                item_id: item_id.clone(),
                reason: reason.clone(),
            });
        }
        InnerEvent::TurnEnd => {
            // MON-63: increment per-agent turn counter
            cmds.push(PersistCommand::IncrementAgentTurns {
                agent_id: agent_id.to_string(),
            });
        }
        InnerEvent::MemorySuggestion {
            title,
            summary,
            content,
        } => {
            if let Some(objective_id) = current_objective_id {
                let payload_json = serde_json::json!({
                    "title": title,
                    "summary": summary,
                    "content": content,
                })
                .to_string();
                cmds.push(PersistCommand::RecordObjectiveEvent {
                    payload: RecordObjectiveEventPayload {
                        objective_id,
                        event_type: "memory_suggestion".to_string(),
                        actor: Some(agent_id.to_string()),
                        payload_json: Some(payload_json),
                        ..Default::default()
                    },
                });
            } else {
                eprintln!(
                    "[monarch] dropping memory_suggestion for {} with no current objective",
                    agent_id
                );
            }
        }
        InnerEvent::ObjectiveReport { report } => {
            if let Some(objective_id) = current_objective_id {
                cmds.push(PersistCommand::CompleteObjective {
                    agent_id: agent_id.to_string(),
                    objective_id,
                    report: report.clone(),
                });
            } else {
                eprintln!(
                    "[monarch] dropping objective_report for {} with no current objective",
                    agent_id
                );
            }
        }
        _ => {}
    }

    cmds
}

// ---- apply arms: message / attachment / classification / stats ----

pub(super) async fn apply_log_event(
    db: &Database,
    agent_id: String,
    session_id: Option<String>,
    event_type: String,
    data: Option<String>,
) -> Result<(), MonarchError> {
    db.log_event_internal(
        Some(&agent_id),
        session_id.as_deref(),
        &event_type,
        data.as_deref(),
    )
    .await
}

pub(super) async fn apply_save_assistant_message(
    db: &Database,
    _app: &Arc<PlMutex<Option<AppHandle>>>,
    _ws_tx: &broadcast::Sender<WsBroadcast>,
    ctx: &mut PersistContext,
    message: MessageRow,
    attachments: Vec<PendingAttachment>,
    pending_classification_id: Option<String>,
) -> Result<(), MonarchError> {
    let session_id = message.session_id.clone();
    let tokens = message.tokens;
    let cost = message.cost;
    let message_id = db.save_message_internal(&message).await?;
    // MON-82: pair the just-saved user row with its paired
    // classification. Two orderings are possible:
    //
    // - Classifier beat us here (rare — Haiku is usually ~1.5 s,
    //   Pi's user `message_end` is instant): row already exists,
    //   backfill its `message_id` now.
    // - Classifier still pending (common): stash the mapping on
    //   the consumer context; `SaveClassification`'s apply picks
    //   it up when the row finally lands.
    //
    // Both paths are best-effort — a failure here shouldn't drop
    // the message; it's a display/analytics tag, not core data.
    if let Some(cid) = pending_classification_id {
        match db
            .backfill_classification_message_id(&cid, message_id)
            .await
        {
            Ok(rows) if rows == 0 => {
                ctx.pending_classification_links.insert(cid, message_id);
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("[monarch] classifier backfill failed for {}: {:?}", cid, e);
                ctx.pending_classification_links.insert(cid, message_id);
            }
        }
    }
    // MON-75: write image attachments to disk and link them
    // back to the row we just inserted. A failure here should
    // not roll back the message — the text still persists and
    // the chat stays legible; the user just sees a thumbnail
    // gap. So surface the error via `?` only after the
    // session-count update so stats stay consistent.
    let mut attach_err: Option<MonarchError> = None;
    for (position, att) in attachments.into_iter().enumerate() {
        match write_attachment_bytes(&att.data_base64, &att.mime_type).await {
            Ok(path) => {
                let path_str = path.to_string_lossy().to_string();
                if let Err(e) = db
                    .save_message_attachment_internal(
                        message_id,
                        &path_str,
                        &att.mime_type,
                        position as i64,
                    )
                    .await
                {
                    attach_err.get_or_insert(e);
                }
            }
            Err(e) => {
                attach_err.get_or_insert(e);
            }
        }
    }
    db.increment_session_message_count(&session_id, tokens, cost)
        .await?;
    match attach_err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

pub(super) async fn apply_save_classification(
    db: &Database,
    ctx: &mut PersistContext,
    mut payload: crate::db::SaveClassificationPayload,
) -> Result<(), MonarchError> {
    // MON-82: if `SaveAssistantMessage` already stashed a
    // message_id for this classification (the common case —
    // classifier returns ~1.5 s after the user row saved),
    // take it so the INSERT lands with a valid FK and no
    // follow-up UPDATE is needed.
    let linked = ctx.pending_classification_links.remove(&payload.id);
    db.save_classification_internal(&payload).await?;
    if let Some(mid) = linked {
        db.backfill_classification_message_id(&payload.id, mid)
            .await?;
    }
    // Suppress unused-mut warning when `linked` is None.
    let _ = &mut payload;
    Ok(())
}

pub(super) async fn apply_insert_memory(
    db: &Database,
    memory_index: &Arc<MemoryIndex>,
    payload: crate::db::InsertMemoryPayload,
) -> Result<(), MonarchError> {
    // MON-100: embed the summary before insert. If the embedder
    // is not initialised (captain hasn't downloaded the model)
    // we still write the row — FTS5 search keeps working off
    // title+summary+content; only the HNSW vector path is
    // skipped until the next rebuild after init.
    let (embedding, embedding_model_id) = if memory_index.is_initialized() {
        match memory_index.embed_to_blob(&payload.summary).await {
            Ok(blob) => (
                Some(blob),
                Some(crate::memory::config::DEFAULT_EMBEDDING_MODEL_ID.to_string()),
            ),
            Err(e) => {
                eprintln!("[monarch] keeper: embed failed: {}", e);
                (None, None)
            }
        }
    } else {
        (None, None)
    };
    db.insert_memory_internal(payload, embedding, embedding_model_id)
        .await
        .map(|_| ())
}

pub(super) async fn apply_rebuild_hnsw(
    db: &Database,
    memory_index: &Arc<MemoryIndex>,
    agent_id: String,
) -> Result<(), MonarchError> {
    // P2 ships full-rebuild — instant-distance is fast enough for
    // P2 volumes (<10k memories per agent). MON-97 (P3d) replaces
    // this with incremental insert.
    let data = db.load_embeddings_for_agent_internal(&agent_id).await?;
    memory_index.rebuild(data).await
}
