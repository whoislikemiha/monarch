//! Inbound sidecar event dispatch: parse JSONL, fan out to persistence
//! and `LiveAgentState` assembly, emit snapshots.
//!
//! The reader task in `sidecar.rs` calls `handle_sidecar_event` once per
//! line from the sidecar stdout. All fan-out decisions (which channel,
//! debounce vs emit-now, desync flag) live here so the process layer and
//! the manager don't need to know about protocol shapes.

use dashmap::DashMap;
use parking_lot::Mutex as PlMutex;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::{broadcast, mpsc};

use crate::agent_state::{ApplyOutcome, DisplayItem, LiveAgentState};
use crate::db::{Database, InsertMemoryPayload, RecordQuestEventPayload};
use crate::memory_index::MemoryIndex;
use crate::sidecar_protocol::{apply_event, AtomicClaim, InnerEvent, SidecarCommand, SidecarEvent};

use super::manager::{AgentManagerInner, AgentStateEntry, InternalDispatch};
use super::persist::{build_persist_commands, EventDurations, PersistCommand};
use super::{WsBroadcast, DEBOUNCE_MILLIS};

/// Look up the session_id for an agent from the consolidated inner state.
/// MON-34: reads the map through the single `parking_lot::Mutex` shared
/// with `AgentManager`. Infallible — `parking_lot` doesn't poison — so the
/// call site no longer has to branch on lock error.
fn get_session_id(inner: &Arc<PlMutex<AgentManagerInner>>, agent_id: &str) -> Option<String> {
    inner.lock().session_map.get(agent_id).cloned()
}

/// Emit an event to both Tauri webview and WebSocket clients.
/// MON-83: promoted from `pub(super)` to `pub(crate)` so non-agent
/// command surfaces (e.g. quest CRUD in `db.rs`) can broadcast their own
/// event channels without rebuilding the dual-emit plumbing.
pub(crate) fn emit_event(
    app: &AppHandle,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    event_name: &str,
    payload: &str,
) {
    let _ = app.emit(event_name, payload.to_string());
    let _ = ws_tx.send(WsBroadcast {
        event: event_name.to_string(),
        payload: payload.to_string(),
    });
}

/// Emit an assembled `LiveAgentState` snapshot on the `agent-state-{id}`
/// channel. The Tauri path passes the value directly so `Emitter::emit`
/// serializes it exactly once — subscribers receive a JSON object rather than
/// a JSON-encoded string wrapped in another JSON string. The WebSocket path
/// keeps the `{event, payload: String}` envelope convention shared with the
/// other broadcast event types.
///
/// MON-38 invariant: callers must not hold an `AgentStateEntry` write guard
/// when invoking this helper. Both emit paths serialize `state` internally,
/// and for a long chat `serde_json::to_string` is O(history); running it
/// under the write lock would make the async sidecar reader O(history)-bound
/// per event. The enforced shape at every call site is
/// `let snap = guard.state.clone(); drop(guard); emit_state_event(.., &snap);`.
pub(super) fn emit_state_event(
    app: &AppHandle,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    event_name: &str,
    state: &LiveAgentState,
) {
    let _ = app.emit(event_name, state);
    if let Ok(payload) = serde_json::to_string(state) {
        let _ = ws_tx.send(WsBroadcast {
            event: event_name.to_string(),
            payload,
        });
    }
}

