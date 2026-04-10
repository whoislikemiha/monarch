use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;

use crate::agent::{AgentManager, WsBroadcast};
use crate::db::Database;
use crate::error::MonarchError;
use crate::models::ModelCache;

/// Shared state passed to each WebSocket connection handler
pub struct WsState {
    pub db: Arc<Database>,
    pub agent_mgr: Arc<AgentManager>,
    pub model_cache: Arc<ModelCache>,
    pub broadcast_rx: broadcast::Sender<WsBroadcast>,
}

pub async fn start_ws_server(state: Arc<WsState>) {
    let port: u16 = std::env::var("MONARCH_WS_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3001);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = match TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[monarch-ws] Failed to bind on {}: {}", addr, e);
            return;
        }
    };
    eprintln!("[monarch-ws] Listening on ws://{}", addr);

    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                let state = state.clone();
                tokio::spawn(handle_connection(state, stream, peer));
            }
            Err(e) => {
                eprintln!("[monarch-ws] Accept error: {}", e);
            }
        }
    }
}

async fn handle_connection(state: Arc<WsState>, stream: TcpStream, peer: SocketAddr) {
    let ws_stream = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("[monarch-ws] Handshake failed for {}: {}", peer, e);
            return;
        }
    };
    eprintln!("[monarch-ws] Client connected: {}", peer);

    let (mut ws_tx, mut ws_rx) = ws_stream.split();
    let mut event_rx = state.broadcast_rx.subscribe();
    let mut subscriptions: HashSet<String> = HashSet::new();

    loop {
        tokio::select! {
            // Incoming message from the WebSocket client
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let response = handle_message(&state, &text, &mut subscriptions).await;
                        if let Some(resp) = response {
                            if ws_tx.send(Message::Text(resp.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {} // ping/pong/binary — ignore
                }
            }
            // Broadcast events from the sidecar
            event = event_rx.recv() => {
                match event {
                    Ok(broadcast) => {
                        if subscriptions.contains(&broadcast.event) {
                            let msg = serde_json::json!({
                                "event": broadcast.event,
                                "payload": broadcast.payload,
                            });
                            if ws_tx.send(Message::Text(msg.to_string().into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        eprintln!("[monarch-ws] Client {} lagged, dropped {} events", peer, n);
                    }
                    Err(_) => break,
                }
            }
        }
    }

    eprintln!("[monarch-ws] Client disconnected: {}", peer);
}

async fn handle_message(
    state: &WsState,
    text: &str,
    subscriptions: &mut HashSet<String>,
) -> Option<String> {
    let parsed: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            return Some(serde_json::json!({"error": format!("Invalid JSON: {}", e)}).to_string());
        }
    };

    let id = parsed.get("id").cloned();
    let cmd = parsed.get("cmd").and_then(|c| c.as_str()).unwrap_or("");
    let args = parsed.get("args").cloned().unwrap_or(Value::Object(Default::default()));

    // Event subscription management
    match cmd {
        "listen" => {
            if let Some(event) = args.get("event").and_then(|e| e.as_str()) {
                subscriptions.insert(event.to_string());
            }
            return Some(make_response(id, Ok(Value::Bool(true))));
        }
        "unlisten" => {
            if let Some(event) = args.get("event").and_then(|e| e.as_str()) {
                subscriptions.remove(event);
            }
            return Some(make_response(id, Ok(Value::Bool(true))));
        }
        _ => {}
    }

    let result = dispatch_command(state, cmd, args).await;
    Some(make_response(id, result))
}

fn make_response(id: Option<Value>, result: Result<Value, MonarchError>) -> String {
    match result {
        Ok(val) => {
            let mut resp = serde_json::json!({"result": val});
            if let Some(id) = id {
                resp["id"] = id;
            }
            resp.to_string()
        }
        Err(e) => {
            // Embed the full ErrorDto (kind, message, details) as the JSON-RPC
            // error.data field so WS clients see the same typed shape Tauri
            // clients get via the MonarchError Serialize impl. The top-level
            // `error` string stays human-readable for backwards compatibility.
            let dto = serde_json::to_value(&e).unwrap_or(Value::Null);
            let mut resp = serde_json::json!({
                "error": e.to_string(),
                "errorData": dto,
            });
            if let Some(id) = id {
                resp["id"] = id;
            }
            resp.to_string()
        }
    }
}

/// Dispatch a command to the appropriate internal handler.
/// Adding a new command = adding one match arm here.
async fn dispatch_command(state: &WsState, cmd: &str, args: Value) -> Result<Value, MonarchError> {
    match cmd {
        // ---- Agent lifecycle ----
        "spawn_agent" => {
            // MON-35: single-shot typed decode. `SpawnAgentRequest` is the
            // shared wire contract between the Tauri command and the WS
            // bridge, so the serde round-trip validates the payload instead
            // of per-field `str_field` / `opt_str` extraction.
            let req: crate::agent::SpawnAgentRequest = serde_json::from_value(args)?;
            crate::agent::ws_spawn_agent(&state.agent_mgr, &state.db, req)?;
            Ok(Value::Null)
        }
        "send_command" => {
            let id = str_field(&args, "id")?;
            let command_json = str_field(&args, "commandJson")?;
            crate::agent::ws_send_command(&state.agent_mgr, &state.db, id, command_json)?;
            Ok(Value::Null)
        }
        "kill_agent" => {
            let id = str_field(&args, "id")?;
            crate::agent::ws_kill_agent(&state.agent_mgr, id)?;
            Ok(Value::Null)
        }
        "load_session_context" => {
            let agent_id = str_field(&args, "agentId")?;
            let source_session_id = str_field(&args, "sourceSessionId")?;
            crate::agent::ws_load_session_context(&state.agent_mgr, &state.db, agent_id, source_session_id)?;
            Ok(Value::Null)
        }
        "new_agent_session" => {
            let agent_id = str_field(&args, "agentId")?;
            let new_session_id = str_field(&args, "newSessionId")?;
            let parent_session_id = opt_str(&args, "parentSessionId");
            crate::agent::ws_new_agent_session(&state.agent_mgr, &state.db, agent_id, new_session_id, parent_session_id)?;
            Ok(Value::Null)
        }
        "switch_agent_session" => {
            let agent_id = str_field(&args, "agentId")?;
            let session_id = str_field(&args, "sessionId")?;
            crate::agent::ws_switch_agent_session(&state.agent_mgr, &state.db, agent_id, session_id)?;
            Ok(Value::Null)
        }
        "respond_extension_ui" => {
            let agent_id = str_field(&args, "agentId")?;
            let request_id = str_field(&args, "requestId")?;
            let value = args.get("value").cloned().unwrap_or(Value::Null);
            crate::agent::ws_respond_extension_ui(&state.agent_mgr, &state.db, agent_id, request_id, value)?;
            Ok(Value::Null)
        }
        "detect_project" => {
            let cwd = str_field(&args, "cwd")?;
            let result = crate::agent::ws_detect_project(&state.db, cwd)?;
            Ok(result.unwrap_or(Value::Null))
        }
        "read_project_instructions" => {
            let cwd = str_field(&args, "cwd")?;
            let result = crate::agent::ws_read_project_instructions(cwd)?;
            Ok(result.map(Value::String).unwrap_or(Value::Null))
        }

        // ---- Models ----
        "get_models" => {
            let provider = str_field(&args, "provider")?;
            let models = crate::models::ws_get_models(&state.model_cache, provider).await?;
            serde_json::to_value(models).map_err(MonarchError::from)
        }
        "get_provider_auth_status" => {
            let provider = str_field(&args, "provider")?;
            let status = crate::models::ws_get_provider_auth_status(provider)?;
            serde_json::to_value(status).map_err(MonarchError::from)
        }

        // ---- Persistence (prompts) ----
        "get_agent_prompt" => {
            let agent_id = str_field(&args, "agentId")?;
            let result = crate::persistence::read_agent_prompt_file(&agent_id)?;
            Ok(result.map(Value::String).unwrap_or(Value::Null))
        }
        "save_agent_prompt" => {
            let agent_id = str_field(&args, "agentId")?;
            let prompt = str_field(&args, "prompt")?;
            crate::persistence::write_agent_prompt_file(&agent_id, &prompt)?;
            Ok(Value::Null)
        }
        "get_prompts_dir" => {
            Ok(Value::String(crate::persistence::prompts_dir_string()))
        }

        // ---- DB: Agents ----
        "db_upsert_agent" => {
            let agent = serde_json::from_value(args.get("agent").cloned().unwrap_or(args.clone()))
                .map_err(|e| MonarchError::invalid_input(format!("Invalid agent: {}", e)))?;
            state.db.upsert_agent_internal(&agent)?;
            Ok(Value::Null)
        }
        "db_get_agents" => {
            let agents = crate::db::ws_get_agents(&state.db)?;
            serde_json::to_value(agents).map_err(MonarchError::from)
        }
        "db_delete_agent" => {
            let agent_id = str_field(&args, "agentId")?;
            crate::db::ws_delete_agent(&state.db, agent_id)?;
            Ok(Value::Null)
        }

        // ---- DB: Sessions ----
        "db_create_session" => {
            let session = serde_json::from_value(args.get("session").cloned().unwrap_or(args.clone()))
                .map_err(|e| MonarchError::invalid_input(format!("Invalid session: {}", e)))?;
            state.db.create_session_internal(&session)?;
            Ok(Value::Null)
        }
        "db_get_sessions" => {
            let agent_id = str_field(&args, "agentId")?;
            let sessions = crate::db::ws_get_sessions(&state.db, agent_id)?;
            serde_json::to_value(sessions).map_err(MonarchError::from)
        }

        // ---- DB: Messages ----
        "db_save_message" => {
            let message = serde_json::from_value(args.get("message").cloned().unwrap_or(args.clone()))
                .map_err(|e| MonarchError::invalid_input(format!("Invalid message: {}", e)))?;
            let id = state.db.save_message_internal(&message)?;
            Ok(Value::Number(id.into()))
        }
        "db_get_messages" => {
            let session_id = str_field(&args, "sessionId")?;
            let messages = crate::db::ws_get_messages(&state.db, session_id)?;
            serde_json::to_value(messages).map_err(MonarchError::from)
        }
        "db_get_messages_with_ancestry" => {
            let session_id = str_field(&args, "sessionId")?;
            let messages = state.db.get_messages_with_ancestry(&session_id)?;
            serde_json::to_value(messages).map_err(MonarchError::from)
        }

        // ---- DB: Memories ----
        "db_save_memory" => {
            let memory = serde_json::from_value(args.get("memory").cloned().unwrap_or(args.clone()))
                .map_err(|e| MonarchError::invalid_input(format!("Invalid memory: {}", e)))?;
            let id = crate::db::ws_save_memory(&state.db, memory)?;
            Ok(Value::Number(id.into()))
        }
        "db_get_memories" => {
            let agent_id = opt_str(&args, "agentId");
            let layer = opt_str(&args, "layer");
            let memories = crate::db::ws_get_memories(&state.db, agent_id, layer)?;
            serde_json::to_value(memories).map_err(MonarchError::from)
        }

        // ---- DB: Events ----
        "db_log_event" => {
            let agent_id = opt_str(&args, "agentId");
            let session_id = opt_str(&args, "sessionId");
            let event_type = str_field(&args, "eventType")?;
            let data = opt_str(&args, "data");
            state.db.log_event_internal(
                agent_id.as_deref(),
                session_id.as_deref(),
                &event_type,
                data.as_deref(),
            )?;
            Ok(Value::Null)
        }

        // ---- DB: Templates ----
        "db_list_agent_templates" => {
            let templates = crate::db::ws_list_agent_templates(&state.db)?;
            serde_json::to_value(templates).map_err(MonarchError::from)
        }
        "db_save_agent_template" => {
            let template = serde_json::from_value(args.get("template").cloned().unwrap_or(args.clone()))
                .map_err(|e| MonarchError::invalid_input(format!("Invalid template: {}", e)))?;
            crate::db::ws_save_agent_template(&state.db, template)?;
            Ok(Value::Null)
        }
        "db_delete_agent_template" => {
            let template_id = str_field(&args, "templateId")?;
            crate::db::ws_delete_agent_template(&state.db, template_id)?;
            Ok(Value::Null)
        }

        // ---- DB: Projects ----
        "db_upsert_project" => {
            let project = serde_json::from_value(args.get("project").cloned().unwrap_or(args.clone()))
                .map_err(|e| MonarchError::invalid_input(format!("Invalid project: {}", e)))?;
            crate::db::ws_upsert_project(&state.db, project)?;
            Ok(Value::Null)
        }
        "db_get_projects" => {
            let projects = crate::db::ws_get_projects(&state.db)?;
            serde_json::to_value(projects).map_err(MonarchError::from)
        }
        "db_get_project_by_path" => {
            let root_path = str_field(&args, "rootPath")?;
            let project = state.db.get_project_by_path_internal(&root_path)?;
            serde_json::to_value(project).map_err(MonarchError::from)
        }
        "db_rename_project" => {
            let project_id = str_field(&args, "projectId")?;
            let name = str_field(&args, "name")?;
            crate::db::ws_rename_project(&state.db, project_id, name)?;
            Ok(Value::Null)
        }
        "db_update_project_instructions" => {
            let project_id = str_field(&args, "projectId")?;
            let instructions = opt_str(&args, "instructions");
            crate::db::ws_update_project_instructions(&state.db, project_id, instructions)?;
            Ok(Value::Null)
        }
        "db_delete_project" => {
            let project_id = str_field(&args, "projectId")?;
            crate::db::ws_delete_project(&state.db, project_id)?;
            Ok(Value::Null)
        }

        // ---- Toolbox ----
        "toolbox_list_tools" => {
            let tools = crate::toolbox::ws_toolbox_list_tools();
            serde_json::to_value(tools).map_err(MonarchError::from)
        }
        "toolbox_placeholder_ping" => {
            let result = crate::toolbox::placeholder::ws_toolbox_placeholder_ping()?;
            Ok(Value::String(result))
        }

        _ => Err(MonarchError::not_found(format!("command {}", cmd))),
    }
}

// ---- Helpers ----

fn str_field(args: &Value, key: &str) -> Result<String, MonarchError> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| MonarchError::invalid_input(format!("Missing required field: {}", key)))
}

fn opt_str(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}
