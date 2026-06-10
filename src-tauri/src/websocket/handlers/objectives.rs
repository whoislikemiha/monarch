use serde_json::Value;

use crate::error::MonarchError;
use crate::websocket::WsState;
use super::str_field;

// ---- DB: Objectives (MON-83) ----
// Write commands emit the matching `objective-*-{id}` channel via the
// shared broadcast pipeline so WS subscribers stay in sync without
// a manual refetch.

pub(crate) async fn db_create_objective(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let payload: crate::db::CreateObjectivePayload =
        serde_json::from_value(args.get("payload").cloned().unwrap_or(args.clone()))
            .map_err(|e| MonarchError::invalid_input(format!("Invalid payload: {}", e)))?;
    let id = state.db.create_objective_internal(&payload).await?;
    let app = state.agent_mgr.get_app_handle()?;
    crate::agent::emit_event(
        &app,
        &state.agent_mgr.ws_broadcast,
        &format!("objective-created-{}", id),
        &serde_json::json!({ "id": id }).to_string(),
    );
    Ok(Value::String(id))
}

pub(crate) async fn db_update_objective(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let payload: crate::db::UpdateObjectivePayload =
        serde_json::from_value(args.get("payload").cloned().unwrap_or(args.clone()))
            .map_err(|e| MonarchError::invalid_input(format!("Invalid payload: {}", e)))?;
    let id = payload.id.clone();
    let before = state.db.get_objective_internal(&id).await?;
    state.db.update_objective_internal(&payload).await?;
    let after = state.db.get_objective_internal(&id).await?;
    let app = state.agent_mgr.get_app_handle()?;
    crate::agent::emit_event(
        &app,
        &state.agent_mgr.ws_broadcast,
        &format!("objective-updated-{}", id),
        &serde_json::json!({ "id": id }).to_string(),
    );
    if let Some(after_objective) = after.as_ref() {
        if after_objective.root_id != after_objective.id {
            crate::agent::emit_event(
                &app,
                &state.agent_mgr.ws_broadcast,
                &format!("objective-updated-{}", after_objective.root_id),
                &serde_json::json!({ "id": after_objective.id, "rootId": after_objective.root_id })
                    .to_string(),
            );
        }
    }
    crate::db::handle_objective_update_side_effects(
        &app,
        &state.db,
        &state.agent_mgr,
        before,
        after,
    )
    .await?;
    Ok(Value::Null)
}

pub(crate) async fn db_get_objective(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let objective_id = str_field(&args, "objectiveId")?;
    let objective = state.db.get_objective_internal(&objective_id).await?;
    serde_json::to_value(objective).map_err(MonarchError::from)
}

pub(crate) async fn db_list_objectives_for_agent(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let agent_id = str_field(&args, "agentId")?;
    let objectives = state.db.list_objectives_for_agent_internal(&agent_id).await?;
    serde_json::to_value(objectives).map_err(MonarchError::from)
}

pub(crate) async fn db_get_objective_tree_for_root(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let root_id = str_field(&args, "rootId")?;
    let tree = state.db.get_objective_tree_for_root_internal(&root_id).await?;
    serde_json::to_value(tree).map_err(MonarchError::from)
}

pub(crate) async fn db_record_objective_event(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let payload: crate::db::RecordObjectiveEventPayload =
        serde_json::from_value(args.get("payload").cloned().unwrap_or(args.clone()))
            .map_err(|e| MonarchError::invalid_input(format!("Invalid payload: {}", e)))?;
    let objective_id = payload.objective_id.clone();
    let event_type = payload.event_type.clone();
    let id = state.db.record_objective_event_internal(&payload).await?;
    let app = state.agent_mgr.get_app_handle()?;
    crate::agent::emit_event(
        &app,
        &state.agent_mgr.ws_broadcast,
        &format!("objective-event-{}", objective_id),
        &serde_json::json!({ "id": id, "eventType": event_type }).to_string(),
    );
    Ok(Value::String(id))
}

pub(crate) async fn db_list_objective_events(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let objective_id = str_field(&args, "objectiveId")?;
    let events = state.db.list_objective_events_internal(&objective_id).await?;
    serde_json::to_value(events).map_err(MonarchError::from)
}

pub(crate) async fn db_update_objective_manual(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let payload: crate::db::ManualObjectiveUpdatePayload =
        serde_json::from_value(args.get("payload").cloned().unwrap_or(args.clone()))
            .map_err(|e| MonarchError::invalid_input(format!("Invalid payload: {}", e)))?;
    let id = payload.id.clone();
    let before = state.db.get_objective_internal(&id).await?;
    let notes = state.db.update_objective_manual_internal(&payload).await?;
    let after = state.db.get_objective_internal(&id).await?;
    let app = state.agent_mgr.get_app_handle()?;
    crate::db::emit_objective_updated_notifications(
        &app,
        &state.agent_mgr.ws_broadcast,
        &id,
        after.as_ref(),
    );
    crate::db::emit_plan_notifications(&app, &state.agent_mgr.ws_broadcast, notes);
    crate::db::handle_objective_update_side_effects(
        &app,
        &state.db,
        &state.agent_mgr,
        before,
        after,
    )
    .await?;
    Ok(Value::Null)
}