/// Handle a single JSONL event from the sidecar.
///
/// MON-14 Phase 1: this is async, owns the per-agent `LiveAgentState`
/// mutation, and emits assembled snapshots on `agent-state-{id}`.
///
/// `agent-event-{id}` is narrowed to out-of-band signals only:
/// `session_ready`, `extension_ui_request`, and sidecar errors. Message and
/// tool events flow exclusively through the assembled `agent-state-{id}`
/// channel — MON-39 removed the Phase-1 dual emission once the frontend
/// `liveAgentStore` took over assembly.
pub(super) async fn handle_sidecar_event(
    app: &AppHandle,
    persist_tx: &mpsc::Sender<PersistCommand>,
    dispatch_tx: &mpsc::Sender<InternalDispatch>,
    db: &Arc<Database>,
    memory_index: &Arc<MemoryIndex>,
    inner: &Arc<PlMutex<AgentManagerInner>>,
    live_states: &Arc<DashMap<String, Arc<AgentStateEntry>>>,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    line: &str,
) {
    // MON-32: parse twice — once as raw Value (for byte-fidelity LogEvent
    // storage of the inner event) and once as typed SidecarEvent for
    // dispatch. The Value clone is O(line-size), trivial for a JSONL line.
    // Typed-parse failures flow through mark_agent_desynced the same way
    // the pre-MON-32 malformed-envelope branch did.
    let raw_value: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            eprintln!(
                "[monarch] Failed to parse sidecar event: {} — line: {}",
                e, line
            );
            return;
        }
    };
    let typed_event: SidecarEvent = match serde_json::from_value(raw_value.clone()) {
        Ok(ev) => ev,
        Err(e) => {
            eprintln!(
                "[monarch] Failed to decode sidecar event: {} — line: {}",
                e, line
            );
            if let Some(agent_id) = raw_value.get("agentId").and_then(|a| a.as_str()) {
                if !agent_id.is_empty() {
                    mark_agent_desynced(app, ws_tx, live_states, agent_id).await;
                }
            }
            return;
        }
    };

    match typed_event {
        SidecarEvent::SessionReady {
            agent_id,
            context_window,
        } => {
            let event_name = format!("agent-event-{}", agent_id);
            let ready_event = serde_json::json!({
                "type": "session_ready",
                "agentId": agent_id,
                "contextWindow": context_window,
            });
            emit_event(app, ws_tx, &event_name, &ready_event.to_string());
        }

        SidecarEvent::SessionDestroyed { agent_id } => {
            let exit_event = format!("agent-exit-{}", agent_id);
            emit_event(
                app,
                ws_tx,
                &exit_event,
                &serde_json::json!(null).to_string(),
            );
            // Clear the live state for this agent so a fresh session starts clean.
            if let Some(entry) = live_states.get(&agent_id).map(|e| e.clone()) {
                // MON-30: bump before acquiring the write lock. If a debounce
                // task is already queued on the lock, it will observe the new
                // generation after handoff and bail; if it's still in its
                // sleep window, the later arrival will see the bump and bail
                // as well. Either way, only the reset snapshot is emitted.
                entry.cancel_generation.fetch_add(1, Ordering::Release);
                let mut guard = entry.inner.write().await;
                if let Some(h) = guard.debounce_handle.take() {
                    h.abort();
                }
                guard.dirty = false;
                guard.state = LiveAgentState::default();
                guard.state.state_version = guard.state.state_version.saturating_add(1);
                let snapshot = guard.state.clone();
                drop(guard);
                emit_state_event(app, ws_tx, &entry.topic, &snapshot);
            }
        }

        SidecarEvent::Event {
            agent_id,
            event: inner_event,
        } => {
            // Flip desync and stop here if the sidecar shipped an event type
            // the Rust side doesn't recognize. Pre-MON-32 this fell through
            // the `apply_event` catch-all; keeping it explicit here lets
            // `build_persist_commands` skip the typed match on Unknown and
            // gives us a place to log the raw payload for forensics.
            if let InnerEvent::Unknown { raw } = &inner_event {
                eprintln!(
                    "[monarch] Unknown sidecar inner event for {}: {}",
                    agent_id, raw
                );
                if !agent_id.is_empty() {
                    mark_agent_desynced(app, ws_tx, live_states, &agent_id).await;
                }
                return;
            }

            // MON-37: enqueue persistence work on the single-consumer mpsc
            // pipeline. Session id is resolved on the producer side so the
            // command carries its own `Option<String>` and ordering holds
            // even if the session map mutates between enqueue and apply.
            // `send().await` intentionally back-pressures the reader if the
            // consumer is lagging — that is the point of a bounded channel.
            let session_id = get_session_id(inner, &agent_id);
            let inner_raw = raw_value.get("event");
            // MON-71: pre-apply peek at the live state for finalized durations
            // persisted alongside assistant/tool-result rows. MessageEnd uses
            // `turn_started_at_ms`; ToolExecutionEnd uses the tool's stamped
            // `started_at_ms`. Reading before apply is load-bearing because
            // apply clears the turn anchor and writes the duration onto the
            // ToolExecution — peeking after would see the mutation.
            let durations = compute_event_durations(live_states, &agent_id, &inner_event).await;
            let current_quest_id =
                current_quest_for_event(app, ws_tx, db, &agent_id, &inner_event).await;
            for cmd in build_persist_commands(
                &agent_id,
                session_id,
                &inner_event,
                inner_raw,
                durations,
                current_quest_id,
            ) {
                if persist_tx.send(cmd).await.is_err() {
                    eprintln!("[monarch] persist consumer closed, dropping event");
                    break;
                }
            }

            // Apply the event to per-agent LiveAgentState and decide whether
            // to emit a snapshot now, debounce it, or skip.
            apply_and_maybe_emit(app, ws_tx, live_states, &agent_id, &inner_event).await;

            // MON-100: continuous-compaction trigger checks. Run after the
            // event applied (so `tokens_since_last_compaction` reflects the
            // post-event sum) and only at "natural" boundaries — TurnEnd
            // for the soft threshold, MessageEnd for the hard threshold.
            // The dispatcher consumes via `Arc<AgentManager>` since
            // Keeper dispatch needs the sidecar pipe + db.
            maybe_trigger_keeper(dispatch_tx, live_states, &agent_id, &inner_event).await;
        }

        SidecarEvent::ExtensionUiRequest { agent_id } => {
            let event_name = format!("agent-event-{}", agent_id);
            emit_event(app, ws_tx, &event_name, line);
        }

        SidecarEvent::Error { agent_id, error } => {
            eprintln!("[monarch] Sidecar error for {}: {}", agent_id, error);
            let event_name = format!("agent-event-{}", agent_id);
            let error_event = serde_json::json!({
                "type": "sidecar_error",
                "error": error,
            });
            emit_event(app, ws_tx, &event_name, &error_event.to_string());
        }

        // MON-82: classifier output for one user turn. Persist-via-pipeline
        // (so ordering with the user `MessageEnd` stays consistent) and
        // rebroadcast to the frontend on a dedicated channel so the pill
        // can resolve from `pending` to `ok`/`failed`.
        SidecarEvent::Classification {
            agent_id,
            id,
            complexity,
            confidence,
            rationale,
            model,
            tokens_in,
            tokens_out,
            latency_ms,
            error,
        } => {
            let payload = crate::db::SaveClassificationPayload {
                id: id.clone(),
                agent_id: agent_id.clone(),
                session_id: None,
                complexity: complexity.clone(),
                confidence,
                rationale: rationale.clone(),
                model: model.clone(),
                tokens_in,
                tokens_out,
                latency_ms,
                error: error.clone(),
            };
            if persist_tx
                .send(PersistCommand::SaveClassification { payload })
                .await
                .is_err()
            {
                eprintln!("[monarch] persist consumer closed, dropping classification");
            }
            let event_name = format!("agent-classification-{}", agent_id);
            let out = serde_json::json!({
                "id": id,
                "agentId": agent_id,
                "complexity": complexity,
                "confidence": confidence,
                "rationale": rationale,
                "model": model,
                "tokensIn": tokens_in,
                "tokensOut": tokens_out,
                "latencyMs": latency_ms,
                "error": error,
            });
            emit_event(app, ws_tx, &event_name, &out.to_string());
        }

        SidecarEvent::KeeperResult {
            agent_id,
            run_id,
            claims,
            compaction_summary,
            model,
            tokens_in,
            tokens_out,
            latency_ms,
            error,
        } => {
            // Surface model + latency at stderr; tokens land on the run row.
            // Pre-emptive observability for calibrating thresholds and
            // catching regressions while Slice B is bedding in.
            if let Some(err) = error.as_ref() {
                eprintln!(
                    "[monarch] keeper_result {} agent={} model={:?} latency_ms={:?} ERROR: {}",
                    run_id, agent_id, model, latency_ms, err
                );
            } else {
                let claims_count = claims.as_ref().map(|c| c.len()).unwrap_or(0);
                eprintln!(
                    "[monarch] keeper_result {} agent={} model={:?} latency_ms={:?} claims={} tokens_in={:?} tokens_out={:?}",
                    run_id, agent_id, model, latency_ms, claims_count, tokens_in, tokens_out
                );
            }
            handle_keeper_result(
                app,
                persist_tx,
                live_states,
                ws_tx,
                inner,
                db,
                &agent_id,
                run_id,
                claims,
                compaction_summary,
                tokens_in,
                tokens_out,
                error,
            )
            .await;
        }

        SidecarEvent::KeeperRewriteApplied {
            agent_id,
            run_id,
            pre_length,
            post_length,
        } => {
            eprintln!(
                "[monarch] keeper_rewrite_applied {} agent={} pre={} post={}",
                run_id, agent_id, pre_length, post_length
            );
            push_status_for_agent(
                app,
                ws_tx,
                live_states,
                &agent_id,
                format!(
                    "✦ Context compacted (Keeper run #{} — {} → {} messages in LLM view)",
                    run_id, pre_length, post_length
                ),
            )
            .await;
        }

        SidecarEvent::MemorySearchRequest {
            agent_id,
            request_id,
            query,
            top_k,
        } => {
            let (results, error) = match crate::memory_search::search_memories_for_agent_internal(
                db,
                memory_index,
                &agent_id,
                &query,
                top_k,
            )
            .await
            {
                Ok(results) => (results, None),
                Err(e) => {
                    eprintln!(
                        "[monarch] memory search failed for {} request {}: {:?}",
                        agent_id, request_id, e
                    );
                    (Vec::new(), Some(e.to_string()))
                }
            };
            let command = SidecarCommand::MemorySearchResponse {
                agent_id,
                request_id,
                results,
                error,
            };
            if dispatch_tx
                .send(InternalDispatch::SendSidecarCommand { command })
                .await
                .is_err()
            {
                eprintln!("[monarch] dispatcher closed, dropping memory search response");
            }
        }

        SidecarEvent::Unknown { raw } => {
            // Envelope-level unknown — the sidecar shipped a top-level
            // message type the Rust side doesn't recognize. Flip desync for
            // any agent id we can pluck out of the raw payload; otherwise
            // just log.
            let agent_id = raw.get("agentId").and_then(|a| a.as_str()).unwrap_or("");
            eprintln!("[monarch] Unknown sidecar envelope: {}", raw);
            if !agent_id.is_empty() {
                mark_agent_desynced(app, ws_tx, live_states, agent_id).await;
            }
        }
    }
}

