use serde_json::Value;

use crate::error::MonarchError;
use crate::websocket::WsState;
use super::{str_field, opt_str};

// ---- DB: Projects ----

pub(crate) async fn db_upsert_project(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let project =
        serde_json::from_value(args.get("project").cloned().unwrap_or(args.clone()))
            .map_err(|e| MonarchError::invalid_input(format!("Invalid project: {}", e)))?;
    state.db.upsert_project_internal(&project).await?;
    Ok(Value::Null)
}

pub(crate) async fn db_get_projects(state: &WsState, _args: Value) -> Result<Value, MonarchError> {
    let projects = state.db.get_projects_internal().await?;
    serde_json::to_value(projects).map_err(MonarchError::from)
}

pub(crate) async fn db_get_project_by_path(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let root_path = str_field(&args, "rootPath")?;
    let project = state.db.get_project_by_path_internal(&root_path).await?;
    serde_json::to_value(project).map_err(MonarchError::from)
}

pub(crate) async fn db_rename_project(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let project_id = str_field(&args, "projectId")?;
    let name = str_field(&args, "name")?;
    state.db.rename_project_internal(&project_id, &name).await?;
    Ok(Value::Null)
}

pub(crate) async fn db_update_project_instructions(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let project_id = str_field(&args, "projectId")?;
    let instructions = opt_str(&args, "instructions");
    state
        .db
        .update_project_instructions_internal(&project_id, instructions.as_deref())
        .await?;
    Ok(Value::Null)
}

pub(crate) async fn db_delete_project(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let project_id = str_field(&args, "projectId")?;
    state.db.delete_project_internal(&project_id).await?;
    Ok(Value::Null)
}
