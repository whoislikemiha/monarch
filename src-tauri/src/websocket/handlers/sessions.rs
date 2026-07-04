use serde_json::Value;

use crate::error::MonarchError;
use crate::websocket::WsState;
use super::str_field;

// ---- DB: Sessions ----

pub(crate) async fn db_create_session(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let session =
        serde_json::from_value(args.get("session").cloned().unwrap_or(args.clone()))
            .map_err(|e| MonarchError::invalid_input(format!("Invalid session: {}", e)))?;
    state.db.create_session_internal(&session).await?;
    Ok(Value::Null)
}

pub(crate) async fn db_get_sessions(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let agent_id = str_field(&args, "agentId")?;
    let sessions = state.db.get_sessions_internal(&agent_id).await?;
    serde_json::to_value(sessions).map_err(MonarchError::from)
}

/// MON-127: per-agent session list with titles + previews.
pub(crate) async fn db_list_session_summaries(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let agent_id = str_field(&args, "agentId")?;
    let summaries = state.db.list_session_summaries_internal(&agent_id).await?;
    serde_json::to_value(summaries).map_err(MonarchError::from)
}

/// MON-127: rename a session (null title clears it).
pub(crate) async fn db_set_session_title(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let session_id = str_field(&args, "sessionId")?;
    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    state
        .db
        .set_session_title_internal(&session_id, title.as_deref())
        .await?;
    Ok(Value::Null)
}

/// MON-127: read-only display items for one session (no ancestry).
pub(crate) async fn get_session_display_items(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let session_id = str_field(&args, "sessionId")?;
    let messages = state.db.get_messages_internal(&session_id).await?;
    let items: Vec<crate::agent::state::DisplayItem> = if messages.is_empty() {
        Vec::new()
    } else {
        crate::agent::state::display_items_from_messages(&messages, "")
            .into_iter()
            .filter(|i| !matches!(i, crate::agent::state::DisplayItem::Status { .. }))
            .collect()
    };
    serde_json::to_value(items).map_err(MonarchError::from)
}

// ---- DB: Messages ----

pub(crate) async fn db_save_message(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let message =
        serde_json::from_value(args.get("message").cloned().unwrap_or(args.clone()))
            .map_err(|e| MonarchError::invalid_input(format!("Invalid message: {}", e)))?;
    let id = state.db.save_message_internal(&message).await?;
    Ok(Value::Number(id.into()))
}

pub(crate) async fn db_get_messages(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let session_id = str_field(&args, "sessionId")?;
    let messages = state.db.get_messages_internal(&session_id).await?;
    serde_json::to_value(messages).map_err(MonarchError::from)
}

pub(crate) async fn db_get_messages_with_ancestry(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let session_id = str_field(&args, "sessionId")?;
    let messages = state.db.get_messages_with_ancestry(&session_id).await?;
    serde_json::to_value(messages).map_err(MonarchError::from)
}

/// MON-130: full tool input/output for one timeline tool row.
pub(crate) async fn db_get_tool_call_detail(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let tool_call_id = str_field(&args, "toolCallId")?;
    let detail = state.db.get_tool_call_detail_internal(&tool_call_id).await?;
    serde_json::to_value(detail).map_err(MonarchError::from)
}