/// MON-100: append a `DisplayItem::Status` to the agent's live state and
/// emit a snapshot. Used by Keeper observability events so the captain sees
/// "Memories distilled" / "Context compacted" rows land in the chat thread.
async fn push_status_for_agent(
    app: &AppHandle,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    live_states: &Arc<DashMap<String, Arc<AgentStateEntry>>>,
    agent_id: &str,
    text: String,
) {
    let Some(entry) = live_states.get(agent_id).map(|e| e.clone()) else {
        return;
    };
    let mut g = entry.inner.write().await;
    g.state.items.push(DisplayItem::Status { text });
    g.state.state_version = g.state.state_version.saturating_add(1);
    let snap = g.state.clone();
    drop(g);
    emit_state_event(app, ws_tx, &entry.topic, &snap);
}

async fn current_quest_for_event(
    app: &AppHandle,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    db: &Arc<Database>,
    agent_id: &str,
    event: &InnerEvent,
) -> Option<String> {
    match event {
        InnerEvent::MemorySuggestion { .. }
        | InnerEvent::ToolExecutionStart { .. }
        | InnerEvent::ToolExecutionEnd { .. }
        | InnerEvent::ExecutorDecision { .. } => db
            .get_agent_current_quest_id_internal(agent_id)
            .await
            .ok()
            .flatten(),
        InnerEvent::ActionTransition { intent, .. } => {
            if let Some(qid) = db
                .get_agent_current_quest_id_internal(agent_id)
                .await
                .ok()
                .flatten()
            {
                return Some(qid);
            }
            let title = intent.trim();
            if title.is_empty() {
                return None;
            }
            match db
                .auto_create_current_quest_internal(agent_id, title, None)
                .await
            {
                Ok(Some(qid)) => {
                    let payload = serde_json::json!({ "id": qid, "agentId": agent_id });
                    emit_event(
                        app,
                        ws_tx,
                        &format!("quest-created-{}", qid),
                        &payload.to_string(),
                    );
                    emit_event(
                        app,
                        ws_tx,
                        &format!("quest-created-for-agent-{}", agent_id),
                        &payload.to_string(),
                    );
                    Some(qid)
                }
                Ok(None) => db
                    .get_agent_current_quest_id_internal(agent_id)
                    .await
                    .ok()
                    .flatten(),
                Err(e) => {
                    eprintln!(
                        "[monarch] P4 action narration could not create quest for {}: {:?}",
                        agent_id, e
                    );
                    None
                }
            }
        }
        InnerEvent::ActionComplete { .. } => None,
        _ => None,
    }
}

