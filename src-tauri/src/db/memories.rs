use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::MonarchError;

use super::Database;

// ---- Row types ----

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
/// MON-99: P2 memory row returned to frontend / used for retrieval.
pub struct MemoryRow {
    pub id: i64,
    pub agent_id: Option<String>,
    pub scope: String,
    pub project_id: Option<String>,
    pub parent_id: Option<i64>,
    pub layer: String,
    pub kind: Option<String>,
    pub title: String,
    pub summary: String,
    pub content: Option<String>,
    pub manual_override: bool,
    pub source_objective_id: Option<String>,
    pub source_session_id: Option<String>,
    pub source_events: Option<String>,
    pub file_refs: Option<String>,
    /// Embedding stored as raw little-endian f32 bytes (not serialized to frontend).
    #[serde(skip)]
    pub embedding: Option<Vec<u8>>,
    pub embedding_model_id: Option<String>,
    pub supersedes_id: Option<i64>,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub last_accessed_at: Option<String>,
    pub access_count: i32,
}

/// Payload for inserting a new memory. Does not include id, created_at, access_count.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct InsertMemoryPayload {
    pub agent_id: Option<String>,
    pub scope: String,
    pub project_id: Option<String>,
    pub parent_id: Option<i64>,
    pub layer: String,
    pub kind: Option<String>,
    pub title: String,
    pub summary: String,
    pub content: Option<String>,
    pub source_objective_id: Option<String>,
    pub source_session_id: Option<String>,
    pub source_events: Option<String>,
    pub file_refs: Option<String>,
    pub supersedes_id: Option<i64>,
}

/// MON-99: Row returned from `memory_keeper_runs`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KeeperRunRow {
    pub id: i64,
    pub agent_id: String,
    pub trigger: String,
    pub objective_id: Option<String>,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub tokens_input: Option<i64>,
    pub tokens_output: Option<i64>,
    pub model_id: Option<String>,
    pub output_summary: Option<String>,
    pub outcome: String,
}

/// MON-99: FTS5 search result — memory id + snippet.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FtsMemoryResult {
    pub memory_id: i64,
    pub rank: f64,
}

// ---- Row mappers ----

pub(super) fn map_memory(row: &Row<'_>) -> rusqlite::Result<MemoryRow> {
    Ok(MemoryRow {
        id: row.get(0)?,
        agent_id: row.get(1)?,
        scope: row
            .get::<_, Option<String>>(2)?
            .unwrap_or_else(|| "self".into()),
        project_id: row.get(3)?,
        parent_id: row.get(4)?,
        layer: row
            .get::<_, Option<String>>(5)?
            .unwrap_or_else(|| "leaf".into()),
        kind: row.get(6)?,
        title: row.get::<_, Option<String>>(7)?.unwrap_or_default(),
        summary: row.get::<_, Option<String>>(8)?.unwrap_or_default(),
        content: row.get(9)?,
        manual_override: row.get::<_, i64>(10).unwrap_or(0) != 0,
        source_objective_id: row.get(11)?,
        source_session_id: row.get(12)?,
        source_events: row.get(13)?,
        file_refs: row.get(14)?,
        embedding: None,
        embedding_model_id: row.get(15)?,
        supersedes_id: row.get(16)?,
        archived_at: row.get(17)?,
        created_at: row.get(18)?,
        last_accessed_at: row.get(19)?,
        access_count: row.get(20)?,
    })
}

pub(super) fn map_keeper_run(row: &Row<'_>) -> rusqlite::Result<KeeperRunRow> {
    Ok(KeeperRunRow {
        id: row.get(0)?,
        agent_id: row.get(1)?,
        trigger: row.get(2)?,
        objective_id: row.get(3)?,
        started_at: row.get(4)?,
        completed_at: row.get(5)?,
        tokens_input: row.get(6)?,
        tokens_output: row.get(7)?,
        model_id: row.get(8)?,
        output_summary: row.get(9)?,
        outcome: row.get(10)?,
    })
}

// ---- impl Database ----

