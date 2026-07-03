use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::MonarchError;

use super::objectives::{
    load_working_memory_tx, save_working_memory_tx, ObjectiveEventNotification,
};
use super::Database;

// ---- Row types ----

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanItemRow {
    pub id: String,
    pub objective_id: String,
    pub parent_id: Option<String>,
    pub title: String,
    pub status: String,
    pub order_index: i32,
    pub created_by: String,
    pub rationale: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

/// Input row for `db_set_plan` / executor `set_plan`. `id` is optional —
/// server generates a UUID if omitted, which is the common case for newly
/// proposed items. Status defaults to `pending` when omitted; the only
/// reason a caller would supply it is when the new plan inherits a
/// previously active item without restarting it.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanItemInput {
    #[serde(default)]
    pub id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub rationale: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
}

/// Bulk replace payload — `db_set_plan(objective_id, items, created_by)`.
/// `created_by` defaults to `'captain'` when called from the manual UI
/// path; sidecar pass-through sets it to `'executor'`. The whole list is
/// authoritative — items not present (matched by id) are deleted.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SetPlanPayload {
    pub objective_id: String,
    pub items: Vec<PlanItemInput>,
    #[serde(default)]
    pub created_by: Option<String>,
    #[serde(default)]
    pub rationale: Option<String>,
}

/// Per-item edit payload. Only non-`None` fields are written. `id` is the
/// row's primary key.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePlanItemPayload {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub rationale: Option<String>,
    #[serde(default)]
    pub order_index: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AddPlanItemPayload {
    pub objective_id: String,
    pub title: String,
    #[serde(default)]
    pub rationale: Option<String>,
    /// Insert this item after the named item id, or at the end when
    /// omitted. Insertion shifts subsequent `order_index` values forward.
    #[serde(default)]
    pub after_item_id: Option<String>,
    #[serde(default)]
    pub created_by: Option<String>,
}

// ---- Validators ----

