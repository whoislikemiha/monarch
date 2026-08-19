use std::path::PathBuf;
use tokio_rusqlite::Connection;

use crate::error::MonarchError;

// ---- Submodules ----

pub mod agents;
pub mod classifications;
pub mod identity;
pub mod memories;
pub mod misc;
pub mod plans;
pub mod projects;
pub mod objectives;
pub mod reports;
pub mod schema;
pub mod sessions;

// ---- pub use re-exports ----
//
// Every item that was previously public via `crate::db::X` must be
// re-exported here so that lib.rs, ws.rs, and all agent/* files compile
// with zero changes.

// agents
pub use agents::AgentRow;
pub use agents::{
    db_archive_agent, db_delete_agent, db_delete_agent_template, db_get_agent_stats,
    db_get_agents, db_list_agent_templates, db_save_agent_template, db_unarchive_agent,
    db_update_agent, db_upsert_agent,
};

// sessions
pub use sessions::{MessageAttachmentRow, MessageRow, SessionRow};
#[allow(unused_imports)]
pub use sessions::{ConversationOverview, SessionSummary};
pub use sessions::{
    db_create_session, db_get_messages, db_get_messages_with_ancestry, db_get_sessions,
    db_get_tool_call_detail, db_list_conversations, db_list_session_summaries, db_save_message,
    db_set_session_title,
};
#[allow(unused_imports)]
pub use sessions::ToolCallDetail;

// projects
pub use projects::ProjectRow;
pub use projects::{
    db_delete_project, db_get_project_by_path, db_get_projects, db_rename_project,
    db_update_project_instructions, db_upsert_project,
};

// objectives
pub use objectives::{
    CreateObjectivePayload, CreateObjectiveRefPayload, ManualObjectiveEventPayload,
    ManualObjectiveUpdatePayload, ObjectiveEventNotification, ObjectiveRow, RecordObjectiveEventPayload,
    TimelineCursor, UpdateObjectivePayload, UpdateObjectiveRefPayload,
};
pub use objectives::{
    db_create_objective, db_create_objective_ref, db_delete_objective_ref, db_get_objective,
    db_get_campaign_root_for_agent, db_get_objective_tree_for_root, db_get_working_memory,
    db_list_agent_timeline, db_list_objective_events,
    db_list_objective_refs, db_list_objectives_for_agent, db_record_manual_objective_event,
    db_record_objective_event, db_update_objective, db_update_objective_manual, db_update_objective_ref,
};
pub use objectives::{
    emit_objective_ref_notification, emit_objective_updated_notifications, handle_objective_update_side_effects,
};

// plans
pub use plans::{AddPlanItemPayload, PlanItemInput, SetPlanPayload, UpdatePlanItemPayload};
pub use plans::{
    db_add_plan_item, db_block_plan_item, db_complete_plan_item, db_delete_plan_item,
    db_get_plan_item, db_list_plan_items, db_set_plan, db_skip_plan_item, db_start_plan_item,
    db_update_plan_item,
};
pub use plans::emit_plan_notifications;

// reports
pub use reports::WriteObjectiveReportPayload;
pub use reports::{db_get_objective_report, db_list_objective_reports_for_agent, db_save_objective_report};
pub use reports::emit_objective_report_notification;

// memories
pub use memories::{InsertMemoryPayload, MemoryRow};
pub use memories::{db_get_memory, db_list_memories_for_agent};

// identity
pub use identity::{CaptainIdentityRow, ShadowIdentityRow};

// classifications
pub use classifications::{ClassificationRow, SaveClassificationPayload};
pub use classifications::{db_get_classification_for_message, db_list_classifications_for_agent};

// misc
pub use misc::{db_get_ui_state, db_log_event, db_set_ui_state};

// ---- __cmd__ re-exports for tauri::generate_handler! ----
//
// `tauri::generate_handler![db::db_foo]` internally references
// `db::__cmd__db_foo`, so each submodule's generated symbol must be
// re-exported here to remain in the `db::` namespace.
#[allow(non_snake_case, unused_imports)]
pub use agents::{
    __cmd__db_archive_agent, __cmd__db_delete_agent, __cmd__db_delete_agent_template,
    __cmd__db_get_agent_stats, __cmd__db_get_agents, __cmd__db_list_agent_templates,
    __cmd__db_save_agent_template, __cmd__db_unarchive_agent, __cmd__db_update_agent,
    __cmd__db_upsert_agent,
};
#[allow(non_snake_case, unused_imports)]
pub use classifications::{
    __cmd__db_get_classification_for_message, __cmd__db_list_classifications_for_agent,
};
#[allow(non_snake_case, unused_imports)]
pub use memories::{__cmd__db_get_memory, __cmd__db_list_memories_for_agent};
#[allow(non_snake_case, unused_imports)]
pub use misc::{__cmd__db_get_ui_state, __cmd__db_log_event, __cmd__db_set_ui_state};
#[allow(non_snake_case, unused_imports)]
pub use plans::{
    __cmd__db_add_plan_item, __cmd__db_block_plan_item, __cmd__db_complete_plan_item,
    __cmd__db_delete_plan_item, __cmd__db_get_plan_item, __cmd__db_list_plan_items,
    __cmd__db_set_plan, __cmd__db_skip_plan_item, __cmd__db_start_plan_item,
    __cmd__db_update_plan_item,
};
#[allow(non_snake_case, unused_imports)]
pub use projects::{
    __cmd__db_delete_project, __cmd__db_get_project_by_path, __cmd__db_get_projects,
    __cmd__db_rename_project, __cmd__db_update_project_instructions, __cmd__db_upsert_project,
};
#[allow(non_snake_case, unused_imports)]
pub use objectives::{
    __cmd__db_create_objective, __cmd__db_create_objective_ref, __cmd__db_delete_objective_ref,
    __cmd__db_get_campaign_root_for_agent, __cmd__db_get_objective,
    __cmd__db_get_objective_tree_for_root, __cmd__db_get_working_memory,
    __cmd__db_list_agent_timeline,
    __cmd__db_list_objective_events, __cmd__db_list_objective_refs, __cmd__db_list_objectives_for_agent,
    __cmd__db_record_manual_objective_event, __cmd__db_record_objective_event, __cmd__db_update_objective,
    __cmd__db_update_objective_manual, __cmd__db_update_objective_ref,
};
#[allow(non_snake_case, unused_imports)]
pub use reports::{
    __cmd__db_get_objective_report, __cmd__db_list_objective_reports_for_agent,
    __cmd__db_save_objective_report,
};
#[allow(non_snake_case, unused_imports)]
pub use sessions::{
    __cmd__db_create_session, __cmd__db_get_messages, __cmd__db_get_messages_with_ancestry,
    __cmd__db_get_sessions, __cmd__db_get_tool_call_detail, __cmd__db_list_conversations,
    __cmd__db_list_session_summaries,
    __cmd__db_save_message, __cmd__db_set_session_title,
};