impl Database {
    /// MON-99: Insert a new memory claim. Returns the new row id.
    pub async fn insert_memory_internal(
        &self,
        payload: InsertMemoryPayload,
        embedding: Option<Vec<u8>>,
        embedding_model_id: Option<String>,
    ) -> Result<i64, MonarchError> {
        Ok(self
            .conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO memories (
                        agent_id, scope, project_id, parent_id, layer, kind,
                        title, summary, content, source_objective_id, source_session_id,
                        source_events, file_refs, embedding, embedding_model_id,
                        supersedes_id
                    ) VALUES (
                        ?1, ?2, ?3, ?4, ?5, ?6,
                        ?7, ?8, ?9, ?10, ?11,
                        ?12, ?13, ?14, ?15,
                        ?16
                    )",
                    params![
                        payload.agent_id,
                        payload.scope,
                        payload.project_id,
                        payload.parent_id,
                        payload.layer,
                        payload.kind,
                        payload.title,
                        payload.summary,
                        payload.content,
                        payload.source_objective_id,
                        payload.source_session_id,
                        payload.source_events,
                        payload.file_refs,
                        embedding,
                        embedding_model_id,
                        payload.supersedes_id,
                    ],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await?)
    }

    /// MON-99: List memories for an agent (agent-scoped, non-archived), ordered newest-first.
    pub async fn list_memories_for_agent_internal(
        &self,
        agent_id: &str,
    ) -> Result<Vec<MemoryRow>, MonarchError> {
        let agent_id = agent_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, agent_id, scope, project_id, parent_id, layer, kind,
                        title, summary, content, manual_override, source_objective_id,
                        source_session_id, source_events, file_refs, embedding_model_id,
                        supersedes_id, archived_at, created_at, last_accessed_at, access_count
                     FROM memories
                     WHERE agent_id = ?1 AND archived_at IS NULL
                     ORDER BY created_at DESC",
                )?;
                let rows = stmt
                    .query_map(params![agent_id], map_memory)?
                    .collect::<rusqlite::Result<Vec<_>>>();
                rows
            })
            .await?)
    }

    /// MON-99: Get a single memory by id.
    pub async fn get_memory_internal(&self, id: i64) -> Result<Option<MemoryRow>, MonarchError> {
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, agent_id, scope, project_id, parent_id, layer, kind,
                        title, summary, content, manual_override, source_objective_id,
                        source_session_id, source_events, file_refs, embedding_model_id,
                        supersedes_id, archived_at, created_at, last_accessed_at, access_count
                     FROM memories WHERE id = ?1",
                )?;
                let mut rows = stmt.query_map(params![id], map_memory)?;
                if let Some(row) = rows.next() {
                    Ok(Some(row?))
                } else {
                    Ok(None)
                }
            })
            .await?)
    }

    /// MON-99: FTS5 search — returns (memory_id, rank) ordered by relevance.
    pub async fn fts_search_memories_internal(
        &self,
        agent_id: &str,
        query: &str,
        limit: i64,
    ) -> Result<Vec<FtsMemoryResult>, MonarchError> {
        let agent_id = agent_id.to_string();
        let query = query.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                // FTS5 may not be available on all builds; return empty on error.
                let mut stmt = match conn.prepare(
                    "SELECT m.id, f.rank FROM memories_fts f
                     JOIN memories m ON m.id = f.rowid
                     WHERE memories_fts MATCH ?1 AND m.agent_id = ?2 AND m.archived_at IS NULL
                     ORDER BY f.rank LIMIT ?3",
                ) {
                    Ok(s) => s,
                    Err(_) => return Ok(vec![]),
                };
                let rows = stmt
                    .query_map(params![query, agent_id, limit], |row| {
                        Ok(FtsMemoryResult {
                            memory_id: row.get(0)?,
                            rank: row.get(1)?,
                        })
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>();
                rows
            })
            .await?)
    }

    /// MON-101: mark retrieved memories as accessed. Best-effort callers may
    /// pass an empty list; archived/missing rows are naturally ignored.
    pub async fn mark_memories_accessed_internal(
        &self,
        memory_ids: Vec<i64>,
    ) -> Result<(), MonarchError> {
        if memory_ids.is_empty() {
            return Ok(());
        }
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.transaction()?;
                for id in memory_ids {
                    tx.execute(
                        "UPDATE memories
                         SET access_count = access_count + 1,
                             last_accessed_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
                         WHERE id = ?1 AND archived_at IS NULL",
                        params![id],
                    )?;
                }
                tx.commit()?;
                Ok(())
            })
            .await?)
    }

    /// MON-99: Load embedding BLOBs for all non-archived memories of an agent.
    /// Returns (id, embedding_bytes) pairs for HNSW index rebuild.
    pub async fn load_embeddings_for_agent_internal(
        &self,
        agent_id: &str,
    ) -> Result<Vec<(i64, Vec<u8>)>, MonarchError> {
        let agent_id = agent_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, embedding FROM memories
                     WHERE agent_id = ?1 AND archived_at IS NULL AND embedding IS NOT NULL",
                )?;
                let rows = stmt
                    .query_map(params![agent_id], |row| {
                        Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>();
                rows
            })
            .await?)
    }

    /// MON-99: Insert a Curator run provenance row. Returns the new id.
    pub async fn insert_keeper_run_internal(
        &self,
        agent_id: &str,
        trigger: &str,
        objective_id: Option<&str>,
        model_id: &str,
    ) -> Result<i64, MonarchError> {
        let agent_id = agent_id.to_string();
        let trigger = trigger.to_string();
        let objective_id = objective_id.map(|s| s.to_string());
        let model_id = model_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO memory_keeper_runs (agent_id, trigger, objective_id, model_id, outcome)
                     VALUES (?1, ?2, ?3, ?4, 'pending')",
                    params![agent_id, trigger, objective_id, model_id],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await?)
    }

    /// MON-100: Most recent successful Curator run for an agent, or None.
    /// Drives slice anchoring (we replay messages newer than this row's
    /// `completed_at`) and the synthesized scaffold's prior summary (its
    /// `output_summary`).
    pub async fn last_successful_keeper_run_internal(
        &self,
        agent_id: &str,
    ) -> Result<Option<KeeperRunRow>, MonarchError> {
        let agent_id = agent_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, agent_id, trigger, objective_id, started_at, completed_at,
                            tokens_input, tokens_output, model_id, output_summary, outcome
                     FROM memory_keeper_runs
                     WHERE agent_id = ?1 AND outcome = 'ok' AND completed_at IS NOT NULL
                     ORDER BY completed_at DESC LIMIT 1",
                )?;
                let mut rows = stmt.query_map(params![agent_id], map_keeper_run)?;
                if let Some(row) = rows.next() {
                    Ok(Some(row?))
                } else {
                    Ok(None)
                }
            })
            .await?)
    }

    /// MON-103: load one Curator run by id so result persistence can use the
    /// run row's trigger / objective provenance instead of whatever objective happens
    /// to be current when the async model call returns.
    pub async fn get_keeper_run_internal(
        &self,
        run_id: i64,
    ) -> Result<Option<KeeperRunRow>, MonarchError> {
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, agent_id, trigger, objective_id, started_at, completed_at,
                            tokens_input, tokens_output, model_id, output_summary, outcome
                     FROM memory_keeper_runs
                     WHERE id = ?1",
                )?;
                let mut rows = stmt.query_map(params![run_id], map_keeper_run)?;
                if let Some(row) = rows.next() {
                    Ok(Some(row?))
                } else {
                    Ok(None)
                }
            })
            .await?)
    }

    /// MON-99: Mark a Curator run as completed (ok | failed | partial).
    pub async fn complete_keeper_run_internal(
        &self,
        run_id: i64,
        outcome: &str,
        output_summary: Option<String>,
        tokens_input: Option<i64>,
        tokens_output: Option<i64>,
    ) -> Result<(), MonarchError> {
        let outcome = outcome.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE memory_keeper_runs SET
                        outcome = ?1, output_summary = ?2,
                        tokens_input = ?3, tokens_output = ?4,
                        completed_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
                     WHERE id = ?5",
                    params![outcome, output_summary, tokens_input, tokens_output, run_id],
                )?;
                Ok(())
            })
            .await?)
    }

    // Legacy wrapper kept so the old Tauri command still compiles.
    pub async fn get_memories_internal(
        &self,
        agent_id: Option<&str>,
        _layer: Option<&str>,
    ) -> Result<Vec<MemoryRow>, MonarchError> {
        if let Some(id) = agent_id {
            self.list_memories_for_agent_internal(id).await
        } else {
            Ok(vec![])
        }
    }
}

// ---- Tauri Commands: Memories ----

/// MON-99: List all non-archived memories for an agent (Memory Inspector v0).
#[tauri::command]
#[specta::specta]
pub async fn db_list_memories_for_agent(
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
) -> Result<Vec<MemoryRow>, MonarchError> {
    db.list_memories_for_agent_internal(&agent_id).await
}

/// MON-99: Get a single memory by id.
#[tauri::command]
#[specta::specta]
pub async fn db_get_memory(
    db: tauri::State<'_, Arc<Database>>,
    id: i64,
) -> Result<Option<MemoryRow>, MonarchError> {
    db.get_memory_internal(id).await
}
