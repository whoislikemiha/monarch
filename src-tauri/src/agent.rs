use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use tauri::{AppHandle, Emitter};

use crate::db::{AgentRow, Database, MessageRow};
use crate::persistence::read_agent_prompt_file;

// ---- Agent state tracking ----

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
    pub session_id: String,
    /// The original create_session JSON, replayed on sidecar crash recovery
    pub create_cmd_json: String,
}

/// Shared agent→session mapping, accessible from both Tauri commands and the reader thread.
type AgentSessionMap = Arc<Mutex<HashMap<String, String>>>;

// ---- Sidecar process management ----

#[allow(dead_code)]
struct SidecarProcess {
    child: Mutex<Child>,
    stdin: Mutex<std::process::ChildStdin>,
}

impl SidecarProcess {
    fn write_command(&self, json: &str) -> Result<(), String> {
        let mut stdin = self.stdin.lock().map_err(|e| e.to_string())?;
        stdin.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
        if !json.ends_with('\n') {
            stdin.write_all(b"\n").map_err(|e| e.to_string())?;
        }
        stdin.flush().map_err(|e| e.to_string())?;
        Ok(())
    }
}

// ---- Agent Manager (manages sidecar + agent state) ----

pub struct AgentManager {
    sidecar: Mutex<Option<Arc<SidecarProcess>>>,
    agents: Mutex<HashMap<String, AgentState>>,
    /// agentId → sessionId mapping, shared with the reader thread
    session_map: AgentSessionMap,
}

impl AgentManager {
    pub fn new() -> Self {
        Self {
            sidecar: Mutex::new(None),
            agents: Mutex::new(HashMap::new()),
            session_map: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn ensure_sidecar(
        &self,
        app: &AppHandle,
        db: &Arc<Database>,
    ) -> Result<Arc<SidecarProcess>, String> {
        let mut sidecar_lock = self.sidecar.lock().map_err(|e| e.to_string())?;

        // Check if existing sidecar is still alive
        if let Some(ref sc) = *sidecar_lock {
            let still_alive = sc.child.lock()
                .ok()
                .and_then(|mut c| c.try_wait().ok())
                .map(|status| status.is_none()) // None = still running
                .unwrap_or(false);
            if still_alive {
                return Ok(sc.clone());
            }
            // Dead — clear it so we respawn below
            eprintln!("[monarch] Sidecar process died, respawning...");
            *sidecar_lock = None;
        }

        // Resolve sidecar path
        let sidecar_script = resolve_sidecar_path()?;

        let mut cmd = Command::new("node");
        cmd.arg(&sidecar_script);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn sidecar: {}", e))?;

        let stdout = child.stdout.take().ok_or("Failed to capture sidecar stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to capture sidecar stderr")?;
        let stdin = child.stdin.take().ok_or("Failed to capture sidecar stdin")?;

        let sc = Arc::new(SidecarProcess {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
        });

        // Stdout reader thread — parse sidecar JSONL events
        let app_clone = app.clone();
        let db_clone = db.clone();
        let session_map_clone = self.session_map.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line) if !line.is_empty() => {
                        handle_sidecar_event(&app_clone, &db_clone, &session_map_clone, &line);
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            eprintln!("[monarch] Sidecar stdout closed");
        });

        // Stderr reader thread — log diagnostics
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                match line {
                    Ok(line) if !line.is_empty() => {
                        eprintln!("[sidecar] {}", line);
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        });

        *sidecar_lock = Some(sc.clone());
        Ok(sc)
    }

    fn send_to_sidecar(&self, json: &str) -> Result<(), String> {
        let sidecar_lock = self.sidecar.lock().map_err(|e| e.to_string())?;
        let sc = sidecar_lock.as_ref().ok_or("Sidecar not running")?;
        sc.write_command(json)
    }