// ---- __specta__fn__ re-exports for specta collect_commands! ----
#[allow(non_snake_case, unused_imports)]
pub use agents::{
    __specta__fn__db_archive_agent, __specta__fn__db_delete_agent,
    __specta__fn__db_delete_agent_template, __specta__fn__db_get_agent_stats,
    __specta__fn__db_get_agents, __specta__fn__db_list_agent_templates,
    __specta__fn__db_save_agent_template, __specta__fn__db_unarchive_agent,
    __specta__fn__db_update_agent, __specta__fn__db_upsert_agent,
};
#[allow(non_snake_case, unused_imports)]
pub use classifications::{
    __specta__fn__db_get_classification_for_message,
    __specta__fn__db_list_classifications_for_agent,
};
#[allow(non_snake_case, unused_imports)]
pub use memories::{__specta__fn__db_get_memory, __specta__fn__db_list_memories_for_agent};
#[allow(non_snake_case, unused_imports)]
pub use misc::{
    __specta__fn__db_get_ui_state, __specta__fn__db_log_event, __specta__fn__db_set_ui_state,
};
#[allow(non_snake_case, unused_imports)]
pub use plans::{
    __specta__fn__db_add_plan_item, __specta__fn__db_block_plan_item,
    __specta__fn__db_complete_plan_item, __specta__fn__db_delete_plan_item,
    __specta__fn__db_get_plan_item, __specta__fn__db_list_plan_items, __specta__fn__db_set_plan,
    __specta__fn__db_skip_plan_item, __specta__fn__db_start_plan_item,
    __specta__fn__db_update_plan_item,
};
#[allow(non_snake_case, unused_imports)]
pub use projects::{
    __specta__fn__db_delete_project, __specta__fn__db_get_project_by_path,
    __specta__fn__db_get_projects, __specta__fn__db_rename_project,
    __specta__fn__db_update_project_instructions, __specta__fn__db_upsert_project,
};
#[allow(non_snake_case, unused_imports)]
pub use objectives::{
    __specta__fn__db_create_objective, __specta__fn__db_create_objective_ref,
    __specta__fn__db_delete_objective_ref, __specta__fn__db_get_objective,
    __specta__fn__db_get_campaign_root_for_agent, __specta__fn__db_get_objective_tree_for_root,
    __specta__fn__db_get_working_memory,
    __specta__fn__db_list_agent_timeline,
    __specta__fn__db_list_objective_events, __specta__fn__db_list_objective_refs,
    __specta__fn__db_list_objectives_for_agent, __specta__fn__db_record_manual_objective_event,
    __specta__fn__db_record_objective_event, __specta__fn__db_update_objective,
    __specta__fn__db_update_objective_manual, __specta__fn__db_update_objective_ref,
};
#[allow(non_snake_case, unused_imports)]
pub use reports::{
    __specta__fn__db_get_objective_report, __specta__fn__db_list_objective_reports_for_agent,
    __specta__fn__db_save_objective_report,
};
#[allow(non_snake_case, unused_imports)]
pub use sessions::{
    __specta__fn__db_create_session, __specta__fn__db_get_messages,
    __specta__fn__db_get_messages_with_ancestry, __specta__fn__db_get_sessions,
    __specta__fn__db_get_tool_call_detail, __specta__fn__db_list_conversations,
    __specta__fn__db_list_session_summaries,
    __specta__fn__db_save_message, __specta__fn__db_set_session_title,
};

// ---- Database struct ----

/// MON-27: backed by `tokio_rusqlite::Connection`, which owns a single
/// `rusqlite::Connection` on a dedicated background thread. Every method is
/// `async` and dispatches work via `conn.call(|c| { ... }).await`; the
/// closure body is plain synchronous `rusqlite` code, so migrations, queries,
/// and transactions are unchanged from the pre-MON-27 shape.
///
/// `Connection` is `Clone` (cheap — internally `Arc`-ed), so `Arc<Database>`
/// in Tauri state works as before and worker tasks can keep their own clone.
pub struct Database {
    pub(super) conn: Connection,
}

fn db_path() -> PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("monarch");
    std::fs::create_dir_all(&dir).ok();
    dir.join("monarch.db")
}

impl Database {
    pub async fn new() -> Result<Self, MonarchError> {
        let path = db_path();
        let conn = Connection::open(&path).await?;

        // Enable WAL mode for better concurrent access
        conn.call(|c| {
            c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
            Ok(())
        })
        .await?;

        let db = Self { conn };
        db.init_schema().await?;
        db.migrate_timestamps_to_rfc3339().await?;
        db.ensure_captain_bootstrap().await?;
        Ok(db)
    }

