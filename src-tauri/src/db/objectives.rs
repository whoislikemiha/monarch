use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use crate::error::MonarchError;

use super::Database;

// ---- Row types ----

// MON-83: Objective system row types. Enums (status/grade/exec_hint/created_by)
// are stored as strings matching the CHECK constraints in the schema. See
// plans/objectives.md for the full design.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ObjectiveRow {
    pub id: String,
    pub root_id: String,
    pub parent_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub scope: Option<String>,
    pub current_direction: Option<String>,
    pub rationale: Option<String>,
    pub status: String,
    pub grade: Option<String>,
    pub exec_hint: Option<String>,
    pub explore_fork_count: Option<i32>,
    pub assignee_shadow_id: Option<String>,
    pub fork_parent_id: Option<String>,
    pub worktree_path: Option<String>,
    pub branch_name: Option<String>,
    pub base_branch: Option<String>,
    pub branched_from_id: Option<String>,
    pub superseded_by_id: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub abandoned_at: Option<String>,
    pub estimated_tokens: Option<i32>,
    pub actual_tokens: Option<i32>,
    pub estimated_duration_ms: Option<i64>,
    pub actual_duration_ms: Option<i64>,
    pub summary: Option<String>,
    /// P1: `'campaign'` for the single per-project root container, `'objective'`
    /// for all real work. The campaign root is never closed/graded/reported.
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ObjectiveEventRow {
    pub id: String,
    pub objective_id: String,
    pub event_type: String,
    pub actor: Option<String>,
    pub payload_json: Option<String>,
    pub created_at: String,
    pub parent_event_id: Option<String>,
    pub author: Option<String>,
    pub surface_override: Option<String>,
    pub payload_schema_version: i32,
    pub plan_item_id: Option<String>,
}

/// Payload for `db_create_objective`. `id` is optional — server generates a
/// UUID if omitted. Defaults: `status='pending'`, `grade='C'`,
/// `exec_hint='in_context'`, `created_by='monarch'`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateObjectivePayload {
    pub id: Option<String>,
    pub parent_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub status: Option<String>,
    pub grade: Option<String>,
    pub exec_hint: Option<String>,
    pub assignee_shadow_id: Option<String>,
    pub created_by: Option<String>,
    /// P1: defaults to `'objective'`. Set `'campaign'` only for a project root.
    pub kind: Option<String>,
}

/// Payload for `db_update_objective`. Only non-`None` fields are written.
/// Lifecycle timestamps (`started_at` / `completed_at` / `abandoned_at`)
/// can be set explicitly by the caller; the Steward owns this in Slice 4+.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateObjectivePayload {
    pub id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub scope: Option<String>,
    pub current_direction: Option<String>,
    pub rationale: Option<String>,
    pub fork_parent_id: Option<String>,
    pub status: Option<String>,
    pub grade: Option<String>,
    pub exec_hint: Option<String>,
    pub assignee_shadow_id: Option<String>,
    pub summary: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub abandoned_at: Option<String>,
}