/// MON-100: enqueue a Keeper run when the running token sum crosses a
/// threshold at the right boundary. Soft threshold fires at `TurnEnd` (next
/// natural breakpoint after crossing); hard threshold fires at `MessageEnd`
/// regardless. Reads thresholds from `memory.toml` per call — the file is
/// tiny (microseconds) and the call rate is at most a couple per turn.
async fn maybe_trigger_keeper(
    dispatch_tx: &mpsc::Sender<InternalDispatch>,
    live_states: &Arc<DashMap<String, Arc<AgentStateEntry>>>,
    agent_id: &str,
    event: &InnerEvent,
) {
    let (is_soft_boundary, is_hard_boundary) = match event {
        InnerEvent::TurnEnd => (true, false),
        // Hard trigger only on assistant `message_end` — that's where the
        // usage delta lands and that's the only role for which the executor
        // is producing live tokens.
        InnerEvent::MessageEnd { message, .. } if message.role == "assistant" => (true, true),
        _ => return,
    };

    let cfg = crate::memory_config::resolved().await;
    if !cfg.enabled {
        return;
    }

    let entry = match live_states.get(agent_id) {
        Some(e) => e.clone(),
        None => return,
    };
    let tokens = {
        let g = entry.inner.read().await;
        if g.state.keeper_in_flight {
            return;
        }
        g.state.tokens_since_last_compaction
    };

    let crossed_hard = is_hard_boundary && tokens >= cfg.hard_threshold_tokens as i64;
    let crossed_soft = is_soft_boundary && tokens >= cfg.soft_threshold_tokens as i64;
    if !(crossed_hard || crossed_soft) {
        return;
    }

    if let Err(e) = dispatch_tx.try_send(InternalDispatch::KeeperRun {
        agent_id: agent_id.to_string(),
        trigger: crate::agent::KeeperRunTrigger::Continuous,
    }) {
        // try_send avoids stalling the reader; the channel is bounded but
        // 32 is plenty for the worst-case rate (≤1 per turn). Failure
        // means the dispatcher is saturated or shutting down — log and
        // wait for the next boundary.
        eprintln!(
            "[monarch] keeper dispatch enqueue failed for {}: {:?}",
            agent_id, e
        );
    }
}

