use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentLifecycleState {
    Idle,
    Busy,
    Stopped,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct AgentState {
    pub lifecycle: AgentLifecycleState,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking_level: Option<String>,
    pub is_streaming: bool,
}

#[allow(dead_code)]
pub struct AgentProcess {
    child: Mutex<Child>,
    stdin: Mutex<std::process::ChildStdin>,
    state: Mutex<AgentState>,
}

impl AgentProcess {
    fn write_command(&self, json: &str) -> Result<(), String> {
        let mut stdin = self.stdin.lock().map_err(|e| e.to_string())?;
        stdin
            .write_all(json.as_bytes())
            .map_err(|e| e.to_string())?;
        if !json.ends_with('\n') {
            stdin.write_all(b"\n").map_err(|e| e.to_string())?;
        }
        stdin.flush().map_err(|e| e.to_string())?;
        Ok(())
    }

    #[allow(dead_code)]
    fn send_pi_command(&self, cmd: &serde_json::Value) -> Result<(), String> {
        let json = serde_json::to_string(cmd).map_err(|e| e.to_string())?;
        self.write_command(&json)
    }
}

pub struct AgentManager {
    agents: Mutex<HashMap<String, Arc<AgentProcess>>>,
}

impl AgentManager {
    pub fn new() -> Self {
        Self {
            agents: Mutex::new(HashMap::new()),
        }
    }

    #[allow(dead_code)]
    pub fn get_agent_state(&self, id: &str) -> Result<AgentState, String> {
        let agents = self.agents.lock().map_err(|e| e.to_string())?;
        let agent = agents.get(id).ok_or("Agent not found")?;
        let state = agent.state.lock().map_err(|e| e.to_string())?;
        Ok(state.clone())
    }

    #[allow(dead_code)]
    pub fn update_agent_state(&self, id: &str, update: impl FnOnce(&mut AgentState)) -> Result<(), String> {
        let agents = self.agents.lock().map_err(|e| e.to_string())?;
        let agent = agents.get(id).ok_or("Agent not found")?;
        let mut state = agent.state.lock().map_err(|e| e.to_string())?;
        update(&mut state);
        Ok(())
    }
}

#[tauri::command]
pub fn spawn_agent(
    app: AppHandle,
    state: tauri::State<'_, AgentManager>,
    id: String,
    provider: Option<String>,
    model: Option<String>,
    thinking_level: Option<String>,
    cwd: Option<String>,
    extensions: Option<Vec<String>>,
    shadow_name: Option<String>,
    shadow_title: Option<String>,
    shadow_grade: Option<String>,
    session_file: Option<String>,
) -> Result<(), String> {
    let mut cmd = Command::new("pi");
    cmd.arg("--mode").arg("rpc");

    // Restore existing session if provided
    if let Some(ref sf) = session_file {
        let path = std::path::Path::new(sf);
        if path.exists() {
            cmd.arg("--session").arg(sf);
        }
    }

    if let Some(ref p) = provider {
        cmd.arg("--provider").arg(p);
    }
    if let Some(ref m) = model {
        cmd.arg("--model").arg(m);
    }
    if let Some(ref t) = thinking_level {
        cmd.arg("--thinking").arg(t);
    }

    // Auto-attach the shadow oath extension if shadow identity is provided
    let has_shadow = shadow_name.is_some() || shadow_title.is_some() || shadow_grade.is_some();
    if has_shadow {
        // Resolve extension path — canonicalize to absolute so Pi can find it regardless of its cwd
        let oath_path = [
            // Env var override
            std::env::var("MONARCH_EXTENSIONS_DIR").ok().map(|d| std::path::PathBuf::from(d).join("shadow-oath.ts")),
            // Relative to cwd (project root)
            std::env::current_dir().ok().map(|d| d.join("extensions/shadow-oath.ts")),
            // cwd might be src-tauri/, go up one
            std::env::current_dir().ok().map(|d| d.join("../extensions/shadow-oath.ts")),
            // Relative to binary (target/debug/monarch -> ../../extensions/)
            std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("../../extensions/shadow-oath.ts"))),
            // Binary might be deeper (target/debug/ -> ../../../extensions/)
            std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("../../../extensions/shadow-oath.ts"))),
        ]
        .into_iter()
        .flatten()
        .find_map(|p| std::fs::canonicalize(&p).ok())
        .unwrap_or_else(|| std::path::PathBuf::from("extensions/shadow-oath.ts"));
        cmd.arg("--extension").arg(&oath_path);

        // Pass shadow identity as environment variables
        if let Some(ref name) = shadow_name {
            cmd.env("SHADOW_NAME", name);
        }
        if let Some(ref title) = shadow_title {
            cmd.env("SHADOW_TITLE", title);
        }
        if let Some(ref grade) = shadow_grade {
            cmd.env("SHADOW_GRADE", grade);
        }
        cmd.env("SHADOW_ID", &id);
        cmd.env("MONARCH_NAME", "Monarch");
    }

    if let Some(ref exts) = extensions {
        for ext in exts {
            cmd.arg("--extension").arg(ext);
        }
    }

    if let Some(ref dir) = cwd {
        cmd.current_dir(dir);
    }

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn pi: {}", e))?;

    let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
    let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;
    let stdin = child.stdin.take().ok_or("Failed to capture stdin")?;

    let agent = Arc::new(AgentProcess {
        child: Mutex::new(child),
        stdin: Mutex::new(stdin),
        state: Mutex::new(AgentState {
            lifecycle: AgentLifecycleState::Idle,
            provider: provider.clone(),
            model: model.clone(),
            thinking_level: thinking_level.clone(),
            is_streaming: false,
        }),
    });

    {
        let mut agents = state.agents.lock().map_err(|e| e.to_string())?;
        agents.insert(id.clone(), agent);
    }

    // Stdout reader thread — forward JSONL events to frontend
    let event_name = format!("agent-event-{}", id);
    let exit_event = format!("agent-exit-{}", id);
    let app_clone = app.clone();
    let agent_ref = {
        let agents = state.agents.lock().map_err(|e| e.to_string())?;
        agents.get(&id).cloned().ok_or("Agent just spawned but not found")?
    };

    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(line) if !line.is_empty() => {
                    let _ = app_clone.emit(&event_name, &line);
                }
                Ok(_) => {} // skip empty lines
                Err(_) => break,
            }
        }
        // Get exit code if possible
        let exit_code = agent_ref
            .child
            .lock()
            .ok()
            .and_then(|mut c| c.try_wait().ok().flatten())
            .map(|s| s.code());
        let _ = app_clone.emit(&exit_event, exit_code);
    });

    // Stderr reader thread — forward diagnostics
    let stderr_event = format!("agent-stderr-{}", id);
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(line) if !line.is_empty() => {
                    let _ = app.emit(&stderr_event, &line);
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub fn send_command(
    state: tauri::State<'_, AgentManager>,
    id: String,
    command_json: String,
) -> Result<(), String> {
    let agents = state.agents.lock().map_err(|e| e.to_string())?;
    let agent = agents.get(&id).ok_or("Agent not found")?;
    agent.write_command(&command_json)
}

/// Send the same prompt to multiple agents simultaneously (Council mode)
#[tauri::command]
pub fn broadcast_prompt(
    state: tauri::State<'_, AgentManager>,
    agent_ids: Vec<String>,
    message: String,
) -> Result<(), String> {
    let prompt_json = serde_json::json!({
        "type": "prompt",
        "message": message,
    })
    .to_string();

    let agents = state.agents.lock().map_err(|e| e.to_string())?;
    let mut errors = Vec::new();

    for id in &agent_ids {
        if let Some(agent) = agents.get(id) {
            if let Err(e) = agent.write_command(&prompt_json) {
                errors.push(format!("{}: {}", id, e));
            }
        } else {
            errors.push(format!("{}: not found", id));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!("Some agents failed: {}", errors.join(", ")))
    }
}

#[tauri::command]
pub fn kill_agent(
    state: tauri::State<'_, AgentManager>,
    id: String,
    graceful: Option<bool>,
) -> Result<(), String> {
    // Send abort with lock held briefly, then release before sleeping
    if graceful.unwrap_or(true) {
        let agents = state.agents.lock().map_err(|e| e.to_string())?;
        if let Some(agent) = agents.get(&id) {
            let _ = agent.write_command(r#"{"type":"abort"}"#);
        }
        drop(agents);
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    let mut agents = state.agents.lock().map_err(|e| e.to_string())?;
    if let Some(agent) = agents.get(&id) {
        let mut child = agent.child.lock().map_err(|e| e.to_string())?;
        let _ = child.kill();
        let _ = child.wait();
    }
    agents.remove(&id);
    Ok(())
}