/// P5 manual editor payload. This narrower path records semantic timeline
/// events for objective-level changes; generic `db_update_objective` remains available
/// for older callers that only need a direct row patch.
#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ManualObjectiveUpdatePayload {
    pub id: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub current_direction: Option<String>,
    #[serde(default)]
    pub rationale: Option<String>,
    #[serde(default)]
    pub grade: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub change_rationale: Option<String>,
    #[serde(default)]
    pub actor: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ObjectiveRefRow {
    pub id: String,
    pub objective_id: String,
    pub ref_type: String,
    pub label: Option<String>,
    pub target: String,
    pub metadata_json: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateObjectiveRefPayload {
    #[serde(default)]
    pub id: Option<String>,
    pub objective_id: String,
    pub ref_type: String,
    #[serde(default)]
    pub label: Option<String>,
    pub target: String,
    #[serde(default)]
    pub metadata_json: Option<String>,
    #[serde(default)]
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateObjectiveRefPayload {
    pub id: String,
    #[serde(default)]
    pub ref_type: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ManualObjectiveEventPayload {
    pub objective_id: String,
    pub event_type: String,
    pub text: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub metadata_json: Option<String>,
    #[serde(default)]
    pub actor: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub surface_override: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RecordObjectiveEventPayload {
    pub objective_id: String,
    pub event_type: String,
    pub actor: Option<String>,
    pub payload_json: Option<String>,
    #[serde(default)]
    pub parent_event_id: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub surface_override: Option<String>,
    #[serde(default)]
    pub payload_schema_version: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct ObjectiveEventNotification {
    pub objective_id: String,
    pub event_id: String,
    pub event_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkingMemoryCurrentAction {
    pub event_id: String,
    pub objective_id: String,
    pub intent: String,
    pub started_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkingMemoryRecentAction {
    pub event_id: String,
    pub objective_id: String,
    pub intent: String,
    pub outcome: String,
    pub completed_at: String,
    #[serde(default)]
    pub auto_closed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkingMemoryPayload {
    pub schema_version: i32,
    pub current_objective_id: Option<String>,
    pub current_objective_path: Vec<String>,
    pub current_action: Option<WorkingMemoryCurrentAction>,
    pub recent_actions: Vec<WorkingMemoryRecentAction>,
    pub updated_at: String,
    // P4b: plan slice. Pointers into `objective_plan_items` for the active
    // objective. Defaults preserve forward compatibility with v1 rows — old
    // payloads deserialize cleanly with both fields empty.
    #[serde(default)]
    pub active_plan_item_id: Option<String>,
    #[serde(default)]
    pub next_plan_item_ids: Vec<String>,
}

// Shared column list for objective_nodes SELECTs. `OBJECTIVE_SELECT_SQL` is the
// single-row lookup by id; `OBJECTIVE_BASE_SELECT` is the prefix for filtered
// list queries (no WHERE clause).
pub(super) const OBJECTIVE_BASE_SELECT: &str = "SELECT \
    id, root_id, parent_id, title, description, scope, current_direction, \
    rationale, status, grade, exec_hint, explore_fork_count, assignee_shadow_id, \
    fork_parent_id, worktree_path, branch_name, \
    base_branch, branched_from_id, superseded_by_id, created_by, created_at, \
    started_at, completed_at, abandoned_at, estimated_tokens, actual_tokens, \
    estimated_duration_ms, actual_duration_ms, summary, kind FROM objective_nodes";

pub(super) const OBJECTIVE_SELECT_SQL: &str = "SELECT \
    id, root_id, parent_id, title, description, scope, current_direction, \
    rationale, status, grade, exec_hint, explore_fork_count, assignee_shadow_id, \
    fork_parent_id, worktree_path, branch_name, \
    base_branch, branched_from_id, superseded_by_id, created_by, created_at, \
    started_at, completed_at, abandoned_at, estimated_tokens, actual_tokens, \
    estimated_duration_ms, actual_duration_ms, summary, kind \
    FROM objective_nodes WHERE id = ?1";

// ---- Row mappers ----

pub(super) fn map_objective(row: &Row<'_>) -> rusqlite::Result<ObjectiveRow> {
    Ok(ObjectiveRow {
        id: row.get(0)?,
        root_id: row.get(1)?,
        parent_id: row.get(2)?,
        title: row.get(3)?,
        description: row.get(4)?,
        scope: row.get(5)?,
        current_direction: row.get(6)?,
        rationale: row.get(7)?,
        status: row.get(8)?,
        grade: row.get(9)?,
        exec_hint: row.get(10)?,
        explore_fork_count: row.get(11)?,
        assignee_shadow_id: row.get(12)?,
        fork_parent_id: row.get(13)?,
        worktree_path: row.get(14)?,
        branch_name: row.get(15)?,
        base_branch: row.get(16)?,
        branched_from_id: row.get(17)?,
        superseded_by_id: row.get(18)?,
        created_by: row.get(19)?,
        created_at: row.get(20)?,
        started_at: row.get(21)?,
        completed_at: row.get(22)?,
        abandoned_at: row.get(23)?,
        estimated_tokens: row.get(24)?,
        actual_tokens: row.get(25)?,
        estimated_duration_ms: row.get(26)?,
        actual_duration_ms: row.get(27)?,
        summary: row.get(28)?,
        kind: row.get(29)?,
    })
}

pub(super) fn map_objective_ref(row: &Row<'_>) -> rusqlite::Result<ObjectiveRefRow> {
    Ok(ObjectiveRefRow {
        id: row.get(0)?,
        objective_id: row.get(1)?,
        ref_type: row.get(2)?,
        label: row.get(3)?,
        target: row.get(4)?,
        metadata_json: row.get(5)?,
        created_by: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

pub(super) fn map_objective_event(row: &Row<'_>) -> rusqlite::Result<ObjectiveEventRow> {
    Ok(ObjectiveEventRow {
        id: row.get(0)?,
        objective_id: row.get(1)?,
        event_type: row.get(2)?,
        actor: row.get(3)?,
        payload_json: row.get(4)?,
        created_at: row.get(5)?,
        parent_event_id: row.get(6)?,
        author: row.get(7)?,
        surface_override: row.get(8)?,
        payload_schema_version: row.get(9)?,
        plan_item_id: row.get(10)?,
    })
}

// ---- Working memory helpers ----

pub(super) fn empty_working_memory(current_objective_id: &str, now: &str) -> WorkingMemoryPayload {
    WorkingMemoryPayload {
        schema_version: 2,
        current_objective_id: Some(current_objective_id.to_string()),
        current_objective_path: Vec::new(),
        current_action: None,
        recent_actions: Vec::new(),
        updated_at: now.to_string(),
        active_plan_item_id: None,
        next_plan_item_ids: Vec::new(),
    }
}

pub(super) fn load_working_memory_tx(
    tx: &rusqlite::Transaction<'_>,
    agent_id: &str,
) -> Option<WorkingMemoryPayload> {
    let payload: String = tx
        .query_row(
            "SELECT payload_json FROM agent_working_memory WHERE agent_id = ?1",
            params![agent_id],
            |row| row.get(0),
        )
        .ok()?;
    serde_json::from_str(&payload).ok()
}

pub(super) fn save_working_memory_tx(
    tx: &rusqlite::Transaction<'_>,
    agent_id: &str,
    wm: &WorkingMemoryPayload,
) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO agent_working_memory (agent_id, payload_json, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(agent_id) DO UPDATE SET
            payload_json = excluded.payload_json,
            updated_at = excluded.updated_at",
        params![
            agent_id,
            serde_json::to_string(wm).unwrap_or_default(),
            wm.updated_at
        ],
    )?;
    Ok(())
}

pub(super) fn objective_path_tx(tx: &rusqlite::Transaction<'_>, objective_id: &str) -> Vec<String> {
    let mut path = Vec::new();
    let mut current = Some(objective_id.to_string());
    while let Some(id) = current {
        let row: Option<(String, String, Option<String>)> = tx
            .query_row(
                "SELECT title, kind, parent_id FROM objective_nodes WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .ok();
        let Some((title, kind, parent_id)) = row else {
            break;
        };
        // P1: the campaign root is a container, not a work step — keep it out
        // of the breadcrumb path (still walk through it for deeper trees).
        if kind != "campaign" {
            path.push(title);
        }
        current = parent_id;
    }
    path.reverse();
    path
}

#[allow(clippy::too_many_arguments)]
pub(super) fn close_action_tx(
    tx: &rusqlite::Transaction<'_>,
    agent_id: &str,
    wm: &mut WorkingMemoryPayload,
    current: &WorkingMemoryCurrentAction,
    outcome: &str,
    auto_closed: bool,
    auto_closed_reason: Option<String>,
    completed_at: &str,
    notes: &mut Vec<ObjectiveEventNotification>,
) -> rusqlite::Result<()> {
    let status = if auto_closed {
        "auto_closed"
    } else {
        "completed"
    };
    let mut parent_payload = serde_json::json!({
        "intent": current.intent,
        "status": status,
        "started_at": current.started_at,
        "completed_at": completed_at,
        "outcome": outcome,
    });
    if let Some(obj) = parent_payload.as_object_mut() {
        if auto_closed {
            obj.insert("auto_closed".to_string(), Value::Bool(true));
        }
        if let Some(reason) = auto_closed_reason.clone() {
            obj.insert("auto_closed_reason".to_string(), Value::String(reason));
        }
    }
    tx.execute(
        "UPDATE objective_events SET payload_json = ?1 WHERE id = ?2",
        params![parent_payload.to_string(), current.event_id],
    )?;

    let outcome_event_id = crate::util::uuid_v4_simple();
    let outcome_payload = serde_json::json!({
        "outcome": outcome,
        "auto_closed": auto_closed,
        "auto_closed_reason": auto_closed_reason,
    })
    .to_string();
    tx.execute(
        "INSERT INTO objective_events (
            id, objective_id, event_type, actor, payload_json, created_at,
            parent_event_id, author, surface_override, payload_schema_version
         )
         VALUES (?1, ?2, 'action_outcome', ?3, ?4, ?5, ?6, 'executor', NULL, 1)",
        params![
            outcome_event_id,
            current.objective_id,
            agent_id,
            outcome_payload,
            completed_at,
            current.event_id
        ],
    )?;

    wm.recent_actions.push(WorkingMemoryRecentAction {
        event_id: current.event_id.clone(),
        objective_id: current.objective_id.clone(),
        intent: current.intent.clone(),
        outcome: outcome.to_string(),
        completed_at: completed_at.to_string(),
        auto_closed: auto_closed.then_some(true),
    });
    if wm.recent_actions.len() > 10 {
        let overflow = wm.recent_actions.len() - 10;
        wm.recent_actions.drain(0..overflow);
    }
    notes.push(ObjectiveEventNotification {
        objective_id: current.objective_id.clone(),
        event_id: current.event_id.clone(),
        event_type: "coherent_action".to_string(),
    });
    notes.push(ObjectiveEventNotification {
        objective_id: current.objective_id.clone(),
        event_id: outcome_event_id,
        event_type: "action_outcome".to_string(),
    });
    Ok(())
}

// ---- impl Database ----

impl Database {
    // ---- MON-83: Objectives ----

    /// Insert a objective node. Uses the payload id if present, otherwise mints a
    /// fresh UUID. `root_id` is resolved from the parent: root objectives have
    /// `root_id = id`; sub-objectives inherit the parent's `root_id`. A
    /// `status_change: null → <status>` event is seeded in the same
    /// transaction so the event log always has a creation entry (Slice 2
    /// read-only UI relies on this for its success criterion).
    pub async fn create_objective_internal(
        &self,
        payload: &CreateObjectivePayload,
    ) -> Result<String, MonarchError> {
        let payload = payload.clone();
        let id = payload
            .id
            .clone()
            .unwrap_or_else(crate::util::uuid_v4_simple);
        let status = payload
            .status
            .clone()
            .unwrap_or_else(|| "pending".to_string());
        let grade = payload.grade.clone().unwrap_or_else(|| "C".to_string());
        let exec_hint = payload
            .exec_hint
            .clone()
            .unwrap_or_else(|| "in_context".to_string());
        let created_by = payload
            .created_by
            .clone()
            .unwrap_or_else(|| "monarch".to_string());
        let kind = payload
            .kind
            .clone()
            .unwrap_or_else(|| "objective".to_string());
        let now = crate::util::chrono_now();
        let event_id = crate::util::uuid_v4_simple();

        let id_for_return = id.clone();
        self.conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                // Resolve root_id: if parent present, inherit its root; else self.
                let root_id: String = if let Some(pid) = payload.parent_id.as_ref() {
                    tx.query_row(
                        "SELECT root_id FROM objective_nodes WHERE id = ?1",
                        params![pid],
                        |row| row.get::<_, String>(0),
                    )?
                } else {
                    id.clone()
                };
                tx.execute(
                    "INSERT INTO objective_nodes (
                        id, root_id, parent_id, title, description,
                        status, grade, exec_hint, assignee_shadow_id,
                        created_by, created_at, kind
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        id,
                        root_id,
                        payload.parent_id,
                        payload.title,
                        payload.description,
                        status,
                        grade,
                        exec_hint,
                        payload.assignee_shadow_id,
                        created_by,
                        now,
                        kind,
                    ],
                )?;
                // Seed the creation event so the event log is never empty.
                let event_payload = serde_json::json!({
                    "from": null,
                    "to": status,
                })
                .to_string();
                tx.execute(
                    "INSERT INTO objective_events (id, objective_id, event_type, actor, payload_json, created_at)
                     VALUES (?1, ?2, 'status_change', ?3, ?4, ?5)",
                    params![event_id, id, created_by, event_payload, now],
                )?;
                tx.commit()?;
                Ok(())
            })
            .await?;
        Ok(id_for_return)
    }

    /// P1: get-or-create a project's single campaign root. The campaign root is
    /// an `objective_nodes` row with `kind='campaign'`, `parent_id=NULL`,
    /// `root_id=self`; it is never closed/graded/reported. Idempotent — returns
    /// the existing root when `projects.root_objective_id` already points at a
    /// live node, otherwise creates one and links it. Atomic via a transaction.
    pub async fn ensure_campaign_root_internal(
        &self,
        project_id: &str,
    ) -> Result<String, MonarchError> {
        let project_id = project_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let id = ensure_campaign_root_tx(&tx, &project_id)?;
                tx.commit()?;
                Ok(id)
            })
            .await?)
    }

    /// P1: the campaign root for an agent's project, if any. Lets the capture
    /// flow place new objectives under the campaign even before the agent has
    /// any assigned work loaded. `None` when the agent has no project or no
    /// campaign root yet.
    pub async fn get_campaign_root_for_agent_internal(
        &self,
        agent_id: &str,
    ) -> Result<Option<ObjectiveRow>, MonarchError> {
        let agent_id = agent_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let sql = format!(
                    "{OBJECTIVE_BASE_SELECT} WHERE id = (
                        SELECT p.root_objective_id FROM agents a
                        JOIN projects p ON p.id = a.project_id
                        WHERE a.id = ?1
                    )"
                );
                conn.query_row(&sql, params![agent_id], map_objective)
                    .optional()
            })
            .await?)
    }

    /// Partial update — only `Some` fields are written. Status / timestamp
    /// changes that carry semantic weight (e.g. status→done) should ALSO
    /// record a `objective_events` row via `record_objective_event_internal`; this
    /// method does not mirror them automatically so the caller keeps full
    /// control over the audit trail.
    pub async fn update_objective_internal(
        &self,
        payload: &UpdateObjectivePayload,
    ) -> Result<(), MonarchError> {
        let payload = payload.clone();
        self.conn
            .call(move |conn| {
                // Build SET clause dynamically. `rusqlite` does not support
                // array-of-params with named columns, so we stringify and
                // push each present field.
                let mut sets: Vec<&str> = Vec::new();
                let mut args: Vec<rusqlite::types::Value> = Vec::new();
                macro_rules! push {
                    ($field:expr, $col:literal) => {
                        if let Some(v) = $field.as_ref() {
                            sets.push(concat!($col, " = ?"));
                            args.push(rusqlite::types::Value::Text(v.clone()));
                        }
                    };
                }
                push!(payload.title, "title");
                push!(payload.description, "description");
                push!(payload.scope, "scope");
                push!(payload.current_direction, "current_direction");
                push!(payload.rationale, "rationale");
                push!(payload.fork_parent_id, "fork_parent_id");
                push!(payload.status, "status");
                push!(payload.grade, "grade");
                push!(payload.exec_hint, "exec_hint");
                push!(payload.assignee_shadow_id, "assignee_shadow_id");
                push!(payload.summary, "summary");
                push!(payload.started_at, "started_at");
                push!(payload.completed_at, "completed_at");
                push!(payload.abandoned_at, "abandoned_at");
                if sets.is_empty() {
                    return Ok(());
                }
                let sql = format!("UPDATE objective_nodes SET {} WHERE id = ?", sets.join(", "));
                args.push(rusqlite::types::Value::Text(payload.id.clone()));
                let params_slice: Vec<&dyn rusqlite::ToSql> =
                    args.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
                conn.execute(&sql, params_slice.as_slice())?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn get_objective_internal(
        &self,
        objective_id: &str,
    ) -> Result<Option<ObjectiveRow>, MonarchError> {
        let objective_id = objective_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(OBJECTIVE_SELECT_SQL)?;
                let mut rows = stmt.query(params![objective_id])?;
                if let Some(row) = rows.next()? {
                    Ok(Some(map_objective(row)?))
                } else {
                    Ok(None)
                }
            })
            .await?)
    }

    /// Every objective where this agent is the assignee, ordered newest-first.
    /// Filter is assignee-only — `agents.current_objective_id` is a pointer into
    /// the tree, not a list key.
    pub async fn list_objectives_for_agent_internal(
        &self,
        agent_id: &str,
    ) -> Result<Vec<ObjectiveRow>, MonarchError> {
        let agent_id = agent_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(&format!(
                    "{} WHERE assignee_shadow_id = ?1 ORDER BY created_at DESC",
                    OBJECTIVE_BASE_SELECT
                ))?;
                let rows = stmt
                    .query_map(params![agent_id], map_objective)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await?)
    }

    /// Full tree under `root_id`, ordered by created_at so a depth-first
    /// reconstruction on the frontend (using parent_id) produces a stable
    /// visual order.
    pub async fn get_objective_tree_for_root_internal(
        &self,
        root_id: &str,
    ) -> Result<Vec<ObjectiveRow>, MonarchError> {
        let root_id = root_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(&format!(
                    "{} WHERE root_id = ?1 ORDER BY created_at ASC",
                    OBJECTIVE_BASE_SELECT
                ))?;
                let rows = stmt
                    .query_map(params![root_id], map_objective)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await?)
    }

    pub async fn record_objective_event_internal(
        &self,
        payload: &RecordObjectiveEventPayload,
    ) -> Result<String, MonarchError> {
        let payload = payload.clone();
        let id = crate::util::uuid_v4_simple();
        let now = crate::util::chrono_now();
        let id_for_return = id.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO objective_events (
                        id, objective_id, event_type, actor, payload_json, created_at,
                        parent_event_id, author, surface_override, payload_schema_version
                     )
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        id,
                        payload.objective_id,
                        payload.event_type,
                        payload.actor,
                        payload.payload_json,
                        now,
                        payload.parent_event_id,
                        payload.author,
                        payload.surface_override,
                        payload.payload_schema_version.unwrap_or(1),
                    ],
                )?;
                Ok(())
            })
            .await?;
        Ok(id_for_return)
    }

    pub async fn list_objective_events_internal(
        &self,
        objective_id: &str,
    ) -> Result<Vec<ObjectiveEventRow>, MonarchError> {
        let objective_id = objective_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, objective_id, event_type, actor, payload_json, created_at,
                            parent_event_id, author, surface_override, payload_schema_version,
                            plan_item_id
                     FROM objective_events WHERE objective_id = ?1 ORDER BY created_at ASC",
                )?;
                let rows = stmt
                    .query_map(params![objective_id], map_objective_event)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await?)
    }

    pub async fn update_objective_manual_internal(
        &self,
        payload: &ManualObjectiveUpdatePayload,
    ) -> Result<Vec<ObjectiveEventNotification>, MonarchError> {
        let payload = payload.clone();
        self.conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let mut stmt = tx.prepare(OBJECTIVE_SELECT_SQL)?;
                let before = stmt.query_row(params![payload.id], map_objective)?;
                drop(stmt);

                let mut sets: Vec<&str> = Vec::new();
                let mut args: Vec<rusqlite::types::Value> = Vec::new();
                macro_rules! push {
                    ($field:expr, $col:literal) => {
                        if let Some(v) = $field.as_ref() {
                            sets.push(concat!($col, " = ?"));
                            args.push(rusqlite::types::Value::Text(v.clone()));
                        }
                    };
                }
                push!(payload.status, "status");
                push!(payload.scope, "scope");
                push!(payload.current_direction, "current_direction");
                push!(payload.rationale, "rationale");
                push!(payload.grade, "grade");
                push!(payload.summary, "summary");
                if !sets.is_empty() {
                    let sql = format!("UPDATE objective_nodes SET {} WHERE id = ?", sets.join(", "));
                    args.push(rusqlite::types::Value::Text(payload.id.clone()));
                    let params_slice: Vec<&dyn rusqlite::ToSql> =
                        args.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
                    tx.execute(&sql, params_slice.as_slice())?;
                }

                let mut stmt = tx.prepare(OBJECTIVE_SELECT_SQL)?;
                let after = stmt.query_row(params![payload.id], map_objective)?;
                drop(stmt);

                let actor = payload.actor.unwrap_or_else(|| "monarch".to_string());
                let author = payload.author.unwrap_or_else(|| "captain".to_string());
                let change_rationale = payload.change_rationale;
                let now = crate::util::chrono_now();
                let mut notes = Vec::new();

                macro_rules! emit_change {
                    ($event_type:literal, $before:expr, $after:expr) => {
                        if $before != $after {
                            let event_id = crate::util::uuid_v4_simple();
                            let event_payload = serde_json::json!({
                                "from": $before,
                                "to": $after,
                                "rationale": change_rationale.clone(),
                            })
                            .to_string();
                            tx.execute(
                                "INSERT INTO objective_events (
                                    id, objective_id, event_type, actor, payload_json, created_at,
                                    parent_event_id, author, surface_override, payload_schema_version
                                 )
                                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, NULL, 1)",
                                params![
                                    event_id,
                                    after.id.clone(),
                                    $event_type,
                                    actor.clone(),
                                    event_payload,
                                    now,
                                    author.clone()
                                ],
                            )?;
                            notes.push(ObjectiveEventNotification {
                                objective_id: after.id.clone(),
                                event_id,
                                event_type: $event_type.to_string(),
                            });
                        }
                    };
                }

                emit_change!("scope_change", before.scope, after.scope);
                emit_change!(
                    "direction_change",
                    before.current_direction,
                    after.current_direction
                );
                emit_change!("objective_rationale_change", before.rationale, after.rationale);
                emit_change!("grade_change", before.grade, after.grade);
                emit_change!("objective_summary_change", before.summary, after.summary);

                tx.commit()?;
                Ok(notes)
            })
            .await
            .map_err(MonarchError::from)
    }

    pub async fn record_manual_objective_event_internal(
        &self,
        payload: &ManualObjectiveEventPayload,
    ) -> Result<String, MonarchError> {
        let event_type = payload.event_type.as_str();
        if !matches!(
            event_type,
            "note" | "blocker" | "blocker_resolved" | "question" | "answer"
        ) {
            return Err(MonarchError::invalid_input(format!(
                "Unsupported manual objective event type: {}",
                event_type
            )));
        }
        let payload_json = serde_json::json!({
            "text": payload.text,
            "title": payload.title,
            "metadata": payload
                .metadata_json
                .as_deref()
                .and_then(|raw| serde_json::from_str::<Value>(raw).ok()),
        })
        .to_string();
        self.record_objective_event_internal(&RecordObjectiveEventPayload {
            objective_id: payload.objective_id.clone(),
            event_type: payload.event_type.clone(),
            actor: Some(
                payload
                    .actor
                    .clone()
                    .unwrap_or_else(|| "monarch".to_string()),
            ),
            payload_json: Some(payload_json),
            author: Some(
                payload
                    .author
                    .clone()
                    .unwrap_or_else(|| "captain".to_string()),
            ),
            surface_override: payload.surface_override.clone(),
            ..Default::default()
        })
        .await
    }

    pub async fn list_objective_refs_internal(
        &self,
        objective_id: &str,
    ) -> Result<Vec<ObjectiveRefRow>, MonarchError> {
        let objective_id = objective_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, objective_id, ref_type, label, target, metadata_json,
                            created_by, created_at, updated_at
                     FROM objective_refs WHERE objective_id = ?1 ORDER BY created_at ASC",
                )?;
                let rows = stmt
                    .query_map(params![objective_id], map_objective_ref)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await?)
    }

    pub async fn get_objective_ref_internal(
        &self,
        id: &str,
    ) -> Result<Option<ObjectiveRefRow>, MonarchError> {
        let id = id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, objective_id, ref_type, label, target, metadata_json,
                            created_by, created_at, updated_at
                     FROM objective_refs WHERE id = ?1",
                )?;
                let mut rows = stmt.query(params![id])?;
                if let Some(row) = rows.next()? {
                    Ok(Some(map_objective_ref(row)?))
                } else {
                    Ok(None)
                }
            })
            .await?)
    }

    pub async fn create_objective_ref_internal(
        &self,
        payload: &CreateObjectiveRefPayload,
    ) -> Result<String, MonarchError> {
        if payload.ref_type.trim().is_empty() {
            return Err(MonarchError::invalid_input("refType required"));
        }
        if payload.target.trim().is_empty() {
            return Err(MonarchError::invalid_input("target required"));
        }
        let payload = payload.clone();
        let id = payload
            .id
            .clone()
            .unwrap_or_else(crate::util::uuid_v4_simple);
        let id_for_return = id.clone();
        let created_by = payload.created_by.unwrap_or_else(|| "captain".to_string());
        let now = crate::util::chrono_now();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO objective_refs (
                        id, objective_id, ref_type, label, target, metadata_json,
                        created_by, created_at, updated_at
                     )
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                    params![
                        id,
                        payload.objective_id,
                        payload.ref_type,
                        payload.label,
                        payload.target,
                        payload.metadata_json,
                        created_by,
                        now
                    ],
                )?;
                Ok(())
            })
            .await?;
        Ok(id_for_return)
    }

    pub async fn update_objective_ref_internal(
        &self,
        payload: &UpdateObjectiveRefPayload,
    ) -> Result<(), MonarchError> {
        let payload = payload.clone();
        self.conn
            .call(move |conn| {
                let mut sets: Vec<&str> = Vec::new();
                let mut args: Vec<rusqlite::types::Value> = Vec::new();
                macro_rules! push {
                    ($field:expr, $col:literal) => {
                        if let Some(v) = $field.as_ref() {
                            sets.push(concat!($col, " = ?"));
                            args.push(rusqlite::types::Value::Text(v.clone()));
                        }
                    };
                }
                push!(payload.ref_type, "ref_type");
                push!(payload.label, "label");
                push!(payload.target, "target");
                push!(payload.metadata_json, "metadata_json");
                if sets.is_empty() {
                    return Ok(());
                }
                sets.push("updated_at = ?");
                args.push(rusqlite::types::Value::Text(crate::util::chrono_now()));
                let sql = format!("UPDATE objective_refs SET {} WHERE id = ?", sets.join(", "));
                args.push(rusqlite::types::Value::Text(payload.id.clone()));
                let params_slice: Vec<&dyn rusqlite::ToSql> =
                    args.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
                conn.execute(&sql, params_slice.as_slice())?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn delete_objective_ref_internal(&self, id: &str) -> Result<(), MonarchError> {
        let id = id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute("DELETE FROM objective_refs WHERE id = ?1", params![id])?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn get_working_memory_internal(
        &self,
        agent_id: &str,
    ) -> Result<Option<WorkingMemoryPayload>, MonarchError> {
        let agent_id = agent_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let payload: Option<String> = conn
                    .query_row(
                        "SELECT payload_json FROM agent_working_memory WHERE agent_id = ?1",
                        params![agent_id],
                        |row| row.get(0),
                    )
                    .ok();
                Ok(payload.and_then(|p| serde_json::from_str(&p).ok()))
            })
            .await?)
    }

    pub async fn record_action_transition_internal(
        &self,
        agent_id: &str,
        objective_id: &str,
        intent: &str,
        previous_outcome: Option<&str>,
    ) -> Result<Vec<ObjectiveEventNotification>, MonarchError> {
        let agent_id = agent_id.to_string();
        let objective_id = objective_id.to_string();
        let intent = intent.to_string();
        let previous_outcome = previous_outcome.map(str::to_string);
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let now = crate::util::chrono_now();
                let mut notes = Vec::new();
                let mut wm = load_working_memory_tx(&tx, &agent_id)
                    .unwrap_or_else(|| empty_working_memory(&objective_id, &now));

                if let Some(current) = wm.current_action.clone() {
                    let (outcome, auto_closed, reason) = match previous_outcome.as_deref() {
                        Some(o) if !o.trim().is_empty() => (o.trim().to_string(), false, None),
                        _ => (
                            "Moved on before recording an outcome.".to_string(),
                            true,
                            Some("new_action_started".to_string()),
                        ),
                    };
                    close_action_tx(
                        &tx,
                        &agent_id,
                        &mut wm,
                        &current,
                        &outcome,
                        auto_closed,
                        reason,
                        &now,
                        &mut notes,
                    )?;
                }

                let event_id = crate::util::uuid_v4_simple();
                let payload = serde_json::json!({
                    "intent": intent,
                    "status": "active",
                    "started_at": now,
                })
                .to_string();
                // P4b: stamp plan_item_id from the L2 plan slice so timeline
                // rendering can group consecutive actions under their plan
                // item without a join through L2. The slice may have shifted
                // since the previous action — recompute against the live
                // table here, not the loaded L2 snapshot.
                let plan_item_id = super::plans::recompute_plan_slice_tx(&tx, &objective_id)
                    .ok()
                    .and_then(|(active, _)| active);
                tx.execute(
                    "INSERT INTO objective_events (
                        id, objective_id, event_type, actor, payload_json, created_at,
                        parent_event_id, author, surface_override, payload_schema_version,
                        plan_item_id
                     )
                     VALUES (?1, ?2, 'coherent_action', ?3, ?4, ?5, NULL, 'executor', NULL, 1, ?6)",
                    params![event_id, objective_id, agent_id, payload, now, plan_item_id],
                )?;
                wm.current_objective_id = Some(objective_id.clone());
                wm.current_objective_path = objective_path_tx(&tx, &objective_id);
                wm.current_action = Some(WorkingMemoryCurrentAction {
                    event_id: event_id.clone(),
                    objective_id: objective_id.clone(),
                    intent,
                    started_at: now.clone(),
                });
                wm.updated_at = now.clone();
                save_working_memory_tx(&tx, &agent_id, &wm)?;
                notes.push(ObjectiveEventNotification {
                    objective_id,
                    event_id,
                    event_type: "coherent_action".to_string(),
                });
                tx.commit()?;
                Ok(notes)
            })
            .await?)
    }

    pub async fn complete_action_internal(
        &self,
        agent_id: &str,
        outcome: &str,
    ) -> Result<Vec<ObjectiveEventNotification>, MonarchError> {
        let agent_id = agent_id.to_string();
        let outcome = outcome.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let now = crate::util::chrono_now();
                let mut notes = Vec::new();
                let Some(mut wm) = load_working_memory_tx(&tx, &agent_id) else {
                    tx.commit()?;
                    return Ok(notes);
                };
                let Some(current) = wm.current_action.clone() else {
                    tx.commit()?;
                    return Ok(notes);
                };
                close_action_tx(
                    &tx,
                    &agent_id,
                    &mut wm,
                    &current,
                    outcome.trim(),
                    false,
                    None,
                    &now,
                    &mut notes,
                )?;
                wm.current_action = None;
                wm.updated_at = now;
                save_working_memory_tx(&tx, &agent_id, &wm)?;
                tx.commit()?;
                Ok(notes)
            })
            .await?)
    }

    pub async fn record_executor_decision_internal(
        &self,
        agent_id: &str,
        objective_id: &str,
        decision: &str,
        rationale: Option<&str>,
    ) -> Result<Vec<ObjectiveEventNotification>, MonarchError> {
        let agent_id = agent_id.to_string();
        let objective_id = objective_id.to_string();
        let decision = decision.to_string();
        let rationale = rationale.map(str::to_string);
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let now = crate::util::chrono_now();
                let parent = load_working_memory_tx(&tx, &agent_id)
                    .and_then(|wm| wm.current_action.map(|a| a.event_id));
                let event_id = crate::util::uuid_v4_simple();
                let payload = serde_json::json!({
                    "decision": decision,
                    "rationale": rationale,
                })
                .to_string();
                tx.execute(
                    "INSERT INTO objective_events (
                        id, objective_id, event_type, actor, payload_json, created_at,
                        parent_event_id, author, surface_override, payload_schema_version
                     )
                     VALUES (?1, ?2, 'executor_decision', ?3, ?4, ?5, ?6, 'executor', NULL, 1)",
                    params![event_id, objective_id, agent_id, payload, now, parent],
                )?;
                tx.commit()?;
                Ok(vec![ObjectiveEventNotification {
                    objective_id,
                    event_id,
                    event_type: "executor_decision".to_string(),
                }])
            })
            .await?)
    }

    pub async fn record_tool_call_start_internal(
        &self,
        agent_id: &str,
        objective_id: &str,
        tool_call_id: &str,
        tool_name: &str,
        args: Option<Value>,
    ) -> Result<Vec<ObjectiveEventNotification>, MonarchError> {
        let agent_id = agent_id.to_string();
        let objective_id = objective_id.to_string();
        let tool_call_id = tool_call_id.to_string();
        let tool_name = tool_name.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let Some(wm) = load_working_memory_tx(&tx, &agent_id) else {
                    tx.commit()?;
                    return Ok(Vec::new());
                };
                let Some(parent) = wm.current_action.map(|a| a.event_id) else {
                    tx.commit()?;
                    return Ok(Vec::new());
                };
                let now = crate::util::chrono_now();
                let event_id = crate::util::uuid_v4_simple();
                let payload = serde_json::json!({
                    "tool_call_id": tool_call_id,
                    "tool_name": tool_name,
                    "args_preview": preview_value(args.as_ref()),
                    "status": "running",
                    "started_at": now,
                })
                .to_string();
                tx.execute(
                    "INSERT INTO objective_events (
                        id, objective_id, event_type, actor, payload_json, created_at,
                        parent_event_id, author, surface_override, payload_schema_version
                     )
                     VALUES (?1, ?2, 'tool_call', ?3, ?4, ?5, ?6, 'executor', NULL, 1)",
                    params![event_id, objective_id, agent_id, payload, now, parent],
                )?;
                tx.commit()?;
                Ok(vec![ObjectiveEventNotification {
                    objective_id,
                    event_id,
                    event_type: "tool_call".to_string(),
                }])
            })
            .await?)
    }

    pub async fn record_tool_call_end_internal(
        &self,
        tool_call_id: &str,
        result: Option<Value>,
        is_error: bool,
        duration_ms: Option<i64>,
    ) -> Result<Vec<ObjectiveEventNotification>, MonarchError> {
        let tool_call_id = tool_call_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let found: Option<(String, String, String)> = tx
                    .query_row(
                        "SELECT id, objective_id, payload_json
                         FROM objective_events
                         WHERE event_type = 'tool_call'
                           AND json_extract(payload_json, '$.tool_call_id') = ?1
                         ORDER BY created_at DESC
                         LIMIT 1",
                        params![tool_call_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .ok();
                let Some((event_id, objective_id, raw_payload)) = found else {
                    tx.commit()?;
                    return Ok(Vec::new());
                };
                let now = crate::util::chrono_now();
                let mut payload: Value =
                    serde_json::from_str(&raw_payload).unwrap_or_else(|_| serde_json::json!({}));
                if let Some(obj) = payload.as_object_mut() {
                    obj.insert(
                        "status".to_string(),
                        Value::String(if is_error { "error" } else { "done" }.to_string()),
                    );
                    obj.insert("is_error".to_string(), Value::Bool(is_error));
                    obj.insert(
                        "result_preview".to_string(),
                        Value::String(preview_value(result.as_ref())),
                    );
                    obj.insert("completed_at".to_string(), Value::String(now));
                    if let Some(duration_ms) = duration_ms {
                        obj.insert(
                            "duration_ms".to_string(),
                            Value::Number(serde_json::Number::from(duration_ms)),
                        );
                    }
                }
                tx.execute(
                    "UPDATE objective_events SET payload_json = ?1 WHERE id = ?2",
                    params![payload.to_string(), event_id],
                )?;
                tx.commit()?;
                Ok(vec![ObjectiveEventNotification {
                    objective_id,
                    event_id,
                    event_type: "tool_call".to_string(),
                }])
            })
            .await?)
    }

    /// MON-105 / P1: on a meaningful user turn, create an objective as a BRANCH
    /// under the agent's project campaign root and set it as the agent's current
    /// objective — but only if there is no active current objective. Returns
    /// `Some(new_id)` when one was created.
    ///
    /// Project-less agents stay ephemeral: no project → no campaign → `Ok(None)`
    /// (the seam where a future per-captain scratch campaign would hook in —
    /// roadmap-v2 P1 defers it).
    pub async fn auto_create_current_objective_internal(
        &self,
        agent_id: &str,
        title: &str,
        description: Option<&str>,
    ) -> Result<Option<String>, MonarchError> {
        let agent_id = agent_id.to_string();
        let title = title.to_string();
        let description = description.map(|s| s.to_string());
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                // Current objective + project, in one query. `None` row = no
                // such agent.
                let row: Option<(Option<String>, Option<String>, Option<String>)> = tx
                    .query_row(
                        "SELECT a.current_objective_id, o.status, a.project_id
                         FROM agents a
                         LEFT JOIN objective_nodes o ON o.id = a.current_objective_id
                         WHERE a.id = ?1",
                        params![agent_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()?;
                let (current_id, status, project_id) = match row {
                    Some(t) => t,
                    None => {
                        tx.commit()?;
                        return Ok(None);
                    }
                };
                // A live (non-terminal) current objective → keep it.
                if let (Some(_), Some(st)) = (&current_id, &status) {
                    let terminal =
                        matches!(st.as_str(), "done" | "verified" | "abandoned" | "superseded");
                    if !terminal {
                        tx.commit()?;
                        return Ok(None);
                    }
                }
                // Project-less agents stay ephemeral (no auto-objective).
                let project_id = match project_id {
                    Some(p) => p,
                    None => {
                        tx.commit()?;
                        return Ok(None);
                    }
                };

                let campaign_root = ensure_campaign_root_tx(&tx, &project_id)?;
                let id = crate::util::uuid_v4_simple();
                let event_id = crate::util::uuid_v4_simple();
                let now = crate::util::chrono_now();
                // Branch UNDER the campaign root: root_id = parent_id = campaign
                // (so get_objective_tree_for_root(campaign) returns the whole tree).
                tx.execute(
                    "INSERT INTO objective_nodes (
                        id, root_id, parent_id, title, description,
                        status, grade, exec_hint, assignee_shadow_id,
                        created_by, created_at, started_at, kind
                    ) VALUES (?1, ?2, ?2, ?3, ?4, 'in_progress', 'C', 'in_context', ?5, 'monarch', ?6, ?6, 'objective')",
                    params![id, campaign_root, title, description, agent_id, now],
                )?;
                let event_payload = serde_json::json!({
                    "from": null,
                    "to": "in_progress",
                    "autoCreated": true,
                })
                .to_string();
                tx.execute(
                    "INSERT INTO objective_events (id, objective_id, event_type, actor, payload_json, created_at)
                     VALUES (?1, ?2, 'status_change', 'monarch', ?3, ?4)",
                    params![event_id, id, event_payload, now],
                )?;
                tx.execute(
                    "UPDATE agents SET current_objective_id = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE id = ?2",
                    params![id, agent_id],
                )?;
                tx.commit()?;
                Ok(Some(id))
            })
            .await?)
    }
}

