use serde_json::Value;

use crate::error::MonarchError;
use crate::websocket::WsState;
use super::str_field;

// ---- DB: Quests (MON-83) ----
// Write commands emit the matching `quest-*-{id}` channel via the
// shared broadcast pipeline so WS subscribers stay in sync without
// a manual refetch.

pub(crate) async fn db_create_quest(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let payload: crate::db::CreateQuestPayload =
        serde_json::from_value(args.get("payload").cloned().unwrap_or(args.clone()))
            .map_err(|e| MonarchError::invalid_input(format!("Invalid payload: {}", e)))?;
    let id = state.db.create_quest_internal(&payload).await?;
    let app = state.agent_mgr.get_app_handle()?;
    crate::agent::emit_event(
        &app,
        &state.agent_mgr.ws_broadcast,
        &format!("quest-created-{}", id),
        &serde_json::json!({ "id": id }).to_string(),
    );
    Ok(Value::String(id))
}

pub(crate) async fn db_update_quest(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let payload: crate::db::UpdateQuestPayload =
        serde_json::from_value(args.get("payload").cloned().unwrap_or(args.clone()))
            .map_err(|e| MonarchError::invalid_input(format!("Invalid payload: {}", e)))?;
    let id = payload.id.clone();
    let before = state.db.get_quest_internal(&id).await?;
    state.db.update_quest_internal(&payload).await?;
    let after = state.db.get_quest_internal(&id).await?;
    let app = state.agent_mgr.get_app_handle()?;
    crate::agent::emit_event(
        &app,
        &state.agent_mgr.ws_broadcast,
        &format!("quest-updated-{}", id),
        &serde_json::json!({ "id": id }).to_string(),
    );
    if let Some(after_quest) = after.as_ref() {
        if after_quest.root_id != after_quest.id {
            crate::agent::emit_event(
                &app,
                &state.agent_mgr.ws_broadcast,
                &format!("quest-updated-{}", after_quest.root_id),
                &serde_json::json!({ "id": after_quest.id, "rootId": after_quest.root_id })
                    .to_string(),
            );
        }
    }
    crate::db::handle_quest_update_side_effects(
        &app,
        &state.db,
        &state.agent_mgr,
        before,
        after,
    )
    .await?;
    Ok(Value::Null)
}

pub(crate) async fn db_get_quest(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let quest_id = str_field(&args, "questId")?;
    let quest = state.db.get_quest_internal(&quest_id).await?;
    serde_json::to_value(quest).map_err(MonarchError::from)
}

pub(crate) async fn db_list_quests_for_agent(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let agent_id = str_field(&args, "agentId")?;
    let quests = state.db.list_quests_for_agent_internal(&agent_id).await?;
    serde_json::to_value(quests).map_err(MonarchError::from)
}

pub(crate) async fn db_get_quest_tree_for_root(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let root_id = str_field(&args, "rootId")?;
    let tree = state.db.get_quest_tree_for_root_internal(&root_id).await?;
    serde_json::to_value(tree).map_err(MonarchError::from)
}

pub(crate) async fn db_record_quest_event(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let payload: crate::db::RecordQuestEventPayload =
        serde_json::from_value(args.get("payload").cloned().unwrap_or(args.clone()))
            .map_err(|e| MonarchError::invalid_input(format!("Invalid payload: {}", e)))?;
    let quest_id = payload.quest_id.clone();
    let event_type = payload.event_type.clone();
    let id = state.db.record_quest_event_internal(&payload).await?;
    let app = state.agent_mgr.get_app_handle()?;
    crate::agent::emit_event(
        &app,
        &state.agent_mgr.ws_broadcast,
        &format!("quest-event-{}", quest_id),
        &serde_json::json!({ "id": id, "eventType": event_type }).to_string(),
    );
    Ok(Value::String(id))
}

pub(crate) async fn db_list_quest_events(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let quest_id = str_field(&args, "questId")?;
    let events = state.db.list_quest_events_internal(&quest_id).await?;
    serde_json::to_value(events).map_err(MonarchError::from)
}

pub(crate) async fn db_update_quest_manual(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let payload: crate::db::ManualQuestUpdatePayload =
        serde_json::from_value(args.get("payload").cloned().unwrap_or(args.clone()))
            .map_err(|e| MonarchError::invalid_input(format!("Invalid payload: {}", e)))?;
    let id = payload.id.clone();
    let before = state.db.get_quest_internal(&id).await?;
    let notes = state.db.update_quest_manual_internal(&payload).await?;
    let after = state.db.get_quest_internal(&id).await?;
    let app = state.agent_mgr.get_app_handle()?;
    crate::db::emit_quest_updated_notifications(
        &app,
        &state.agent_mgr.ws_broadcast,
        &id,
        after.as_ref(),
    );
    crate::db::emit_plan_notifications(&app, &state.agent_mgr.ws_broadcast, notes);
    crate::db::handle_quest_update_side_effects(
        &app,
        &state.db,
        &state.agent_mgr,
        before,
        after,
    )
    .await?;
    Ok(Value::Null)
}

