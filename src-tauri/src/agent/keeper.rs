use dashmap::DashMap;
use parking_lot::Mutex as PlMutex;
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::{broadcast, mpsc};

use crate::db::{Database, InsertMemoryPayload, MemoryRow, MessageRow, RecordObjectiveEventPayload};
use crate::sidecar_protocol::AtomicClaim;

use super::event_handler::{emit_state_event, push_status_for_agent};
use super::manager::{AgentManagerInner, AgentStateEntry, InternalDispatch};
use super::persist::PersistCommand;
use super::WsBroadcast;

/// MON-100: render the Curator's input slice as plain text. The Curator
/// system prompt teaches the model the section structure (PRIOR SUMMARY /
/// RELATED MEMORIES / RECENT ACTIVITY); this helper produces that layout
/// verbatim so the prompt text and the rendering stay in lockstep.
pub(super) fn render_keeper_slice(
    prior_summary: Option<&str>,
    related: &[MemoryRow],
    messages: &[MessageRow],
    objective_report: Option<&str>,
) -> String {
    let mut s = String::new();
    if let Some(p) = prior_summary {
        let trimmed = p.trim();
        if !trimmed.is_empty() {
            s.push_str("## PRIOR SUMMARY (last compaction tick)\n\n");
            s.push_str(trimmed);
            s.push_str("\n\n");
        }
    }
    if !related.is_empty() {
        s.push_str("## RELATED MEMORIES (already known — do not re-claim)\n\n");
        for m in related {
            s.push_str(&format!("- {}: {}\n", m.title, m.summary));
        }
        s.push('\n');
    }
    // P6 Slice D (MON-122): the executor's first-person report on the closing
    // objective, included only for objective-close runs. Placed before the raw stream
    // so the Curator reads the executor's own framing first; the JSON shape is
    // kept verbatim (the report tool already trims field sizes) and the LLM
    // is told via the section header that this is first-person.
    if let Some(report) = objective_report {
        let trimmed = report.trim();
        if !trimmed.is_empty() {
            s.push_str("## OBJECTIVE REPORT (first-person from the executor)\n\n");
            s.push_str(trimmed);
            s.push_str("\n\n");
        }
    }
    s.push_str("## RECENT ACTIVITY\n\n");
    for m in messages {
        let body = super::objective_prompt::extract_text_from_stored_content(&m.content);
        let body = if body.trim().is_empty() {
            m.content.clone()
        } else {
            body
        };
        s.push_str(&format!("[{} @ {}]\n{}\n\n", m.role, m.timestamp, body));
    }
    s
}

/// MON-100: enqueue a Curator run when the running token sum crosses a
/// threshold at the right boundary. Soft threshold fires at `TurnEnd` (next
/// natural breakpoint after crossing); hard threshold fires at `MessageEnd`
/// regardless. Reads thresholds from `memory.toml` per call — the file is
/// tiny (microseconds) and the call rate is at most a couple per turn.
pub(super) async fn maybe_trigger_keeper(
    dispatch_tx: &mpsc::Sender<InternalDispatch>,
    live_states: &Arc<DashMap<String, Arc<AgentStateEntry>>>,
    agent_id: &str,
    event: &crate::sidecar_protocol::InnerEvent,
) {
    let (is_soft_boundary, is_hard_boundary) = match event {
        crate::sidecar_protocol::InnerEvent::TurnEnd => (true, false),
        // Hard trigger only on assistant `message_end` — that's where the
        // usage delta lands and that's the only role for which the executor
        // is producing live tokens.
        crate::sidecar_protocol::InnerEvent::MessageEnd { message, .. }
            if message.role == "assistant" =>
        {
            (true, true)
        }
        _ => return,
    };

    let cfg = crate::memory::config::resolved().await;
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
/// CompleteKeeperRun → RecordObjectiveEvent (when a current objective is set) →
/// RebuildHnsw. The sidecar already rewrote Pi's `state.messages` in-place.
#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_keeper_result(
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

    // Resolve the Curator run provenance once. Objective-close runs must attach
    // memories/events to the objective that closed, even if the agent has moved
    // on and auto-created a new current objective before the model returns.
    let run_row = db.get_keeper_run_internal(run_id).await.ok().flatten();
    let trigger = run_row
        .as_ref()
        .map(|r| r.trigger.clone())
        .unwrap_or_else(|| "continuous".to_string());
    let provenance_objective_id = run_row.as_ref().and_then(|r| r.objective_id.clone());
    let current_objective_id = if provenance_objective_id.is_some() {
        provenance_objective_id
    } else {
        db.get_agent_current_objective_id_internal(agent_id)
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
            source_objective_id: current_objective_id.clone(),
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

    // P6 Slice D (MON-122): attribute the closing objective's first-person report
    // to this Curator run. Objective-close runs only; other triggers leave the
    // report attribution alone. No-op when no report row exists for the objective
    // — logged inside the apply path so dispatch here can stay declarative.
    if trigger == "objective_close" {
        if let Some(qid) = run_row.as_ref().and_then(|r| r.objective_id.clone()) {
            if persist_tx
                .send(PersistCommand::AttributeObjectiveReport {
                    agent_id: agent_id.to_string(),
                    objective_id: qid,
                    run_id,
                })
                .await
                .is_err()
            {
                return;
            }
        }
    }

    // Compaction tick on the objective timeline — only when an agent has a
    // current objective. Plan: when no objective is set, the run is visible only
    // via `memory_keeper_runs` and the new memories themselves.
    if let Some(qid) = current_objective_id {
        let payload_json = serde_json::json!({
            "keeper_run_id": run_id,
            "trigger": trigger,
            "claims_count": claims.len(),
            "summary": summary,
        })
        .to_string();
        let _ = persist_tx
            .send(PersistCommand::RecordObjectiveEvent {
                payload: RecordObjectiveEventPayload {
                    objective_id: qid,
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

    // MON-123: visible signal in the chat thread. The Curator no longer
    // rewrites live context (Pi's native compaction owns the window now); this
    // status row confirms the Curator succeeded and N memories landed in L3.
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
        format!("◈ Curator distilled {} (run #{})", memories_label, run_id),
    )
    .await;
}
