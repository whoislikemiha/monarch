use serde_json::Value;

use crate::error::MonarchError;
use crate::websocket::WsState;
use super::{str_field, opt_str};

pub(crate) fn emit_plan_notifications(
    state: &WsState,
    notes: Vec<crate::db::QuestEventNotification>,
) -> Result<(), MonarchError> {
    let app = state.agent_mgr.get_app_handle()?;
    for note in notes {
        crate::agent::emit_event(
            &app,
            &state.agent_mgr.ws_broadcast,
            &format!("quest-event-{}", note.quest_id),
            &serde_json::json!({ "id": note.event_id, "eventType": note.event_type }).to_string(),
        );
    }
    Ok(())
}

pub(crate) async fn db_list_plan_items(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let quest_id = str_field(&args, "questId")?;
    let items = state.db.list_plan_items_internal(&quest_id).await?;
    serde_json::to_value(items).map_err(MonarchError::from)
}

pub(crate) async fn db_get_plan_item(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let item_id = str_field(&args, "itemId")?;
    let item = state.db.get_plan_item_internal(&item_id).await?;
    serde_json::to_value(item).map_err(MonarchError::from)
}

pub(crate) async fn db_set_plan(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let payload: crate::db::SetPlanPayload =
        serde_json::from_value(args.get("payload").cloned().unwrap_or(args.clone()))
            .map_err(|e| MonarchError::invalid_input(format!("Invalid payload: {}", e)))?;
    let notes = state.db.set_plan_internal(&payload).await?;
    emit_plan_notifications(state, notes)?;
    Ok(Value::Null)
}

pub(crate) async fn db_add_plan_item(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let payload: crate::db::AddPlanItemPayload =
        serde_json::from_value(args.get("payload").cloned().unwrap_or(args.clone()))
            .map_err(|e| MonarchError::invalid_input(format!("Invalid payload: {}", e)))?;
    let (id, notes) = state.db.add_plan_item_internal(&payload).await?;
    emit_plan_notifications(state, notes)?;
    Ok(Value::String(id))
}

pub(crate) async fn db_update_plan_item(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let payload: crate::db::UpdatePlanItemPayload =
        serde_json::from_value(args.get("payload").cloned().unwrap_or(args.clone()))
            .map_err(|e| MonarchError::invalid_input(format!("Invalid payload: {}", e)))?;
    let notes = state.db.update_plan_item_internal(&payload).await?;
    emit_plan_notifications(state, notes)?;
    Ok(Value::Null)
}

pub(crate) async fn db_delete_plan_item(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let item_id = str_field(&args, "itemId")?;
    let notes = state.db.delete_plan_item_internal(&item_id).await?;
    emit_plan_notifications(state, notes)?;
    Ok(Value::Null)
}

pub(crate) async fn db_start_plan_item(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let item_id = str_field(&args, "itemId")?;
    let notes = state.db.start_plan_item_internal(&item_id).await?;
    emit_plan_notifications(state, notes)?;
    Ok(Value::Null)
}

pub(crate) async fn db_complete_plan_item(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let item_id = str_field(&args, "itemId")?;
    let outcome = opt_str(&args, "outcome");
    let notes = state
        .db
        .complete_plan_item_internal(&item_id, outcome.as_deref())
        .await?;
    emit_plan_notifications(state, notes)?;
    Ok(Value::Null)
}

pub(crate) async fn db_skip_plan_item(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let item_id = str_field(&args, "itemId")?;
    let reason = opt_str(&args, "reason");
    let notes = state
        .db
        .skip_plan_item_internal(&item_id, reason.as_deref())
        .await?;
    emit_plan_notifications(state, notes)?;
    Ok(Value::Null)
}

pub(crate) async fn db_block_plan_item(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let item_id = str_field(&args, "itemId")?;
    let reason = str_field(&args, "reason")?;
    let notes = state.db.block_plan_item_internal(&item_id, &reason).await?;
    emit_plan_notifications(state, notes)?;
    Ok(Value::Null)
}
