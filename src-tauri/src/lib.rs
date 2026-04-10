mod agent;
mod db;
mod models;
mod persistence;

use agent::AgentManager;
use db::Database;
use models::ModelCache;
use std::sync::Arc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let database = Arc::new(Database::new().expect("Failed to initialize database"));

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AgentManager::new())
        .manage(ModelCache::new())
        .manage(database)
        .invoke_handler(tauri::generate_handler![
            // Agent lifecycle (sidecar-based)
            agent::spawn_agent,
            agent::send_command,
            agent::broadcast_prompt,
            agent::kill_agent,
            agent::load_session_context,
            agent::new_agent_session,
            agent::switch_agent_session,
            agent::respond_extension_ui,
            // Models
            models::get_models,
            models::get_provider_auth_status,
            // Prompt file management
            persistence::get_agent_prompt,
            persistence::save_agent_prompt,
            persistence::get_prompts_dir,
            // SQLite persistence
            db::db_upsert_agent,
            db::db_get_agents,
            db::db_delete_agent,
            db::db_create_session,
            db::db_get_sessions,
            db::db_save_message,
            db::db_get_messages,
            db::db_get_messages_with_ancestry,
            db::db_save_memory,
            db::db_get_memories,
            db::db_log_event,
            // Projects
            db::db_upsert_project,
            db::db_get_projects,
            db::db_get_project_by_path,
            db::db_rename_project,
            db::db_update_project_instructions,
            db::db_delete_project,
            // Project detection
            agent::detect_project,
            agent::read_project_instructions,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
