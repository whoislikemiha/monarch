use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::MonarchError;

use super::Database;

// ---- Row types ----

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ObjectiveReportRow {
    pub id: String,
    pub objective_id: String,
    pub agent_id: Option<String>,
    pub payload: String,
    pub created_at: String,
    pub updated_at: String,
    pub distilled_by_keeper_run_id: Option<i64>,
}

/// MON-119: payload for upserting a objective report. `agent_id` is omitted —
/// the write helper resolves it from `objective_nodes.assignee_shadow_id`.
/// `payload` is opaque JSON in Slice A; Slice B's sidecar tool defines the
/// structured shape.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WriteObjectiveReportPayload {
    #[serde(default)]
    pub id: Option<String>,
    pub objective_id: String,
    pub payload: String,
}

// ---- Row mappers ----

fn map_objective_report(row: &Row<'_>) -> rusqlite::Result<ObjectiveReportRow> {
    Ok(ObjectiveReportRow {
        id: row.get(0)?,
        objective_id: row.get(1)?,
        agent_id: row.get(2)?,
        payload: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        distilled_by_keeper_run_id: row.get(6)?,
    })
}

// ---- impl Database ----

impl Database {
    pub async fn upsert_objective_report_internal(
        &self,
        payload: &WriteObjectiveReportPayload,
    ) -> Result<String, MonarchError> {
        if payload.objective_id.trim().is_empty() {
            return Err(MonarchError::invalid_input("objectiveId required"));
        }
        if payload.payload.trim().is_empty() {
            return Err(MonarchError::invalid_input("payload required"));
        }
        let payload = payload.clone();
        let provided_id = payload
            .id
            .clone()
            .unwrap_or_else(crate::util::uuid_v4_simple);
        let now = crate::util::chrono_now();
        Ok(self
            .conn
            .call(move |conn| {
                let agent_id: Option<String> = conn
                    .query_row(
                        "SELECT assignee_shadow_id FROM objective_nodes WHERE id = ?1",
                        params![payload.objective_id],
                        |row| row.get(0),
                    )
                    .map_err(|e| match e {
                        rusqlite::Error::QueryReturnedNoRows => {
                            rusqlite::Error::SqliteFailure(
                                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CONSTRAINT),
                                Some(format!("objective_nodes row not found for {}", payload.objective_id)),
                            )
                        }
                        other => other,
                    })?;
                // Try INSERT first; on UNIQUE(objective_id) conflict, update the
                // existing row's payload/agent_id/updated_at and return its id.
                let inserted = conn.execute(
                    "INSERT INTO objective_reports (
                        id, objective_id, agent_id, payload, created_at, updated_at
                     )
                     VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                     ON CONFLICT(objective_id) DO UPDATE SET
                        payload = excluded.payload,
                        agent_id = excluded.agent_id,
                        updated_at = excluded.updated_at",
                    params![
                        provided_id,
                        payload.objective_id,
                        agent_id,
                        payload.payload,
                        now,
                    ],
                )?;
                debug_assert!(inserted == 1);
                let id: String = conn.query_row(
                    "SELECT id FROM objective_reports WHERE objective_id = ?1",
                    params![payload.objective_id],
                    |row| row.get(0),
                )?;
                Ok(id)
            })
            .await?)
    }

    /// MON-119: fetch the single report for a objective (or None).
    pub async fn get_objective_report_by_objective_internal(
        &self,
        objective_id: &str,
    ) -> Result<Option<ObjectiveReportRow>, MonarchError> {
        let objective_id = objective_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, objective_id, agent_id, payload, created_at, updated_at,
                            distilled_by_keeper_run_id
                     FROM objective_reports WHERE objective_id = ?1",
                )?;
                let mut rows = stmt.query(params![objective_id])?;
                if let Some(row) = rows.next()? {
                    Ok(Some(map_objective_report(row)?))
                } else {
                    Ok(None)
                }
            })
            .await?)
    }

    /// MON-119: list every report written by a specific agent, newest first.
    /// Justifies the denormalized `agent_id` column — a JOIN through
    /// `objective_nodes` would be slower and stop working after agent archive.
    pub async fn list_objective_reports_for_agent_internal(
        &self,
        agent_id: &str,
    ) -> Result<Vec<ObjectiveReportRow>, MonarchError> {
        let agent_id = agent_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, objective_id, agent_id, payload, created_at, updated_at,
                            distilled_by_keeper_run_id
                     FROM objective_reports WHERE agent_id = ?1
                     ORDER BY created_at DESC",
                )?;
                let rows = stmt
                    .query_map(params![agent_id], map_objective_report)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await?)
    }

    /// MON-122: P6 Slice D — attribute a objective's report to the Keeper run
    /// that distilled it. Returns `true` if a report row was updated, `false`
    /// when no report exists for the objective. Idempotent on the
    /// `(objective_id, run_id)` pair — re-running the Keeper for the same objective
    /// simply rewrites the attribution.
    pub async fn attribute_objective_report_to_keeper_run_internal(
        &self,
        objective_id: &str,
        run_id: i64,
    ) -> Result<bool, MonarchError> {
        let objective_id = objective_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let n = conn.execute(
                    "UPDATE objective_reports SET distilled_by_keeper_run_id = ?1
                     WHERE objective_id = ?2",
                    params![run_id, objective_id],
                )?;
                Ok(n > 0)
            })
            .await?)
    }
}

// ---- MON-119: P6 Slice A — first-person objective reports ----
//
// Captain-initiated saves go through `db_save_objective_report` and write
// directly via `upsert_objective_report_internal` (matching the
// `db_create_objective_ref` precedent). Sidecar-originated writes (Slice B)
// flow through `PersistCommand::WriteObjectiveReport` instead so they preserve
// ordering against surrounding objective events. Both paths emit on
// `objective-report-{objective_id}` so the captain UI (Slice C) can subscribe
// once and see writes regardless of origin.

#[tauri::command]
#[specta::specta]
pub async fn db_save_objective_report(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    payload: WriteObjectiveReportPayload,
) -> Result<String, MonarchError> {
    let objective_id = payload.objective_id.clone();
    let id = db.upsert_objective_report_internal(&payload).await?;
    emit_objective_report_notification(&app, &agent_mgr.ws_broadcast, &objective_id, "saved", &id);
    Ok(id)
}

#[tauri::command]
#[specta::specta]
pub async fn db_get_objective_report(
    db: tauri::State<'_, Arc<Database>>,
    objective_id: String,
) -> Result<Option<ObjectiveReportRow>, MonarchError> {
    db.get_objective_report_by_objective_internal(&objective_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_list_objective_reports_for_agent(
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
) -> Result<Vec<ObjectiveReportRow>, MonarchError> {
    db.list_objective_reports_for_agent_internal(&agent_id).await
}

pub fn emit_objective_report_notification(
    app: &tauri::AppHandle,
    ws_tx: &tokio::sync::broadcast::Sender<crate::agent::WsBroadcast>,
    objective_id: &str,
    action: &str,
    report_id: &str,
) {
    crate::agent::emit_event(
        app,
        ws_tx,
        &format!("objective-report-{}", objective_id),
        &serde_json::json!({ "id": report_id, "objectiveId": objective_id, "action": action }).to_string(),
    );
}