fn validate_plan_status(status: &str) -> rusqlite::Result<()> {
    match status {
        "pending" | "active" | "completed" | "skipped" | "blocked" => Ok(()),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

fn validate_plan_created_by(created_by: &str) -> rusqlite::Result<()> {
    match created_by {
        "captain" | "executor" | "monarch" => Ok(()),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

// ---- Transaction helpers ----

fn lookup_plan_item_tx(
    tx: &rusqlite::Transaction<'_>,
    item_id: &str,
) -> rusqlite::Result<Option<(String, String, Option<String>)>> {
    let row = tx
        .query_row(
            "SELECT pi.objective_id, pi.status, q.assignee_shadow_id
             FROM objective_plan_items pi
             INNER JOIN objective_nodes q ON q.id = pi.objective_id
             WHERE pi.id = ?1",
            params![item_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .ok();
    Ok(row)
}

fn insert_plan_event_tx(
    tx: &rusqlite::Transaction<'_>,
    objective_id: &str,
    event_type: &str,
    plan_item_id: Option<&str>,
    payload_json: &str,
    now: &str,
) -> rusqlite::Result<String> {
    let event_id = crate::util::uuid_v4_simple();
    tx.execute(
        "INSERT INTO objective_events (
            id, objective_id, event_type, actor, payload_json, created_at,
            parent_event_id, author, surface_override, payload_schema_version,
            plan_item_id
         )
         VALUES (?1, ?2, ?3, NULL, ?4, ?5, NULL, 'executor', NULL, 1, ?6)",
        params![
            event_id,
            objective_id,
            event_type,
            payload_json,
            now,
            plan_item_id
        ],
    )?;
    Ok(event_id)
}

/// Recompute the plan slice for a objective: the active item id (if any) and
/// up to three pending items in order. Used by `sync_plan_l2_tx` and by
/// the read path that surfaces the slice into Agent View.
pub(super) fn recompute_plan_slice_tx(
    tx: &rusqlite::Transaction<'_>,
    objective_id: &str,
) -> rusqlite::Result<(Option<String>, Vec<String>)> {
    let active: Option<String> = tx
        .query_row(
            "SELECT id FROM objective_plan_items
             WHERE objective_id = ?1 AND status = 'active'
             ORDER BY order_index ASC
             LIMIT 1",
            params![objective_id],
            |row| row.get(0),
        )
        .ok();
    let mut next = Vec::with_capacity(3);
    let mut stmt = tx.prepare(
        "SELECT id FROM objective_plan_items
         WHERE objective_id = ?1 AND status = 'pending'
         ORDER BY order_index ASC
         LIMIT 3",
    )?;
    let mut rows = stmt.query(params![objective_id])?;
    while let Some(row) = rows.next()? {
        next.push(row.get(0)?);
    }
    Ok((active, next))
}

/// If any agent's L2 currently points at this objective, recompute its plan
/// slice and write it back. We filter by the L2 payload's own
/// `currentObjectiveId` (not `agents.current_objective_id`) because action
/// transitions update L2 directly and the column-side pointer can lag —
/// L2 is the authoritative live state for which objective an agent is on.
fn sync_plan_l2_tx(
    tx: &rusqlite::Transaction<'_>,
    objective_id: &str,
    now: &str,
) -> rusqlite::Result<()> {
    let agent_ids: Vec<String> = {
        let mut stmt = tx.prepare(
            "SELECT agent_id FROM agent_working_memory
             WHERE json_extract(payload_json, '$.currentObjectiveId') = ?1",
        )?;
        let rows = stmt
            .query_map(params![objective_id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    if agent_ids.is_empty() {
        return Ok(());
    }
    let (active, next) = recompute_plan_slice_tx(tx, objective_id)?;
    for agent_id in agent_ids {
        let Some(mut wm) = load_working_memory_tx(tx, &agent_id) else {
            continue;
        };
        if wm.current_objective_id.as_deref() != Some(objective_id) {
            continue;
        }
        wm.active_plan_item_id = active.clone();
        wm.next_plan_item_ids = next.clone();
        wm.updated_at = now.to_string();
        save_working_memory_tx(tx, &agent_id, &wm)?;
    }
    Ok(())
}

fn map_plan_item(row: &Row<'_>) -> rusqlite::Result<PlanItemRow> {
    Ok(PlanItemRow {
        id: row.get(0)?,
        objective_id: row.get(1)?,
        parent_id: row.get(2)?,
        title: row.get(3)?,
        status: row.get(4)?,
        order_index: row.get(5)?,
        created_by: row.get(6)?,
        rationale: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        completed_at: row.get(10)?,
    })
}

// ---- impl Database ----

impl Database {
    /// frontend store keeps this list per objective and refreshes when
    /// `objective-event-{objective_id}` carries a `plan_*` event type.
    pub async fn list_plan_items_internal(
        &self,
        objective_id: &str,
    ) -> Result<Vec<PlanItemRow>, MonarchError> {
        let objective_id = objective_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, objective_id, parent_id, title, status, order_index,
                            created_by, rationale, created_at, updated_at, completed_at
                     FROM objective_plan_items
                     WHERE objective_id = ?1
                     ORDER BY order_index ASC",
                )?;
                let rows = stmt
                    .query_map(params![objective_id], map_plan_item)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await?)
    }

    pub async fn get_plan_item_internal(
        &self,
        item_id: &str,
    ) -> Result<Option<PlanItemRow>, MonarchError> {
        let item_id = item_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let row = conn
                    .query_row(
                        "SELECT id, objective_id, parent_id, title, status, order_index,
                                created_by, rationale, created_at, updated_at, completed_at
                         FROM objective_plan_items WHERE id = ?1",
                        params![item_id],
                        map_plan_item,
                    )
                    .ok();
                Ok(row)
            })
            .await?)
    }

    /// Bulk replace a objective's plan. Existing rows whose ids are in the
    /// payload are preserved (status untouched); rows missing from the
    /// payload are deleted; new rows arrive with `status='pending'`.
    /// Emits `plan_created` when the objective had no prior plan, otherwise
    /// `plan_changed`. The active assignee's L2 plan slice is recomputed
    /// and saved when the agent's `current_objective_id` matches.
    pub async fn set_plan_internal(
        &self,
        payload: &SetPlanPayload,
    ) -> Result<Vec<ObjectiveEventNotification>, MonarchError> {
        let payload = payload.clone();
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let now = crate::util::chrono_now();
                let objective_id = payload.objective_id.clone();
                let created_by = payload
                    .created_by
                    .clone()
                    .unwrap_or_else(|| "captain".to_string());
                validate_plan_created_by(&created_by)?;

                let prior_count: i64 = tx
                    .query_row(
                        "SELECT COUNT(*) FROM objective_plan_items WHERE objective_id = ?1",
                        params![objective_id],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);

                // Resolve final ids upfront so we can delete any existing
                // row not in the new list (one statement, FK-friendly).
                let mut final_ids: Vec<String> = Vec::with_capacity(payload.items.len());
                for input in &payload.items {
                    let id = input.id.clone().unwrap_or_else(crate::util::uuid_v4_simple);
                    final_ids.push(id);
                }

                if !final_ids.is_empty() {
                    let placeholders = std::iter::repeat("?")
                        .take(final_ids.len())
                        .collect::<Vec<_>>()
                        .join(",");
                    let sql = format!(
                        "DELETE FROM objective_plan_items WHERE objective_id = ? AND id NOT IN ({})",
                        placeholders
                    );
                    let mut stmt = tx.prepare(&sql)?;
                    let mut bound: Vec<&dyn rusqlite::ToSql> =
                        Vec::with_capacity(1 + final_ids.len());
                    bound.push(&objective_id);
                    for id in &final_ids {
                        bound.push(id);
                    }
                    stmt.execute(rusqlite::params_from_iter(bound))?;
                } else {
                    tx.execute(
                        "DELETE FROM objective_plan_items WHERE objective_id = ?1",
                        params![objective_id],
                    )?;
                }

                for (idx, input) in payload.items.iter().enumerate() {
                    let id = &final_ids[idx];
                    let order_index = idx as i32;
                    let status = input
                        .status
                        .clone()
                        .unwrap_or_else(|| "pending".to_string());
                    validate_plan_status(&status)?;
                    tx.execute(
                        "INSERT INTO objective_plan_items
                            (id, objective_id, parent_id, title, status, order_index,
                             created_by, rationale, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
                         ON CONFLICT(id) DO UPDATE SET
                            title = excluded.title,
                            parent_id = excluded.parent_id,
                            rationale = excluded.rationale,
                            order_index = excluded.order_index,
                            updated_at = excluded.updated_at",
                        params![
                            id,
                            objective_id,
                            input.parent_id,
                            input.title.trim(),
                            status,
                            order_index,
                            created_by,
                            input.rationale,
                            now,
                        ],
                    )?;
                }

                let event_type = if prior_count == 0 {
                    "plan_created"
                } else {
                    "plan_changed"
                };
                let payload_json = serde_json::json!({
                    "item_ids": final_ids,
                    "rationale": payload.rationale,
                    "created_by": created_by,
                })
                .to_string();
                let event_id =
                    insert_plan_event_tx(&tx, &objective_id, event_type, None, &payload_json, &now)?;
                sync_plan_l2_tx(&tx, &objective_id, &now)?;
                tx.commit()?;
                Ok(vec![ObjectiveEventNotification {
                    objective_id,
                    event_id,
                    event_type: event_type.to_string(),
                }])
            })
            .await?)
    }

    /// Append (or insert after a named item) a single new plan item. Emits
    /// `plan_changed`. Newly added items always start as `pending`.
    pub async fn add_plan_item_internal(
        &self,
        payload: &AddPlanItemPayload,
    ) -> Result<(String, Vec<ObjectiveEventNotification>), MonarchError> {
        let payload = payload.clone();
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let now = crate::util::chrono_now();
                let id = crate::util::uuid_v4_simple();
                let created_by = payload
                    .created_by
                    .clone()
                    .unwrap_or_else(|| "captain".to_string());
                validate_plan_created_by(&created_by)?;
                let objective_id = payload.objective_id.clone();

                let order_index = if let Some(after_id) = payload.after_item_id.as_deref() {
                    let after_order: Option<i32> = tx
                        .query_row(
                            "SELECT order_index FROM objective_plan_items
                             WHERE id = ?1 AND objective_id = ?2",
                            params![after_id, objective_id],
                            |row| row.get(0),
                        )
                        .ok();
                    match after_order {
                        Some(o) => {
                            tx.execute(
                                "UPDATE objective_plan_items
                                 SET order_index = order_index + 1,
                                     updated_at = ?2
                                 WHERE objective_id = ?1 AND order_index > ?3",
                                params![objective_id, now, o],
                            )?;
                            o + 1
                        }
                        None => {
                            let max: Option<i32> = tx
                                .query_row(
                                    "SELECT MAX(order_index) FROM objective_plan_items
                                     WHERE objective_id = ?1",
                                    params![objective_id],
                                    |row| row.get(0),
                                )
                                .ok()
                                .flatten();
                            max.map(|m| m + 1).unwrap_or(0)
                        }
                    }
                } else {
                    let max: Option<i32> = tx
                        .query_row(
                            "SELECT MAX(order_index) FROM objective_plan_items WHERE objective_id = ?1",
                            params![objective_id],
                            |row| row.get(0),
                        )
                        .ok()
                        .flatten();
                    max.map(|m| m + 1).unwrap_or(0)
                };

                tx.execute(
                    "INSERT INTO objective_plan_items
                        (id, objective_id, parent_id, title, status, order_index,
                         created_by, rationale, created_at, updated_at)
                     VALUES (?1, ?2, NULL, ?3, 'pending', ?4, ?5, ?6, ?7, ?7)",
                    params![
                        id,
                        objective_id,
                        payload.title.trim(),
                        order_index,
                        created_by,
                        payload.rationale,
                        now,
                    ],
                )?;

                let payload_json = serde_json::json!({
                    "item_id": id,
                    "title": payload.title,
                    "after_item_id": payload.after_item_id,
                    "created_by": created_by,
                })
                .to_string();
                let event_id = insert_plan_event_tx(
                    &tx,
                    &objective_id,
                    "plan_changed",
                    None,
                    &payload_json,
                    &now,
                )?;
                sync_plan_l2_tx(&tx, &objective_id, &now)?;
                tx.commit()?;
                Ok((
                    id,
                    vec![ObjectiveEventNotification {
                        objective_id,
                        event_id,
                        event_type: "plan_changed".to_string(),
                    }],
                ))
            })
            .await?)
    }

    /// Edit a single plan item's title / rationale / order_index. Emits
    /// `plan_changed` when something actually changed; no-op (empty
    /// notification list) otherwise.
    pub async fn update_plan_item_internal(
        &self,
        payload: &UpdatePlanItemPayload,
    ) -> Result<Vec<ObjectiveEventNotification>, MonarchError> {
        let payload = payload.clone();
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let now = crate::util::chrono_now();
                let Some((objective_id, _, _)) = lookup_plan_item_tx(&tx, &payload.id)? else {
                    tx.commit()?;
                    return Ok(Vec::new());
                };
                let mut changed = false;
                if let Some(title) = payload.title.as_deref() {
                    tx.execute(
                        "UPDATE objective_plan_items SET title = ?1, updated_at = ?2 WHERE id = ?3",
                        params![title.trim(), now, payload.id],
                    )?;
                    changed = true;
                }
                if let Some(rationale) = &payload.rationale {
                    tx.execute(
                        "UPDATE objective_plan_items SET rationale = ?1, updated_at = ?2 WHERE id = ?3",
                        params![rationale, now, payload.id],
                    )?;
                    changed = true;
                }
                if let Some(new_order) = payload.order_index {
                    tx.execute(
                        "UPDATE objective_plan_items SET order_index = ?1, updated_at = ?2 WHERE id = ?3",
                        params![new_order, now, payload.id],
                    )?;
                    changed = true;
                }
                if !changed {
                    tx.commit()?;
                    return Ok(Vec::new());
                }
                let payload_json = serde_json::json!({
                    "item_id": payload.id,
                    "fields": {
                        "title": payload.title,
                        "rationale": payload.rationale,
                        "order_index": payload.order_index,
                    },
                })
                .to_string();
                let event_id = insert_plan_event_tx(
                    &tx,
                    &objective_id,
                    "plan_changed",
                    None,
                    &payload_json,
                    &now,
                )?;
                sync_plan_l2_tx(&tx, &objective_id, &now)?;
                tx.commit()?;
                Ok(vec![ObjectiveEventNotification {
                    objective_id,
                    event_id,
                    event_type: "plan_changed".to_string(),
                }])
            })
            .await?)
    }

    pub async fn delete_plan_item_internal(
        &self,
        item_id: &str,
    ) -> Result<Vec<ObjectiveEventNotification>, MonarchError> {
        let item_id = item_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let now = crate::util::chrono_now();
                let Some((objective_id, _, _)) = lookup_plan_item_tx(&tx, &item_id)? else {
                    tx.commit()?;
                    return Ok(Vec::new());
                };
                tx.execute(
                    "DELETE FROM objective_plan_items WHERE id = ?1",
                    params![item_id],
                )?;
                let payload_json = serde_json::json!({
                    "deleted_item_id": item_id,
                })
                .to_string();
                let event_id = insert_plan_event_tx(
                    &tx,
                    &objective_id,
                    "plan_changed",
                    None,
                    &payload_json,
                    &now,
                )?;
                sync_plan_l2_tx(&tx, &objective_id, &now)?;
                tx.commit()?;
                Ok(vec![ObjectiveEventNotification {
                    objective_id,
                    event_id,
                    event_type: "plan_changed".to_string(),
                }])
            })
            .await?)
    }

    /// Mark a plan item active. At most one item per objective may be active —
    /// any sibling currently in `active` is silently reset to `pending`
    /// (the caller owns explicit completion / skip / block; the reset is
    /// a defensive invariant, not a status transition the supervisor sees).
    pub async fn start_plan_item_internal(
        &self,
        item_id: &str,
    ) -> Result<Vec<ObjectiveEventNotification>, MonarchError> {
        let item_id = item_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let now = crate::util::chrono_now();
                let Some((objective_id, _, _)) = lookup_plan_item_tx(&tx, &item_id)? else {
                    tx.commit()?;
                    return Ok(Vec::new());
                };
                tx.execute(
                    "UPDATE objective_plan_items
                     SET status = 'pending', updated_at = ?2
                     WHERE objective_id = ?1 AND status = 'active' AND id <> ?3",
                    params![objective_id, now, item_id],
                )?;
                tx.execute(
                    "UPDATE objective_plan_items
                     SET status = 'active', updated_at = ?2, completed_at = NULL
                     WHERE id = ?1",
                    params![item_id, now],
                )?;
                let payload_json = serde_json::json!({ "item_id": item_id }).to_string();
                let event_id = insert_plan_event_tx(
                    &tx,
                    &objective_id,
                    "plan_item_started",
                    Some(&item_id),
                    &payload_json,
                    &now,
                )?;
                sync_plan_l2_tx(&tx, &objective_id, &now)?;
                tx.commit()?;
                Ok(vec![ObjectiveEventNotification {
                    objective_id,
                    event_id,
                    event_type: "plan_item_started".to_string(),
                }])
            })
            .await?)
    }

    pub async fn complete_plan_item_internal(
        &self,
        item_id: &str,
        outcome: Option<&str>,
    ) -> Result<Vec<ObjectiveEventNotification>, MonarchError> {
        let item_id = item_id.to_string();
        let outcome = outcome.map(str::to_string);
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let now = crate::util::chrono_now();
                let Some((objective_id, _, _)) = lookup_plan_item_tx(&tx, &item_id)? else {
                    tx.commit()?;
                    return Ok(Vec::new());
                };
                tx.execute(
                    "UPDATE objective_plan_items
                     SET status = 'completed', updated_at = ?2, completed_at = ?2
                     WHERE id = ?1",
                    params![item_id, now],
                )?;
                let payload_json = serde_json::json!({
                    "item_id": item_id,
                    "outcome": outcome,
                })
                .to_string();
                let event_id = insert_plan_event_tx(
                    &tx,
                    &objective_id,
                    "plan_item_completed",
                    Some(&item_id),
                    &payload_json,
                    &now,
                )?;
                sync_plan_l2_tx(&tx, &objective_id, &now)?;
                tx.commit()?;
                Ok(vec![ObjectiveEventNotification {
                    objective_id,
                    event_id,
                    event_type: "plan_item_completed".to_string(),
                }])
            })
            .await?)
    }

    pub async fn skip_plan_item_internal(
        &self,
        item_id: &str,
        reason: Option<&str>,
    ) -> Result<Vec<ObjectiveEventNotification>, MonarchError> {
        let item_id = item_id.to_string();
        let reason = reason.map(str::to_string);
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let now = crate::util::chrono_now();
                let Some((objective_id, _, _)) = lookup_plan_item_tx(&tx, &item_id)? else {
                    tx.commit()?;
                    return Ok(Vec::new());
                };
                tx.execute(
                    "UPDATE objective_plan_items
                     SET status = 'skipped', updated_at = ?2, completed_at = ?2
                     WHERE id = ?1",
                    params![item_id, now],
                )?;
                let payload_json = serde_json::json!({
                    "item_id": item_id,
                    "reason": reason,
                })
                .to_string();
                let event_id = insert_plan_event_tx(
                    &tx,
                    &objective_id,
                    "plan_item_skipped",
                    Some(&item_id),
                    &payload_json,
                    &now,
                )?;
                sync_plan_l2_tx(&tx, &objective_id, &now)?;
                tx.commit()?;
                Ok(vec![ObjectiveEventNotification {
                    objective_id,
                    event_id,
                    event_type: "plan_item_skipped".to_string(),
                }])
            })
            .await?)
    }

    /// Resolve the agent's current active plan item — the row whose
    /// `status = 'active'` on the agent's L2 `currentObjectiveId`. Used by the
    /// persist pipeline when a sidecar plan-lifecycle event arrives
    /// without an explicit item id (the executor tool implicitly targets
    /// the active item). Returns `None` if the agent's L2 has no current
    /// objective, or if no item is active on it.
    pub async fn get_active_plan_item_for_agent_internal(
        &self,
        agent_id: &str,
    ) -> Result<Option<String>, MonarchError> {
        let agent_id = agent_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let item: Option<String> = conn
                    .query_row(
                        "SELECT pi.id FROM objective_plan_items pi
                         INNER JOIN agent_working_memory w
                            ON json_extract(w.payload_json, '$.currentObjectiveId') = pi.objective_id
                         WHERE w.agent_id = ?1 AND pi.status = 'active'
                         ORDER BY pi.order_index ASC
                         LIMIT 1",
                        params![agent_id],
                        |row| row.get(0),
                    )
                    .ok();
                Ok(item)
            })
            .await?)
    }

    pub async fn block_plan_item_internal(
        &self,
        item_id: &str,
        reason: &str,
    ) -> Result<Vec<ObjectiveEventNotification>, MonarchError> {
        let item_id = item_id.to_string();
        let reason = reason.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let now = crate::util::chrono_now();
                let Some((objective_id, _, _)) = lookup_plan_item_tx(&tx, &item_id)? else {
                    tx.commit()?;
                    return Ok(Vec::new());
                };
                tx.execute(
                    "UPDATE objective_plan_items
                     SET status = 'blocked', updated_at = ?2
                     WHERE id = ?1",
                    params![item_id, now],
                )?;
                let payload_json = serde_json::json!({
                    "item_id": item_id,
                    "reason": reason,
                })
                .to_string();
                let event_id = insert_plan_event_tx(
                    &tx,
                    &objective_id,
                    "plan_item_blocked",
                    Some(&item_id),
                    &payload_json,
                    &now,
                )?;
                sync_plan_l2_tx(&tx, &objective_id, &now)?;
                tx.commit()?;
                Ok(vec![ObjectiveEventNotification {
                    objective_id,
                    event_id,
                    event_type: "plan_item_blocked".to_string(),
                }])
            })
            .await?)
    }
}

