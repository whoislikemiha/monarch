use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShadowIdentity {
    pub shadow_name: String,
    pub shadow_title: String,
    pub shadow_grade: String,
}

/// A session record — one conversation with timestamps and model info
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecord {
    pub session_file: String,
    pub session_id: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub started_at: String,
    pub message_count: Option<u32>,
}

/// Persistent agent — survives restarts, tracks all sessions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedAgent {
    pub id: String,
    pub name: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking_level: Option<String>,
    pub cwd: Option<String>,
    pub shadow: Option<ShadowIdentity>,
    /// Current active session
    pub active_session: Option<SessionRecord>,
    /// All past sessions, newest first
    pub sessions: Vec<SessionRecord>,
}

fn monarch_dir() -> PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("monarch");
    std::fs::create_dir_all(&dir).ok();
    dir
}

fn registry_path() -> PathBuf {
    monarch_dir().join("agents.json")
}

fn prompts_dir() -> PathBuf {
    let dir = monarch_dir().join("prompts");
    std::fs::create_dir_all(&dir).ok();
    dir
}

#[tauri::command]
pub fn save_agents(agents: Vec<SavedAgent>) -> Result<(), String> {
    let path = registry_path();
    let json = serde_json::to_string_pretty(&agents).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("Failed to save agents: {}", e))
}

#[tauri::command]
pub fn load_agents() -> Result<Vec<SavedAgent>, String> {
    let path = registry_path();
    if !path.exists() {
        return Ok(vec![]);
    }
    let json = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| format!("Failed to parse agents: {}", e))
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

/// Read messages from a Pi session file (JSONL format)
#[tauri::command]
pub fn read_session_messages(session_file: String) -> Result<Vec<serde_json::Value>, String> {
    let path = std::path::Path::new(&session_file);
    if !path.exists() {
        return Err(format!("Session file not found: {}", session_file));
    }

    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut messages = Vec::new();

    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
            // Only include actual messages (user/assistant), not metadata
            if entry.get("type").and_then(|t| t.as_str()) == Some("message") {
                if let Some(msg) = entry.get("message") {
                    messages.push(msg.clone());
                }
            }
        }
    }

    Ok(messages)
}