    /// In-memory SQLite instance for unit tests. Not WAL (in-memory databases
    /// don't support it) and schema is initialised the same way as the
    /// on-disk constructor.
    #[cfg(test)]
    pub async fn new_in_memory() -> Result<Self, MonarchError> {
        let conn = Connection::open_in_memory().await?;
        conn.call(|c| {
            c.execute_batch("PRAGMA foreign_keys=ON;")?;
            Ok(())
        })
        .await?;
        let db = Self { conn };
        db.init_schema().await?;
        db.migrate_timestamps_to_rfc3339().await?;
        Ok(db)
    }
}

// ---- Tests ----

#[cfg(test)]
mod tests {
    use rusqlite::params;
    use super::*;
    use crate::db::agents::AgentRow;
    use crate::db::plans::{AddPlanItemPayload, PlanItemInput, SetPlanPayload};
    use crate::db::reports::WriteObjectiveReportPayload;

    fn test_agent(id: &str) -> AgentRow {
        let now = crate::util::chrono_now();
        AgentRow {
            id: id.to_string(),
            name: id.to_string(),
            project_id: None,
            shadow_name: Some("Atlas".to_string()),
            shadow_title: Some("Test Agent".to_string()),
            shadow_grade: Some("Junior".to_string()),
            provider: Some("lmstudio".to_string()),
            model: Some("test-model".to_string()),
            thinking_level: Some("off".to_string()),
            cwd: Some("/tmp".to_string()),
            custom_prompt: None,
            context_window: None,
            created_at: now.clone(),
            updated_at: now,
            archived_at: None,
            avatar_type: None,
            avatar_path: None,
        }
    }

    async fn seed_agent_and_objective(db: &Database) -> (String, String) {
        let agent_id = "agent-p4".to_string();
        db.ensure_agent_exists_internal(&test_agent(&agent_id))
            .await
            .expect("agent");
        let objective_id = db
            .create_objective_internal(&CreateObjectivePayload {
                id: None,
                parent_id: None,
                title: "Test objective".to_string(),
                description: None,
                status: Some("in_progress".to_string()),
                grade: Some("C".to_string()),
                exec_hint: Some("in_context".to_string()),
                assignee_shadow_id: Some(agent_id.clone()),
                created_by: Some("monarch".to_string()),
                kind: None,
            })
            .await
            .expect("objective");
        (agent_id, objective_id)
    }

    #[tokio::test]
    async fn action_transition_sets_current_action() {
        let db = Database::new_in_memory().await.expect("db");
        let (agent_id, objective_id) = seed_agent_and_objective(&db).await;

        db.record_action_transition_internal(
            &agent_id,
            &objective_id,
            "Understand the failing authentication test",
            None,
        )
        .await
        .expect("transition");

        let wm = db
            .get_working_memory_internal(&agent_id)
            .await
            .expect("wm")
            .expect("wm row");
        let current = wm.current_action.expect("current action");
        assert_eq!(current.intent, "Understand the failing authentication test");
        assert_eq!(current.objective_id, objective_id);
        assert!(wm.recent_actions.is_empty());
    }

    #[tokio::test]
    async fn action_transition_closes_previous_action_with_outcome() {
        let db = Database::new_in_memory().await.expect("db");
        let (agent_id, objective_id) = seed_agent_and_objective(&db).await;

        db.record_action_transition_internal(&agent_id, &objective_id, "Map auth flow", None)
            .await
            .expect("first");
        db.record_action_transition_internal(
            &agent_id,
            &objective_id,
            "Patch expiry handler",
            Some("Found expired sessions return 401 instead of redirecting."),
        )
        .await
        .expect("second");

        let wm = db
            .get_working_memory_internal(&agent_id)
            .await
            .expect("wm")
            .expect("wm row");
        assert_eq!(
            wm.current_action.expect("current").intent,
            "Patch expiry handler"
        );
        assert_eq!(wm.recent_actions.len(), 1);
        assert_eq!(wm.recent_actions[0].intent, "Map auth flow");
        assert_eq!(
            wm.recent_actions[0].outcome,
            "Found expired sessions return 401 instead of redirecting."
        );
        assert_eq!(wm.recent_actions[0].auto_closed, None);
    }

    #[tokio::test]
    async fn complete_action_clears_current_action_and_records_outcome_child() {
        let db = Database::new_in_memory().await.expect("db");
        let (agent_id, objective_id) = seed_agent_and_objective(&db).await;

        db.record_action_transition_internal(&agent_id, &objective_id, "Edit session restore", None)
            .await
            .expect("transition");
        db.complete_action_internal(&agent_id, "Session restore now follows ancestry.")
            .await
            .expect("complete");

        let wm = db
            .get_working_memory_internal(&agent_id)
            .await
            .expect("wm")
            .expect("wm row");
        assert!(wm.current_action.is_none());
        assert_eq!(wm.recent_actions.len(), 1);
        assert_eq!(
            wm.recent_actions[0].outcome,
            "Session restore now follows ancestry."
        );

        let events = db
            .list_objective_events_internal(&objective_id)
            .await
            .expect("events");
        let action = events
            .iter()
            .find(|ev| ev.event_type == "coherent_action")
            .expect("action");
        let outcome = events
            .iter()
            .find(|ev| ev.event_type == "action_outcome")
            .expect("outcome");
        assert_eq!(outcome.parent_event_id.as_deref(), Some(action.id.as_str()));
    }

