use serde_json::Value;

use crate::error::MonarchError;
use crate::websocket::WsState;
use super::{str_field, opt_str};

// ---- Models ----

pub(crate) async fn get_models(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let provider = str_field(&args, "provider")?;
    let force_refresh = args
        .get("forceRefresh")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let models =
        crate::models::ws_get_models(&state.model_cache, provider, force_refresh).await?;
    serde_json::to_value(models).map_err(MonarchError::from)
}

pub(crate) async fn get_provider_auth_status(_state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let provider = str_field(&args, "provider")?;
    let status = crate::models::ws_get_provider_auth_status(provider)?;
    serde_json::to_value(status).map_err(MonarchError::from)
}

// ---- Persistence (prompts) ----

pub(crate) async fn get_agent_prompt(_state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let agent_id = str_field(&args, "agentId")?;
    let result = crate::persistence::read_agent_prompt_file(&agent_id).await?;
    Ok(result.map(Value::String).unwrap_or(Value::Null))
}

pub(crate) async fn save_agent_prompt(_state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let agent_id = str_field(&args, "agentId")?;
    let prompt = str_field(&args, "prompt")?;
    crate::persistence::write_agent_prompt_file(&agent_id, &prompt).await?;
    Ok(Value::Null)
}

pub(crate) async fn get_prompts_dir(_state: &WsState, _args: Value) -> Result<Value, MonarchError> {
    Ok(Value::String(
        crate::persistence::prompts_dir_string().await?,
    ))
}

// ---- DB: Events ----

pub(crate) async fn db_log_event(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let agent_id = opt_str(&args, "agentId");
    let session_id = opt_str(&args, "sessionId");
    let event_type = str_field(&args, "eventType")?;
    let data = opt_str(&args, "data");
    state
        .db
        .log_event_internal(
            agent_id.as_deref(),
            session_id.as_deref(),
            &event_type,
            data.as_deref(),
        )
        .await?;
    Ok(Value::Null)
}

// ---- DB: Templates ----

pub(crate) async fn db_list_agent_templates(state: &WsState, _args: Value) -> Result<Value, MonarchError> {
    let templates = state.db.list_agent_templates_internal().await?;
    serde_json::to_value(templates).map_err(MonarchError::from)
}

pub(crate) async fn db_save_agent_template(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let template =
        serde_json::from_value(args.get("template").cloned().unwrap_or(args.clone()))
            .map_err(|e| MonarchError::invalid_input(format!("Invalid template: {}", e)))?;
    state.db.save_agent_template_internal(&template).await?;
    Ok(Value::Null)
}

pub(crate) async fn db_delete_agent_template(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let template_id = str_field(&args, "templateId")?;
    state
        .db
        .delete_agent_template_internal(&template_id)
        .await?;
    Ok(Value::Null)
}

// ---- Toolbox ----

pub(crate) async fn toolbox_list_tools(_state: &WsState, _args: Value) -> Result<Value, MonarchError> {
    let tools = crate::toolbox::ws_toolbox_list_tools();
    serde_json::to_value(tools).map_err(MonarchError::from)
}

pub(crate) async fn toolbox_placeholder_ping(_state: &WsState, _args: Value) -> Result<Value, MonarchError> {
    let result = crate::toolbox::placeholder::ws_toolbox_placeholder_ping()?;
    Ok(Value::String(result))
}

// ---- Project / path helpers ----

pub(crate) async fn detect_project(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let cwd = str_field(&args, "cwd")?;
    let result = crate::project::detect_project(&state.db, &cwd).await?;
    Ok(result.unwrap_or(Value::Null))
}

pub(crate) async fn read_project_instructions(_state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let cwd = str_field(&args, "cwd")?;
    let result = crate::project::read_project_instructions(&cwd);
    Ok(result.map(Value::String).unwrap_or(Value::Null))
}

pub(crate) async fn list_paths(_state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let cwd = str_field(&args, "cwd")?;
    let query = str_field(&args, "query")?;
    let result =
        tokio::task::spawn_blocking(move || crate::ui::mention::list_paths_inner(&cwd, &query))
            .await
            .map_err(|e| {
                MonarchError::persistence(format!("list_paths join error: {e}"))
            })??;
    serde_json::to_value(result).map_err(MonarchError::from)
}

// ---- MON-82: classifier config (global) ----

pub(crate) async fn classifier_get_config(_state: &WsState, _args: Value) -> Result<Value, MonarchError> {
    let cfg = crate::config::classifier::resolved().await;
    serde_json::to_value(cfg).map_err(MonarchError::from)
}

pub(crate) async fn classifier_set_config(_state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let config: crate::config::classifier::ClassifierConfig =
        serde_json::from_value(args.get("config").cloned().unwrap_or(Value::Null))
            .map_err(|e| MonarchError::invalid_input(format!("Invalid config: {}", e)))?;
    crate::config::classifier::write_raw(&config).await?;
    serde_json::to_value(crate::config::classifier::resolve(config)).map_err(MonarchError::from)
}

pub(crate) async fn classifier_get_config_path(_state: &WsState, _args: Value) -> Result<Value, MonarchError> {
    let path = crate::config::classifier::config_path()?;
    Ok(Value::String(path.to_string_lossy().to_string()))
}