/// P1: get-or-create a project's campaign root within an existing transaction.
/// The campaign root is an `objective_nodes` row with `kind='campaign'`,
/// `parent_id=NULL`, `root_id=self`; never closed/graded/reported. Returns the
/// existing root when `projects.root_objective_id` points at a live node (the
/// JOIN guards a dangling pointer), otherwise creates one and links it. Shared
/// by `ensure_campaign_root_internal` and `auto_create_current_objective_internal`
/// so both stay atomic with their surrounding work.
fn ensure_campaign_root_tx(
    tx: &rusqlite::Transaction<'_>,
    project_id: &str,
) -> rusqlite::Result<String> {
    if let Some(id) = tx
        .query_row(
            "SELECT o.id FROM projects p
             JOIN objective_nodes o ON o.id = p.root_objective_id
             WHERE p.id = ?1",
            params![project_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    {
        return Ok(id);
    }
    let name: String = tx
        .query_row(
            "SELECT name FROM projects WHERE id = ?1",
            params![project_id],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or_else(|| "Project".to_string());
    let new_id = crate::util::uuid_v4_simple();
    let title = format!("{name} campaign");
    tx.execute(
        "INSERT INTO objective_nodes (
            id, root_id, parent_id, title, status, created_by, created_at, kind
        ) VALUES (?1, ?1, NULL, ?2, 'in_progress', 'monarch',
            strftime('%Y-%m-%dT%H:%M:%SZ','now'), 'campaign')",
        params![new_id, title],
    )?;
    tx.execute(
        "UPDATE projects SET root_objective_id = ?1,
           updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE id = ?2",
        params![new_id, project_id],
    )?;
    Ok(new_id)
}

fn preview_value(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    let raw = match value {
        Value::String(s) => s.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
    };
    let compact = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.len() <= 500 {
        compact
    } else {
        format!("{}...", compact.chars().take(497).collect::<String>())
    }
}

// ---- Tauri Commands: Objectives (MON-83) ----
//
// Write commands take the `AgentManager` state so they can broadcast event
// channels (`objective-created-{id}` / `objective-updated-{id}` /
// `objective-event-{objectiveId}`) via the shared `ws_broadcast` sender. Slice 2
// payloads are small — the event is the objective id and minimal metadata so
// subscribers can re-fetch with `db_get_objective` / `db_list_objective_events`.

#[tauri::command]
#[specta::specta]
pub async fn db_create_objective(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    payload: CreateObjectivePayload,
) -> Result<String, MonarchError> {
    let assignee = payload.assignee_shadow_id.clone();
    let id = db.create_objective_internal(&payload).await?;
    crate::agent::emit_event(
        &app,
        &agent_mgr.ws_broadcast,
        &format!("objective-created-{}", id),
        &serde_json::json!({ "id": id.clone() }).to_string(),
    );
    if let Some(agent_id) = assignee {
        crate::agent::emit_event(
            &app,
            &agent_mgr.ws_broadcast,
            &format!("objective-created-for-agent-{}", agent_id),
            &serde_json::json!({ "id": id, "agentId": agent_id }).to_string(),
        );
    }
    Ok(id)
}

#[tauri::command]
#[specta::specta]
pub async fn db_update_objective(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    payload: UpdateObjectivePayload,
) -> Result<(), MonarchError> {
    let id = payload.id.clone();
    let before = db.get_objective_internal(&id).await?;
    db.update_objective_internal(&payload).await?;
    let after = db.get_objective_internal(&id).await?;
    crate::agent::emit_event(
        &app,
        &agent_mgr.ws_broadcast,
        &format!("objective-updated-{}", id),
        &serde_json::json!({ "id": id }).to_string(),
    );
    if let Some(after_objective) = after.as_ref() {
        if after_objective.root_id != after_objective.id {
            crate::agent::emit_event(
                &app,
                &agent_mgr.ws_broadcast,
                &format!("objective-updated-{}", after_objective.root_id),
                &serde_json::json!({ "id": after_objective.id, "rootId": after_objective.root_id })
                    .to_string(),
            );
        }
    }
    handle_objective_update_side_effects(&app, db.inner(), agent_mgr.inner(), before, after).await?;
    Ok(())
}

pub async fn handle_objective_update_side_effects(
    app: &tauri::AppHandle,
    db: &Arc<Database>,
    agent_mgr: &Arc<crate::agent::AgentManager>,
    before: Option<ObjectiveRow>,
    after: Option<ObjectiveRow>,
) -> Result<(), MonarchError> {
    let Some(after) = after else {
        return Ok(());
    };
    let before_status = before.as_ref().map(|q| q.status.as_str());
    if before_status == Some(after.status.as_str()) {
        return Ok(());
    }

    let event_payload = serde_json::json!({
        "from": before_status,
        "to": after.status.as_str(),
    })
    .to_string();
    let event_id = db
        .record_objective_event_internal(&RecordObjectiveEventPayload {
            objective_id: after.id.clone(),
            event_type: "status_change".to_string(),
            actor: Some("monarch".to_string()),
            payload_json: Some(event_payload),
            ..Default::default()
        })
        .await?;
    crate::agent::emit_event(
        app,
        &agent_mgr.ws_broadcast,
        &format!("objective-event-{}", after.id),
        &serde_json::json!({ "id": event_id, "eventType": "status_change" }).to_string(),
    );

    let transitioned_to_done = before_status != Some("done") && after.status == "done";
    if !transitioned_to_done {
        return Ok(());
    }

    let Some(agent_id) = after.assignee_shadow_id.clone() else {
        return Ok(());
    };
    db.clear_agent_current_objective_if_matches_internal(&agent_id, &after.id)
        .await?;
    let since = after
        .started_at
        .clone()
        .or_else(|| Some(after.created_at.clone()));
    if let Err(e) = agent_mgr
        .dispatch_keeper_run(
            db,
            &agent_id,
            crate::agent::KeeperRunTrigger::ObjectiveClose {
                objective_id: after.id.clone(),
                since,
            },
        )
        .await
    {
        eprintln!(
            "[monarch] objective-close keeper dispatch failed for {} objective {}: {:?}",
            agent_id, after.id, e
        );
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn db_get_objective(
    db: tauri::State<'_, Arc<Database>>,
    objective_id: String,
) -> Result<Option<ObjectiveRow>, MonarchError> {
    db.get_objective_internal(&objective_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_list_objectives_for_agent(
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
) -> Result<Vec<ObjectiveRow>, MonarchError> {
    db.list_objectives_for_agent_internal(&agent_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_get_objective_tree_for_root(
    db: tauri::State<'_, Arc<Database>>,
    root_id: String,
) -> Result<Vec<ObjectiveRow>, MonarchError> {
    db.get_objective_tree_for_root_internal(&root_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_get_campaign_root_for_agent(
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
) -> Result<Option<ObjectiveRow>, MonarchError> {
    db.get_campaign_root_for_agent_internal(&agent_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_record_objective_event(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    payload: RecordObjectiveEventPayload,
) -> Result<String, MonarchError> {
    let objective_id = payload.objective_id.clone();
    let event_type = payload.event_type.clone();
    let id = db.record_objective_event_internal(&payload).await?;
    crate::agent::emit_event(
        &app,
        &agent_mgr.ws_broadcast,
        &format!("objective-event-{}", objective_id),
        &serde_json::json!({ "id": id, "eventType": event_type }).to_string(),
    );
    Ok(id)
}

#[tauri::command]
#[specta::specta]
pub async fn db_list_objective_events(
    db: tauri::State<'_, Arc<Database>>,
    objective_id: String,
) -> Result<Vec<ObjectiveEventRow>, MonarchError> {
    db.list_objective_events_internal(&objective_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_update_objective_manual(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    payload: ManualObjectiveUpdatePayload,
) -> Result<(), MonarchError> {
    let id = payload.id.clone();
    let before = db.get_objective_internal(&id).await?;
    let notes = db.update_objective_manual_internal(&payload).await?;
    let after = db.get_objective_internal(&id).await?;
    emit_objective_updated_notifications(&app, &agent_mgr.ws_broadcast, &id, after.as_ref());
    super::plans::emit_plan_notifications(&app, &agent_mgr.ws_broadcast, notes);
    handle_objective_update_side_effects(&app, db.inner(), agent_mgr.inner(), before, after).await?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn db_record_manual_objective_event(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    payload: ManualObjectiveEventPayload,
) -> Result<String, MonarchError> {
    let objective_id = payload.objective_id.clone();
    let event_type = payload.event_type.clone();
    let id = db.record_manual_objective_event_internal(&payload).await?;
    crate::agent::emit_event(
        &app,
        &agent_mgr.ws_broadcast,
        &format!("objective-event-{}", objective_id),
        &serde_json::json!({ "id": id, "eventType": event_type }).to_string(),
    );
    Ok(id)
}

#[tauri::command]
#[specta::specta]
pub async fn db_list_objective_refs(
    db: tauri::State<'_, Arc<Database>>,
    objective_id: String,
) -> Result<Vec<ObjectiveRefRow>, MonarchError> {
    db.list_objective_refs_internal(&objective_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_create_objective_ref(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    payload: CreateObjectiveRefPayload,
) -> Result<String, MonarchError> {
    let objective_id = payload.objective_id.clone();
    let id = db.create_objective_ref_internal(&payload).await?;
    emit_objective_ref_notification(&app, &agent_mgr.ws_broadcast, &objective_id, "created", &id);
    Ok(id)
}

#[tauri::command]
#[specta::specta]
pub async fn db_update_objective_ref(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    payload: UpdateObjectiveRefPayload,
) -> Result<(), MonarchError> {
    let id = payload.id.clone();
    let before = db.get_objective_ref_internal(&id).await?;
    db.update_objective_ref_internal(&payload).await?;
    if let Some(row) = before {
        emit_objective_ref_notification(&app, &agent_mgr.ws_broadcast, &row.objective_id, "updated", &id);
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn db_delete_objective_ref(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    ref_id: String,
) -> Result<(), MonarchError> {
    let before = db.get_objective_ref_internal(&ref_id).await?;
    db.delete_objective_ref_internal(&ref_id).await?;
    if let Some(row) = before {
        emit_objective_ref_notification(
            &app,
            &agent_mgr.ws_broadcast,
            &row.objective_id,
            "deleted",
            &ref_id,
        );
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn db_get_working_memory(
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
) -> Result<Option<WorkingMemoryPayload>, MonarchError> {
    db.get_working_memory_internal(&agent_id).await
}

pub fn emit_objective_updated_notifications(
    app: &tauri::AppHandle,
    ws_tx: &tokio::sync::broadcast::Sender<crate::agent::WsBroadcast>,
    id: &str,
    after: Option<&ObjectiveRow>,
) {
    crate::agent::emit_event(
        app,
        ws_tx,
        &format!("objective-updated-{}", id),
        &serde_json::json!({ "id": id }).to_string(),
    );
    if let Some(after_objective) = after {
        if after_objective.root_id != after_objective.id {
            crate::agent::emit_event(
                app,
                ws_tx,
                &format!("objective-updated-{}", after_objective.root_id),
                &serde_json::json!({ "id": after_objective.id, "rootId": after_objective.root_id })
                    .to_string(),
            );
        }
    }
}

pub fn emit_objective_ref_notification(
    app: &tauri::AppHandle,
    ws_tx: &tokio::sync::broadcast::Sender<crate::agent::WsBroadcast>,
    objective_id: &str,
    action: &str,
    ref_id: &str,
) {
    crate::agent::emit_event(
        app,
        ws_tx,
        &format!("objective-refs-{}", objective_id),
        &serde_json::json!({ "id": ref_id, "objectiveId": objective_id, "action": action }).to_string(),
    );
}