    /// Recover from a dead sidecar: respawn it and recreate all tracked agent sessions
    /// with their full config and session context.
    fn recover_sidecar(
        &self,
        app: &AppHandle,
        db: &Arc<Database>,
    ) -> Result<(), String> {
        self.ensure_sidecar(app, db)?;

        // Snapshot agents and their session mappings
        let agents_snapshot = {
            let agents = self.agents.lock().map_err(|e| e.to_string())?;
            agents.clone()
        };
        let session_snapshot = {
            let map = self.session_map.lock().map_err(|e| e.to_string())?;
            map.clone()
        };

        for (agent_id, state) in &agents_snapshot {
            // Replay the original create_session command (includes cwd, shadow, etc.)
            let _ = self.send_to_sidecar(&state.create_cmd_json);

            // Replay session context from SQLite
            if let Some(session_id) = session_snapshot.get(agent_id) {
                if let Ok(messages) = db.get_messages_with_ancestry(session_id) {
                    if !messages.is_empty() {
                        let msg_array: Vec<serde_json::Value> = messages
                            .iter()
                            .filter(|m| m.role == "user" || m.role == "assistant" || m.role == "toolResult")
                            .map(|m| serde_json::json!({
                                "role": m.role,
                                "content": m.content,
                                "model": m.model,
                            }))
                            .collect();

                        let load_cmd = serde_json::json!({
                            "type": "load_session",
                            "agentId": agent_id,
                            "messages": msg_array,
                        });
                        if let Ok(json) = serde_json::to_string(&load_cmd) {
                            let _ = self.send_to_sidecar(&json);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Send a command to the sidecar, recovering from crash if needed.
    fn send_with_recovery(
        &self,
        app: &AppHandle,
        db: &Arc<Database>,
        json: &str,
    ) -> Result<(), String> {
        // Fast path
        match self.send_to_sidecar(json) {
            Ok(()) => return Ok(()),
            Err(_) => {
                eprintln!("[monarch] Send failed, attempting sidecar recovery...");
            }
        }

        self.recover_sidecar(app, db)?;

        // Retry the original command
        self.send_to_sidecar(json)
    }
}

/// Resolve the sidecar script path
fn resolve_sidecar_path() -> Result<String, String> {
    let candidates = [
        std::env::var("MONARCH_SIDECAR_PATH").ok().map(std::path::PathBuf::from),
        std::env::current_dir().ok().map(|d| d.join("sidecar/dist/index.js")),
        std::env::current_dir().ok().map(|d| d.join("../sidecar/dist/index.js")),
        std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("../../sidecar/dist/index.js"))),
        std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("../../../sidecar/dist/index.js"))),
    ];

    candidates
        .into_iter()
        .flatten()
        .find(|p| p.exists())
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| "Could not find sidecar/dist/index.js".to_string())
}

/// Look up the session_id for an agent from the shared map
fn get_session_id(session_map: &AgentSessionMap, agent_id: &str) -> Option<String> {
    session_map.lock().ok().and_then(|m| m.get(agent_id).cloned())
}

/// Handle a single JSONL event from the sidecar
fn handle_sidecar_event(
    app: &AppHandle,
    db: &Arc<Database>,
    session_map: &AgentSessionMap,
    line: &str,
) {
    let parsed: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[monarch] Failed to parse sidecar event: {} — line: {}", e, line);
            return;
        }
    };

    let event_type = parsed.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let agent_id = parsed.get("agentId").and_then(|a| a.as_str()).unwrap_or("");

    match event_type {
        "session_ready" => {
            let event_name = format!("agent-event-{}", agent_id);
            let ready_event = serde_json::json!({
                "type": "session_ready",
                "agentId": agent_id,
            });
            let _ = app.emit(&event_name, ready_event.to_string());
        }

        "session_destroyed" => {
            let exit_event = format!("agent-exit-{}", agent_id);
            let _ = app.emit(&exit_event, serde_json::json!(null));
        }

        "event" => {
            if let Some(inner_event) = parsed.get("event") {
                let inner_type = inner_event
                    .get("type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("");

                // Persist to DB based on event type
                persist_event(db, session_map, agent_id, inner_type, inner_event);

                // Forward to frontend
                let event_name = format!("agent-event-{}", agent_id);
                let _ = app.emit(&event_name, inner_event.to_string());
            }
        }

        "extension_ui_request" => {
            let event_name = format!("agent-event-{}", agent_id);
            let _ = app.emit(&event_name, line);
        }

        "error" => {
            let error_msg = parsed.get("error").and_then(|e| e.as_str()).unwrap_or("Unknown error");
            eprintln!("[monarch] Sidecar error for {}: {}", agent_id, error_msg);
            // Forward as a notification event the frontend can display
            let event_name = format!("agent-event-{}", agent_id);
            let error_event = serde_json::json!({
                "type": "sidecar_error",
                "error": error_msg,
            });
            let _ = app.emit(&event_name, error_event.to_string());
        }

        _ => {
            eprintln!("[monarch] Unknown sidecar event type: {}", event_type);
        }
    }
}

