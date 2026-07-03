use serde_json::Value;

use crate::error::MonarchError;
use crate::websocket::WsState;
use crate::websocket::handlers::{agents, memories, misc, plans, projects, objectives, sessions};

/// Dispatch a command to the appropriate internal handler.
/// Adding a new command = adding one match arm here.
pub(crate) async fn dispatch_command(
    state: &WsState,
    cmd: &str,
    args: Value,
) -> Result<Value, MonarchError> {
    match cmd {
        // ---- Agent lifecycle ----
        "spawn_agent" => agents::spawn_agent(state, args).await,
        "send_command" => agents::send_command(state, args).await,
        "kill_agent" => agents::kill_agent(state, args).await,
        "load_session_context" => agents::load_session_context(state, args).await,
        "new_agent_session" => agents::new_agent_session(state, args).await,
        "switch_agent_session" => agents::switch_agent_session(state, args).await,
        "respond_extension_ui" => agents::respond_extension_ui(state, args).await,
        "detect_project" => misc::detect_project(state, args).await,
        "read_project_instructions" => misc::read_project_instructions(state, args).await,
        "list_paths" => misc::list_paths(state, args).await,

        // ---- Models ----
        "get_models" => misc::get_models(state, args).await,
        "get_provider_auth_status" => misc::get_provider_auth_status(state, args).await,

        // ---- Persistence (prompts) ----
        "get_agent_prompt" => misc::get_agent_prompt(state, args).await,
        "save_agent_prompt" => misc::save_agent_prompt(state, args).await,
        "get_prompts_dir" => misc::get_prompts_dir(state, args).await,

        // ---- DB: Agents ----
        "db_upsert_agent" => agents::db_upsert_agent(state, args).await,
        "db_update_agent" => agents::db_update_agent(state, args).await,
        "db_get_agents" => agents::db_get_agents(state, args).await,
        "db_archive_agent" => agents::db_archive_agent(state, args).await,
        "db_unarchive_agent" => agents::db_unarchive_agent(state, args).await,
        "db_delete_agent" => agents::db_delete_agent(state, args).await,

        // ---- DB: Sessions ----
        "db_create_session" => sessions::db_create_session(state, args).await,
        "db_get_sessions" => sessions::db_get_sessions(state, args).await,
        "db_list_session_summaries" => sessions::db_list_session_summaries(state, args).await,
        "db_set_session_title" => sessions::db_set_session_title(state, args).await,
        "get_session_display_items" => sessions::get_session_display_items(state, args).await,

        // ---- DB: Messages ----
        "db_save_message" => sessions::db_save_message(state, args).await,
        "db_get_messages" => sessions::db_get_messages(state, args).await,
        "db_get_messages_with_ancestry" => sessions::db_get_messages_with_ancestry(state, args).await,

        // ---- DB: Memories (MON-99) ----
        "db_list_memories_for_agent" => memories::db_list_memories_for_agent(state, args).await,
        "db_get_memory" => memories::db_get_memory(state, args).await,
        "memory_search_for_agent" => memories::memory_search_for_agent(state, args).await,

        // ---- DB: Events ----
        "db_log_event" => misc::db_log_event(state, args).await,

        // ---- DB: Templates ----
        "db_list_agent_templates" => misc::db_list_agent_templates(state, args).await,
        "db_save_agent_template" => misc::db_save_agent_template(state, args).await,
        "db_delete_agent_template" => misc::db_delete_agent_template(state, args).await,

        // ---- DB: Projects ----
        "db_upsert_project" => projects::db_upsert_project(state, args).await,
        "db_get_projects" => projects::db_get_projects(state, args).await,
        "db_get_project_by_path" => projects::db_get_project_by_path(state, args).await,
        "db_rename_project" => projects::db_rename_project(state, args).await,
        "db_update_project_instructions" => projects::db_update_project_instructions(state, args).await,
        "db_delete_project" => projects::db_delete_project(state, args).await,

        // ---- Toolbox ----
        "toolbox_list_tools" => misc::toolbox_list_tools(state, args).await,
        "toolbox_placeholder_ping" => misc::toolbox_placeholder_ping(state, args).await,

        // ---- DB: Objectives (MON-83) ----
        // Write commands emit the matching `objective-*-{id}` channel via the
        // shared broadcast pipeline so WS subscribers stay in sync without
        // a manual refetch.
        "db_create_objective" => objectives::db_create_objective(state, args).await,
        "db_update_objective" => objectives::db_update_objective(state, args).await,
        "db_get_objective" => objectives::db_get_objective(state, args).await,
        "db_list_objectives_for_agent" => objectives::db_list_objectives_for_agent(state, args).await,
        "db_get_objective_tree_for_root" => objectives::db_get_objective_tree_for_root(state, args).await,
        "db_get_campaign_root_for_agent" => objectives::db_get_campaign_root_for_agent(state, args).await,
        "db_record_objective_event" => objectives::db_record_objective_event(state, args).await,
        "db_list_objective_events" => objectives::db_list_objective_events(state, args).await,
        "db_list_agent_timeline" => objectives::db_list_agent_timeline(state, args).await,
        "db_update_objective_manual" => objectives::db_update_objective_manual(state, args).await,
        "db_record_manual_objective_event" => objectives::db_record_manual_objective_event(state, args).await,
        "db_list_objective_refs" => objectives::db_list_objective_refs(state, args).await,
        "db_create_objective_ref" => objectives::db_create_objective_ref(state, args).await,
        "db_update_objective_ref" => objectives::db_update_objective_ref(state, args).await,
        "db_delete_objective_ref" => objectives::db_delete_objective_ref(state, args).await,
        "db_save_objective_report" => objectives::db_save_objective_report(state, args).await,
        "db_get_objective_report" => objectives::db_get_objective_report(state, args).await,
        "db_list_objective_reports_for_agent" => objectives::db_list_objective_reports_for_agent(state, args).await,
        "db_get_working_memory" => objectives::db_get_working_memory(state, args).await,
        "db_list_plan_items" => plans::db_list_plan_items(state, args).await,
        "db_get_plan_item" => plans::db_get_plan_item(state, args).await,
        "db_set_plan" => plans::db_set_plan(state, args).await,
        "db_add_plan_item" => plans::db_add_plan_item(state, args).await,
        "db_update_plan_item" => plans::db_update_plan_item(state, args).await,
        "db_delete_plan_item" => plans::db_delete_plan_item(state, args).await,
        "db_start_plan_item" => plans::db_start_plan_item(state, args).await,
        "db_complete_plan_item" => plans::db_complete_plan_item(state, args).await,
        "db_skip_plan_item" => plans::db_skip_plan_item(state, args).await,
        "db_block_plan_item" => plans::db_block_plan_item(state, args).await,

        // MON-82: Classifications (read-only over WS).
        "db_list_classifications_for_agent" => objectives::db_list_classifications_for_agent(state, args).await,
        "db_get_classification_for_message" => objectives::db_get_classification_for_message(state, args).await,

        // Agent service record (stats panel).
        "db_get_agent_stats" => agents::db_get_agent_stats(state, args).await,

        // ---- MON-98: Supervisor / agent identity ----
        "get_captain_identity" => agents::get_captain_identity(state, args).await,
        "upsert_captain_identity" => agents::upsert_captain_identity(state, args).await,
        "get_shadow_identity" => agents::get_shadow_identity(state, args).await,
        "upsert_shadow_identity" => agents::upsert_shadow_identity(state, args).await,

        // ---- MON-82: classifier config (global) ----
        "classifier_get_config" => misc::classifier_get_config(state, args).await,
        "classifier_set_config" => misc::classifier_set_config(state, args).await,
        "classifier_get_config_path" => misc::classifier_get_config_path(state, args).await,

        // ---- MON-99: Memory config ----
        "memory_get_config" => memories::memory_get_config(state, args).await,
        "memory_set_config" => memories::memory_set_config(state, args).await,
        "memory_get_config_path" => memories::memory_get_config_path(state, args).await,
        "memory_index_status" => memories::memory_index_status(state, args).await,
        "memory_download_and_init" => memories::memory_download_and_init(state, args).await,
        "memory_smoke_insert" => memories::memory_smoke_insert(state, args).await,

        _ => Err(MonarchError::not_found(format!("command {}", cmd))),
    }
}
