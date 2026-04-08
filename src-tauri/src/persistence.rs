use std::path::PathBuf;

fn monarch_dir() -> PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("monarch");
    std::fs::create_dir_all(&dir).ok();
    dir
}

fn prompts_dir() -> PathBuf {
    let dir = monarch_dir().join("prompts");
    std::fs::create_dir_all(&dir).ok();
    dir
}

#[tauri::command]
pub fn get_agent_prompt(agent_id: String) -> Result<Option<String>, String> {
    let path = prompts_dir().join(format!("{}.md", agent_id));
    if path.exists() {
        std::fs::read_to_string(&path)
            .map(Some)
            .map_err(|e| e.to_string())
    } else {
        Ok(None)
    }
}

#[tauri::command]
pub fn save_agent_prompt(agent_id: String, prompt: String) -> Result<(), String> {
    let path = prompts_dir().join(format!("{}.md", agent_id));
    std::fs::write(&path, prompt).map_err(|e| format!("Failed to save prompt: {}", e))
}

#[tauri::command]
pub fn get_prompts_dir() -> String {
    prompts_dir().to_string_lossy().to_string()
}