/// Persist event data to SQLite based on event type
fn persist_event(
    db: &Arc<Database>,
    session_map: &AgentSessionMap,
    agent_id: &str,
    event_type: &str,
    event: &serde_json::Value,
) {
    let session_id = get_session_id(session_map, agent_id);

    // Log all events to the events table
    let data = serde_json::to_string(event).ok();
    let _ = db.log_event_internal(
        Some(agent_id),
        session_id.as_deref(),
        event_type,
        data.as_deref(),
    );

    // Only persist messages if we have a valid session_id
    let session_id = match session_id {
        Some(sid) => sid,
        None => return, // Can't persist without a session
    };

    match event_type {
        "message_end" => {
            if let Some(message) = event.get("message") {
                let role = message
                    .get("role")
                    .and_then(|r| r.as_str())
                    .unwrap_or("unknown");
                let content = if let Some(content) = message.get("content") {
                    serde_json::to_string(content).unwrap_or_default()
                } else {
                    String::new()
                };
                let model = message
                    .get("model")
                    .and_then(|m| m.as_str())
                    .map(String::from);

                let usage = message.get("usage");
                let tokens = usage
                    .and_then(|u| u.get("totalTokens"))
                    .and_then(|t| t.as_i64())
                    .unwrap_or(0) as i32;
                let cost = usage
                    .and_then(|u| u.get("cost"))
                    .and_then(|c| c.as_f64())
                    .or_else(|| {
                        usage
                            .and_then(|u| u.get("cost"))
                            .and_then(|c| c.get("total"))
                            .and_then(|t| t.as_f64())
                    })
                    .unwrap_or(0.0);

                let _ = db.save_message_internal(&MessageRow {
                    id: 0,
                    session_id: session_id.clone(),
                    role: role.to_string(),
                    content,
                    model,
                    tokens,
                    cost,
                    timestamp: chrono_now(),
                });

                // Update session stats
                let _ = db.increment_session_message_count(&session_id, tokens, cost);
            }
        }
        "tool_execution_end" => {
            let tool_call_id = event.get("toolCallId").and_then(|n| n.as_str()).unwrap_or("");
            let tool_name = event.get("toolName").and_then(|n| n.as_str()).unwrap_or("unknown");
            let result = event.get("result").map(|r| serde_json::to_string(r).unwrap_or_default()).unwrap_or_default();
            let is_error = event.get("isError").and_then(|e| e.as_bool()).unwrap_or(false);

            let content = serde_json::json!({
                "toolCallId": tool_call_id,
                "toolName": tool_name,
                "result": result,
                "isError": is_error,
            }).to_string();

            let _ = db.save_message_internal(&MessageRow {
                id: 0,
                session_id,
                role: "toolResult".to_string(),
                content,
                model: None,
                tokens: 0,
                cost: 0.0,
                timestamp: chrono_now(),
            });
        }
        _ => {}
    }
}

fn chrono_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    format!("{}", secs)
}

// ---- Tauri Commands ----

#[tauri::command]
pub fn spawn_agent(
    app: AppHandle,
    state: tauri::State<'_, AgentManager>,
    db: tauri::State<'_, Arc<Database>>,
    id: String,
    session_id: String,
    provider: Option<String>,
    model: Option<String>,
    thinking_level: Option<String>,
    cwd: Option<String>,
    shadow_name: Option<String>,
    shadow_title: Option<String>,
    shadow_grade: Option<String>,
) -> Result<(), String> {
    // Ensure sidecar is running
    state.ensure_sidecar(&app, &db)?;

    let now = chrono_now();
    let provider_value = provider.clone();
    let model_value = model.clone();
    let thinking_value = thinking_level.clone();

    // Persist the agent/session on the backend as the source of truth for FK-safe
    // message logging, even if the frontend-side write was skipped or failed.
    db.upsert_agent_internal(&AgentRow {
        id: id.clone(),
        name: shadow_name
            .clone()
            .or_else(|| shadow_title.clone())
            .unwrap_or_else(|| id.clone()),
        shadow_name: shadow_name.clone(),
        shadow_title: shadow_title.clone(),
        shadow_grade: shadow_grade.clone(),
        provider: provider_value.clone(),
        model: model_value.clone(),
        thinking_level: thinking_value.clone(),
        cwd: cwd.clone(),
        custom_prompt: None,
        created_at: now.clone(),
        updated_at: now.clone(),
    })?;

    if !db.session_exists_internal(&session_id)? {
        db.create_session_internal(&crate::db::SessionRow {
            id: session_id.clone(),
            agent_id: id.clone(),
            pi_session_file: None,
            model: model_value.clone(),
            provider: provider_value.clone(),
            started_at: now.clone(),
            ended_at: None,
            message_count: 0,
            total_tokens: 0,
            total_cost: 0.0,
            parent_session_id: None,
        })?;
    }

    // Register the agent→session mapping so the reader thread can persist events
    {
        let mut map = state.session_map.lock().map_err(|e| e.to_string())?;
        map.insert(id.clone(), session_id.clone());
    }

    // Build create_session command
    let shadow = if shadow_name.is_some() || shadow_title.is_some() || shadow_grade.is_some() {
        Some(serde_json::json!({
            "name": shadow_name.as_deref().unwrap_or("Shadow"),
            "title": shadow_title.as_deref().unwrap_or("Shadow Soldier"),
            "grade": shadow_grade.as_deref().unwrap_or("Knight"),
            "id": &id,
        }))
    } else {
        None
    };

    let cmd = serde_json::json!({
        "type": "create_session",
        "agentId": id,
        "cwd": cwd.as_deref().unwrap_or("."),
        "provider": provider.as_deref().unwrap_or("anthropic"),
        "model": model.as_deref().unwrap_or("claude-sonnet-4-5"),
        "thinkingLevel": thinking_level.as_deref().unwrap_or("medium"),
        "shadow": shadow,
        "customPrompt": read_agent_prompt_file(&id)?
            .filter(|prompt| !prompt.trim().is_empty()),
    });

    let json = serde_json::to_string(&cmd).map_err(|e| e.to_string())?;
    state.send_to_sidecar(&json)?;

    // Track agent state with the full create command for crash recovery
    let mut agents = state.agents.lock().map_err(|e| e.to_string())?;
    agents.insert(
        id.clone(),
        AgentState {
            lifecycle: AgentLifecycleState::Idle,
            provider,
            model,
            thinking_level,
            is_streaming: false,
            session_id,
            create_cmd_json: json,
        },
    );

    Ok(())
}

