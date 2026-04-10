use std::path::PathBuf;

use crate::error::MonarchError;

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

pub fn read_agent_prompt_file(agent_id: &str) -> Result<Option<String>, MonarchError> {
    let path = prompts_dir().join(format!("{}.md", agent_id));
    if path.exists() {
        Ok(Some(std::fs::read_to_string(&path)?))
    } else {
        Ok(None)
    }
}

#[tauri::command]
#[specta::specta]
pub fn get_agent_prompt(agent_id: String) -> Result<Option<String>, MonarchError> {
    read_agent_prompt_file(&agent_id)
}

#[tauri::command]
#[specta::specta]
pub fn save_agent_prompt(agent_id: String, prompt: String) -> Result<(), MonarchError> {
    let path = prompts_dir().join(format!("{}.md", agent_id));
    std::fs::write(&path, prompt)?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn get_prompts_dir() -> String {
    prompts_dir().to_string_lossy().to_string()
}

// ---- WebSocket wrappers ----

pub fn ws_save_agent_prompt(agent_id: String, prompt: String) -> Result<(), MonarchError> {
    let path = prompts_dir().join(format!("{}.md", agent_id));
    std::fs::write(&path, prompt)?;
    Ok(())
}

pub fn ws_get_prompts_dir() -> String {
    prompts_dir().to_string_lossy().to_string()
}