/// MON-100: handle one `keeper_result` from the sidecar.
///
/// On error: log + mark the run as 'error' in the DB; clear `keeper_in_flight`
/// so the next threshold crossing can retry; leave the token counter alone
/// (we want to retry from the same anchor).
///
/// On success: clear `keeper_in_flight` + reset `tokens_since_last_compaction`
/// in the live state, then enqueue persist commands FIFO: N × InsertMemory →
/// CompleteKeeperRun → RecordQuestEvent (when a current quest is set) →
/// RebuildHnsw. The sidecar already rewrote Pi's `state.messages` in-place.
#[allow(clippy::too_many_arguments)]
async fn handle_keeper_result(
    app: &AppHandle,
    persist_tx: &mpsc::Sender<PersistCommand>,
    live_states: &Arc<DashMap<String, Arc<AgentStateEntry>>>,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    inner: &Arc<PlMutex<AgentManagerInner>>,
    db: &Arc<Database>,
    agent_id: &str,
    run_id: i64,
    claims: Option<Vec<AtomicClaim>>,
    compaction_summary: Option<String>,
    tokens_in: Option<i64>,
    tokens_out: Option<i64>,
    error: Option<String>,
) {
    // Reset live-state flags before enqueueing persistence work so the very
    // next event for this agent sees a clean window.
    if let Some(entry) = live_states.get(agent_id).map(|e| e.clone()) {
        let mut g = entry.inner.write().await;
        g.state.keeper_in_flight = false;
        if error.is_none() {
            g.state.tokens_since_last_compaction = 0;
        }
        g.state.state_version = g.state.state_version.saturating_add(1);
        let snap = g.state.clone();
        drop(g);
        emit_state_event(app, ws_tx, &entry.topic, &snap);
    }

    // Failure path: just close out the run row; do not write memories.
    if let Some(err_msg) = error.as_ref() {
        eprintln!(
            "[monarch] keeper run {} for {} failed: {}",
            run_id, agent_id, err_msg
        );
        let _ = persist_tx
            .send(PersistCommand::CompleteKeeperRun {
                run_id,
                outcome: "error".to_string(),
                output_summary: Some(err_msg.clone()),
                tokens_in,
                tokens_out,
            })
            .await;
        return;
    }

    let claims = claims.unwrap_or_default();
    let summary = compaction_summary.unwrap_or_default();
    let session_id = inner.lock().session_map.get(agent_id).cloned();

    // Resolve the Keeper run provenance once. Quest-close runs must attach
    // memories/events to the quest that closed, even if the agent has moved
    // on and auto-created a new current quest before the model returns.
    let run_row = db.get_keeper_run_internal(run_id).await.ok().flatten();
    let trigger = run_row
        .as_ref()
        .map(|r| r.trigger.clone())
        .unwrap_or_else(|| "continuous".to_string());
    let provenance_quest_id = run_row.as_ref().and_then(|r| r.quest_id.clone());
    let current_quest_id = if provenance_quest_id.is_some() {
        provenance_quest_id
    } else {
        db.get_agent_current_quest_id_internal(agent_id)
            .await
            .ok()
            .flatten()
    };

    // Provenance: `source_events` carries the message ids that fed the
    // slice. P2 ships an empty array here — the substrate already records
    // raw events in `events` and the slice rendering is deterministic from
    // `last_keeper_run.completed_at`, so this is informational. P3+
    // populates it once we want fine-grained replay.
    for c in claims.iter() {
        let payload = InsertMemoryPayload {
            agent_id: Some(agent_id.to_string()),
            scope: "self".to_string(),
            project_id: None,
            parent_id: None,
            layer: "leaf".to_string(),
            kind: c.kind.clone(),
            title: c.title.clone(),
            summary: c.summary.clone(),
            content: Some(c.content.clone()),
            source_quest_id: current_quest_id.clone(),
            source_session_id: session_id.clone(),
            source_events: None,
            file_refs: None,
            supersedes_id: None,
        };
        if persist_tx
            .send(PersistCommand::InsertMemory {
                agent_id: agent_id.to_string(),
                payload,
            })
            .await
            .is_err()
        {
            eprintln!("[monarch] persist consumer closed, dropping InsertMemory");
            return;
        }
    }

    // Mark the run as ok with the produced summary + token counts.
    if persist_tx
        .send(PersistCommand::CompleteKeeperRun {
            run_id,
            outcome: "ok".to_string(),
            output_summary: Some(summary.clone()),
            tokens_in,
            tokens_out,
        })
        .await
        .is_err()
    {
        return;
    }

    // P6 Slice D (MON-122): attribute the closing quest's first-person report
    // to this Keeper run. Quest-close runs only; other triggers leave the
    // report attribution alone. No-op when no report row exists for the quest
    // — logged inside the apply path so dispatch here can stay declarative.
    if trigger == "quest_close" {
        if let Some(qid) = run_row.as_ref().and_then(|r| r.quest_id.clone()) {
            if persist_tx
                .send(PersistCommand::AttributeQuestReport {
                    agent_id: agent_id.to_string(),
                    quest_id: qid,
                    run_id,
                })
                .await
                .is_err()
            {
                return;
            }
        }
    }

    // Compaction tick on the quest timeline — only when an agent has a
    // current quest. Plan: when no quest is set, the run is visible only
    // via `memory_keeper_runs` and the new memories themselves.
    if let Some(qid) = current_quest_id {
        let payload_json = serde_json::json!({
            "keeper_run_id": run_id,
            "trigger": trigger,
            "claims_count": claims.len(),
            "summary": summary,
        })
        .to_string();
        let _ = persist_tx
            .send(PersistCommand::RecordQuestEvent {
                payload: RecordQuestEventPayload {
                    quest_id: qid,
                    event_type: "compaction_tick".to_string(),
                    actor: Some("keeper".to_string()),
                    payload_json: Some(payload_json),
                    author: Some("keeper".to_string()),
                    ..Default::default()
                },
            })
            .await;
    }

    // HNSW rebuild last so the index is consistent before any subsequent
    // retrieval reads.
    let _ = persist_tx
        .send(PersistCommand::RebuildHnsw {
            agent_id: agent_id.to_string(),
        })
        .await;

    // MON-100: visible signal in the chat thread. The actual `state.messages`
    // rewrite happens at the next `turn_end`; this status row just confirms
    // the Keeper itself succeeded and N memories landed.
    let memories_label = if claims.len() == 1 {
        "1 memory".to_string()
    } else {
        format!("{} memories", claims.len())
    };
    push_status_for_agent(
        app,
        ws_tx,
        live_states,
        agent_id,
        format!(
            "◈ Keeper distilled {} (run #{}) — context compacts at next turn end",
            memories_label, run_id
        ),
    )
    .await;
}

