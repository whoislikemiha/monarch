//! MON-99 (Slice A): Debug-only smoke-test Tauri command for the memory
//! substrate. Lets the captain insert a memory by hand from devtools to
//! exercise embed → DB insert → HNSW rebuild end-to-end before Slice B's
//! Keeper writes the first real one.
//!
//! Gated at runtime via `cfg!(debug_assertions)` rather than `#[cfg(...)]`
//! on the function so the signature is always compiled and the generated
//! `bindings.ts` stays stable across debug and release builds. Release
//! builds short-circuit with an error and never touch the DB or the index.
//!
//! D2 (locked in `thoughts/plan/MON-99.md`): kept permanently for repro.

use std::sync::Arc;
use tauri::State;

use crate::db::{Database, InsertMemoryPayload};
use crate::error::MonarchError;
use crate::memory::config;
use crate::memory::index::MemoryIndex;

#[tauri::command]
#[specta::specta]
pub async fn memory_smoke_insert(
    db: State<'_, Arc<Database>>,
    index: State<'_, Arc<MemoryIndex>>,
    agent_id: String,
    title: String,
    content: String,
) -> Result<i64, MonarchError> {
    if !cfg!(debug_assertions) {
        return Err(MonarchError::persistence(
            "memory_smoke_insert is only available in debug builds",
        ));
    }

    let cfg = config::resolved().await;
    let text = format!("{title}\n\n{content}");
    let embedding = index.embed_to_blob(&text).await?;

    let payload = InsertMemoryPayload {
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

    let new_id = db
        .insert_memory_internal(payload, Some(embedding), Some(cfg.embedding_model_id))
        .await?;

    let pairs = db.load_embeddings_for_agent_internal(&agent_id).await?;
    index.rebuild(pairs).await?;

    Ok(new_id)
}
