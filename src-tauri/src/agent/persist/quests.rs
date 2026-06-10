use parking_lot::Mutex as PlMutex;
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tokio::sync::broadcast;

use crate::agent::WsBroadcast;
use crate::db::{
    Database, QuestEventNotification, RecordQuestEventPayload, UpdateQuestPayload,
    WriteQuestReportPayload,
};
use crate::error::MonarchError;
use crate::sidecar_protocol::QuestReport;
use crate::util::chrono_now;

use crate::agent::emit_event;

pub(super) fn emit_quest_notifications(
    app: &Arc<PlMutex<Option<AppHandle>>>,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    notes: Vec<QuestEventNotification>,
) {
    if notes.is_empty() {
        return;
    }
    let app_opt = app.lock().clone();
    let Some(app) = app_opt else {
        return;
    };
    for note in notes {
        emit_event(
            &app,
            ws_tx,
            &format!("quest-event-{}", note.quest_id),
            &serde_json::json!({ "id": note.event_id, "eventType": note.event_type }).to_string(),
        );
    }
}

// ---- apply arms: quest / plan / report ----

pub(super) async fn apply_complete_keeper_run(
    db: &Database,
    run_id: i64,
    outcome: String,
    output_summary: Option<String>,
    tokens_in: Option<i64>,
    tokens_out: Option<i64>,
) -> Result<(), MonarchError> {
    db.complete_keeper_run_internal(run_id, &outcome, output_summary, tokens_in, tokens_out)
        .await
}

pub(super) async fn apply_attribute_quest_report(
    db: &Database,
    quest_id: String,
    run_id: i64,
) -> Result<(), MonarchError> {
    // No-op when no report row exists for the quest. Logged so a
    // quiet wiring regression is visible, but never an error —
    // a quest can close without ever having a report.
    let attributed = db
        .attribute_quest_report_to_keeper_run_internal(&quest_id, run_id)
        .await?;
    if !attributed {
        eprintln!(
            "[monarch] keeper run {} closed quest {} with no report to attribute",
            run_id, quest_id
        );
    }
    Ok(())
}

