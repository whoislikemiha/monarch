//! MON-37: single-consumer persistence pipeline.
//!
//! Before this change, each inbound sidecar event fanned out via a dropped
//! `spawn_blocking` JoinHandle. The default blocking pool has up to 512
//! workers, so under a burst `message_end` could race ahead of an earlier
//! `tool_execution_end` for the same message and land in SQLite out of
//! order. Errors were also silently swallowed by `let _ = ...`.
//!
//! The fix: one bounded mpsc channel, one consumer task, one command at a
//! time. MON-27 replaced the `spawn_blocking` hop with a direct `.await` on
//! `Database`'s async methods (now backed by `tokio-rusqlite`) — ordering is
//! still restored because the loop awaits each command before pulling the
//! next, and errors still surface via `mark_agent_desynced`.

use dashmap::DashMap;
use parking_lot::Mutex as PlMutex;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::{broadcast, mpsc};

use crate::db::{Database, InsertMemoryPayload, MessageRow, RecordQuestEventPayload, SaveClassificationPayload};
use crate::error::MonarchError;
use crate::memory_index::MemoryIndex;
use crate::persistence::write_attachment_bytes;
use crate::sidecar_protocol::InnerEvent;
use crate::util::chrono_now;

use super::event_handler::{emit_event, mark_agent_desynced};
use super::manager::AgentStateEntry;
use super::WsBroadcast;

/// MON-75: raw image content extracted from a user `message_end`,
/// awaiting the parent message's DB id before it can be written to disk
/// and linked via `message_attachments`. Base64 is held in-memory until
/// the consumer applies the command — typical payloads are ~1 MB and
/// there is exactly one send in flight per agent.
#[derive(Debug, Clone)]
pub(super) struct PendingAttachment {
    pub data_base64: String,
    pub mime_type: String,
}

/// MON-82: per-consumer state that survives between `PersistCommand`s.
/// Required because the classifier round-trip (~1.5 s on Haiku) usually
/// lands AFTER Pi's user `message_end` in the pipeline, so the
/// `SaveAssistantMessage` apply stashes the pending (classification_id →
/// message_id) pair here and `SaveClassification`'s apply consumes it
/// when the classification row is finally inserted.
#[derive(Default)]
pub(super) struct PersistContext {
    pending_classification_links: HashMap<String, i64>,
}