/// MON-30: body of the debounce task, factored out so it can be unit-tested
/// without an `AppHandle`. Takes the inner write lock, clears the handle, and
/// decides whether the arm is still valid:
///
/// - If `cancel_generation` no longer matches `arm_gen`, the arm was
///   invalidated by a concurrent kill / `session_destroyed` /
///   `rebuild_state_from_session`. Return `None` **without clearing `dirty`**
///   so a later event on a still-alive entry will re-arm and flush the
///   latest state.
/// - If `dirty` is false, a terminal event already flushed since the arm.
///   Return `None`.
/// - Otherwise clear `dirty`, clone the state under the guard, and return
///   the snapshot so the caller can `emit_state_event` with the guard dropped.
async fn try_consume_debounce_snapshot(
    entry: &Arc<AgentStateEntry>,
    arm_gen: u64,
) -> Option<LiveAgentState> {
    let mut g = entry.inner.write().await;
    g.debounce_handle = None;
    if entry.cancel_generation.load(Ordering::Acquire) != arm_gen {
        return None;
    }
    if !g.dirty {
        return None;
    }
    g.dirty = false;
    let snapshot = g.state.clone();
    drop(g);
    Some(snapshot)
}

/// MON-71: read the pre-apply live state once, returning `EventDurations`
/// with any finalized-now values so `build_persist_commands` can embed
/// them in the row written to SQLite. Takes a read lock; never blocks
/// beyond the copy of two i64s. Returns default (all None) for agents
/// without a live state entry (e.g. very first event before lazy init).
async fn compute_event_durations(
    live_states: &Arc<DashMap<String, Arc<AgentStateEntry>>>,
    agent_id: &str,
    event: &InnerEvent,
) -> EventDurations {
    let entry = match live_states.get(agent_id) {
        Some(e) => e.clone(),
        None => return EventDurations::default(),
    };
    let now_ms = chrono::Utc::now().timestamp_millis();
    let guard = entry.inner.read().await;
    match event {
        InnerEvent::MessageEnd { message, .. } if message.role == "assistant" => EventDurations {
            turn_duration_ms: guard
                .state
                .turn_started_at_ms
                .map(|start| now_ms.saturating_sub(start)),
            tool_duration_ms: None,
        },
        InnerEvent::ToolExecutionEnd { tool_call_id, .. } => EventDurations {
            turn_duration_ms: None,
            tool_duration_ms: guard
                .state
                .tool_executions
                .get(tool_call_id)
                .and_then(|e| e.started_at_ms)
                .map(|start| now_ms.saturating_sub(start)),
        },
        _ => EventDurations::default(),
    }
}