pub(crate) async fn db_record_manual_quest_event(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let payload: crate::db::ManualQuestEventPayload =
        serde_json::from_value(args.get("payload").cloned().unwrap_or(args.clone()))
            .map_err(|e| MonarchError::invalid_input(format!("Invalid payload: {}", e)))?;
    let quest_id = payload.quest_id.clone();
    let event_type = payload.event_type.clone();
    let id = state
        .db
        .record_manual_quest_event_internal(&payload)
        .await?;
    let app = state.agent_mgr.get_app_handle()?;
    crate::agent::emit_event(
        &app,
        &state.agent_mgr.ws_broadcast,
        &format!("quest-event-{}", quest_id),
        &serde_json::json!({ "id": id, "eventType": event_type }).to_string(),
    );
    Ok(Value::String(id))
}

pub(crate) async fn db_list_quest_refs(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let quest_id = str_field(&args, "questId")?;
    let refs = state.db.list_quest_refs_internal(&quest_id).await?;
    serde_json::to_value(refs).map_err(MonarchError::from)
}

pub(crate) async fn db_create_quest_ref(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let payload: crate::db::CreateQuestRefPayload =
        serde_json::from_value(args.get("payload").cloned().unwrap_or(args.clone()))
            .map_err(|e| MonarchError::invalid_input(format!("Invalid payload: {}", e)))?;
    let quest_id = payload.quest_id.clone();
    let id = state.db.create_quest_ref_internal(&payload).await?;
    let app = state.agent_mgr.get_app_handle()?;
    crate::db::emit_quest_ref_notification(
        &app,
        &state.agent_mgr.ws_broadcast,
        &quest_id,
        "created",
        &id,
    );
    Ok(Value::String(id))
}

pub(crate) async fn db_update_quest_ref(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let payload: crate::db::UpdateQuestRefPayload =
        serde_json::from_value(args.get("payload").cloned().unwrap_or(args.clone()))
            .map_err(|e| MonarchError::invalid_input(format!("Invalid payload: {}", e)))?;
    let id = payload.id.clone();
    let before = state.db.get_quest_ref_internal(&id).await?;
    state.db.update_quest_ref_internal(&payload).await?;
    if let Some(row) = before {
        let app = state.agent_mgr.get_app_handle()?;
        crate::db::emit_quest_ref_notification(
            &app,
            &state.agent_mgr.ws_broadcast,
            &row.quest_id,
            "updated",
            &id,
        );
    }
    Ok(Value::Null)
}

pub(crate) async fn db_delete_quest_ref(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let ref_id = str_field(&args, "refId")?;
    let before = state.db.get_quest_ref_internal(&ref_id).await?;
    state.db.delete_quest_ref_internal(&ref_id).await?;
    if let Some(row) = before {
        let app = state.agent_mgr.get_app_handle()?;
        crate::db::emit_quest_ref_notification(
            &app,
            &state.agent_mgr.ws_broadcast,
            &row.quest_id,
            "deleted",
            &ref_id,
        );
    }
    Ok(Value::Null)
}

pub(crate) async fn db_save_quest_report(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let payload: crate::db::WriteQuestReportPayload =
        serde_json::from_value(args.get("payload").cloned().unwrap_or(args.clone()))
            .map_err(|e| MonarchError::invalid_input(format!("Invalid payload: {}", e)))?;
    let quest_id = payload.quest_id.clone();
    let id = state.db.upsert_quest_report_internal(&payload).await?;
    let app = state.agent_mgr.get_app_handle()?;
    crate::db::emit_quest_report_notification(
        &app,
        &state.agent_mgr.ws_broadcast,
        &quest_id,
        "saved",
        &id,
    );
    Ok(Value::String(id))
}

pub(crate) async fn db_get_quest_report(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let quest_id = str_field(&args, "questId")?;
    let report = state.db.get_quest_report_by_quest_internal(&quest_id).await?;
    serde_json::to_value(report).map_err(MonarchError::from)
}

pub(crate) async fn db_list_quest_reports_for_agent(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let agent_id = str_field(&args, "agentId")?;
    let reports = state
        .db
        .list_quest_reports_for_agent_internal(&agent_id)
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

