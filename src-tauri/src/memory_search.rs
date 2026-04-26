//! MON-101: hybrid memory retrieval for user-turn prompt injection.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::db::{Database, MemoryRow};
use crate::error::MonarchError;
use crate::memory_index::MemoryIndex;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MemorySearchResult {
    pub memory: MemoryRow,
    /// `fts`, `vector`, or `hybrid`.
    pub source: String,
    pub fts_rank: Option<f64>,
    /// 1-based position from the vector index result list.
    pub vector_rank: Option<i32>,
}

pub async fn search_memories_for_agent_internal(
    db: &Arc<Database>,
    index: &Arc<MemoryIndex>,
    agent_id: &str,
    query: &str,
    top_k: Option<u32>,
) -> Result<Vec<MemorySearchResult>, MonarchError> {
    let query = query.trim();
    if query.is_empty() || !index.is_initialized() {
        return Ok(vec![]);
    }

    let cfg = crate::memory_config::resolved().await;
    let limit = top_k.unwrap_or(cfg.top_k).max(1).min(20) as usize;

    let fts_query = fts_match_query(query);
    let fts_hits = if fts_query.is_empty() {
        Vec::new()
    } else {
        db.fts_search_memories_internal(agent_id, &fts_query, limit as i64)
            .await
            .unwrap_or_default()
    };
    let vector_ids = index.query(query, limit).await.unwrap_or_default();

    let mut fts_by_id: HashMap<i64, f64> = HashMap::new();
    let mut fts_order: Vec<i64> = Vec::new();
    for hit in fts_hits {
        fts_by_id.entry(hit.memory_id).or_insert(hit.rank);
        if !fts_order.contains(&hit.memory_id) {
            fts_order.push(hit.memory_id);
        }
    }

    let mut vector_rank_by_id: HashMap<i64, i32> = HashMap::new();
    let mut vector_order: Vec<i64> = Vec::new();
    for (idx, id) in vector_ids.into_iter().enumerate() {
        vector_rank_by_id.entry(id).or_insert((idx + 1) as i32);
        if !vector_order.contains(&id) {
            vector_order.push(id);
        }
    }

    let vector_set: HashSet<i64> = vector_order.iter().copied().collect();
    let mut ordered_ids: Vec<i64> = Vec::new();

    for id in &fts_order {
        if vector_set.contains(id) {
            ordered_ids.push(*id);
        }
    }
    for id in &fts_order {
        if !ordered_ids.contains(id) {
            ordered_ids.push(*id);
        }
    }
    for id in &vector_order {
        if !ordered_ids.contains(id) {
            ordered_ids.push(*id);
        }
    }
    ordered_ids.truncate(limit);

    let mut out = Vec::new();
    for id in ordered_ids {
        let Some(memory) = db.get_memory_internal(id).await? else {
            continue;
        };
        if memory.archived_at.is_some() || memory.agent_id.as_deref() != Some(agent_id) {
            continue;
        }
        let fts_rank = fts_by_id.get(&id).copied();
        let vector_rank = vector_rank_by_id.get(&id).copied();
        let source = match (fts_rank.is_some(), vector_rank.is_some()) {
            (true, true) => "hybrid",
            (true, false) => "fts",
            (false, true) => "vector",
            (false, false) => "unknown",
        }
        .to_string();
        out.push(MemorySearchResult {
            memory,
            source,
            fts_rank,
            vector_rank,
        });
    }

    let accessed: Vec<i64> = out.iter().map(|r| r.memory.id).collect();
    db.mark_memories_accessed_internal(accessed).await?;

    Ok(out)
}

#[tauri::command]
#[specta::specta]
pub async fn memory_search_for_agent(
    db: tauri::State<'_, Arc<Database>>,
    index: tauri::State<'_, Arc<MemoryIndex>>,
    agent_id: String,
    query: String,
    top_k: Option<u32>,
) -> Result<Vec<MemorySearchResult>, MonarchError> {
    search_memories_for_agent_internal(&db, &index, &agent_id, &query, top_k).await
}

fn fts_match_query(query: &str) -> String {
    query
        .split(|c: char| !c.is_alphanumeric())
        .map(str::trim)
        .filter(|s| s.len() >= 2)
        .take(12)
        .collect::<Vec<_>>()
        .join(" OR ")
}
