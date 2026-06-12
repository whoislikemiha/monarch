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

use crate::agent::state::{ApplyOutcome, DisplayItem, LiveAgentState};
use crate::db::Database;
use crate::memory::index::MemoryIndex;
use crate::sidecar_protocol::{apply_event, InnerEvent, SidecarCommand, SidecarEvent};

use super::keeper::{handle_keeper_result, maybe_trigger_keeper};
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
/// command surfaces (e.g. objective CRUD in `db.rs`) can broadcast their own
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
            let current_objective_id =
                current_objective_for_event(app, ws_tx, db, &agent_id, &inner_event).await;
            for cmd in build_persist_commands(
                &agent_id,
                session_id,
                &inner_event,
                inner_raw,
                durations,
                current_objective_id,
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

        SidecarEvent::MemorySearchRequest {
            agent_id,
            request_id,
            query,
            top_k,
        } => {
            let (results, error) = match crate::memory::search::search_memories_for_agent_internal(
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
pub(super) async fn push_status_for_agent(
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

async fn current_objective_for_event(
    app: &AppHandle,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    db: &Arc<Database>,
    agent_id: &str,
    event: &InnerEvent,
) -> Option<String> {
    match event {
        InnerEvent::MemorySuggestion { .. } | InnerEvent::ToolExecutionEnd { .. } => db
            .get_agent_current_objective_id_internal(agent_id)
            .await
            .ok()
            .flatten(),
        // MON-124: tool calls and decisions with no current objective land on
        // the agent's scratch objective instead of being dropped — unscoped
        // work is still part of the record.
        InnerEvent::ToolExecutionStart { .. } | InnerEvent::ExecutorDecision { .. } => {
            match db
                .get_agent_current_objective_id_internal(agent_id)
                .await
                .ok()
                .flatten()
            {
                Some(qid) => Some(qid),
                None => db.ensure_scratch_objective_internal(agent_id).await.ok(),
            }
        }
        InnerEvent::ActionTransition { intent, .. } => {
            resolve_narration_objective(app, ws_tx, db, agent_id, intent).await
        }
        InnerEvent::ActionComplete { .. } => None,
        _ => None,
    }
}

/// Narration needs an objective: the agent's current one, else auto-create
/// from the intent (project agents, MON-105), else the per-agent scratch
/// objective (project-less — MON-124).
async fn resolve_narration_objective(
    app: &AppHandle,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    db: &Arc<Database>,
    agent_id: &str,
    intent: &str,
) -> Option<String> {
    if let Some(qid) = db
        .get_agent_current_objective_id_internal(agent_id)
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
        .auto_create_current_objective_internal(agent_id, title, None)
        .await
    {
        Ok(Some(qid)) => {
            let payload = serde_json::json!({ "id": qid, "agentId": agent_id });
            emit_event(
                app,
                ws_tx,
                &format!("objective-created-{}", qid),
                &payload.to_string(),
            );
            emit_event(
                app,
                ws_tx,
                &format!("objective-created-for-agent-{}", agent_id),
                &payload.to_string(),
            );
            Some(qid)
        }
        // No project → no campaign to branch under: the scratch objective is
        // the durable home for unscoped narration.
        Ok(None) => db.ensure_scratch_objective_internal(agent_id).await.ok(),
        Err(e) => {
            eprintln!(
                "[monarch] P4 action narration could not create objective for {}: {:?}",
                agent_id, e
            );
            None
        }
    }
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