    #[tokio::test]
    async fn tool_call_start_and_end_update_one_child_event() {
        let db = Database::new_in_memory().await.expect("db");
        let (agent_id, objective_id) = seed_agent_and_objective(&db).await;
        db.record_action_transition_internal(&agent_id, &objective_id, "Run focused test", None)
            .await
            .expect("transition");

        db.record_tool_call_start_internal(
            &agent_id,
            &objective_id,
            "tc-1",
            "bash",
            Some(serde_json::json!({ "cmd": "cargo test auth" })),
        )
        .await
        .expect("tool start");
        db.record_tool_call_end_internal(
            "tc-1",
            Some(serde_json::json!({ "output": "ok" })),
            false,
            Some(123),
        )
        .await
        .expect("tool end");

        let events = db
            .list_objective_events_internal(&objective_id)
            .await
            .expect("events");
        let tools: Vec<_> = events
            .iter()
            .filter(|ev| ev.event_type == "tool_call")
            .collect();
        assert_eq!(tools.len(), 1);
        assert!(tools[0].parent_event_id.is_some());
        let payload: serde_json::Value =
            serde_json::from_str(tools[0].payload_json.as_deref().unwrap()).unwrap();
        assert_eq!(payload["tool_call_id"], "tc-1");
        assert_eq!(payload["status"], "done");
        assert_eq!(payload["duration_ms"], 123);
    }

    // ---- P4b (MON-111) plan lifecycle tests ----

    fn plan_input(title: &str) -> PlanItemInput {
        PlanItemInput {
            id: None,
            title: title.to_string(),
            rationale: None,
            status: None,
            parent_id: None,
        }
    }

    async fn seed_plan(db: &Database, objective_id: &str, titles: &[&str]) -> Vec<String> {
        let payload = SetPlanPayload {
            objective_id: objective_id.to_string(),
            items: titles.iter().map(|t| plan_input(t)).collect(),
            created_by: Some("captain".to_string()),
            rationale: None,
        };
        db.set_plan_internal(&payload).await.expect("set plan");
        let items = db
            .list_plan_items_internal(objective_id)
            .await
            .expect("list items");
        items.into_iter().map(|i| i.id).collect()
    }

    #[tokio::test]
    async fn set_plan_inserts_ordered_items_and_emits_plan_created() {
        let db = Database::new_in_memory().await.expect("db");
        let (_, objective_id) = seed_agent_and_objective(&db).await;

        db.set_plan_internal(&SetPlanPayload {
            objective_id: objective_id.clone(),
            items: vec![plan_input("inspect auth flow"), plan_input("patch handler")],
            created_by: Some("captain".to_string()),
            rationale: Some("expiry redirect bug".to_string()),
        })
        .await
        .expect("set plan");

        let items = db.list_plan_items_internal(&objective_id).await.expect("list");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "inspect auth flow");
        assert_eq!(items[0].order_index, 0);
        assert_eq!(items[0].status, "pending");
        assert_eq!(items[1].title, "patch handler");
        assert_eq!(items[1].order_index, 1);

