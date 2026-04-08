mod agent;
mod db;
mod models;
mod persistence;

use agent::AgentManager;
use db::Database;
use models::ModelCache;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let database = Database::new().expect("Failed to initialize database");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AgentManager::new())
        .manage(ModelCache::new())
        .manage(database)
        .invoke_handler(tauri::generate_handler![
            agent::spawn_agent,
            agent::send_command,
            agent::broadcast_prompt,
            agent::kill_agent,
            models::get_models,
            // Legacy JSON persistence (kept for migration)
            persistence::save_agents,
            persistence::load_agents,
            persistence::get_agent_prompt,
            persistence::save_agent_prompt,
            persistence::get_prompts_dir,
            persistence::read_session_messages,
            // SQLite persistence
            db::db_upsert_agent,
            db::db_get_agents,
            db::db_delete_agent,
            db::db_create_session,
            db::db_get_sessions,
            db::db_update_session,
            db::db_save_message,
            db::db_get_messages,
            db::db_save_memory,
            db::db_get_memories,
            db::db_log_event,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
