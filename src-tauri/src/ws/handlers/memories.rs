use serde_json::Value;

use crate::error::MonarchError;
use crate::ws::WsState;
use super::str_field;

// ---- DB: Memories (MON-99) ----

pub(crate) async fn db_list_memories_for_agent(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let agent_id = str_field(&args, "agentId")?;
    let memories = state.db.list_memories_for_agent_internal(&agent_id).await?;
    serde_json::to_value(memories).map_err(MonarchError::from)
}

pub(crate) async fn db_get_memory(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let id: i64 = args
        .get("id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| MonarchError::invalid_input("missing id"))?;
    let memory = state.db.get_memory_internal(id).await?;
    serde_json::to_value(memory).map_err(MonarchError::from)
}

pub(crate) async fn memory_search_for_agent(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let agent_id = str_field(&args, "agentId")?;
    let query = str_field(&args, "query")?;
    let top_k = args.get("topK").and_then(|v| v.as_u64()).map(|v| v as u32);
    let results = crate::memory_search::search_memories_for_agent_internal(
        &state.db,
        &state.memory_index,
        &agent_id,
        &query,
        top_k,
    )
    .await?;
    serde_json::to_value(results).map_err(MonarchError::from)
}

// ---- MON-99: Memory config ----

pub(crate) async fn memory_get_config(_state: &WsState, _args: Value) -> Result<Value, MonarchError> {
    let cfg = crate::memory_config::resolved().await;
    serde_json::to_value(cfg).map_err(MonarchError::from)
}

pub(crate) async fn memory_set_config(_state: &WsState, args: Value) -> Result<Value, MonarchError> {
    let raw: crate::memory_config::MemoryConfig =
        serde_json::from_value(args).map_err(|e| {
            MonarchError::invalid_input(format!("Invalid memory config: {}", e))
        })?;
    let resolved = crate::memory_config::resolve(raw.clone());
    crate::memory_config::write_raw_ws(&raw).await?;
    serde_json::to_value(resolved).map_err(MonarchError::from)
}

pub(crate) async fn memory_get_config_path(_state: &WsState, _args: Value) -> Result<Value, MonarchError> {
    let path = crate::memory_config::config_path_ws()?;
    Ok(Value::String(path))
}

pub(crate) async fn memory_index_status(state: &WsState, _args: Value) -> Result<Value, MonarchError> {
    Ok(Value::Bool(state.memory_index.is_initialized()))
}

pub(crate) async fn memory_download_and_init(state: &WsState, _args: Value) -> Result<Value, MonarchError> {
    state.memory_index.ensure_model_downloaded().await?;
    state.memory_index.init_embedder().await?;
    Ok(Value::Null)
}

pub(crate) async fn memory_smoke_insert(state: &WsState, args: Value) -> Result<Value, MonarchError> {
    if !cfg!(debug_assertions) {
        return Err(MonarchError::persistence(
            "memory_smoke_insert is only available in debug builds",
        ));
    }
    let agent_id = str_field(&args, "agentId")?;
    let title = str_field(&args, "title")?;
    let content = str_field(&args, "content")?;
    let cfg = crate::memory_config::resolved().await;
    let text = format!("{title}\n\n{content}");
    let embedding = state.memory_index.embed_to_blob(&text).await?;
    let payload = crate::db::InsertMemoryPayload {
        agent_id: Some(agent_id.clone()),
        scope: "self".to_string(),
        project_id: None,
        parent_id: None,
        layer: "leaf".to_string(),
        kind: Some("claim".to_string()),
        title: title.clone(),
        summary: title,
        content: Some(content),
        source_quest_id: None,
        source_session_id: None,
        source_events: None,
        file_refs: None,
        supersedes_id: None,
    };
    let new_id = state
        .db
        .insert_memory_internal(payload, Some(embedding), Some(cfg.embedding_model_id))
        .await?;
    let pairs = state
        .db
        .load_embeddings_for_agent_internal(&agent_id)
        .await?;
    state.memory_index.rebuild(pairs).await?;
    Ok(Value::Number(new_id.into()))
}