// ---- Tauri Commands: Plans ----
//
// P4b: plan commands are exposed both as Tauri commands (frontend) and
// consumed via the sidecar event pipeline (ws.rs / persist.rs → internal
// methods) so the supervisor can propose and the executor can advance a plan
// directly. Both end up calling the same `*_internal` methods, so plan
// state stays consistent across origins.

#[tauri::command]
#[specta::specta]
pub async fn db_list_plan_items(
    db: tauri::State<'_, Arc<Database>>,
    objective_id: String,
) -> Result<Vec<PlanItemRow>, MonarchError> {
    db.list_plan_items_internal(&objective_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_get_plan_item(
    db: tauri::State<'_, Arc<Database>>,
    item_id: String,
) -> Result<Option<PlanItemRow>, MonarchError> {
    db.get_plan_item_internal(&item_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_set_plan(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    payload: SetPlanPayload,
) -> Result<(), MonarchError> {
    let notes = db.set_plan_internal(&payload).await?;
    emit_plan_notifications(&app, &agent_mgr.ws_broadcast, notes);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn db_add_plan_item(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    payload: AddPlanItemPayload,
) -> Result<String, MonarchError> {
    let (id, notes) = db.add_plan_item_internal(&payload).await?;
    emit_plan_notifications(&app, &agent_mgr.ws_broadcast, notes);
    Ok(id)
}

#[tauri::command]
#[specta::specta]
pub async fn db_update_plan_item(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    payload: UpdatePlanItemPayload,
) -> Result<(), MonarchError> {
    let notes = db.update_plan_item_internal(&payload).await?;
    emit_plan_notifications(&app, &agent_mgr.ws_broadcast, notes);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn db_delete_plan_item(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    item_id: String,
) -> Result<(), MonarchError> {
    let notes = db.delete_plan_item_internal(&item_id).await?;
    emit_plan_notifications(&app, &agent_mgr.ws_broadcast, notes);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn db_start_plan_item(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    item_id: String,
) -> Result<(), MonarchError> {
    let notes = db.start_plan_item_internal(&item_id).await?;
    emit_plan_notifications(&app, &agent_mgr.ws_broadcast, notes);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn db_complete_plan_item(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    item_id: String,
    outcome: Option<String>,
) -> Result<(), MonarchError> {
    let notes = db
        .complete_plan_item_internal(&item_id, outcome.as_deref())
        .await?;
    emit_plan_notifications(&app, &agent_mgr.ws_broadcast, notes);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn db_skip_plan_item(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    item_id: String,
    reason: Option<String>,
) -> Result<(), MonarchError> {
    let notes = db
        .skip_plan_item_internal(&item_id, reason.as_deref())
        .await?;
    emit_plan_notifications(&app, &agent_mgr.ws_broadcast, notes);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn db_block_plan_item(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    item_id: String,
    reason: String,
) -> Result<(), MonarchError> {
    let notes = db.block_plan_item_internal(&item_id, &reason).await?;
    emit_plan_notifications(&app, &agent_mgr.ws_broadcast, notes);
    Ok(())
}

pub fn emit_plan_notifications(
    app: &tauri::AppHandle,
    ws_tx: &tokio::sync::broadcast::Sender<crate::agent::WsBroadcast>,
    notes: Vec<ObjectiveEventNotification>,
) {
    for note in notes {
        crate::agent::emit_event(
            app,
            ws_tx,
            &format!("objective-event-{}", note.objective_id),
            &serde_json::json!({ "id": note.event_id, "eventType": note.event_type }).to_string(),
        );
    }
}
