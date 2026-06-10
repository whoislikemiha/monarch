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