#[tauri::command]
pub fn send_command(
    app: AppHandle,
    state: tauri::State<'_, AgentManager>,
    db: tauri::State<'_, Arc<Database>>,
    id: String,
    command_json: String,
) -> Result<(), String> {
    let mut cmd: serde_json::Value =
        serde_json::from_str(&command_json).map_err(|e| e.to_string())?;

    if let Some(obj) = cmd.as_object_mut() {
        obj.insert("agentId".to_string(), serde_json::Value::String(id));
    }

    let json = serde_json::to_string(&cmd).map_err(|e| e.to_string())?;
    state.send_with_recovery(&app, &db, &json)
}

/// Send the same prompt to multiple agents simultaneously (Council mode)
#[tauri::command]
pub fn broadcast_prompt(
    app: AppHandle,
    state: tauri::State<'_, AgentManager>,
    db: tauri::State<'_, Arc<Database>>,
    agent_ids: Vec<String>,
    message: String,
) -> Result<(), String> {
    let mut errors = Vec::new();

    for id in &agent_ids {
        let cmd = serde_json::json!({
            "type": "prompt",
            "agentId": id,
            "message": message,
        });
        let json = serde_json::to_string(&cmd).map_err(|e| e.to_string())?;
        if let Err(e) = state.send_with_recovery(&app, &db, &json) {
            errors.push(format!("{}: {}", id, e));
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
    _graceful: Option<bool>,
) -> Result<(), String> {
    let cmd = serde_json::json!({
        "type": "destroy_session",
        "agentId": id,
    });
    let json = serde_json::to_string(&cmd).map_err(|e| e.to_string())?;
    let _ = state.send_to_sidecar(&json);

    // Clean up state
    let mut agents = state.agents.lock().map_err(|e| e.to_string())?;
    agents.remove(&id);
    drop(agents);

    let mut map = state.session_map.lock().map_err(|e| e.to_string())?;
    map.remove(&id);

    Ok(())
}

/// Load messages from a previous SQLite session into the sidecar's agent context.
/// This gives the LLM conversational continuity when restoring.
#[tauri::command]
pub fn load_session_context(
    app: AppHandle,
    state: tauri::State<'_, AgentManager>,
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
    source_session_id: String,
) -> Result<(), String> {
    // Load messages from DB, following parent session chain for full context
    let messages = db.get_messages_with_ancestry(&source_session_id)?;

    if messages.is_empty() {
        return Ok(()); // Nothing to replay
    }

    // Convert to sidecar format — include all message types for full context
    let msg_array: Vec<serde_json::Value> = messages
        .iter()
        .filter(|m| m.role == "user" || m.role == "assistant" || m.role == "toolResult")
        .map(|m| {
            serde_json::json!({
                "role": m.role,
                "content": m.content,
                "model": m.model,
            })
        })
        .collect();

    let cmd = serde_json::json!({
        "type": "load_session",
        "agentId": agent_id,
        "messages": msg_array,
    });

    let json = serde_json::to_string(&cmd).map_err(|e| e.to_string())?;
    state.send_with_recovery(&app, &db, &json)
}

/// Create a new session for an existing agent.
/// Creates a DB row, updates the agent→session mapping, and tells the sidecar to reset.
#[tauri::command]
pub fn new_agent_session(
    app: AppHandle,
    state: tauri::State<'_, AgentManager>,
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
    new_session_id: String,
    parent_session_id: Option<String>,
) -> Result<(), String> {
    // End the old session
    let old_session_id = {
        let map = state.session_map.lock().map_err(|e| e.to_string())?;
        map.get(&agent_id).cloned()
    };
    if let Some(old_sid) = &old_session_id {
        let _ = db.update_session_internal(old_sid, None, None, None, Some(&chrono_now()));
    }

    // Create new session row in DB with optional parent link
    let agent_state = {
        let agents = state.agents.lock().map_err(|e| e.to_string())?;
        agents.get(&agent_id).cloned()
    };
    let (model, provider) = agent_state
        .map(|s| (s.model.clone(), s.provider.clone()))
        .unwrap_or((None, None));

    // Recreate a minimal agent row if the DB entry was pruned or never persisted.
    // This prevents the new session insert from tripping the sessions.agent_id FK.
    db.ensure_agent_exists_internal(&AgentRow {
        id: agent_id.clone(),
        name: agent_id.clone(),
        shadow_name: None,
        shadow_title: None,
        shadow_grade: None,
        provider: provider.clone(),
        model: model.clone(),
        thinking_level: None,
        cwd: None,
        custom_prompt: None,
        created_at: chrono_now(),
        updated_at: chrono_now(),
    })?;

    let valid_parent_session_id = match parent_session_id {
        Some(parent_id) if db.session_exists_internal(&parent_id)? => Some(parent_id),
        _ => None,
    };

    db.create_session_internal(&crate::db::SessionRow {
        id: new_session_id.clone(),
        agent_id: agent_id.clone(),
        pi_session_file: None,
        model,
        provider,
        started_at: chrono_now(),
        ended_at: None,
        message_count: 0,
        total_tokens: 0,
        total_cost: 0.0,
        parent_session_id: valid_parent_session_id,
    })?;

    // Update the agent→session mapping
    {
        let mut map = state.session_map.lock().map_err(|e| e.to_string())?;
        map.insert(agent_id.clone(), new_session_id);
    }

    // Tell the sidecar to reset its in-memory session
    let cmd = serde_json::json!({
        "type": "new_session",
        "agentId": agent_id,
    });
    let json = serde_json::to_string(&cmd).map_err(|e| e.to_string())?;
    state.send_with_recovery(&app, &db, &json)
}

/// Switch an agent to an existing persisted session instead of creating a new one.
/// Resets the sidecar's in-memory conversation and updates DB/session routing so
/// subsequent messages are appended to the selected session.
#[tauri::command]
pub fn switch_agent_session(
    app: AppHandle,
    state: tauri::State<'_, AgentManager>,
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
    session_id: String,
) -> Result<(), String> {
    if !db.session_exists_internal(&session_id)? {
        return Err(format!("Session not found: {}", session_id));
    }

    let old_session_id = {
        let map = state.session_map.lock().map_err(|e| e.to_string())?;
        map.get(&agent_id).cloned()
    };

    if let Some(old_sid) = &old_session_id {
        if old_sid != &session_id {
            let _ = db.update_session_internal(old_sid, None, None, None, Some(&chrono_now()));
        }
    }

    {
        let mut map = state.session_map.lock().map_err(|e| e.to_string())?;
        map.insert(agent_id.clone(), session_id.clone());
    }

    {
        let mut agents = state.agents.lock().map_err(|e| e.to_string())?;
        if let Some(agent) = agents.get_mut(&agent_id) {
            agent.session_id = session_id.clone();
        }
    }

    let cmd = serde_json::json!({
        "type": "new_session",
        "agentId": agent_id,
    });
    let json = serde_json::to_string(&cmd).map_err(|e| e.to_string())?;
    state.send_with_recovery(&app, &db, &json)
}

/// Forward extension UI response from frontend to sidecar
#[tauri::command]
pub fn respond_extension_ui(
    app: AppHandle,
    state: tauri::State<'_, AgentManager>,
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
    request_id: String,
    value: serde_json::Value,
) -> Result<(), String> {
    let cmd = serde_json::json!({
        "type": "extension_ui_response",
        "agentId": agent_id,
        "requestId": request_id,
        "value": value,
    });
    let json = serde_json::to_string(&cmd).map_err(|e| e.to_string())?;
    state.send_with_recovery(&app, &db, &json)
}