/// Route one typed inner event through the free `apply_event` in
/// `sidecar_protocol` and emit a snapshot on `agent-state-{id}` per the
/// returned `ApplyOutcome`. No guard is held across the emit or across any
/// await other than the lock acquire.
async fn apply_and_maybe_emit(
    app: &AppHandle,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    live_states: &Arc<DashMap<String, Arc<AgentStateEntry>>>,
    agent_id: &str,
    inner_event: &InnerEvent,
) {
    if agent_id.is_empty() {
        return;
    }

    // Lazy entry creation on first event for this agent.
    let entry = live_states
        .entry(agent_id.to_string())
        .or_insert_with(|| Arc::new(AgentStateEntry::new(agent_id)))
        .clone();

    // EmitNow branch: clone inside the guard, then drop(guard) before emit so
    // serialization runs without the RwLock write guard held (MON-38).
    let mut guard = entry.inner.write().await;
    let outcome = apply_event(&mut guard.state, inner_event);

    let snapshot_to_emit: Option<LiveAgentState> = match outcome {
        ApplyOutcome::NoOp => None,
        ApplyOutcome::EmitNow => {
            guard.dirty = false;
            if let Some(h) = guard.debounce_handle.take() {
                h.abort();
            }
            Some(guard.state.clone())
        }
        ApplyOutcome::Debounce => {
            guard.dirty = true;
            if guard.debounce_handle.is_none() {
                // MON-30: snapshot the cancel generation at arm time. The
                // debounce task compares against the current value after
                // taking the inner write lock; if kill/destroy/rebuild
                // bumped it in the meantime the task bails without emitting.
                let arm_gen = entry.cancel_generation.load(Ordering::Acquire);
                let entry_clone = entry.clone();
                let app_clone = app.clone();
                let ws_tx_clone = ws_tx.clone();
                let handle = tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(DEBOUNCE_MILLIS)).await;
                    if let Some(snapshot) =
                        try_consume_debounce_snapshot(&entry_clone, arm_gen).await
                    {
                        emit_state_event(&app_clone, &ws_tx_clone, &entry_clone.topic, &snapshot);
                    }
                });
                guard.debounce_handle = Some(handle);
            }
            None
        }
    };
    drop(guard);

    if let Some(snapshot) = snapshot_to_emit {
        emit_state_event(app, ws_tx, &entry.topic, &snapshot);
    }
}

/// Flip the `desynced` flag on an agent's `LiveAgentState` and emit a
/// snapshot. Called from the sidecar reader task when a line cannot be
/// reconciled with the current state. Surfaced via the dev-only indicator
/// (`VITE_MONARCH_DEBUG_DESYNC`); the flag resets on the next `message_start`.
pub(super) async fn mark_agent_desynced(
    app: &AppHandle,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    live_states: &Arc<DashMap<String, Arc<AgentStateEntry>>>,
    agent_id: &str,
) {
    let entry = live_states
        .entry(agent_id.to_string())
        .or_insert_with(|| Arc::new(AgentStateEntry::new(agent_id)))
        .clone();
    let mut guard = entry.inner.write().await;
    guard.state.mark_desynced();
    // MON-38: clone + explicit drop before emit_state_event.
    let snapshot = guard.state.clone();
    drop(guard);

    emit_state_event(app, ws_tx, &entry.topic, &snapshot);
}
