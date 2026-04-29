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
use crate::memory_index::MemoryIndex;
use crate::models::ModelCache;

/// Shared state passed to each WebSocket connection handler
pub struct WsState {
    pub db: Arc<Database>,
    pub agent_mgr: Arc<AgentManager>,
    pub model_cache: Arc<ModelCache>,
    pub memory_index: Arc<MemoryIndex>,
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
    let args = parsed
        .get("args")
        .cloned()
        .unwrap_or(Value::Object(Default::default()));

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
pub(crate) async fn dispatch_command(
    state: &WsState,
    cmd: &str,
    args: Value,
) -> Result<Value, MonarchError> {
    match cmd {
        // ---- Agent lifecycle ----
        "spawn_agent" => {
            // MON-35: single-shot typed decode. `SpawnAgentRequest` is the
            // shared wire contract between the Tauri command and the WS
            // bridge, so the serde round-trip validates the payload instead
            // of per-field `str_field` / `opt_str` extraction.
            let req: crate::agent::SpawnAgentRequest = serde_json::from_value(args)?;
            let app = state.agent_mgr.get_app_handle()?;
            state.agent_mgr.spawn(&app, &state.db, req).await?;
            Ok(Value::Null)
        }
        "send_command" => {
            let id = str_field(&args, "id")?;
            let command_json = str_field(&args, "commandJson")?;
            let app = state.agent_mgr.get_app_handle()?;
            state
                .agent_mgr
                .send_command(&app, &state.db, id, command_json)
                .await?;
            Ok(Value::Null)
        }
        "kill_agent" => {
            let id = str_field(&args, "id")?;
            state.agent_mgr.kill(&id).await?;
            Ok(Value::Null)
        }
        "load_session_context" => {
            let agent_id = str_field(&args, "agentId")?;
            let source_session_id = str_field(&args, "sourceSessionId")?;
            let app = state.agent_mgr.get_app_handle()?;
            state
                .agent_mgr
                .load_session_context(&app, &state.db, agent_id, source_session_id)
                .await?;
            Ok(Value::Null)
        }
        "new_agent_session" => {
            let agent_id = str_field(&args, "agentId")?;
            let new_session_id = str_field(&args, "newSessionId")?;
            let parent_session_id = opt_str(&args, "parentSessionId");
            let app = state.agent_mgr.get_app_handle()?;
            state
                .agent_mgr
                .new_session(&app, &state.db, agent_id, new_session_id, parent_session_id)
                .await?;
            Ok(Value::Null)
        }
        "switch_agent_session" => {
            let agent_id = str_field(&args, "agentId")?;
            let session_id = str_field(&args, "sessionId")?;
            let app = state.agent_mgr.get_app_handle()?;
            state
                .agent_mgr
                .switch_session(&app, &state.db, agent_id, session_id)
                .await?;
            Ok(Value::Null)
        }
        "respond_extension_ui" => {
            let req: crate::agent::ExtensionUiResponseRequest = serde_json::from_value(args)?;
            let app = state.agent_mgr.get_app_handle()?;
            state
                .agent_mgr
                .respond_extension_ui(&app, &state.db, req)
                .await?;
            Ok(Value::Null)
        }
        "detect_project" => {
            let cwd = str_field(&args, "cwd")?;
            let result = crate::project::detect_project(&state.db, &cwd).await?;
            Ok(result.unwrap_or(Value::Null))
        }
        "read_project_instructions" => {
            let cwd = str_field(&args, "cwd")?;
            let result = crate::project::read_project_instructions(&cwd);
            Ok(result.map(Value::String).unwrap_or(Value::Null))
        }
        "list_paths" => {
            let cwd = str_field(&args, "cwd")?;
            let query = str_field(&args, "query")?;
            let result =
                tokio::task::spawn_blocking(move || crate::mention::list_paths_inner(&cwd, &query))
                    .await
                    .map_err(|e| {
                        MonarchError::persistence(format!("list_paths join error: {e}"))
                    })??;
            serde_json::to_value(result).map_err(MonarchError::from)
        }

        // ---- Models ----
        "get_models" => {
            let provider = str_field(&args, "provider")?;
            let force_refresh = args
                .get("forceRefresh")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let models =
                crate::models::ws_get_models(&state.model_cache, provider, force_refresh).await?;
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
            let result = crate::persistence::read_agent_prompt_file(&agent_id).await?;
            Ok(result.map(Value::String).unwrap_or(Value::Null))
        }
        "save_agent_prompt" => {
            let agent_id = str_field(&args, "agentId")?;
            let prompt = str_field(&args, "prompt")?;
            crate::persistence::write_agent_prompt_file(&agent_id, &prompt).await?;
            Ok(Value::Null)
        }
        "get_prompts_dir" => Ok(Value::String(
            crate::persistence::prompts_dir_string().await?,
        )),

        // ---- DB: Agents ----
        "db_upsert_agent" => {
            let agent = serde_json::from_value(args.get("agent").cloned().unwrap_or(args.clone()))
                .map_err(|e| MonarchError::invalid_input(format!("Invalid agent: {}", e)))?;
            state.db.upsert_agent_internal(&agent).await?;
            Ok(Value::Null)
        }
        "db_update_agent" => {
            let payload =
                serde_json::from_value(args.get("payload").cloned().unwrap_or(args.clone()))
                    .map_err(|e| MonarchError::invalid_input(format!("Invalid payload: {}", e)))?;
            state.db.update_agent_internal(&payload).await?;
            Ok(Value::Null)
        }
        "db_get_agents" => {
            let include_archived = args
                .get("includeArchived")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let agents = state.db.get_agents_internal(include_archived).await?;
            serde_json::to_value(agents).map_err(MonarchError::from)
        }
        "db_archive_agent" => {
            let agent_id = str_field(&args, "agentId")?;
            state.db.archive_agent_internal(&agent_id).await?;
            Ok(Value::Null)
        }
        "db_unarchive_agent" => {
            let agent_id = str_field(&args, "agentId")?;
            state.db.unarchive_agent_internal(&agent_id).await?;
            Ok(Value::Null)
        }
        "db_delete_agent" => {
            let agent_id = str_field(&args, "agentId")?;
            state.db.delete_agent_internal(&agent_id).await?;
            Ok(Value::Null)
        }

        // ---- DB: Sessions ----
        "db_create_session" => {
            let session =
                serde_json::from_value(args.get("session").cloned().unwrap_or(args.clone()))
                    .map_err(|e| MonarchError::invalid_input(format!("Invalid session: {}", e)))?;
            state.db.create_session_internal(&session).await?;
            Ok(Value::Null)
        }
        "db_get_sessions" => {
            let agent_id = str_field(&args, "agentId")?;
            let sessions = state.db.get_sessions_internal(&agent_id).await?;
            serde_json::to_value(sessions).map_err(MonarchError::from)
        }

        // ---- DB: Messages ----
        "db_save_message" => {
            let message =
                serde_json::from_value(args.get("message").cloned().unwrap_or(args.clone()))
                    .map_err(|e| MonarchError::invalid_input(format!("Invalid message: {}", e)))?;
            let id = state.db.save_message_internal(&message).await?;
            Ok(Value::Number(id.into()))
        }
        "db_get_messages" => {
            let session_id = str_field(&args, "sessionId")?;
            let messages = state.db.get_messages_internal(&session_id).await?;
            serde_json::to_value(messages).map_err(MonarchError::from)
        }
        "db_get_messages_with_ancestry" => {
            let session_id = str_field(&args, "sessionId")?;
            let messages = state.db.get_messages_with_ancestry(&session_id).await?;
            serde_json::to_value(messages).map_err(MonarchError::from)
        }

        // ---- DB: Memories (MON-99) ----
        "db_list_memories_for_agent" => {
            let agent_id = str_field(&args, "agentId")?;
            let memories = state.db.list_memories_for_agent_internal(&agent_id).await?;
            serde_json::to_value(memories).map_err(MonarchError::from)
        }
        "db_get_memory" => {
            let id: i64 = args
                .get("id")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| MonarchError::invalid_input("missing id"))?;
            let memory = state.db.get_memory_internal(id).await?;
            serde_json::to_value(memory).map_err(MonarchError::from)
        }
        "memory_search_for_agent" => {
            let agent_id = str_field(&args, "agentId")?;
            let query = str_field(&args, "query")?;
            let top_k = args.get("topK").and_then(|v| v.as_u64()).map(|v| v as u32);
            let results = crate::memory_search::search_memories_for_agent_internal(
                &state.db,
                &state.memory_index,
                &agent_id,
                &query,
                top_k,
            )
            .await?;
            serde_json::to_value(results).map_err(MonarchError::from)
        }

        // ---- DB: Events ----
        "db_log_event" => {
            let agent_id = opt_str(&args, "agentId");
            let session_id = opt_str(&args, "sessionId");
            let event_type = str_field(&args, "eventType")?;
            let data = opt_str(&args, "data");
            state
                .db
                .log_event_internal(
                    agent_id.as_deref(),
                    session_id.as_deref(),
                    &event_type,
                    data.as_deref(),
                )
                .await?;
            Ok(Value::Null)
        }

        // ---- DB: Templates ----
        "db_list_agent_templates" => {
            let templates = state.db.list_agent_templates_internal().await?;
            serde_json::to_value(templates).map_err(MonarchError::from)
        }
        "db_save_agent_template" => {
            let template =
                serde_json::from_value(args.get("template").cloned().unwrap_or(args.clone()))
                    .map_err(|e| MonarchError::invalid_input(format!("Invalid template: {}", e)))?;
            state.db.save_agent_template_internal(&template).await?;
            Ok(Value::Null)
        }
        "db_delete_agent_template" => {
            let template_id = str_field(&args, "templateId")?;
            state
                .db
                .delete_agent_template_internal(&template_id)
                .await?;
            Ok(Value::Null)
        }

        // ---- DB: Projects ----
        "db_upsert_project" => {
            let project =
                serde_json::from_value(args.get("project").cloned().unwrap_or(args.clone()))
                    .map_err(|e| MonarchError::invalid_input(format!("Invalid project: {}", e)))?;
            state.db.upsert_project_internal(&project).await?;
            Ok(Value::Null)
        }
        "db_get_projects" => {
            let projects = state.db.get_projects_internal().await?;
            serde_json::to_value(projects).map_err(MonarchError::from)
        }
        "db_get_project_by_path" => {
            let root_path = str_field(&args, "rootPath")?;
            let project = state.db.get_project_by_path_internal(&root_path).await?;
            serde_json::to_value(project).map_err(MonarchError::from)
        }
        "db_rename_project" => {
            let project_id = str_field(&args, "projectId")?;
            let name = str_field(&args, "name")?;
            state.db.rename_project_internal(&project_id, &name).await?;
            Ok(Value::Null)
        }
        "db_update_project_instructions" => {
            let project_id = str_field(&args, "projectId")?;
            let instructions = opt_str(&args, "instructions");
            state
                .db
                .update_project_instructions_internal(&project_id, instructions.as_deref())
                .await?;
            Ok(Value::Null)
        }
        "db_delete_project" => {
            let project_id = str_field(&args, "projectId")?;
            state.db.delete_project_internal(&project_id).await?;
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

        // ---- DB: Quests (MON-83) ----
        // Write commands emit the matching `quest-*-{id}` channel via the
        // shared broadcast pipeline so WS subscribers stay in sync without
        // a manual refetch.
        "db_create_quest" => {
            let payload: crate::db::CreateQuestPayload =
                serde_json::from_value(args.get("payload").cloned().unwrap_or(args.clone()))
                    .map_err(|e| MonarchError::invalid_input(format!("Invalid payload: {}", e)))?;
            let id = state.db.create_quest_internal(&payload).await?;
            let app = state.agent_mgr.get_app_handle()?;
            crate::agent::emit_event(
                &app,
                &state.agent_mgr.ws_broadcast,
                &format!("quest-created-{}", id),
                &serde_json::json!({ "id": id }).to_string(),
            );
            Ok(Value::String(id))
        }
        "db_update_quest" => {
            let payload: crate::db::UpdateQuestPayload =
                serde_json::from_value(args.get("payload").cloned().unwrap_or(args.clone()))
                    .map_err(|e| MonarchError::invalid_input(format!("Invalid payload: {}", e)))?;
            let id = payload.id.clone();
            let before = state.db.get_quest_internal(&id).await?;
            state.db.update_quest_internal(&payload).await?;
            let after = state.db.get_quest_internal(&id).await?;
            let app = state.agent_mgr.get_app_handle()?;
            crate::agent::emit_event(
                &app,
                &state.agent_mgr.ws_broadcast,
                &format!("quest-updated-{}", id),
                &serde_json::json!({ "id": id }).to_string(),
            );
            if let Some(after_quest) = after.as_ref() {
                if after_quest.root_id != after_quest.id {
                    crate::agent::emit_event(
                        &app,
                        &state.agent_mgr.ws_broadcast,
                        &format!("quest-updated-{}", after_quest.root_id),
                        &serde_json::json!({ "id": after_quest.id, "rootId": after_quest.root_id })
                            .to_string(),
                    );
                }
            }
            crate::db::handle_quest_update_side_effects(
                &app,
                &state.db,
                &state.agent_mgr,
                before,
                after,
            )
            .await?;
            Ok(Value::Null)
        }
        "db_get_quest" => {
            let quest_id = str_field(&args, "questId")?;
            let quest = state.db.get_quest_internal(&quest_id).await?;
            serde_json::to_value(quest).map_err(MonarchError::from)
        }
        "db_list_quests_for_agent" => {
            let agent_id = str_field(&args, "agentId")?;
            let quests = state.db.list_quests_for_agent_internal(&agent_id).await?;
            serde_json::to_value(quests).map_err(MonarchError::from)
        }
        "db_get_quest_tree_for_root" => {
            let root_id = str_field(&args, "rootId")?;
            let tree = state.db.get_quest_tree_for_root_internal(&root_id).await?;
            serde_json::to_value(tree).map_err(MonarchError::from)
        }
        "db_record_quest_event" => {
            let payload: crate::db::RecordQuestEventPayload =
                serde_json::from_value(args.get("payload").cloned().unwrap_or(args.clone()))
                    .map_err(|e| MonarchError::invalid_input(format!("Invalid payload: {}", e)))?;
            let quest_id = payload.quest_id.clone();
            let event_type = payload.event_type.clone();
            let id = state.db.record_quest_event_internal(&payload).await?;
            let app = state.agent_mgr.get_app_handle()?;
            crate::agent::emit_event(
                &app,
                &state.agent_mgr.ws_broadcast,
                &format!("quest-event-{}", quest_id),
                &serde_json::json!({ "id": id, "eventType": event_type }).to_string(),
            );
            Ok(Value::String(id))
        }
        "db_list_quest_events" => {
            let quest_id = str_field(&args, "questId")?;
            let events = state.db.list_quest_events_internal(&quest_id).await?;
            serde_json::to_value(events).map_err(MonarchError::from)
        }
        "db_get_working_memory" => {
            let agent_id = str_field(&args, "agentId")?;
            let wm = state.db.get_working_memory_internal(&agent_id).await?;
            serde_json::to_value(wm).map_err(MonarchError::from)
        }

        // MON-82: Classifications (read-only over WS).
        "db_list_classifications_for_agent" => {
            let agent_id = str_field(&args, "agentId")?;
            let limit = args.get("limit").and_then(|v| v.as_i64());
            let rows = state
                .db
                .list_classifications_for_agent_internal(&agent_id, limit)
                .await?;
            serde_json::to_value(rows).map_err(MonarchError::from)
        }
        "db_get_classification_for_message" => {
            let message_id = args
                .get("messageId")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| MonarchError::invalid_input("messageId required"))?;
            let row = state
                .db
                .get_classification_for_message_internal(message_id)
                .await?;
            serde_json::to_value(row).map_err(MonarchError::from)
        }

        // ---- MON-98: Captain / shadow identity ----
        "get_captain_identity" => {
            let row = state.db.get_captain_identity_internal().await?;
            serde_json::to_value(row).map_err(MonarchError::from)
        }
        "upsert_captain_identity" => {
            let req: crate::agent::commands::UpsertCaptainIdentityRequest =
                serde_json::from_value(args)
                    .map_err(|e| MonarchError::invalid_input(format!("Invalid request: {}", e)))?;
            state
                .db
                .upsert_captain_identity_internal(&req.name, &req.payload, req.edit_note.as_deref())
                .await?;
            let payload = if req.payload.is_empty() {
                None
            } else {
                Some(req.payload)
            };
            state.agent_mgr.refresh_captain_identity(payload).await?;
            Ok(Value::Null)
        }
        "get_shadow_identity" => {
            let agent_id = str_field(&args, "agentId")?;
            let row = state.db.get_shadow_identity_internal(&agent_id).await?;
            serde_json::to_value(row).map_err(MonarchError::from)
        }
        "upsert_shadow_identity" => {
            let req: crate::agent::commands::UpsertShadowIdentityRequest =
                serde_json::from_value(args)
                    .map_err(|e| MonarchError::invalid_input(format!("Invalid request: {}", e)))?;
            state
                .db
                .upsert_shadow_identity_internal(
                    &req.agent_id,
                    &req.payload,
                    req.edit_note.as_deref(),
                )
                .await?;
            let payload = if req.payload.is_empty() {
                None
            } else {
                Some(req.payload)
            };
            state
                .agent_mgr
                .refresh_shadow_identity(&req.agent_id, payload)
                .await?;
            Ok(Value::Null)
        }

        // ---- MON-99: Memory config ----
        "memory_get_config" => {
            let cfg = crate::memory_config::resolved().await;
            serde_json::to_value(cfg).map_err(MonarchError::from)
        }
        "memory_set_config" => {
            let raw: crate::memory_config::MemoryConfig =
                serde_json::from_value(args).map_err(|e| {
                    MonarchError::invalid_input(format!("Invalid memory config: {}", e))
                })?;
            let resolved = crate::memory_config::resolve(raw.clone());
            crate::memory_config::write_raw_ws(&raw).await?;
            serde_json::to_value(resolved).map_err(MonarchError::from)
        }
        "memory_get_config_path" => {
            let path = crate::memory_config::config_path_ws()?;
            Ok(Value::String(path))
        }
        "memory_index_status" => Ok(Value::Bool(state.memory_index.is_initialized())),
        "memory_download_and_init" => {
            state.memory_index.ensure_model_downloaded().await?;
            state.memory_index.init_embedder().await?;
            Ok(Value::Null)
        }
        "memory_smoke_insert" => {
            if !cfg!(debug_assertions) {
                return Err(MonarchError::persistence(
                    "memory_smoke_insert is only available in debug builds",
                ));
            }
            let agent_id = str_field(&args, "agentId")?;
            let title = str_field(&args, "title")?;
            let content = str_field(&args, "content")?;
            let cfg = crate::memory_config::resolved().await;
            let text = format!("{title}\n\n{content}");
            let embedding = state.memory_index.embed_to_blob(&text).await?;
            let payload = crate::db::InsertMemoryPayload {
                agent_id: Some(agent_id.clone()),
                scope: "self".to_string(),
                project_id: None,
                parent_id: None,
                layer: "leaf".to_string(),
                kind: Some("claim".to_string()),
                title: title.clone(),
                summary: title,
                content: Some(content),
                source_quest_id: None,
                source_session_id: None,
                source_events: None,
                file_refs: None,
                supersedes_id: None,
            };
            let new_id = state
                .db
                .insert_memory_internal(payload, Some(embedding), Some(cfg.embedding_model_id))
                .await?;
            let pairs = state
                .db
                .load_embeddings_for_agent_internal(&agent_id)
                .await?;
            state.memory_index.rebuild(pairs).await?;
            Ok(Value::Number(new_id.into()))
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