/// A persistence effect to apply in FIFO order by the single consumer.
#[derive(Debug)]
pub(super) enum PersistCommand {
    /// Log one event row. Always emitted for every sidecar `event` arrival,
    /// matching the pre-MON-37 behaviour.
    LogEvent {
        agent_id: String,
        session_id: Option<String>,
        event_type: String,
        data: Option<String>,
    },
    /// Persist an assistant `message_end`. Applying this variant performs
    /// both the `save_message_internal` and the
    /// `increment_session_message_count` call in that order, so the stats
    /// update cannot race the insert.
    ///
    /// MON-75: user messages may also carry `attachments` — base64 image
    /// blobs the LLM was shown. They are written to disk and linked via
    /// `message_attachments` only after the parent row gets its id back
    /// from the insert, keeping the attachment → message FK valid.
    SaveAssistantMessage {
        agent_id: String,
        message: MessageRow,
        attachments: Vec<PendingAttachment>,
        /// MON-82: only set for user-role messages paired with an in-flight
        /// classification. Backfilled inline after the insert yields
        /// `message_id` so the FK lands in the same apply call.
        pending_classification_id: Option<String>,
    },
    /// Persist a `tool_execution_end` as a synthesized `toolResult` row.
    SaveToolResult {
        agent_id: String,
        message: MessageRow,
    },
    /// MON-63: increment per-agent lifetime token/cost/message stats.
    IncrementAgentStats {
        agent_id: String,
        input_tokens: i64,
        output_tokens: i64,
        cost: f64,
    },
    /// MON-63: increment per-agent turn counter.
    IncrementAgentTurns {
        agent_id: String,
    },
    /// MON-63: record a tool execution for per-agent tool usage tracking.
    RecordToolUsage {
        agent_id: String,
        tool_name: String,
        is_error: bool,
    },
    /// MON-82: persist a classifier result. Insert only — the row's
    /// `message_id` stays NULL and is filled in by the `SaveAssistantMessage`
    /// apply when the paired user message lands.
    SaveClassification {
        payload: SaveClassificationPayload,
    },
    /// MON-100: insert one Keeper-produced atomic claim. The consumer embeds
    /// `payload.summary` via `MemoryIndex::embed_to_blob` before the insert
    /// so the new row carries an embedding immediately. If the embedder is
    /// not initialised the row is still written (without an embedding) and
    /// the subsequent `RebuildHnsw` simply skips it — the captain can still
    /// see the memory in the Inspector and FTS5 retrieval still works.
    InsertMemory {
        agent_id: String,
        payload: InsertMemoryPayload,
    },
    /// MON-100: mark a `memory_keeper_runs` row complete with outcome +
    /// summary + token counts.
    CompleteKeeperRun {
        run_id: i64,
        outcome: String,
        output_summary: Option<String>,
        tokens_in: Option<i64>,
        tokens_out: Option<i64>,
    },
    /// MON-100 / MON-83: append one row to `quest_events` and broadcast on
    /// `quest-event-{questId}` so the QuestTimelineTool wakes. Wraps the
    /// existing `db.record_quest_event_internal` + `agent::emit_event` pair
    /// from the `db_record_quest_event` Tauri command.
    RecordQuestEvent {
        payload: RecordQuestEventPayload,
    },
    /// MON-100: full-rebuild the per-agent HNSW index from current DB
    /// embeddings. Runs last in a Keeper-tick burst so the index is
    /// consistent before the next read. P3d (MON-97) replaces this with
    /// incremental insert.
    RebuildHnsw {
        agent_id: String,
    },
}

impl PersistCommand {
    fn agent_id(&self) -> &str {
        match self {
            Self::LogEvent { agent_id, .. }
            | Self::SaveAssistantMessage { agent_id, .. }
            | Self::SaveToolResult { agent_id, .. }
            | Self::IncrementAgentStats { agent_id, .. }
            | Self::IncrementAgentTurns { agent_id, .. }
            | Self::RecordToolUsage { agent_id, .. }
            | Self::InsertMemory { agent_id, .. }
            | Self::RebuildHnsw { agent_id, .. } => agent_id,
            Self::SaveClassification { payload } => &payload.agent_id,
            // CompleteKeeperRun + RecordQuestEvent don't carry an agent id
            // directly — failures still log but cannot flip a per-agent
            // desync flag. Empty string causes the consumer's desync helper
            // to short-circuit (`if agent_id.is_empty()`).
            Self::CompleteKeeperRun { .. } | Self::RecordQuestEvent { .. } => "",
        }
    }

