mod agent;
mod models;
mod persistence;

use agent::AgentManager;
use models::ModelCache;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AgentManager::new())
        .manage(ModelCache::new())
        .invoke_handler(tauri::generate_handler![
            agent::spawn_agent,
            agent::send_command,
            agent::broadcast_prompt,
            agent::kill_agent,
            models::get_models,
            persistence::save_agents,
            persistence::load_agents,
            persistence::get_agent_prompt,
            persistence::save_agent_prompt,
            persistence::get_prompts_dir,
            persistence::read_session_messages,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
