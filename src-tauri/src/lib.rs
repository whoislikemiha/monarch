mod agent;
mod db;
mod models;
mod persistence;
mod toolbox;
mod ws;

use agent::AgentManager;
use db::Database;
use models::ModelCache;
use std::sync::Arc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let database = Arc::new(Database::new().expect("Failed to initialize database"));
    let agent_mgr = Arc::new(AgentManager::new());
    let model_cache = Arc::new(ModelCache::new());

    // Clones for the WS server
    let ws_db = database.clone();
    let ws_agent_mgr = agent_mgr.clone();
    let ws_model_cache = model_cache.clone();
    let ws_broadcast = agent_mgr.ws_broadcast.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(agent_mgr)
        .manage(model_cache)
        .manage(database)
        .setup(move |app| {
            // Store AppHandle so WS-initiated commands can access the sidecar
            ws_agent_mgr.set_app_handle(app.handle().clone());

            // Start the WebSocket bridge server
            let ws_state = Arc::new(ws::WsState {
                db: ws_db,
                agent_mgr: ws_agent_mgr,
                model_cache: ws_model_cache,
                broadcast_rx: ws_broadcast,
            });
            std::thread::spawn(move || {
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to create WS tokio runtime")
                    .block_on(ws::start_ws_server(ws_state));
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Agent lifecycle (sidecar-based)
            agent::spawn_agent,
            agent::send_command,
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
            // Agent templates
            db::db_list_agent_templates,
            db::db_save_agent_template,
            db::db_delete_agent_template,
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
            // UI state
            db::db_get_ui_state,
            db::db_set_ui_state,
            // Toolbox
            toolbox::toolbox_list_tools,
            toolbox::placeholder::toolbox_placeholder_ping,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
