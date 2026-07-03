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

mod messages;
mod objectives;
mod util;

use dashmap::DashMap;
use parking_lot::Mutex as PlMutex;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::{broadcast, mpsc};

use crate::db::{
    Database, InsertMemoryPayload, MessageRow, RecordObjectiveEventPayload, SaveClassificationPayload,
    SetPlanPayload, WriteObjectiveReportPayload,
};
use crate::error::MonarchError;
use crate::memory::index::MemoryIndex;
use crate::sidecar_protocol::ObjectiveReport;

use super::event_handler::mark_agent_desynced;
use super::manager::AgentStateEntry;
use super::WsBroadcast;

pub(crate) use messages::{build_persist_commands, EventDurations};

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
    /// MON-100: insert one Curator-produced atomic claim. The consumer embeds
    /// `payload.summary` via `MemoryIndex::embed_to_blob` before the insert
    /// so the new row carries an embedding immediately. If the embedder is
    /// not initialised the row is still written (without an embedding) and
    /// the subsequent `RebuildHnsw` simply skips it — the supervisor can still
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
    /// MON-122: P6 Slice D — attribute a objective's first-person report to the
    /// Curator run that distilled it. Sent from `handle_keeper_result` after a
    /// successful `objective_close` run that had a report row. No-op when no
    /// report exists for the objective. Rides the pipeline so the write is
    /// serialized with the in-flight `InsertMemory` / `CompleteKeeperRun`
    /// commands from the same run.
    AttributeObjectiveReport {
        agent_id: String,
        objective_id: String,
        run_id: i64,
    },
    /// MON-100 / MON-83: append one row to `objective_events` and broadcast on
    /// `objective-event-{objectiveId}` so the ObjectiveTimelineTool wakes. Wraps the
    /// existing `db.record_objective_event_internal` + `agent::emit_event` pair
    /// from the `db_record_objective_event` Tauri command.
    RecordObjectiveEvent {
        payload: RecordObjectiveEventPayload,
    },
    /// MON-119: P6 Slice A — upsert a first-person objective report. The
    /// sidecar `complete_objective(report)` tool in Slice B is the intended
    /// producer; this variant lives here so the pipeline is ready when
    /// that wiring lands. Supervisor-initiated saves bypass the pipeline and
    /// go straight through the `db_save_objective_report` Tauri command,
    /// matching the `db_create_objective_ref` precedent.
    WriteObjectiveReport {
        payload: WriteObjectiveReportPayload,
    },
    /// MON-120: P6 Slice B — the executor's `complete_objective` produced a
    /// first-person report. The apply upserts the report into
    /// `objective_reports`, then for a terminal `outcome` (`done` / `abandoned`)
    /// transitions the objective's status and runs the same objective-close side
    /// effects as the supervisor's `db_update_objective` path (status_change
    /// event, clear current-objective pointer, dispatch objective-close Curator).
    /// Report write happens before the status transition so the objective-close
    /// Curator tick (Slice D) sees the report.
    CompleteObjective {
        agent_id: String,
        objective_id: String,
        report: ObjectiveReport,
    },
    ActionTransition {
        agent_id: String,
        objective_id: String,
        intent: String,
        previous_outcome: Option<String>,
    },
    ActionComplete {
        agent_id: String,
        outcome: String,
    },
    ExecutorDecision {
        agent_id: String,
        objective_id: String,
        decision: String,
        rationale: Option<String>,
    },
    ToolCallStart {
        agent_id: String,
        objective_id: String,
        tool_call_id: String,
        tool_name: String,
        args: Option<serde_json::Value>,
    },
    ToolCallEnd {
        agent_id: String,
        tool_call_id: String,
        result: Option<serde_json::Value>,
        is_error: bool,
        duration_ms: Option<i64>,
    },
    /// P4b: bulk replace a objective's plan. The sidecar `set_plan` tool
    /// emits this; manual UI edits go directly through the Tauri
    /// command path (which calls `Database::set_plan_internal`).
    PlanSet {
        agent_id: String,
        payload: SetPlanPayload,
    },
    /// P4b: mark a plan item active. `item_id` is resolved upstream by
    /// the executor tool — Slice A does not invent ids.
    PlanItemStart {
        agent_id: String,
        item_id: String,
    },
    /// P4b: complete the currently active plan item on the agent's
    /// current objective. The active item is looked up server-side from
    /// `objective_plan_items.status = 'active'`.
    PlanItemComplete {
        agent_id: String,
        outcome: Option<String>,
    },
    /// P4b: skip the named item, or the current active item if `None`.
    PlanItemSkip {
        agent_id: String,
        item_id: Option<String>,
        reason: Option<String>,
    },
    /// P4b: block the named item, or the current active item if `None`.
    PlanItemBlock {
        agent_id: String,
        item_id: Option<String>,
        reason: String,
    },
    /// MON-100: full-rebuild the per-agent HNSW index from current DB
    /// embeddings. Runs last in a Curator-tick burst so the index is
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
            | Self::RebuildHnsw { agent_id, .. }
            | Self::ActionTransition { agent_id, .. }
            | Self::ActionComplete { agent_id, .. }
            | Self::ExecutorDecision { agent_id, .. }
            | Self::ToolCallStart { agent_id, .. }
            | Self::ToolCallEnd { agent_id, .. }
            | Self::PlanSet { agent_id, .. }
            | Self::PlanItemStart { agent_id, .. }
            | Self::PlanItemComplete { agent_id, .. }
            | Self::PlanItemSkip { agent_id, .. }
            | Self::PlanItemBlock { agent_id, .. }
            | Self::CompleteObjective { agent_id, .. }
            | Self::AttributeObjectiveReport { agent_id, .. } => agent_id,
            Self::SaveClassification { payload } => &payload.agent_id,
            // CompleteKeeperRun + RecordObjectiveEvent + WriteObjectiveReport don't
            // carry an agent id directly — failures still log but cannot
            // flip a per-agent desync flag. Empty string causes the
            // consumer's desync helper to short-circuit
            // (`if agent_id.is_empty()`). WriteObjectiveReport resolves the
            // agent from objective_nodes.assignee_shadow_id inside the apply.
            Self::CompleteKeeperRun { .. }
            | Self::RecordObjectiveEvent { .. }
            | Self::WriteObjectiveReport { .. } => "",
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
                messages::apply_log_event(db, agent_id, session_id, event_type, data).await
            }
            Self::SaveAssistantMessage {
                message,
                attachments,
                pending_classification_id,
                ..
            } => {
                messages::apply_save_assistant_message(
                    db,
                    app,
                    ws_tx,
                    ctx,
                    message,
                    attachments,
                    pending_classification_id,
                )
                .await
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
            Self::IncrementAgentTurns { agent_id } => db.increment_agent_turns(&agent_id).await,
            Self::RecordToolUsage {
                agent_id,
                tool_name,
                is_error,
            } => db.record_tool_usage(&agent_id, &tool_name, is_error).await,
            Self::SaveClassification { payload } => {
                messages::apply_save_classification(db, ctx, payload).await
            }
            Self::InsertMemory {
                agent_id: _,
                payload,
            } => messages::apply_insert_memory(db, memory_index, payload).await,
            Self::CompleteKeeperRun {
                run_id,
                outcome,
                output_summary,
                tokens_in,
                tokens_out,
            } => {
                objectives::apply_complete_keeper_run(
                    db,
                    run_id,
                    outcome,
                    output_summary,
                    tokens_in,
                    tokens_out,
                )
                .await
            }
            Self::AttributeObjectiveReport {
                agent_id: _,
                objective_id,
                run_id,
            } => objectives::apply_attribute_objective_report(db, objective_id, run_id).await,
            Self::RecordObjectiveEvent { payload } => {
                objectives::apply_record_objective_event(db, app, ws_tx, payload).await
            }
            Self::WriteObjectiveReport { payload } => {
                objectives::apply_write_objective_report(db, app, ws_tx, payload).await
            }
            Self::CompleteObjective {
                agent_id: _,
                objective_id,
                report,
            } => objectives::apply_complete_objective(db, app, ws_tx, objective_id, report).await,
            Self::ActionTransition {
                agent_id,
                objective_id,
                intent,
                previous_outcome,
            } => {
                objectives::apply_action_transition(
                    db,
                    app,
                    ws_tx,
                    agent_id,
                    objective_id,
                    intent,
                    previous_outcome,
                )
                .await
            }
            Self::ActionComplete { agent_id, outcome } => {
                objectives::apply_action_complete(db, app, ws_tx, agent_id, outcome).await
            }
            Self::ExecutorDecision {
                agent_id,
                objective_id,
                decision,
                rationale,
            } => {
                objectives::apply_executor_decision(
                    db, app, ws_tx, agent_id, objective_id, decision, rationale,
                )
                .await
            }
            Self::ToolCallStart {
                agent_id,
                objective_id,
                tool_call_id,
                tool_name,
                args,
            } => {
                objectives::apply_tool_call_start(
                    db,
                    app,
                    ws_tx,
                    agent_id,
                    objective_id,
                    tool_call_id,
                    tool_name,
                    args,
                )
                .await
            }
            Self::ToolCallEnd {
                tool_call_id,
                result,
                is_error,
                duration_ms,
                ..
            } => {
                objectives::apply_tool_call_end(db, app, ws_tx, tool_call_id, result, is_error, duration_ms)
                    .await
            }
            Self::PlanSet { payload, .. } => {
                objectives::apply_plan_set(db, app, ws_tx, payload).await
            }
            Self::PlanItemStart { item_id, .. } => {
                objectives::apply_plan_item_start(db, app, ws_tx, item_id).await
            }
            Self::PlanItemComplete { agent_id, outcome } => {
                objectives::apply_plan_item_complete(db, app, ws_tx, agent_id, outcome).await
            }
            Self::PlanItemSkip {
                agent_id,
                item_id,
                reason,
            } => objectives::apply_plan_item_skip(db, app, ws_tx, agent_id, item_id, reason).await,
            Self::PlanItemBlock {
                agent_id,
                item_id,
                reason,
            } => objectives::apply_plan_item_block(db, app, ws_tx, agent_id, item_id, reason).await,
            Self::RebuildHnsw { agent_id } => {
                messages::apply_rebuild_hnsw(db, memory_index, agent_id).await
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::CreateObjectivePayload;
    use crate::sidecar_protocol::{InnerEvent, ObjectiveReport};

    fn sample_report(outcome: &str) -> ObjectiveReport {
        ObjectiveReport {
            summary: "shipped slice B".to_string(),
            outcome: outcome.to_string(),
            decisions: vec![],
            learned: vec!["report-on-close is a single call".to_string()],
            artifacts: vec![],
            open_threads: vec![],
            reflection: "tight slice".to_string(),
            grade: "A".to_string(),
        }
    }

    /// Create a objective with no assignee — the report's `agent_id` resolves to
    /// NULL, so no agent row is needed for the FK.
    async fn seed_objective(db: &Database) -> String {
        db.create_objective_internal(&CreateObjectivePayload {
            id: None,
            parent_id: None,
            title: "Test objective".to_string(),
            description: None,
            status: Some("in_progress".to_string()),
            grade: Some("C".to_string()),
            exec_hint: Some("in_context".to_string()),
            assignee_shadow_id: None,
            created_by: Some("monarch".to_string()),
            kind: None,
        })
        .await
        .expect("create objective")
    }

    /// Consumer context with no `AppHandle`. The `CompleteObjective` apply still
    /// upserts the report and transitions objective status (both DB-only); only
    /// `handle_objective_update_side_effects` is gated on the handle and skipped.
    fn headless_ctx() -> (
        Arc<MemoryIndex>,
        Arc<PlMutex<Option<AppHandle>>>,
        broadcast::Sender<WsBroadcast>,
    ) {
        let memory_index = Arc::new(MemoryIndex::new(std::env::temp_dir()));
        let app: Arc<PlMutex<Option<AppHandle>>> = Arc::new(PlMutex::new(None));
        let (ws_tx, _rx) = broadcast::channel(8);
        (memory_index, app, ws_tx)
    }

    #[test]
    fn objective_report_event_builds_complete_objective_command() {
        let event = InnerEvent::ObjectiveReport {
            report: sample_report("done"),
        };
        let cmds = build_persist_commands(
            "agent-1",
            Some("sess-1".to_string()),
            &event,
            None,
            EventDurations::default(),
            Some("objective-1".to_string()),
        );
        // LogEvent is always first; CompleteObjective follows.
        assert_eq!(cmds.len(), 2);
        assert!(matches!(cmds[0], PersistCommand::LogEvent { .. }));
        match &cmds[1] {
            PersistCommand::CompleteObjective {
                agent_id,
                objective_id,
                report,
            } => {
                assert_eq!(agent_id, "agent-1");
                assert_eq!(objective_id, "objective-1");
                assert_eq!(report.outcome, "done");
            }
            other => panic!("expected CompleteObjective, got {:?}", other),
        }
    }

    #[test]
    fn objective_report_event_without_current_objective_drops_command() {
        let event = InnerEvent::ObjectiveReport {
            report: sample_report("done"),
        };
        let cmds = build_persist_commands(
            "agent-1",
            Some("sess-1".to_string()),
            &event,
            None,
            EventDurations::default(),
            None,
        );
        // Only the always-on LogEvent — no CompleteObjective without a objective.
        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], PersistCommand::LogEvent { .. }));
    }

    #[tokio::test]
    async fn complete_objective_apply_writes_report_and_closes_done() {
        let db = Database::new_in_memory().await.expect("db");
        let objective_id = seed_objective(&db).await;
        let (memory_index, app, ws_tx) = headless_ctx();
        let mut ctx = PersistContext::default();

        PersistCommand::CompleteObjective {
            agent_id: "agent-1".to_string(),
            objective_id: objective_id.clone(),
            report: sample_report("done"),
        }
        .apply(&db, &memory_index, &app, &ws_tx, &mut ctx)
        .await
        .expect("apply");

        // The report row landed, carrying the structured payload verbatim.
        let report = db
            .get_objective_report_by_objective_internal(&objective_id)
            .await
            .expect("get report")
            .expect("report row");
        assert!(report.payload.contains("shipped slice B"));

        // ...and the objective closed as done with a completion timestamp.
        let objective = db
            .get_objective_internal(&objective_id)
            .await
            .expect("get objective")
            .expect("objective");
        assert_eq!(objective.status, "done");
        assert!(objective.completed_at.is_some());
        assert!(objective.abandoned_at.is_none());
    }

    #[tokio::test]
    async fn complete_objective_apply_abandoned_sets_abandoned_status() {
        let db = Database::new_in_memory().await.expect("db");
        let objective_id = seed_objective(&db).await;
        let (memory_index, app, ws_tx) = headless_ctx();
        let mut ctx = PersistContext::default();

        PersistCommand::CompleteObjective {
            agent_id: "agent-1".to_string(),
            objective_id: objective_id.clone(),
            report: sample_report("abandoned"),
        }
        .apply(&db, &memory_index, &app, &ws_tx, &mut ctx)
        .await
        .expect("apply");

        let objective = db
            .get_objective_internal(&objective_id)
            .await
            .expect("get objective")
            .expect("objective");
        assert_eq!(objective.status, "abandoned");
        assert!(objective.abandoned_at.is_some());
        assert!(objective.completed_at.is_none());
    }

    #[tokio::test]
    async fn complete_objective_apply_blocked_writes_report_leaves_objective_open() {
        let db = Database::new_in_memory().await.expect("db");
        let objective_id = seed_objective(&db).await;
        let (memory_index, app, ws_tx) = headless_ctx();
        let mut ctx = PersistContext::default();

        PersistCommand::CompleteObjective {
            agent_id: "agent-1".to_string(),
            objective_id: objective_id.clone(),
            report: sample_report("blocked"),
        }
        .apply(&db, &memory_index, &app, &ws_tx, &mut ctx)
        .await
        .expect("apply");

        // Non-terminal outcome: the report is still recorded...
        let report = db
            .get_objective_report_by_objective_internal(&objective_id)
            .await
            .expect("get report");
        assert!(report.is_some());

        // ...but the objective stays open at its original status.
        let objective = db
            .get_objective_internal(&objective_id)
            .await
            .expect("get objective")
            .expect("objective");
        assert_eq!(objective.status, "in_progress");
        assert!(objective.completed_at.is_none());
        assert!(objective.abandoned_at.is_none());
    }

    // ---- P6 Slice D (MON-122): AttributeObjectiveReport apply path -----------

    /// Seed a objective, upsert a first-person report on it, and insert a Curator
    /// run row. Returns `(objective_id, report_id, run_id)` for the test to drive
    /// the attribute command.
    async fn seed_objective_report_and_run(db: &Database) -> (String, String, i64) {
        let objective_id = seed_objective(db).await;
        let report_id = db
            .upsert_objective_report_internal(&crate::db::WriteObjectiveReportPayload {
                id: None,
                objective_id: objective_id.clone(),
                payload: "{\"summary\":\"slice D test\",\"outcome\":\"done\"}".to_string(),
            })
            .await
            .expect("upsert report");
        let run_id = db
            .insert_keeper_run_internal("agent-1", "objective_close", Some(&objective_id), "test/model")
            .await
            .expect("insert keeper run");
        (objective_id, report_id, run_id)
    }

    #[tokio::test]
    async fn attribute_objective_report_sets_distilled_by_keeper_run_id() {
        let db = Database::new_in_memory().await.expect("db");
        let (memory_index, app, ws_tx) = headless_ctx();
        let mut ctx = PersistContext::default();
        let (objective_id, _report_id, run_id) = seed_objective_report_and_run(&db).await;

        PersistCommand::AttributeObjectiveReport {
            agent_id: "agent-1".to_string(),
            objective_id: objective_id.clone(),
            run_id,
        }
        .apply(&db, &memory_index, &app, &ws_tx, &mut ctx)
        .await
        .expect("apply");

        let report = db
            .get_objective_report_by_objective_internal(&objective_id)
            .await
            .expect("get report")
            .expect("report row");
        assert_eq!(report.distilled_by_keeper_run_id, Some(run_id));
    }

    #[tokio::test]
    async fn attribute_objective_report_rewrites_attribution_on_rerun() {
        let db = Database::new_in_memory().await.expect("db");
        let (memory_index, app, ws_tx) = headless_ctx();
        let mut ctx = PersistContext::default();
        let (objective_id, _report_id, first_run) = seed_objective_report_and_run(&db).await;
        let second_run = db
            .insert_keeper_run_internal("agent-1", "objective_close", Some(&objective_id), "test/model")
            .await
            .expect("second run");

        // First attribution.
        PersistCommand::AttributeObjectiveReport {
            agent_id: "agent-1".to_string(),
            objective_id: objective_id.clone(),
            run_id: first_run,
        }
        .apply(&db, &memory_index, &app, &ws_tx, &mut ctx)
        .await
        .expect("apply first");

        // Re-running the Curator for the same objective must overwrite cleanly,
        // not violate uniqueness.
        PersistCommand::AttributeObjectiveReport {
            agent_id: "agent-1".to_string(),
            objective_id: objective_id.clone(),
            run_id: second_run,
        }
        .apply(&db, &memory_index, &app, &ws_tx, &mut ctx)
        .await
        .expect("apply second");

        let report = db
            .get_objective_report_by_objective_internal(&objective_id)
            .await
            .expect("get report")
            .expect("report row");
        assert_eq!(report.distilled_by_keeper_run_id, Some(second_run));
    }

    #[tokio::test]
    async fn attribute_objective_report_is_noop_when_no_report_exists() {
        let db = Database::new_in_memory().await.expect("db");
        let (memory_index, app, ws_tx) = headless_ctx();
        let mut ctx = PersistContext::default();

        // Objective exists, report does not. Objective-close runs can still fire on
        // objectives that never produced a report (e.g. supervisor-edited close),
        // and the apply must not error on that path.
        let objective_id = seed_objective(&db).await;
        let run_id = db
            .insert_keeper_run_internal("agent-1", "objective_close", Some(&objective_id), "test/model")
            .await
            .expect("insert keeper run");

        PersistCommand::AttributeObjectiveReport {
            agent_id: "agent-1".to_string(),
            objective_id: objective_id.clone(),
            run_id,
        }
        .apply(&db, &memory_index, &app, &ws_tx, &mut ctx)
        .await
        .expect("apply (no-op)");

        // Sanity: still no report row was created by the no-op.
        let report = db
            .get_objective_report_by_objective_internal(&objective_id)
            .await
            .expect("get report");
        assert!(report.is_none());
    }
}
