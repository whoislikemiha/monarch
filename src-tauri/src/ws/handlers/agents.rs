use serde_json::Value;

use crate::error::MonarchError;
use crate::ws::WsState;
use super::{str_field, opt_str};

// ---- Agent lifecycle ----

pub(crate) async fn spawn_agent(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    // MON-35: single-shot typed decode. `SpawnAgentRequest` is the
    // shared wire contract between the Tauri command and the WS
    // bridge, so the serde round-trip validates the payload instead
    // of per-field `str_field` / `opt_str` extraction.
    let req: crate::agent::SpawnAgentRequest = serde_json::from_value(args)?;
    let app = state.agent_mgr.get_app_handle()?;
    state.agent_mgr.spawn(&app, &state.db, req).await?;
    Ok(Value::Null)
}

pub(crate) async fn send_command(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let id = str_field(&args, "id")?;
    let command_json = str_field(&args, "commandJson")?;
    let app = state.agent_mgr.get_app_handle()?;
    state
        .agent_mgr
        .send_command(&app, &state.db, id, command_json)
        .await?;
    Ok(Value::Null)
}

pub(crate) async fn kill_agent(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let id = str_field(&args, "id")?;
    state.agent_mgr.kill(&id).await?;
    Ok(Value::Null)
}

pub(crate) async fn load_session_context(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let agent_id = str_field(&args, "agentId")?;
    let source_session_id = str_field(&args, "sourceSessionId")?;
    let app = state.agent_mgr.get_app_handle()?;
    state
        .agent_mgr
        .load_session_context(&app, &state.db, agent_id, source_session_id)
        .await?;
    Ok(Value::Null)
}

pub(crate) async fn new_agent_session(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let agent_id = str_field(&args, "agentId")?;
    let new_session_id = str_field(&args, "newSessionId")?;
    let parent_session_id = opt_str(&args, "parentSessionId");
    let app = state.agent_mgr.get_app_handle()?;
    state
        .agent_mgr
        .new_session(&app, &state.db, agent_id, new_session_id, parent_session_id)
        .await?;
    Ok(Value::Null)
}

pub(crate) async fn switch_agent_session(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let agent_id = str_field(&args, "agentId")?;
    let session_id = str_field(&args, "sessionId")?;
    let app = state.agent_mgr.get_app_handle()?;
    state
        .agent_mgr
        .switch_session(&app, &state.db, agent_id, session_id)
        .await?;
    Ok(Value::Null)
}

pub(crate) async fn respond_extension_ui(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let req: crate::agent::ExtensionUiResponseRequest = serde_json::from_value(args)?;
    let app = state.agent_mgr.get_app_handle()?;
    state
        .agent_mgr
        .respond_extension_ui(&app, &state.db, req)
        .await?;
    Ok(Value::Null)
}

// ---- DB: Agents ----

pub(crate) async fn db_upsert_agent(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let agent = serde_json::from_value(args.get("agent").cloned().unwrap_or(args.clone()))
        .map_err(|e| MonarchError::invalid_input(format!("Invalid agent: {}", e)))?;
    state.db.upsert_agent_internal(&agent).await?;
    Ok(Value::Null)
}

pub(crate) async fn db_update_agent(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let payload =
        serde_json::from_value(args.get("payload").cloned().unwrap_or(args.clone()))
            .map_err(|e| MonarchError::invalid_input(format!("Invalid payload: {}", e)))?;
    state.db.update_agent_internal(&payload).await?;
    Ok(Value::Null)
}

pub(crate) async fn db_get_agents(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let include_archived = args
        .get("includeArchived")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let agents = state.db.get_agents_internal(include_archived).await?;
    serde_json::to_value(agents).map_err(MonarchError::from)
}

pub(crate) async fn db_archive_agent(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let agent_id = str_field(&args, "agentId")?;
    state.db.archive_agent_internal(&agent_id).await?;
    Ok(Value::Null)
}

pub(crate) async fn db_unarchive_agent(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let agent_id = str_field(&args, "agentId")?;
    state.db.unarchive_agent_internal(&agent_id).await?;
    Ok(Value::Null)
}

pub(crate) async fn db_delete_agent(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let agent_id = str_field(&args, "agentId")?;
    state.db.delete_agent_internal(&agent_id).await?;
    Ok(Value::Null)
}

// ---- MON-98: Captain / shadow identity ----

pub(crate) async fn get_captain_identity(state: &WsState, _args: Value) -> Result<Value, MonarchError> {
    let row = state.db.get_captain_identity_internal().await?;
    serde_json::to_value(row).map_err(MonarchError::from)
}

pub(crate) async fn upsert_captain_identity(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let req: crate::agent::commands::UpsertCaptainIdentityRequest =
        serde_json::from_value(args)
            .map_err(|e| MonarchError::invalid_input(format!("Invalid request: {}", e)))?;
    state
        .db
        .upsert_captain_identity_internal(&req.name, &req.payload, req.edit_note.as_deref())
        .await?;
    let payload = if req.payload.is_empty() {
        None
    } else {
        Some(req.payload)
    };
    state.agent_mgr.refresh_captain_identity(payload).await?;
    Ok(Value::Null)
}

pub(crate) async fn get_shadow_identity(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let agent_id = str_field(&args, "agentId")?;
    let row = state.db.get_shadow_identity_internal(&agent_id).await?;
    serde_json::to_value(row).map_err(MonarchError::from)
}

pub(crate) async fn upsert_shadow_identity(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let req: crate::agent::commands::UpsertShadowIdentityRequest =
        serde_json::from_value(args)
            .map_err(|e| MonarchError::invalid_input(format!("Invalid request: {}", e)))?;
    state
        .db
        .upsert_shadow_identity_internal(
            &req.agent_id,
            &req.payload,
            req.edit_note.as_deref(),
        )
        .await?;
    let payload = if req.payload.is_empty() {
        None
    } else {
        Some(req.payload)
    };
    state
        .agent_mgr
        .refresh_shadow_identity(&req.agent_id, payload)
        .await?;
    Ok(Value::Null)
}
