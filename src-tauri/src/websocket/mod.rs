mod dispatch;
pub mod handlers;

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
use crate::memory::index::MemoryIndex;
use crate::models::ModelCache;

pub(crate) use dispatch::dispatch_command;

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