    async fn apply(
        self,
        db: &Database,
        memory_index: &Arc<MemoryIndex>,
        app: &Arc<PlMutex<Option<AppHandle>>>,
        ws_tx: &broadcast::Sender<WsBroadcast>,
        ctx: &mut PersistContext,
    ) -> Result<(), MonarchError> {
        match self {
            Self::LogEvent {
                agent_id,
                session_id,
                event_type,
                data,
                ..
            } => {
                db.log_event_internal(
                    Some(&agent_id),
                    session_id.as_deref(),
                    &event_type,
                    data.as_deref(),
                )
                .await
            }
            Self::SaveAssistantMessage {
                message,
                attachments,
                pending_classification_id,
                ..
            } => {
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
                            ctx.pending_classification_links
                                .insert(cid, message_id);
                        }
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!(
                                "[monarch] classifier backfill failed for {}: {:?}",
                                cid, e
                            );
                            ctx.pending_classification_links
                                .insert(cid, message_id);
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
            Self::SaveToolResult { message, .. } => {
                db.save_message_internal(&message).await.map(|_| ())
            }
            Self::IncrementAgentStats {
                agent_id,
                input_tokens,
                output_tokens,
                cost,
            } => {
                db.increment_agent_stats(&agent_id, input_tokens, output_tokens, cost)
                    .await
            }
            Self::IncrementAgentTurns { agent_id } => {
                db.increment_agent_turns(&agent_id).await
            }
            Self::RecordToolUsage {
                agent_id,
                tool_name,
                is_error,
            } => {
                db.record_tool_usage(&agent_id, &tool_name, is_error)
                    .await
            }
            Self::SaveClassification { mut payload } => {
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
            Self::InsertMemory { agent_id: _, payload } => {
                // MON-100: embed the summary before insert. If the embedder
                // is not initialised (captain hasn't downloaded the model)
                // we still write the row — FTS5 search keeps working off
                // title+summary+content; only the HNSW vector path is
                // skipped until the next rebuild after init.
                let (embedding, embedding_model_id) = if memory_index
                    .is_initialized()
                {
                    match memory_index.embed_to_blob(&payload.summary).await {
                        Ok(blob) => (
                            Some(blob),
                            Some(crate::memory_config::DEFAULT_EMBEDDING_MODEL_ID.to_string()),
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
            Self::CompleteKeeperRun {
                run_id,
                outcome,
                output_summary,
                tokens_in,
                tokens_out,
            } => {
                db.complete_keeper_run_internal(
                    run_id,
                    &outcome,
                    output_summary,
                    tokens_in,
                    tokens_out,
                )
                .await
            }
            Self::RecordQuestEvent { payload } => {
                let quest_id = payload.quest_id.clone();
                let event_type = payload.event_type.clone();
                let id = db.record_quest_event_internal(&payload).await?;
                // Mirrors the `db_record_quest_event` Tauri command's broadcast
                // so the QuestTimelineTool wakes regardless of how the event
                // was authored (UI button, Keeper tick, executor, …).
                let app_opt = app.lock().clone();
                if let Some(app) = app_opt {
                    emit_event(
                        &app,
                        ws_tx,
                        &format!("quest-event-{}", quest_id),
                        &serde_json::json!({ "id": id, "eventType": event_type }).to_string(),
                    );
                }
                Ok(())
            }
            Self::RebuildHnsw { agent_id } => {
                // P2 ships full-rebuild — instant-distance is fast enough for
                // P2 volumes (<10k memories per agent). MON-97 (P3d) replaces
                // this with incremental insert.
                let data = db
                    .load_embeddings_for_agent_internal(&agent_id)
                    .await?;
                memory_index.rebuild(data).await
            }
        }
    }
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
#[derive(Default, Clone, Copy)]
pub(super) struct EventDurations {
    pub turn_duration_ms: Option<i64>,
    pub tool_duration_ms: Option<i64>,
}

pub(super) fn build_persist_commands(
    agent_id: &str,
    session_id: Option<String>,
    event: &InnerEvent,
    inner_raw: Option<&serde_json::Value>,
    durations: EventDurations,
    current_quest_id: Option<String>,
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
            if let (Some(d), Some(obj)) = (durations.tool_duration_ms, content_obj.as_object_mut()) {
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
                tool_name,
                is_error: *is_error,
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
            if let Some(quest_id) = current_quest_id {
                let payload_json = serde_json::json!({
                    "title": title,
                    "summary": summary,
                    "content": content,
                })
                .to_string();
                cmds.push(PersistCommand::RecordQuestEvent {
                    payload: RecordQuestEventPayload {
                        quest_id,
                        event_type: "memory_suggestion".to_string(),
                        actor: Some(agent_id.to_string()),
                        payload_json: Some(payload_json),
                    },
                });
            } else {
                eprintln!(
                    "[monarch] dropping memory_suggestion for {} with no current quest",
                    agent_id
                );
            }
        }
        _ => {}
    }

    cmds
}

/// MON-75: split a user-message `content` Value into (text-only content,
/// list of image attachments to persist). Handles both wire shapes the
/// sidecar forwards:
///   - array of blocks: `[{type:"text",text}, {type:"image",data,mimeType}]`
///   - single string (no images possible)
///   - anything else falls through unchanged with an empty attachment list.
/// The returned content has image blocks removed; if the resulting array
/// is empty the content falls back to an empty string so downstream code
/// that expects a content column never sees null.
fn extract_image_attachments(
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

/// Stable snake_case tag for an `InnerEvent`, used by `LogEvent.event_type`
/// so the persisted shape matches the pre-MON-32 string dispatch.
fn inner_event_tag(event: &InnerEvent) -> &'static str {
    match event {
        InnerEvent::AgentStart => "agent_start",
        InnerEvent::AgentEnd => "agent_end",
        InnerEvent::TurnStart => "turn_start",
        InnerEvent::TurnEnd => "turn_end",
        InnerEvent::MessageStart { .. } => "message_start",
        InnerEvent::MessageUpdate { .. } => "message_update",
        InnerEvent::MessageEnd { .. } => "message_end",
        InnerEvent::ToolExecutionStart { .. } => "tool_execution_start",
        InnerEvent::ToolExecutionEnd { .. } => "tool_execution_end",
        InnerEvent::MemorySuggestion { .. } => "memory_suggestion",
        InnerEvent::CompactionStart { .. } => "compaction_start",
        InnerEvent::CompactionEnd { .. } => "compaction_end",
        InnerEvent::AutoRetryStart { .. } => "auto_retry_start",
        InnerEvent::AutoRetryEnd => "auto_retry_end",
        InnerEvent::QueueUpdate => "queue_update",
        InnerEvent::ToolExecutionUpdate => "tool_execution_update",
        InnerEvent::Unknown { .. } => "unknown",
    }
}

/// MON-37: the single-consumer persistence task. Drains the bounded mpsc
/// in FIFO order and applies each command inside `spawn_blocking` (await-ed
/// so one command finishes before the next starts — that is what restores
/// ordering). Errors are logged and flip `mark_agent_desynced` so the
/// dev-only indicator surfaces DB problems the same way it surfaces
/// parser failures. The loop never panics on error; it keeps draining.
///
/// Pure-async, Tauri-free at the type level: `AppHandle` is reached via
/// the `Arc<Mutex<Option<_>>>` slot and is `None` until Tauri setup wires
/// it, so failures that happen before setup simply skip the desync emit.
/// This shape lets a future `#[cfg(test)]` harness drive the loop with a
/// stub receiver once Rust tests run on Windows.
pub(super) async fn run_persist_consumer(
    mut rx: mpsc::Receiver<PersistCommand>,
    db: Arc<Database>,
    memory_index: Arc<MemoryIndex>,
    live_states: Arc<DashMap<String, Arc<AgentStateEntry>>>,
    ws_tx: broadcast::Sender<WsBroadcast>,
    app_handle: Arc<PlMutex<Option<AppHandle>>>,
) {
    let mut ctx = PersistContext::default();
    while let Some(cmd) = rx.recv().await {
        let agent_id = cmd.agent_id().to_string();
        let err: String = match cmd
            .apply(&db, &memory_index, &app_handle, &ws_tx, &mut ctx)
            .await
        {
            Ok(()) => continue,
            Err(e) => e.to_string(),
        };
        eprintln!("[monarch] persist failed: {}", err);

        if agent_id.is_empty() {
            continue;
        }
        let app_opt = app_handle.lock().clone();
        if let Some(app) = app_opt {
            mark_agent_desynced(&app, &ws_tx, &live_states, &agent_id).await;
        }
    }
    eprintln!("[monarch] persist consumer exited");
}
