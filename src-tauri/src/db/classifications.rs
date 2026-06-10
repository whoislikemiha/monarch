use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::MonarchError;

use super::Database;

// ---- MON-82: Classifications ----

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClassificationRow {
    pub id: String,
    pub message_id: Option<i64>,
    pub agent_id: String,
    pub session_id: Option<String>,
    pub complexity: Option<String>,
    pub confidence: Option<f64>,
    pub rationale: Option<String>,
    pub model: Option<String>,
    pub tokens_in: Option<i32>,
    pub tokens_out: Option<i32>,
    pub latency_ms: Option<i32>,
    pub error: Option<String>,
    pub created_at: String,
}

/// Payload written when the sidecar emits a classification event. The
/// sidecar mints `id` so the frontend can reconcile a pending pill with the
/// resolved row (and so backfill can link by exact id, not "latest
/// unlinked"). `message_id` is always `None` at insert time — the user
/// message row doesn't exist yet; `backfill_classification_message_id`
/// fills it when Pi emits user `MessageEnd`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveClassificationPayload {
    pub id: String,
    pub agent_id: String,
    pub session_id: Option<String>,
    pub complexity: Option<String>,
    pub confidence: Option<f64>,
    pub rationale: Option<String>,
    pub model: Option<String>,
    pub tokens_in: Option<i32>,
    pub tokens_out: Option<i32>,
    pub latency_ms: Option<i32>,
    pub error: Option<String>,
}

// MON-82: Classifications. Shared column list; callers append the WHERE
// clause they need.
const CLASSIFICATION_BASE_SELECT: &str = "SELECT \
    id, message_id, agent_id, session_id, complexity, confidence, rationale, \
    model, tokens_in, tokens_out, latency_ms, error, created_at \
    FROM classifications";

// ---- Row mappers ----

fn map_classification(row: &Row<'_>) -> rusqlite::Result<ClassificationRow> {
    Ok(ClassificationRow {
        id: row.get(0)?,
        message_id: row.get(1)?,
        agent_id: row.get(2)?,
        session_id: row.get(3)?,
        complexity: row.get(4)?,
        confidence: row.get(5)?,
        rationale: row.get(6)?,
        model: row.get(7)?,
        tokens_in: row.get(8)?,
        tokens_out: row.get(9)?,
        latency_ms: row.get(10)?,
        error: row.get(11)?,
        created_at: row.get(12)?,
    })
}

// ---- impl Database ----

impl Database {
    // ---- MON-82: Classifications ----

    /// Insert a classification row. `message_id` is always null at insert
    /// time (see `SaveClassificationPayload`); `backfill_classification_message_id`
    /// sets it later when the user message row lands.
    pub async fn save_classification_internal(
        &self,
        payload: &SaveClassificationPayload,
    ) -> Result<(), MonarchError> {
        let payload = payload.clone();
        let now = crate::util::chrono_now();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO classifications (
                        id, message_id, agent_id, session_id, complexity,
                        confidence, rationale, model, tokens_in, tokens_out,
                        latency_ms, error, created_at
                    ) VALUES (
                        ?1, NULL, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12
                    )",
                    params![
                        payload.id,
                        payload.agent_id,
                        payload.session_id,
                        payload.complexity,
                        payload.confidence,
                        payload.rationale,
                        payload.model,
                        payload.tokens_in,
                        payload.tokens_out,
                        payload.latency_ms,
                        payload.error,
                        now,
                    ],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    /// Link an existing classification row to a persisted user message.
    /// Returns the number of rows updated — `0` means the classification
    /// row isn't in the table yet (classifier still in flight); the caller
    /// should stash the mapping and re-apply on `SaveClassification`.
    pub async fn backfill_classification_message_id(
        &self,
        classification_id: &str,
        message_id: i64,
    ) -> Result<usize, MonarchError> {
        let classification_id = classification_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let rows = conn.execute(
                    "UPDATE classifications SET message_id = ?1 WHERE id = ?2",
                    params![message_id, classification_id],
                )?;
                Ok(rows)
            })
            .await?)
    }

    pub async fn list_classifications_for_agent_internal(
        &self,
        agent_id: &str,
        limit: Option<i64>,
    ) -> Result<Vec<ClassificationRow>, MonarchError> {
        let agent_id = agent_id.to_string();
        let limit = limit.unwrap_or(200);
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(&format!(
                    "{} WHERE agent_id = ?1 ORDER BY created_at DESC LIMIT ?2",
                    CLASSIFICATION_BASE_SELECT
                ))?;
                let rows = stmt
                    .query_map(params![agent_id, limit], map_classification)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await?)
    }

    pub async fn get_classification_for_message_internal(
        &self,
        message_id: i64,
    ) -> Result<Option<ClassificationRow>, MonarchError> {
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(&format!(
                    "{} WHERE message_id = ?1 ORDER BY created_at DESC LIMIT 1",
                    CLASSIFICATION_BASE_SELECT
                ))?;
                let mut rows = stmt.query(params![message_id])?;
                if let Some(row) = rows.next()? {
                    Ok(Some(map_classification(row)?))
                } else {
                    Ok(None)
                }
            })
            .await?)
    }
}

// ---- MON-82: Classifications ----
//
// Writes are sidecar-originated and flow through the MON-37 persistence
// pipeline (see `agent/persist.rs`); no Tauri `db_save_classification`
// command is exposed. The commands below are read-only helpers for the
// frontend (e.g. the classifier settings tool / future analytics).

#[tauri::command]
#[specta::specta]
pub async fn db_list_classifications_for_agent(
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
    limit: Option<i64>,
) -> Result<Vec<ClassificationRow>, MonarchError> {
    db.list_classifications_for_agent_internal(&agent_id, limit)
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn db_get_classification_for_message(
    db: tauri::State<'_, Arc<Database>>,
    message_id: i64,
) -> Result<Option<ClassificationRow>, MonarchError> {
    db.get_classification_for_message_internal(message_id).await
}