pub(crate) async fn db_record_manual_objective_event(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let payload: crate::db::ManualObjectiveEventPayload =
        serde_json::from_value(args.get("payload").cloned().unwrap_or(args.clone()))
            .map_err(|e| MonarchError::invalid_input(format!("Invalid payload: {}", e)))?;
    let objective_id = payload.objective_id.clone();
    let event_type = payload.event_type.clone();
    let id = state
        .db
        .record_manual_objective_event_internal(&payload)
        .await?;
    let app = state.agent_mgr.get_app_handle()?;
    crate::agent::emit_event(
        &app,
        &state.agent_mgr.ws_broadcast,
        &format!("objective-event-{}", objective_id),
        &serde_json::json!({ "id": id, "eventType": event_type }).to_string(),
    );
    Ok(Value::String(id))
}

pub(crate) async fn db_list_objective_refs(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let objective_id = str_field(&args, "objectiveId")?;
    let refs = state.db.list_objective_refs_internal(&objective_id).await?;
    serde_json::to_value(refs).map_err(MonarchError::from)
}

pub(crate) async fn db_create_objective_ref(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let payload: crate::db::CreateObjectiveRefPayload =
        serde_json::from_value(args.get("payload").cloned().unwrap_or(args.clone()))
            .map_err(|e| MonarchError::invalid_input(format!("Invalid payload: {}", e)))?;
    let objective_id = payload.objective_id.clone();
    let id = state.db.create_objective_ref_internal(&payload).await?;
    let app = state.agent_mgr.get_app_handle()?;
    crate::db::emit_objective_ref_notification(
        &app,
        &state.agent_mgr.ws_broadcast,
        &objective_id,
        "created",
        &id,
    );
    Ok(Value::String(id))
}

pub(crate) async fn db_update_objective_ref(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let payload: crate::db::UpdateObjectiveRefPayload =
        serde_json::from_value(args.get("payload").cloned().unwrap_or(args.clone()))
            .map_err(|e| MonarchError::invalid_input(format!("Invalid payload: {}", e)))?;
    let id = payload.id.clone();
    let before = state.db.get_objective_ref_internal(&id).await?;
    state.db.update_objective_ref_internal(&payload).await?;
    if let Some(row) = before {
        let app = state.agent_mgr.get_app_handle()?;
        crate::db::emit_objective_ref_notification(
            &app,
            &state.agent_mgr.ws_broadcast,
            &row.objective_id,
            "updated",
            &id,
        );
    }
    Ok(Value::Null)
}

pub(crate) async fn db_delete_objective_ref(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let ref_id = str_field(&args, "refId")?;
    let before = state.db.get_objective_ref_internal(&ref_id).await?;
    state.db.delete_objective_ref_internal(&ref_id).await?;
    if let Some(row) = before {
        let app = state.agent_mgr.get_app_handle()?;
        crate::db::emit_objective_ref_notification(
            &app,
            &state.agent_mgr.ws_broadcast,
            &row.objective_id,
            "deleted",
            &ref_id,
        );
    }
    Ok(Value::Null)
}

pub(crate) async fn db_save_objective_report(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let payload: crate::db::WriteObjectiveReportPayload =
        serde_json::from_value(args.get("payload").cloned().unwrap_or(args.clone()))
            .map_err(|e| MonarchError::invalid_input(format!("Invalid payload: {}", e)))?;
    let objective_id = payload.objective_id.clone();
    let id = state.db.upsert_objective_report_internal(&payload).await?;
    let app = state.agent_mgr.get_app_handle()?;
    crate::db::emit_objective_report_notification(
        &app,
        &state.agent_mgr.ws_broadcast,
        &objective_id,
        "saved",
        &id,
    );
    Ok(Value::String(id))
}

pub(crate) async fn db_get_objective_report(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let objective_id = str_field(&args, "objectiveId")?;
    let report = state.db.get_objective_report_by_objective_internal(&objective_id).await?;
    serde_json::to_value(report).map_err(MonarchError::from)
}

pub(crate) async fn db_list_objective_reports_for_agent(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let agent_id = str_field(&args, "agentId")?;
    let reports = state
        .db
        .list_objective_reports_for_agent_internal(&agent_id)
        .await?;
    serde_json::to_value(reports).map_err(MonarchError::from)
}

pub(crate) async fn db_get_working_memory(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let agent_id = str_field(&args, "agentId")?;
    let wm = state.db.get_working_memory_internal(&agent_id).await?;
    serde_json::to_value(wm).map_err(MonarchError::from)
}

// MON-82: Classifications (read-only over WS).

pub(crate) async fn db_list_classifications_for_agent(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let agent_id = str_field(&args, "agentId")?;
    let limit = args.get("limit").and_then(|v| v.as_i64());
    let rows = state
        .db
        .list_classifications_for_agent_internal(&agent_id, limit)
        .await?;
    serde_json::to_value(rows).map_err(MonarchError::from)
}

pub(crate) async fn db_get_classification_for_message(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let message_id = args
        .get("messageId")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| MonarchError::invalid_input("messageId required"))?;
    let row = state
        .db
        .get_classification_for_message_internal(message_id)
        .await?;
    serde_json::to_value(row).map_err(MonarchError::from)
}