pub(super) async fn apply_record_quest_event(
    db: &Database,
    app: &Arc<PlMutex<Option<AppHandle>>>,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    payload: RecordQuestEventPayload,
) -> Result<(), MonarchError> {
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

pub(super) async fn apply_write_quest_report(
    db: &Database,
    app: &Arc<PlMutex<Option<AppHandle>>>,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    payload: WriteQuestReportPayload,
) -> Result<(), MonarchError> {
    let quest_id = payload.quest_id.clone();
    let id = db.upsert_quest_report_internal(&payload).await?;
    // Same broadcast shape as RecordQuestEvent so the captain UI
    // (Slice C) wakes when a Keeper or executor write lands,
    // matching how `db_save_quest_report` Tauri command emits.
    let app_opt = app.lock().clone();
    if let Some(app) = app_opt {
        emit_event(
            &app,
            ws_tx,
            &format!("quest-report-{}", quest_id),
            &serde_json::json!({ "id": id, "action": "saved" }).to_string(),
        );
    }
    Ok(())
}

pub(super) async fn apply_complete_quest(
    db: &Database,
    app: &Arc<PlMutex<Option<AppHandle>>>,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    quest_id: String,
    report: QuestReport,
) -> Result<(), MonarchError> {
    // 1. Persist the report first — the quest-close Keeper tick
    //    (Slice D) must see it. The structured report is stored
    //    verbatim as the JSON `payload`.
    let payload_json = serde_json::to_string(&report)?;
    let report_id = db
        .upsert_quest_report_internal(&WriteQuestReportPayload {
            id: None,
            quest_id: quest_id.clone(),
            payload: payload_json,
        })
        .await?;
    let app_opt = app.lock().clone();
    if let Some(app) = app_opt.as_ref() {
        emit_event(
            app,
            ws_tx,
            &format!("quest-report-{}", quest_id),
            &serde_json::json!({ "id": report_id, "action": "saved" }).to_string(),
        );
    }
    // 2. Terminal outcomes close the quest. `blocked` / `partial`
    //    (and any unrecognized outcome) record the report but
    //    leave the quest open.
    let new_status = match report.outcome.as_str() {
        "done" => Some("done"),
        "abandoned" => Some("abandoned"),
        _ => None,
    };
    if let Some(status) = new_status {
        let before = db.get_quest_internal(&quest_id).await?;
        let now = chrono_now();
        db.update_quest_internal(&UpdateQuestPayload {
            id: quest_id.clone(),
            title: None,
            description: None,
            scope: None,
            current_direction: None,
            rationale: None,
            fork_parent_id: None,
            status: Some(status.to_string()),
            grade: None,
            exec_hint: None,
            assignee_shadow_id: None,
            summary: None,
            started_at: None,
            completed_at: (status == "done").then(|| now.clone()),
            abandoned_at: (status == "abandoned").then(|| now.clone()),
        })
        .await?;
        let after = db.get_quest_internal(&quest_id).await?;
        // 3. Run the same quest-close side effects as the
        //    captain's `db_update_quest` path: status_change
        //    event, clear the agent current-quest pointer,
        //    dispatch the quest-close Keeper run. Reached via
        //    `AppHandle::state()` because the persist consumer
        //    is not handed `AgentManager` directly.
        if let Some(app) = app_opt {
            let db_arc = app.state::<Arc<Database>>().inner().clone();
            let mgr_arc = app
                .state::<Arc<crate::agent::AgentManager>>()
                .inner()
                .clone();
            crate::db::handle_quest_update_side_effects(&app, &db_arc, &mgr_arc, before, after)
                .await?;
        }
    }
    Ok(())
}

pub(super) async fn apply_action_transition(
    db: &Database,
    app: &Arc<PlMutex<Option<AppHandle>>>,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    agent_id: String,
    quest_id: String,
    intent: String,
    previous_outcome: Option<String>,
) -> Result<(), MonarchError> {
    let notes = db
        .record_action_transition_internal(
            &agent_id,
            &quest_id,
            &intent,
            previous_outcome.as_deref(),
        )
        .await?;
    emit_quest_notifications(app, ws_tx, notes);
    Ok(())
}

pub(super) async fn apply_action_complete(
    db: &Database,
    app: &Arc<PlMutex<Option<AppHandle>>>,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    agent_id: String,
    outcome: String,
) -> Result<(), MonarchError> {
    let notes = db.complete_action_internal(&agent_id, &outcome).await?;
    emit_quest_notifications(app, ws_tx, notes);
    Ok(())
}

pub(super) async fn apply_executor_decision(
    db: &Database,
    app: &Arc<PlMutex<Option<AppHandle>>>,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    agent_id: String,
    quest_id: String,
    decision: String,
    rationale: Option<String>,
) -> Result<(), MonarchError> {
    let notes = db
        .record_executor_decision_internal(
            &agent_id,
            &quest_id,
            &decision,
            rationale.as_deref(),
        )
        .await?;
    emit_quest_notifications(app, ws_tx, notes);
    Ok(())
}

pub(super) async fn apply_tool_call_start(
    db: &Database,
    app: &Arc<PlMutex<Option<AppHandle>>>,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    agent_id: String,
    quest_id: String,
    tool_call_id: String,
    tool_name: String,
    args: Option<serde_json::Value>,
) -> Result<(), MonarchError> {
    let notes = db
        .record_tool_call_start_internal(
            &agent_id,
            &quest_id,
            &tool_call_id,
            &tool_name,
            args,
        )
        .await?;
    emit_quest_notifications(app, ws_tx, notes);
    Ok(())
}

pub(super) async fn apply_tool_call_end(
    db: &Database,
    app: &Arc<PlMutex<Option<AppHandle>>>,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    tool_call_id: String,
    result: Option<serde_json::Value>,
    is_error: bool,
    duration_ms: Option<i64>,
) -> Result<(), MonarchError> {
    let notes = db
        .record_tool_call_end_internal(&tool_call_id, result, is_error, duration_ms)
        .await?;
    emit_quest_notifications(app, ws_tx, notes);
    Ok(())
}

pub(super) async fn apply_plan_set(
    db: &Database,
    app: &Arc<PlMutex<Option<AppHandle>>>,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    payload: crate::db::SetPlanPayload,
) -> Result<(), MonarchError> {
    let notes = db.set_plan_internal(&payload).await?;
    emit_quest_notifications(app, ws_tx, notes);
    Ok(())
}

pub(super) async fn apply_plan_item_start(
    db: &Database,
    app: &Arc<PlMutex<Option<AppHandle>>>,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    item_id: String,
) -> Result<(), MonarchError> {
    let notes = db.start_plan_item_internal(&item_id).await?;
    emit_quest_notifications(app, ws_tx, notes);
    Ok(())
}

pub(super) async fn apply_plan_item_complete(
    db: &Database,
    app: &Arc<PlMutex<Option<AppHandle>>>,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    agent_id: String,
    outcome: Option<String>,
) -> Result<(), MonarchError> {
    let target = db
        .get_active_plan_item_for_agent_internal(&agent_id)
        .await?;
    let Some(item_id) = target else {
        return Ok(());
    };
    let notes = db
        .complete_plan_item_internal(&item_id, outcome.as_deref())
        .await?;
    emit_quest_notifications(app, ws_tx, notes);
    Ok(())
}

pub(super) async fn apply_plan_item_skip(
    db: &Database,
    app: &Arc<PlMutex<Option<AppHandle>>>,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    agent_id: String,
    item_id: Option<String>,
    reason: Option<String>,
) -> Result<(), MonarchError> {
    let target = match item_id {
        Some(id) => Some(id),
        None => {
            db.get_active_plan_item_for_agent_internal(&agent_id)
                .await?
        }
    };
    let Some(item_id) = target else {
        return Ok(());
    };
    let notes = db
        .skip_plan_item_internal(&item_id, reason.as_deref())
        .await?;
    emit_quest_notifications(app, ws_tx, notes);
    Ok(())
}

pub(super) async fn apply_plan_item_block(
    db: &Database,
    app: &Arc<PlMutex<Option<AppHandle>>>,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    agent_id: String,
    item_id: Option<String>,
    reason: String,
) -> Result<(), MonarchError> {
    let target = match item_id {
        Some(id) => Some(id),
        None => {
            db.get_active_plan_item_for_agent_internal(&agent_id)
                .await?
        }
    };
    let Some(item_id) = target else {
        return Ok(());
    };
    let notes = db.block_plan_item_internal(&item_id, &reason).await?;
    emit_quest_notifications(app, ws_tx, notes);
    Ok(())
}