        let events = db
            .list_objective_events_internal(&objective_id)
            .await
            .expect("events");
        let plan_events: Vec<_> = events
            .iter()
            .filter(|ev| ev.event_type == "plan_created")
            .collect();
        assert_eq!(plan_events.len(), 1, "exactly one plan_created event");
    }

    #[tokio::test]
    async fn set_plan_replaces_existing_items_and_emits_plan_changed() {
        let db = Database::new_in_memory().await.expect("db");
        let (_, objective_id) = seed_agent_and_objective(&db).await;
        let ids = seed_plan(&db, &objective_id, &["A", "B", "C"]).await;

        // Keep B (by id), drop A and C, add a new D at the end.
        db.set_plan_internal(&SetPlanPayload {
            objective_id: objective_id.clone(),
            items: vec![
                PlanItemInput {
                    id: Some(ids[1].clone()),
                    title: "B".to_string(),
                    rationale: None,
                    status: None,
                    parent_id: None,
                },
                plan_input("D"),
            ],
            created_by: Some("captain".to_string()),
            rationale: None,
        })
        .await
        .expect("replace");

        let items = db.list_plan_items_internal(&objective_id).await.expect("list");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, ids[1]);
        assert_eq!(items[1].title, "D");

        let events = db
            .list_objective_events_internal(&objective_id)
            .await
            .expect("events");
        assert!(events.iter().any(|ev| ev.event_type == "plan_changed"));
    }

    #[tokio::test]
    async fn add_plan_item_appends_at_end_when_no_after_id() {
        let db = Database::new_in_memory().await.expect("db");
        let (_, objective_id) = seed_agent_and_objective(&db).await;
        seed_plan(&db, &objective_id, &["A", "B"]).await;

        db.add_plan_item_internal(&AddPlanItemPayload {
            objective_id: objective_id.clone(),
            title: "C".to_string(),
            rationale: None,
            after_item_id: None,
            created_by: Some("captain".to_string()),
        })
        .await
        .expect("add");

        let items = db.list_plan_items_internal(&objective_id).await.expect("list");
        assert_eq!(items.len(), 3);
        assert_eq!(items[2].title, "C");
        assert_eq!(items[2].order_index, 2);
    }

    #[tokio::test]
    async fn add_plan_item_inserts_after_named_item_and_shifts_following() {
        let db = Database::new_in_memory().await.expect("db");
        let (_, objective_id) = seed_agent_and_objective(&db).await;
        let ids = seed_plan(&db, &objective_id, &["A", "B", "C"]).await;

        db.add_plan_item_internal(&AddPlanItemPayload {
            objective_id: objective_id.clone(),
            title: "Between".to_string(),
            rationale: None,
            after_item_id: Some(ids[0].clone()),
            created_by: Some("captain".to_string()),
        })
        .await
        .expect("insert");

        let items = db.list_plan_items_internal(&objective_id).await.expect("list");
        assert_eq!(
            items.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(),
            vec!["A", "Between", "B", "C"]
        );
    }

    #[tokio::test]
    async fn start_plan_item_sets_active_and_resets_prior_active() {
        let db = Database::new_in_memory().await.expect("db");
        let (agent_id, objective_id) = seed_agent_and_objective(&db).await;
        // Anchor L2's currentObjectiveId so sync_plan_l2_tx picks up the row.
        db.record_action_transition_internal(&agent_id, &objective_id, "warm up", None)
            .await
            .expect("anchor");
        let ids = seed_plan(&db, &objective_id, &["A", "B"]).await;

        db.start_plan_item_internal(&ids[0]).await.expect("start A");
        let wm_after_a = db
            .get_working_memory_internal(&agent_id)
            .await
            .expect("wm")
            .expect("row");
        assert_eq!(wm_after_a.active_plan_item_id.as_ref(), Some(&ids[0]));
        assert_eq!(wm_after_a.next_plan_item_ids, vec![ids[1].clone()]);

        db.start_plan_item_internal(&ids[1]).await.expect("start B");
        let after_b = db.list_plan_items_internal(&objective_id).await.expect("list");
        let by_id: std::collections::HashMap<&str, &str> = after_b
            .iter()
            .map(|i| (i.id.as_str(), i.status.as_str()))
            .collect();
        assert_eq!(by_id[ids[0].as_str()], "pending", "A reverts to pending");
        assert_eq!(by_id[ids[1].as_str()], "active", "B is now active");

        let wm_after_b = db
            .get_working_memory_internal(&agent_id)
            .await
            .expect("wm")
            .expect("row");
        assert_eq!(wm_after_b.active_plan_item_id.as_ref(), Some(&ids[1]));
    }

    #[tokio::test]
    async fn complete_plan_item_clears_active_no_auto_advance() {
        let db = Database::new_in_memory().await.expect("db");
        let (agent_id, objective_id) = seed_agent_and_objective(&db).await;
        db.record_action_transition_internal(&agent_id, &objective_id, "warm up", None)
            .await
            .expect("anchor");
        let ids = seed_plan(&db, &objective_id, &["A", "B"]).await;
        db.start_plan_item_internal(&ids[0]).await.expect("start");

        db.complete_plan_item_internal(&ids[0], Some("done"))
            .await
            .expect("complete");

        let item = db
            .get_plan_item_internal(&ids[0])
            .await
            .expect("get")
            .expect("row");
        assert_eq!(item.status, "completed");
        assert!(item.completed_at.is_some());

        let wm = db
            .get_working_memory_internal(&agent_id)
            .await
            .expect("wm")
            .expect("row");
        // No auto-advance — supervisor decides what's next.
        assert!(wm.active_plan_item_id.is_none());
    }

    #[tokio::test]
    async fn skip_and_block_record_status_and_emit_events() {
        let db = Database::new_in_memory().await.expect("db");
        let (_, objective_id) = seed_agent_and_objective(&db).await;
        let ids = seed_plan(&db, &objective_id, &["A", "B"]).await;

        db.skip_plan_item_internal(&ids[0], Some("not needed"))
            .await
            .expect("skip");
        db.block_plan_item_internal(&ids[1], "waiting on review")
            .await
            .expect("block");

        let items = db.list_plan_items_internal(&objective_id).await.expect("list");
        assert_eq!(items[0].status, "skipped");
        assert_eq!(items[1].status, "blocked");

        let events = db
            .list_objective_events_internal(&objective_id)
            .await
            .expect("events");
        assert!(events.iter().any(|ev| ev.event_type == "plan_item_skipped"));
        assert!(events.iter().any(|ev| ev.event_type == "plan_item_blocked"));
    }

    #[tokio::test]
    async fn coherent_action_stamps_plan_item_id_when_item_active() {
        let db = Database::new_in_memory().await.expect("db");
        let (agent_id, objective_id) = seed_agent_and_objective(&db).await;
        db.record_action_transition_internal(&agent_id, &objective_id, "warm up", None)
            .await
            .expect("anchor");
        let ids = seed_plan(&db, &objective_id, &["A"]).await;
        db.start_plan_item_internal(&ids[0]).await.expect("start");

        db.record_action_transition_internal(
            &agent_id,
            &objective_id,
            "patch handler",
            Some("warmed up"),
        )
        .await
        .expect("transition");

        let events = db
            .list_objective_events_internal(&objective_id)
            .await
            .expect("events");
        let action = events
            .iter()
            .filter(|ev| ev.event_type == "coherent_action")
            .find(|ev| {
                ev.payload_json
                    .as_deref()
                    .map_or(false, |p| p.contains("patch handler"))
            })
            .expect("action row");
        // We wrote the column directly; verify it lands by reading back.
        let plan_item_id_in_row: Option<String> = db
            .conn
            .call({
                let id = action.id.clone();
                move |c| -> tokio_rusqlite::Result<Option<String>> {
                    let v = c.query_row(
                        "SELECT plan_item_id FROM objective_events WHERE id = ?1",
                        params![id],
                        |row| row.get::<_, Option<String>>(0),
                    )?;
                    Ok(v)
                }
            })
            .await
            .expect("query");
        assert_eq!(plan_item_id_in_row.as_ref(), Some(&ids[0]));
    }

    #[tokio::test]
    async fn coherent_action_skips_plan_item_id_when_no_active_item() {
        let db = Database::new_in_memory().await.expect("db");
        let (agent_id, objective_id) = seed_agent_and_objective(&db).await;
        // Plan exists but nothing is active.
        seed_plan(&db, &objective_id, &["A", "B"]).await;

        db.record_action_transition_internal(&agent_id, &objective_id, "freeform exploration", None)
            .await
            .expect("transition");

        let events = db
            .list_objective_events_internal(&objective_id)
            .await
            .expect("events");
        let action = events
            .iter()
            .find(|ev| ev.event_type == "coherent_action")
            .expect("action");
        let plan_item_id_in_row: Option<String> = db
            .conn
            .call({
                let id = action.id.clone();
                move |c| -> tokio_rusqlite::Result<Option<String>> {
                    let v = c.query_row(
                        "SELECT plan_item_id FROM objective_events WHERE id = ?1",
                        params![id],
                        |row| row.get::<_, Option<String>>(0),
                    )?;
                    Ok(v)
                }
            })
            .await
            .expect("query");
        assert!(plan_item_id_in_row.is_none());
    }

    #[tokio::test]
    async fn get_active_plan_item_for_agent_resolves_via_l2() {
        let db = Database::new_in_memory().await.expect("db");
        let (agent_id, objective_id) = seed_agent_and_objective(&db).await;
        db.record_action_transition_internal(&agent_id, &objective_id, "warm up", None)
            .await
            .expect("anchor");
        let ids = seed_plan(&db, &objective_id, &["A"]).await;
        db.start_plan_item_internal(&ids[0]).await.expect("start");

        let resolved = db
            .get_active_plan_item_for_agent_internal(&agent_id)
            .await
            .expect("resolve");
        assert_eq!(resolved.as_ref(), Some(&ids[0]));
    }

    #[tokio::test]
    async fn working_memory_v1_payload_deserializes_with_default_plan_slice() {
        let db = Database::new_in_memory().await.expect("db");
        let (agent_id, _objective_id) = seed_agent_and_objective(&db).await;
        // Manually write a v1-shaped payload (no plan slice fields).
        let now = crate::util::chrono_now();
        let v1_payload = serde_json::json!({
            "schemaVersion": 1,
            "currentObjectiveId": null,
            "currentObjectivePath": [],
            "currentAction": null,
            "recentActions": [],
            "updatedAt": now,
        });
        db.conn
            .call({
                let agent_id = agent_id.clone();
                let v1 = v1_payload.to_string();
                let now = now.clone();
                move |c| -> tokio_rusqlite::Result<()> {
                    c.execute(
                        "INSERT INTO agent_working_memory (agent_id, payload_json, updated_at)
                         VALUES (?1, ?2, ?3)",
                        params![agent_id, v1, now],
                    )?;
                    Ok(())
                }
            })
            .await
            .expect("insert v1 row");

        let wm = db
            .get_working_memory_internal(&agent_id)
            .await
            .expect("wm")
            .expect("row");
        assert_eq!(wm.schema_version, 1);
        assert!(wm.active_plan_item_id.is_none());
        assert!(wm.next_plan_item_ids.is_empty());
    }

    // ---- MON-119: P6 Slice A — objective_reports ----

    #[tokio::test]
    async fn objective_reports_migration_is_idempotent() {
        let db = Database::new_in_memory().await.expect("db");
        // new_in_memory already ran init_schema once. Running it again must
        // not panic — every CREATE TABLE / CREATE INDEX uses IF NOT EXISTS.
        db.init_schema().await.expect("re-run init_schema");
    }

    // ---- P1: quest_* -> objective_* rename migration ----

    #[tokio::test]
    async fn quest_to_objective_rename_migration_converts_and_is_idempotent() {
        // Build a DB carrying a pre-P1 (quest_*) schema fragment with data,
        // WITHOUT running init_schema, then run init_schema and confirm the
        // guarded rename converts tables/columns, preserves data, drops stale
        // indexes, and is a no-op on a second run.
        let conn = Connection::open_in_memory().await.expect("conn");
        conn.call(|c| -> tokio_rusqlite::Result<()> {
            c.execute_batch(
                // Mirror the columns the base init indexes (session_id,
                // agent_id, layer) so this fixture looks like a real existing
                // DB: full base tables + the legacy quest_* columns/tables.
                // foreign_keys=ON + a real messages->quest_nodes FK so the test
                // proves RENAME TO rewrites cross-table FK references (the app
                // runs with foreign_keys=ON; a dangling FK would break inserts).
                "PRAGMA foreign_keys=ON;
                 CREATE TABLE quest_nodes (id TEXT PRIMARY KEY, title TEXT);
                 INSERT INTO quest_nodes (id, title) VALUES ('q1','old work');
                 CREATE TABLE quest_reports (id TEXT PRIMARY KEY, quest_id TEXT REFERENCES quest_nodes(id));
                 CREATE TABLE messages (id INTEGER PRIMARY KEY, session_id TEXT, quest_id TEXT REFERENCES quest_nodes(id));
                 CREATE TABLE agents (id TEXT PRIMARY KEY, current_quest_id TEXT);
                 CREATE TABLE memory_keeper_runs (id INTEGER PRIMARY KEY, quest_id TEXT);
                 CREATE TABLE memories (id INTEGER PRIMARY KEY, agent_id TEXT, layer TEXT, source_quest_id TEXT);
                 CREATE INDEX idx_quest_nodes_root ON quest_nodes(id);",
            )?;
            Ok(())
        })
        .await
        .expect("seed old schema");

        let db = Database { conn };
        db.init_schema().await.expect("init_schema runs rename migration");

        db.conn
            .call(|c| -> tokio_rusqlite::Result<()> {
                let table = |name: &str| -> bool {
                    c.query_row(
                        &format!(
                            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{name}'"
                        ),
                        [],
                        |r| r.get::<_, i64>(0),
                    )
                    .unwrap_or(0)
                        > 0
                };
                let col = |t: &str, name: &str| -> bool {
                    let mut stmt = c.prepare(&format!("PRAGMA table_info({t})")).unwrap();
                    let cols: Vec<String> = stmt
                        .query_map([], |r| r.get::<_, String>(1))
                        .unwrap()
                        .filter_map(Result::ok)
                        .collect();
                    cols.iter().any(|x| x == name)
                };
                let index = |name: &str| -> bool {
                    c.query_row(
                        &format!(
                            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='{name}'"
                        ),
                        [],
                        |r| r.get::<_, i64>(0),
                    )
                    .unwrap_or(0)
                        > 0
                };

                assert!(table("objective_nodes"), "table renamed");
                assert!(!table("quest_nodes"), "old table gone");
                let title: String = c
                    .query_row("SELECT title FROM objective_nodes WHERE id='q1'", [], |r| {
                        r.get(0)
                    })
                    .expect("seeded row preserved across rename");
                assert_eq!(title, "old work");

                assert!(col("messages", "objective_id") && !col("messages", "quest_id"));
                assert!(
                    col("agents", "current_objective_id") && !col("agents", "current_quest_id")
                );
                assert!(col("memory_keeper_runs", "objective_id"));
                assert!(col("memories", "source_objective_id"));
                assert!(col("objective_reports", "objective_id"));
                assert!(!index("idx_quest_nodes_root"), "stale index dropped");

                // RENAME TO must rewrite cross-table FK references so nothing
                // dangles to the old table name under foreign_keys=ON.
                let messages_sql: String = c
                    .query_row("SELECT sql FROM sqlite_master WHERE name='messages'", [], |r| {
                        r.get(0)
                    })
                    .expect("messages schema");
                assert!(
                    messages_sql.contains("objective_nodes")
                        && !messages_sql.contains("quest_nodes"),
                    "messages FK rewritten to objective_nodes (got: {messages_sql})"
                );
                // The rewritten FK is live: a valid target inserts, a bogus one
                // is rejected (proves enforcement points at objective_nodes).
                c.execute("INSERT INTO messages (id, objective_id) VALUES (1, 'q1')", [])
                    .expect("insert with valid FK target");
                assert!(
                    c.execute("INSERT INTO messages (id, objective_id) VALUES (2, 'nope')", [])
                        .is_err(),
                    "FK enforced against objective_nodes"
                );
                Ok(())
            })
            .await
            .expect("verify migrated schema");

        // Second run must be a clean no-op (no quest_* left to rename).
        db.init_schema()
            .await
            .expect("re-run init_schema is idempotent after migration");
    }

    // ---- P1: campaign root (typed node) ----

    #[tokio::test]
    async fn ensure_campaign_root_is_idempotent_and_typed() {
        let db = Database::new_in_memory().await.expect("db");
        let now = crate::util::chrono_now();
        let project_id = db
            .ensure_project_internal(&crate::db::ProjectRow {
                id: "p1".into(),
                name: "Aurora".into(),
                root_path: "/tmp/aurora".into(),
                instructions: None,
                created_at: now.clone(),
                updated_at: now,
                root_objective_id: None,
            })
            .await
            .expect("project");

        let root1 = db
            .ensure_campaign_root_internal(&project_id)
            .await
            .expect("root1");
        let root2 = db
            .ensure_campaign_root_internal(&project_id)
            .await
            .expect("root2");
        assert_eq!(root1, root2, "one campaign root per project (idempotent)");

        // The root is a typed campaign node: self-rooted, unparented.
        let row = db
            .get_objective_internal(&root1)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(row.kind, "campaign");
        assert_eq!(row.root_id, root1);
        assert_eq!(row.parent_id, None);

        // projects.root_objective_id points at it.
        let proj = db
            .get_project_by_path_internal("/tmp/aurora")
            .await
            .expect("proj")
            .expect("row");
        assert_eq!(proj.root_objective_id, Some(root1));
    }

    #[tokio::test]
    async fn auto_create_branches_under_campaign_root_and_skips_projectless() {
        let db = Database::new_in_memory().await.expect("db");
        let now = crate::util::chrono_now();

        // Project-less agent → ephemeral, no objective created.
        let a0 = test_agent("a0");
        db.upsert_agent_internal(&a0).await.expect("a0");
        let none = db
            .auto_create_current_objective_internal("a0", "do a thing here", None)
            .await
            .expect("call");
        assert_eq!(none, None, "project-less agent stays ephemeral");

        // Agent inside a project.
        let project_id = db
            .ensure_project_internal(&crate::db::ProjectRow {
                id: "p1".into(),
                name: "Aurora".into(),
                root_path: "/tmp/aurora".into(),
                instructions: None,
                created_at: now.clone(),
                updated_at: now,
                root_objective_id: None,
            })
            .await
            .expect("project");
        let mut a1 = test_agent("a1");
        a1.project_id = Some(project_id);
        db.upsert_agent_internal(&a1).await.expect("a1");

        let created = db
            .auto_create_current_objective_internal("a1", "migrate the auth module", None)
            .await
            .expect("call")
            .expect("created");
        let row = db
            .get_objective_internal(&created)
            .await
            .expect("get")
            .expect("row");
        let campaign = db
            .get_project_by_path_internal("/tmp/aurora")
            .await
            .expect("p")
            .expect("row")
            .root_objective_id
            .expect("campaign root");
        assert_eq!(row.kind, "objective");
        assert_eq!(row.parent_id.as_deref(), Some(campaign.as_str()), "branch under campaign");
        assert_eq!(row.root_id, campaign, "root_id is the campaign, not self");
        assert_ne!(row.root_id, created, "not a fresh per-turn root");
        assert_eq!(row.assignee_shadow_id.as_deref(), Some("a1"));

        // A live current objective → second call is a no-op.
        let again = db
            .auto_create_current_objective_internal("a1", "another thing entirely", None)
            .await
            .expect("call");
        assert_eq!(again, None, "no second objective while current is live");
    }

    #[tokio::test]
    async fn get_campaign_root_for_agent_resolves_via_project() {
        let db = Database::new_in_memory().await.expect("db");
        let now = crate::util::chrono_now();

        // No project → None.
        let a0 = test_agent("a0");
        db.upsert_agent_internal(&a0).await.expect("a0");
        assert!(db
            .get_campaign_root_for_agent_internal("a0")
            .await
            .expect("call")
            .is_none());

        // Agent in a project with a campaign root → returns the campaign node.
        let project_id = db
            .ensure_project_internal(&crate::db::ProjectRow {
                id: "p1".into(),
                name: "Aurora".into(),
                root_path: "/tmp/aurora".into(),
                instructions: None,
                created_at: now.clone(),
                updated_at: now,
                root_objective_id: None,
            })
            .await
            .expect("project");
        db.ensure_campaign_root_internal(&project_id)
            .await
            .expect("root");
        let mut a1 = test_agent("a1");
        a1.project_id = Some(project_id);
        db.upsert_agent_internal(&a1).await.expect("a1");

        let root = db
            .get_campaign_root_for_agent_internal("a1")
            .await
            .expect("call")
            .expect("campaign root");
        assert_eq!(root.kind, "campaign");
    }

    #[tokio::test]
    async fn upsert_objective_report_inserts_and_fetches_round_trip() {
        let db = Database::new_in_memory().await.expect("db");
        let (_agent_id, objective_id) = seed_agent_and_objective(&db).await;

        let id = db
            .upsert_objective_report_internal(&WriteObjectiveReportPayload {
                id: None,
                objective_id: objective_id.clone(),
                payload: r#"{"summary":"shipped the auth fix"}"#.to_string(),
            })
            .await
            .expect("insert");

        let row = db
            .get_objective_report_by_objective_internal(&objective_id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(row.id, id);
        assert_eq!(row.objective_id, objective_id);
        assert_eq!(row.payload, r#"{"summary":"shipped the auth fix"}"#);
        assert_eq!(row.distilled_by_keeper_run_id, None);
    }

    #[tokio::test]
    async fn upsert_objective_report_replaces_payload_on_conflict() {
        let db = Database::new_in_memory().await.expect("db");
        let (_agent_id, objective_id) = seed_agent_and_objective(&db).await;

        let first = db
            .upsert_objective_report_internal(&WriteObjectiveReportPayload {
                id: None,
                objective_id: objective_id.clone(),
                payload: r#"{"summary":"draft"}"#.to_string(),
            })
            .await
            .expect("first");

        let second = db
            .upsert_objective_report_internal(&WriteObjectiveReportPayload {
                id: None,
                objective_id: objective_id.clone(),
                payload: r#"{"summary":"final"}"#.to_string(),
            })
            .await
            .expect("second");

        // UNIQUE(objective_id) keeps the original row; the second call updates it.
        assert_eq!(
            first, second,
            "upsert keeps the original id on conflict"
        );

        let row = db
            .get_objective_report_by_objective_internal(&objective_id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(row.payload, r#"{"summary":"final"}"#);
        assert!(
            row.updated_at >= row.created_at,
            "updated_at moves forward on revision (created_at={}, updated_at={})",
            row.created_at,
            row.updated_at
        );
    }

    #[tokio::test]
    async fn upsert_objective_report_denormalizes_agent_id_from_objective() {
        let db = Database::new_in_memory().await.expect("db");
        let (agent_id, objective_id) = seed_agent_and_objective(&db).await;

        db.upsert_objective_report_internal(&WriteObjectiveReportPayload {
            id: None,
            objective_id: objective_id.clone(),
            payload: r#"{"summary":"x"}"#.to_string(),
        })
        .await
        .expect("insert");

        let row = db
            .get_objective_report_by_objective_internal(&objective_id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(row.agent_id.as_deref(), Some(agent_id.as_str()));

        let listed = db
            .list_objective_reports_for_agent_internal(&agent_id)
            .await
            .expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].objective_id, objective_id);
    }

    #[tokio::test]
    async fn upsert_objective_report_rejects_unknown_objective_id() {
        let db = Database::new_in_memory().await.expect("db");
        let result = db
            .upsert_objective_report_internal(&WriteObjectiveReportPayload {
                id: None,
                objective_id: "no-such-objective".to_string(),
                payload: r#"{}"#.to_string(),
            })
            .await;
        assert!(
            result.is_err(),
            "writing a report for a non-existent objective must fail"
        );
    }

    #[tokio::test]
    async fn agent_archive_nulls_objective_report_attribution() {
        // ON DELETE SET NULL on agents — deleting the agent leaves the
        // report row in place but clears agent_id so retrieval through the
        // objective still works while attribution stops pointing at a ghost.
        let db = Database::new_in_memory().await.expect("db");
        let (agent_id, objective_id) = seed_agent_and_objective(&db).await;

        db.upsert_objective_report_internal(&WriteObjectiveReportPayload {
            id: None,
            objective_id: objective_id.clone(),
            payload: r#"{"summary":"x"}"#.to_string(),
        })
        .await
        .expect("insert");

        // Hard-delete the agent. (Production uses archive, not delete; this
        // test exercises the FK behavior directly so the constraint is
        // verified without depending on archive semantics.)
        let agent_id_clone = agent_id.clone();
        db.conn
            .call(move |c| -> tokio_rusqlite::Result<()> {
                c.execute("DELETE FROM agents WHERE id = ?1", params![agent_id_clone])?;
                Ok(())
            })
            .await
            .expect("delete agent");

        let row = db
            .get_objective_report_by_objective_internal(&objective_id)
            .await
            .expect("get")
            .expect("row still present");
        assert_eq!(row.agent_id, None, "agent_id should be NULL after delete");

        let listed = db
            .list_objective_reports_for_agent_internal(&agent_id)
            .await
            .expect("list");
        assert!(
            listed.is_empty(),
            "agent listing should not return reports whose agent_id is NULL"
        );
    }
}
