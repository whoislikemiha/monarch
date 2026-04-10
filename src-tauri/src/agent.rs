use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader as TokioBufReader};
use tokio::process::{Child as TokioChild, Command as TokioCommand};
use tokio::sync::{broadcast, mpsc, RwLock};

type TaskHandle = tauri::async_runtime::JoinHandle<()>;

use crate::agent_state::{
    display_items_from_messages, ApplyOutcome, DisplayItem, LiveAgentState,
};
use crate::db::{AgentRow, Database, MessageRow, ProjectRow};
use crate::persistence::read_agent_prompt_file;
use std::path::{Path, PathBuf};

/// Debounce window for streaming `message_update` events. Token-rate chunks
/// would otherwise clone + serialize the full snapshot per token; 16ms caps
/// the emit rate at ~60fps which is visually equivalent and ~10x cheaper on
/// token-heavy turns. Terminal events (message_end, tool_execution_end, etc.)
/// bypass this and flush immediately so perceived "done" transitions stay
/// latency-free.
const DEBOUNCE_MILLIS: u64 = 16;

/// A broadcast event sent to WebSocket clients
#[derive(Debug, Clone, Serialize)]
pub struct WsBroadcast {
    pub event: String,
    pub payload: String,
}

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
//
// MON-14 Phase 1 moves the sidecar onto `tokio::process`. The stdout reader is
// async (tokio task) so per-agent state assembly can `.await` locks cleanly.
// The write path stays synchronous from the caller's POV: Tauri command
// handlers are still sync functions and call `write_command(json)` which
// enqueues on an unbounded mpsc channel drained by a dedicated writer task.
// This avoids the "convert tokio::ChildStdin into std::ChildStdin" dance while
// keeping MON-14's blast radius on the read side. The follow-up issue (MON-27)
// migrates the command handlers themselves to async.

#[allow(dead_code)]
struct SidecarProcess {
    /// Kept so we can observe liveness via `try_wait()` and kill on shutdown.
    child: Mutex<TokioChild>,
    /// Sync send into the dedicated writer task. Dropping this closes the
    /// channel, which stops the writer task and closes the pipe.
    stdin_tx: mpsc::UnboundedSender<String>,
}

impl SidecarProcess {
    fn write_command(&self, json: &str) -> Result<(), String> {
        let mut line = json.to_string();
        if !line.ends_with('\n') {
            line.push('\n');
        }
        self.stdin_tx
            .send(line)
            .map_err(|e| format!("sidecar writer closed: {}", e))
    }
}

// ---- Agent Manager (manages sidecar + agent state) ----

/// Per-agent live state entry. Holds the assembled `LiveAgentState` plus the
/// debounce coalescing state used by the reader task. Kept separate from the
/// wire type so `LiveAgentState` stays purely data.
#[derive(Default)]
pub struct AgentStateEntry {
    pub state: LiveAgentState,
    /// Set true by `message_update`; cleared when the debounce task fires and
    /// emits. Gives the debounce task a way to skip redundant emits if a
    /// terminal event already flushed since the timer was armed.
    pub dirty: bool,
    /// In-flight debounce task, if any. Aborted + taken by terminal events
    /// so they can flush immediately without racing the timer.
    pub debounce_handle: Option<TaskHandle>,
}

pub struct AgentManager {
    sidecar: Mutex<Option<Arc<SidecarProcess>>>,
    agents: Mutex<HashMap<String, AgentState>>,
    /// agentId → sessionId mapping, shared with the reader task
    session_map: AgentSessionMap,
    /// Per-agent assembled state, owned by this Rust process and emitted on
    /// `agent-state-{id}`. Outer DashMap is sync-friendly; inner RwLock is
    /// tokio-native because the reader task is async. Entries are lazily
    /// created on first event for an agent.
    live_states: Arc<DashMap<String, Arc<RwLock<AgentStateEntry>>>>,
    /// Broadcast channel for forwarding events to WebSocket clients
    pub ws_broadcast: broadcast::Sender<WsBroadcast>,
    /// Stored AppHandle for WS-initiated commands that need sidecar access
    app_handle: Mutex<Option<AppHandle>>,
}

impl AgentManager {
    pub fn new() -> Self {
        let (ws_broadcast, _) = broadcast::channel(256);
        Self {
            sidecar: Mutex::new(None),
            agents: Mutex::new(HashMap::new()),
            session_map: Arc::new(Mutex::new(HashMap::new())),
            live_states: Arc::new(DashMap::new()),
            ws_broadcast,
            app_handle: Mutex::new(None),
        }
    }

    /// Store the AppHandle after Tauri setup so WS commands can use it
    pub fn set_app_handle(&self, handle: AppHandle) {
        if let Ok(mut h) = self.app_handle.lock() {
            *h = Some(handle);
        }
    }

    fn get_app_handle(&self) -> Result<AppHandle, String> {
        self.app_handle
            .lock()
            .map_err(|e| e.to_string())?
            .clone()
            .ok_or_else(|| "AppHandle not initialized".to_string())
    }

    fn ensure_sidecar(
        &self,
        app: &AppHandle,
        db: &Arc<Database>,
    ) -> Result<Arc<SidecarProcess>, String> {
        let mut sidecar_lock = self.sidecar.lock().map_err(|e| e.to_string())?;

        // Check if existing sidecar is still alive. `tokio::process::Child::try_wait`
        // is synchronous and does not require a runtime context, so this is safe
        // to call from a sync Tauri command handler.
        if let Some(ref sc) = *sidecar_lock {
            let still_alive = sc
                .child
                .lock()
                .ok()
                .and_then(|mut c| c.try_wait().ok())
                .map(|status| status.is_none())
                .unwrap_or(false);
            if still_alive {
                return Ok(sc.clone());
            }
            eprintln!("[monarch] Sidecar process died, respawning...");
            *sidecar_lock = None;
        }

        let sidecar_script = resolve_sidecar_path()?;

        let mut cmd = TokioCommand::new("node");
        cmd.arg(&sidecar_script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(false);

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn sidecar: {}", e))?;

        let stdout = child
            .stdout
            .take()
            .ok_or("Failed to capture sidecar stdout")?;
        let stderr = child
            .stderr
            .take()
            .ok_or("Failed to capture sidecar stderr")?;
        let stdin = child
            .stdin
            .take()
            .ok_or("Failed to capture sidecar stdin")?;

        // Writer task: drains the mpsc and writes each line to the tokio
        // ChildStdin. Sync callers enqueue via `stdin_tx.send(..)`.
        let (stdin_tx, mut stdin_rx) = mpsc::unbounded_channel::<String>();
        tauri::async_runtime::spawn(async move {
            use tokio::io::AsyncWriteExt;
            let mut stdin = stdin;
            while let Some(line) = stdin_rx.recv().await {
                if let Err(e) = stdin.write_all(line.as_bytes()).await {
                    eprintln!("[monarch] Sidecar stdin write failed: {}", e);
                    break;
                }
                if let Err(e) = stdin.flush().await {
                    eprintln!("[monarch] Sidecar stdin flush failed: {}", e);
                    break;
                }
            }
            eprintln!("[monarch] Sidecar writer task exited");
        });

        let sc = Arc::new(SidecarProcess {
            child: Mutex::new(child),
            stdin_tx,
        });

        // Stdout reader task: async loop, one line → one handle_sidecar_event.
        // Owns clones of everything the handler needs; no `self` captured.
        let app_clone = app.clone();
        let db_clone = db.clone();
        let session_map_clone = self.session_map.clone();
        let live_states_clone = self.live_states.clone();
        let ws_tx = self.ws_broadcast.clone();
        tauri::async_runtime::spawn(async move {
            let mut lines = TokioBufReader::new(stdout).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) if !line.is_empty() => {
                        handle_sidecar_event(
                            &app_clone,
                            &db_clone,
                            &session_map_clone,
                            &live_states_clone,
                            &ws_tx,
                            &line,
                        )
                        .await;
                    }
                    Ok(Some(_)) => {}
                    Ok(None) => break,
                    Err(e) => {
                        eprintln!("[monarch] Sidecar stdout read error: {}", e);
                        break;
                    }
                }
            }
            eprintln!("[monarch] Sidecar stdout closed");
        });

        // Stderr reader task — log diagnostics. Async for symmetry with stdout.
        tauri::async_runtime::spawn(async move {
            let mut lines = TokioBufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.is_empty() {
                    eprintln!("[sidecar] {}", line);
                }
            }
        });

        *sidecar_lock = Some(sc.clone());
        Ok(sc)
    }

    /// Get or lazily create the live-state entry for an agent.
    fn live_entry(&self, agent_id: &str) -> Arc<RwLock<AgentStateEntry>> {
        self.live_states
            .entry(agent_id.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(AgentStateEntry::default())))
            .clone()
    }

    /// Drop an agent's live-state entry entirely (on kill).
    fn remove_live_entry(&self, agent_id: &str) {
        if let Some((_, entry)) = self.live_states.remove(agent_id) {
            // Best-effort abort any in-flight debounce so it doesn't emit
            // after the entry is gone. We try_write here to avoid blocking a
            // sync caller; if the lock is held right now the debounce task
            // itself is running and will observe the gone entry shortly.
            if let Ok(mut guard) = entry.try_write() {
                if let Some(h) = guard.debounce_handle.take() {
                    h.abort();
                }
            }
        }
    }

    fn send_to_sidecar(&self, json: &str) -> Result<(), String> {
        let sidecar_lock = self.sidecar.lock().map_err(|e| e.to_string())?;
        let sc = sidecar_lock.as_ref().ok_or("Sidecar not running")?;
        sc.write_command(json)
    }

    /// Recover from a dead sidecar: respawn it and recreate all tracked agent sessions
    /// with their full config and session context.
    ///
    /// MON-14: also rebuilds each agent's `LiveAgentState.items` from SQLite
    /// ancestry and emits one snapshot per recovered agent on `agent-state-{id}`.
    /// Mid-stream assembly (partial streaming message, in-flight tool group)
    /// is intentionally dropped — we cannot reconstruct it from persisted rows
    /// and showing a frozen partial state would be worse than a clean reset.
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
            let messages_opt = if let Some(session_id) = session_snapshot.get(agent_id) {
                db.get_messages_with_ancestry(session_id).ok()
            } else {
                None
            };

            if let Some(messages) = &messages_opt {
                if !messages.is_empty() {
                    let msg_array: Vec<serde_json::Value> = messages
                        .iter()
                        .filter(|m| {
                            m.role == "user"
                                || m.role == "assistant"
                                || m.role == "toolResult"
                        })
                        .map(|m| {
                            serde_json::json!({
                                "role": m.role,
                                "content": m.content,
                                "model": m.model,
                            })
                        })
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

            // Rebuild live state and emit a single snapshot so the frontend
            // (once wired in Phase 2) picks up the restored items without
            // needing a manual refresh.
            let items: Vec<DisplayItem> = messages_opt
                .as_ref()
                .map(|msgs| {
                    display_items_from_messages(msgs, "Session restored after sidecar restart")
                })
                .unwrap_or_else(|| {
                    vec![DisplayItem::Status {
                        text: "Session restored after sidecar restart".to_string(),
                    }]
                });

            let entry = self.live_entry(agent_id);
            let snapshot_json = {
                // Block briefly on the write lock. Recovery is rare and
                // single-threaded per agent, so contention is effectively zero.
                let mut guard = match entry.try_write() {
                    Ok(g) => g,
                    Err(_) => {
                        // Someone else is mutating; skip the emit rather than
                        // stall recovery. The next real event will re-emit.
                        continue;
                    }
                };
                if let Some(h) = guard.debounce_handle.take() {
                    h.abort();
                }
                guard.dirty = false;
                guard.state.reset_with_items(items);
                serde_json::to_string(&guard.state).ok()
            };

            if let Some(payload) = snapshot_json {
                let event_name = format!("agent-state-{}", agent_id);
                emit_event(app, &self.ws_broadcast, &event_name, &payload);
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

    /// Rebuild the assembled `LiveAgentState` for an agent from a persisted
    /// SQLite session, replace the in-memory entry, and emit a snapshot on
    /// `agent-state-{id}`. Returns the new state so direct callers (the
    /// `rebuild_agent_state_from_session` Tauri command) can skip a round-trip
    /// through the event channel.
    ///
    /// Passing `session_id = None` resets the entry to an empty state with a
    /// single status item — used by "new session" flows.
    pub async fn rebuild_state_from_session(
        &self,
        app: &AppHandle,
        db: &Arc<Database>,
        agent_id: &str,
        session_id: Option<&str>,
        status_text: &str,
    ) -> Result<LiveAgentState, String> {
        let items: Vec<DisplayItem> = match session_id {
            Some(sid) => {
                let messages = db.get_messages_with_ancestry(sid).unwrap_or_default();
                if messages.is_empty() {
                    vec![DisplayItem::Status {
                        text: format!("{} (no stored messages)", status_text),
                    }]
                } else {
                    display_items_from_messages(&messages, status_text)
                }
            }
            None => vec![DisplayItem::Status {
                text: status_text.to_string(),
            }],
        };

        let entry = self.live_entry(agent_id);
        let snapshot = {
            let mut guard = entry.write().await;
            if let Some(h) = guard.debounce_handle.take() {
                h.abort();
            }
            guard.dirty = false;
            guard.state.reset_with_items(items);
            guard.state.clone()
        };

        if let Ok(payload) = serde_json::to_string(&snapshot) {
            let event_name = format!("agent-state-{}", agent_id);
            emit_event(app, &self.ws_broadcast, &event_name, &payload);
        }

        Ok(snapshot)
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

/// Emit an event to both Tauri webview and WebSocket clients
fn emit_event(app: &AppHandle, ws_tx: &broadcast::Sender<WsBroadcast>, event_name: &str, payload: &str) {
    let _ = app.emit(event_name, payload.to_string());
    let _ = ws_tx.send(WsBroadcast {
        event: event_name.to_string(),
        payload: payload.to_string(),
    });
}

/// Handle a single JSONL event from the sidecar.
///
/// MON-14 Phase 1: this is now async, owns the per-agent `LiveAgentState`
/// mutation, and emits assembled snapshots on `agent-state-{id}` in addition
/// to the legacy raw `agent-event-{id}` forwarding. The dual emission is
/// intentional: Phase 2 switches the frontend to the new channel, at which
/// point the legacy forwarding of message/tool events can be removed.
///
/// `session_ready`, `extension_ui_request`, and error pings stay on
/// `agent-event-{id}` only — they are not folded into `LiveAgentState`.
async fn handle_sidecar_event(
    app: &AppHandle,
    db: &Arc<Database>,
    session_map: &AgentSessionMap,
    live_states: &Arc<DashMap<String, Arc<RwLock<AgentStateEntry>>>>,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    line: &str,
) {
    let parsed: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            eprintln!(
                "[monarch] Failed to parse sidecar event: {} — line: {}",
                e, line
            );
            return;
        }
    };

    let event_type = parsed.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let agent_id = parsed.get("agentId").and_then(|a| a.as_str()).unwrap_or("");

    match event_type {
        "session_ready" => {
            let event_name = format!("agent-event-{}", agent_id);
            let context_window = parsed.get("contextWindow").and_then(|v| v.as_i64());
            let ready_event = serde_json::json!({
                "type": "session_ready",
                "agentId": agent_id,
                "contextWindow": context_window,
            });
            emit_event(app, ws_tx, &event_name, &ready_event.to_string());
        }

        "session_destroyed" => {
            let exit_event = format!("agent-exit-{}", agent_id);
            emit_event(
                app,
                ws_tx,
                &exit_event,
                &serde_json::json!(null).to_string(),
            );
            // Clear the live state for this agent so a fresh session starts clean.
            if let Some(entry) = live_states.get(agent_id).map(|e| e.clone()) {
                let mut guard = entry.write().await;
                if let Some(h) = guard.debounce_handle.take() {
                    h.abort();
                }
                guard.dirty = false;
                guard.state = LiveAgentState::default();
                guard.state.state_version = guard.state.state_version.saturating_add(1);
                if let Ok(payload) = serde_json::to_string(&guard.state) {
                    let state_event = format!("agent-state-{}", agent_id);
                    emit_event(app, ws_tx, &state_event, &payload);
                }
            }
        }

        "event" => {
            let inner_event = match parsed.get("event") {
                Some(e) => e,
                None => {
                    // Malformed sidecar line: an "event" envelope with no
                    // inner event. Surface via the dev-only desync indicator.
                    if !agent_id.is_empty() {
                        mark_agent_desynced(app, ws_tx, live_states, agent_id).await;
                    }
                    return;
                }
            };
            let inner_type = inner_event
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("");

            // Persist to SQLite via spawn_blocking — rusqlite is blocking and
            // we do not want to stall the async reader task.
            // TODO(MON-27): remove spawn_blocking after migrating db.rs to
            // tokio-rusqlite (or equivalent non-blocking SQLite).
            let db_task = db.clone();
            let session_map_task = session_map.clone();
            let agent_id_task = agent_id.to_string();
            let inner_type_task = inner_type.to_string();
            let inner_event_task = inner_event.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || {
                persist_event(
                    &db_task,
                    &session_map_task,
                    &agent_id_task,
                    &inner_type_task,
                    &inner_event_task,
                );
            });

            // Legacy raw-channel forwarding. Preserved during Phase 1 so the
            // existing frontend (which still assembles from these events)
            // keeps working unchanged. Phase 2 removes the subscriber; the
            // follow-up issue removes this emit.
            let event_name = format!("agent-event-{}", agent_id);
            emit_event(app, ws_tx, &event_name, &inner_event.to_string());

            // Apply the event to per-agent LiveAgentState and decide whether
            // to emit a snapshot now, debounce it, or skip.
            apply_and_maybe_emit(
                app,
                ws_tx,
                live_states,
                agent_id,
                inner_event,
            )
            .await;
        }

        "extension_ui_request" => {
            let event_name = format!("agent-event-{}", agent_id);
            emit_event(app, ws_tx, &event_name, line);
        }

        "error" => {
            let error_msg = parsed
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("Unknown error");
            eprintln!("[monarch] Sidecar error for {}: {}", agent_id, error_msg);
            let event_name = format!("agent-event-{}", agent_id);
            let error_event = serde_json::json!({
                "type": "sidecar_error",
                "error": error_msg,
            });
            emit_event(app, ws_tx, &event_name, &error_event.to_string());
        }

        _ => {
            eprintln!("[monarch] Unknown sidecar event type: {}", event_type);
        }
    }
}

/// Route one inner event through `LiveAgentState::apply_event` and emit a
/// snapshot on `agent-state-{id}` per the returned `ApplyOutcome`. No guard
/// is held across the emit or across any await other than the lock acquire.
async fn apply_and_maybe_emit(
    app: &AppHandle,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    live_states: &Arc<DashMap<String, Arc<RwLock<AgentStateEntry>>>>,
    agent_id: &str,
    inner_event: &serde_json::Value,
) {
    if agent_id.is_empty() {
        return;
    }

    // Lazy entry creation on first event for this agent.
    let entry = live_states
        .entry(agent_id.to_string())
        .or_insert_with(|| Arc::new(RwLock::new(AgentStateEntry::default())))
        .clone();

    // Clone the small bits of context the debounce task will need. Must be
    // owned so they can outlive the caller's borrow.
    let snapshot_to_emit: Option<String> = {
        let mut guard = entry.write().await;

        let outcome = guard.state.apply_event(inner_event);

        match outcome {
            ApplyOutcome::NoOp => None,
            ApplyOutcome::EmitNow => {
                guard.dirty = false;
                if let Some(h) = guard.debounce_handle.take() {
                    h.abort();
                }
                serde_json::to_string(&guard.state).ok()
            }
            ApplyOutcome::Debounce => {
                guard.dirty = true;
                if guard.debounce_handle.is_none() {
                    let entry_clone = entry.clone();
                    let agent_id_owned = agent_id.to_string();
                    let app_clone = app.clone();
                    let ws_tx_clone = ws_tx.clone();
                    let handle = tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(DEBOUNCE_MILLIS)).await;
                        let mut g = entry_clone.write().await;
                        g.debounce_handle = None;
                        if !g.dirty {
                            return;
                        }
                        g.dirty = false;
                        let payload = match serde_json::to_string(&g.state) {
                            Ok(p) => p,
                            Err(_) => return,
                        };
                        let event_name = format!("agent-state-{}", agent_id_owned);
                        // Release the guard before emit — emit can block on
                        // broadcast channel send under worst case.
                        drop(g);
                        emit_event(&app_clone, &ws_tx_clone, &event_name, &payload);
                    });
                    guard.debounce_handle = Some(handle);
                }
                None
            }
        }
    };

    if let Some(payload) = snapshot_to_emit {
        let event_name = format!("agent-state-{}", agent_id);
        emit_event(app, ws_tx, &event_name, &payload);
    }
}

/// Flip the `desynced` flag on an agent's `LiveAgentState` and emit a
/// snapshot. Called from the sidecar reader task when a line cannot be
/// reconciled with the current state. Surfaced via the dev-only indicator
/// (`VITE_MONARCH_DEBUG_DESYNC`); the flag resets on the next `message_start`.
async fn mark_agent_desynced(
    app: &AppHandle,
    ws_tx: &broadcast::Sender<WsBroadcast>,
    live_states: &Arc<DashMap<String, Arc<RwLock<AgentStateEntry>>>>,
    agent_id: &str,
) {
    let entry = live_states
        .entry(agent_id.to_string())
        .or_insert_with(|| Arc::new(RwLock::new(AgentStateEntry::default())))
        .clone();
    let payload = {
        let mut guard = entry.write().await;
        guard.state.mark_desynced();
        serde_json::to_string(&guard.state).ok()
    };
    if let Some(payload) = payload {
        let event_name = format!("agent-state-{}", agent_id);
        emit_event(app, ws_tx, &event_name, &payload);
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

// ---- Project Detection ----

/// Walk up from `start` looking for a `.git` directory. Returns the directory containing `.git`.
fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Read instruction files from a project root.
/// Reads both AGENTS.md and CLAUDE.md if present, concatenating them.
fn read_instructions_from_root(root: &Path) -> Option<String> {
    let mut parts = Vec::new();
    for name in &["AGENTS.md", "CLAUDE.md"] {
        let path = root.join(name);
        if path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                }
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

/// Detect project from cwd → find or create project row → return (project_id, instructions).
/// DB instructions take precedence. On first creation, populate from files.
fn resolve_project(
    db: &Database,
    cwd: &str,
) -> Result<(Option<String>, Option<String>), String> {
    let cwd_path = Path::new(cwd);
    let root = match find_project_root(cwd_path) {
        Some(r) => r,
        None => return Ok((None, None)),
    };
    let root_str = root.to_string_lossy().to_string();

    // Read file-based instructions for initial population
    let file_instructions = read_instructions_from_root(&root);

    // Ensure project exists (race-safe: returns the winning row's id)
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root_str.clone());
    let candidate_id = format!("project-{}", uuid_v4_simple());
    let now = chrono_now();
    let project_id = db.ensure_project_internal(&ProjectRow {
        id: candidate_id,
        name,
        root_path: root_str.clone(),
        instructions: file_instructions.clone(),
        created_at: now.clone(),
        updated_at: now,
    })?;

    // Prefer DB instructions (user may have edited them); fall back to files
    let db_project = db.get_project_by_path_internal(&root_str)?;
    let instructions = db_project
        .and_then(|p| p.instructions)
        .filter(|s| !s.trim().is_empty())
        .or(file_instructions);

    Ok((Some(project_id), instructions))
}

fn uuid_v4_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:x}-{:x}", t, std::process::id())
}

#[tauri::command]
#[specta::specta]
pub fn detect_project(
    db: tauri::State<'_, Arc<Database>>,
    cwd: String,
) -> Result<Option<serde_json::Value>, String> {
    let cwd_path = Path::new(&cwd);
    let root = match find_project_root(cwd_path) {
        Some(r) => r,
        None => return Ok(None),
    };
    let root_str = root.to_string_lossy().to_string();
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root_str.clone());
    let existing = db.get_project_by_path_internal(&root_str)?;
    let file_instructions = read_instructions_from_root(&root);
    Ok(Some(serde_json::json!({
        "rootPath": root_str,
        "name": existing.as_ref().map(|p| p.name.as_str()).unwrap_or(&name),
        "projectId": existing.as_ref().map(|p| p.id.as_str()),
        "hasInstructions": existing.as_ref()
            .and_then(|p| p.instructions.as_ref())
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
            || file_instructions.is_some(),
    })))
}

#[tauri::command]
#[specta::specta]
pub fn read_project_instructions(cwd: String) -> Result<Option<String>, String> {
    let cwd_path = Path::new(&cwd);
    let root = match find_project_root(cwd_path) {
        Some(r) => r,
        None => return Ok(None),
    };
    Ok(read_instructions_from_root(&root))
}

// ---- Tauri Commands ----

#[tauri::command]
#[specta::specta]
pub fn spawn_agent(
    app: AppHandle,
    state: tauri::State<'_, Arc<AgentManager>>,
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
    context_window: Option<i32>,
) -> Result<(), String> {
    // Ensure sidecar is running
    state.ensure_sidecar(&app, &db)?;

    let now = chrono_now();
    let provider_value = provider.clone();
    let model_value = model.clone();
    let thinking_value = thinking_level.clone();

    // Detect project from cwd and read instruction files
    let effective_cwd = cwd.as_deref().unwrap_or(".");
    let (project_id, project_instructions) = resolve_project(&db, effective_cwd)?;

    // Persist the agent/session on the backend as the source of truth for FK-safe
    // message logging, even if the frontend-side write was skipped or failed.
    // If the caller didn't supply a context window (e.g. restore flow),
    // reuse the one persisted on the agent row so we don't silently lose it.
    let effective_context_window = match context_window {
        Some(cw) => Some(cw),
        None => db
            .get_agent_context_window_internal(&id)
            .ok()
            .flatten(),
    };

    db.upsert_agent_internal(&AgentRow {
        id: id.clone(),
        name: shadow_name
            .clone()
            .or_else(|| shadow_title.clone())
            .unwrap_or_else(|| id.clone()),
        project_id: project_id.clone(),
        shadow_name: shadow_name.clone(),
        shadow_title: shadow_title.clone(),
        shadow_grade: shadow_grade.clone(),
        provider: provider_value.clone(),
        model: model_value.clone(),
        thinking_level: thinking_value.clone(),
        cwd: cwd.clone(),
        custom_prompt: None,
        context_window: effective_context_window,
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
        "cwd": effective_cwd,
        "provider": provider.as_deref().unwrap_or("anthropic"),
        "model": model.as_deref().unwrap_or("claude-sonnet-4-5"),
        "thinkingLevel": thinking_level.as_deref().unwrap_or("medium"),
        "shadow": shadow,
        "customPrompt": read_agent_prompt_file(&id)?
            .filter(|prompt| !prompt.trim().is_empty()),
        "projectInstructions": project_instructions,
        "contextWindow": effective_context_window,
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
#[specta::specta]
pub fn send_command(
    app: AppHandle,
    state: tauri::State<'_, Arc<AgentManager>>,
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

#[tauri::command]
#[specta::specta]
pub fn kill_agent(
    state: tauri::State<'_, Arc<AgentManager>>,
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
    drop(map);

    state.remove_live_entry(&id);

    Ok(())
}

/// Return the current assembled live state for an agent. This is the "pull"
/// half of the pull-then-subscribe pattern: Phase 2's frontend calls this on
/// mount to seed `liveAgentStore`, then listens on `agent-state-{id}` for
/// subsequent updates and reconciles by `stateVersion`.
///
/// Returns `None` if no entry exists for this agent (no events have arrived
/// yet, or the agent was killed). Callers should treat `None` as "empty
/// state" rather than an error.
#[tauri::command]
#[specta::specta]
pub async fn get_agent_state(
    state: tauri::State<'_, Arc<AgentManager>>,
    agent_id: String,
) -> Result<Option<LiveAgentState>, String> {
    let entry = match state.live_states.get(&agent_id) {
        Some(e) => e.clone(),
        None => return Ok(None),
    };
    let guard = entry.read().await;
    Ok(Some(guard.state.clone()))
}

/// Rebuild the assembled `LiveAgentState` for an agent from a SQLite session
/// and publish a snapshot on `agent-state-{id}`. Returns the new state so the
/// frontend can seed its store without waiting for the event loopback.
///
/// `session_id = None` clears the state (used for "new session" flows).
#[tauri::command]
#[specta::specta]
pub async fn rebuild_agent_state_from_session(
    app: AppHandle,
    state: tauri::State<'_, Arc<AgentManager>>,
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
    session_id: Option<String>,
    status_text: String,
) -> Result<LiveAgentState, String> {
    state
        .rebuild_state_from_session(
            &app,
            &db,
            &agent_id,
            session_id.as_deref(),
            &status_text,
        )
        .await
}

/// Load messages from a previous SQLite session into the sidecar's agent context.
/// This gives the LLM conversational continuity when restoring.
#[tauri::command]
#[specta::specta]
pub fn load_session_context(
    app: AppHandle,
    state: tauri::State<'_, Arc<AgentManager>>,
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
#[specta::specta]
pub fn new_agent_session(
    app: AppHandle,
    state: tauri::State<'_, Arc<AgentManager>>,
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
        project_id: None,
        shadow_name: None,
        shadow_title: None,
        shadow_grade: None,
        provider: provider.clone(),
        model: model.clone(),
        thinking_level: None,
        cwd: None,
        custom_prompt: None,
        context_window: None,
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
#[specta::specta]
pub fn switch_agent_session(
    app: AppHandle,
    state: tauri::State<'_, Arc<AgentManager>>,
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
#[specta::specta]
pub fn respond_extension_ui(
    app: AppHandle,
    state: tauri::State<'_, Arc<AgentManager>>,
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

// ---- WebSocket wrappers ----
// These mirror the Tauri commands but take raw state instead of tauri::State extractors.

pub fn ws_spawn_agent(
    mgr: &AgentManager,
    db: &Arc<Database>,
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
    let app = mgr.get_app_handle()?;
    mgr.ensure_sidecar(&app, db)?;

    let now = chrono_now();
    let effective_cwd = cwd.as_deref().unwrap_or(".");
    let (project_id, project_instructions) = resolve_project(db, effective_cwd)?;

    db.upsert_agent_internal(&AgentRow {
        id: id.clone(),
        name: shadow_name.clone().or_else(|| shadow_title.clone()).unwrap_or_else(|| id.clone()),
        project_id: project_id.clone(),
        shadow_name: shadow_name.clone(),
        shadow_title: shadow_title.clone(),
        shadow_grade: shadow_grade.clone(),
        provider: provider.clone(),
        model: model.clone(),
        thinking_level: thinking_level.clone(),
        cwd: cwd.clone(),
        custom_prompt: None,
        context_window: None,
        created_at: now.clone(),
        updated_at: now.clone(),
    })?;

    if !db.session_exists_internal(&session_id)? {
        db.create_session_internal(&crate::db::SessionRow {
            id: session_id.clone(),
            agent_id: id.clone(),
            pi_session_file: None,
            model: model.clone(),
            provider: provider.clone(),
            started_at: now,
            ended_at: None,
            message_count: 0,
            total_tokens: 0,
            total_cost: 0.0,
            parent_session_id: None,
        })?;
    }

    {
        let mut map = mgr.session_map.lock().map_err(|e| e.to_string())?;
        map.insert(id.clone(), session_id.clone());
    }

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
        "cwd": effective_cwd,
        "provider": provider.as_deref().unwrap_or("anthropic"),
        "model": model.as_deref().unwrap_or("claude-sonnet-4-5"),
        "thinkingLevel": thinking_level.as_deref().unwrap_or("medium"),
        "shadow": shadow,
        "customPrompt": read_agent_prompt_file(&id)?.filter(|p| !p.trim().is_empty()),
        "projectInstructions": project_instructions,
    });

    let json = serde_json::to_string(&cmd).map_err(|e| e.to_string())?;
    mgr.send_to_sidecar(&json)?;

    let mut agents = mgr.agents.lock().map_err(|e| e.to_string())?;
    agents.insert(id, AgentState {
        lifecycle: AgentLifecycleState::Idle,
        provider,
        model,
        thinking_level,
        is_streaming: false,
        session_id,
        create_cmd_json: json,
    });

    Ok(())
}

pub fn ws_send_command(
    mgr: &AgentManager,
    db: &Arc<Database>,
    id: String,
    command_json: String,
) -> Result<(), String> {
    let app = mgr.get_app_handle()?;
    let mut cmd: serde_json::Value = serde_json::from_str(&command_json).map_err(|e| e.to_string())?;
    if let Some(obj) = cmd.as_object_mut() {
        obj.insert("agentId".to_string(), serde_json::Value::String(id));
    }
    let json = serde_json::to_string(&cmd).map_err(|e| e.to_string())?;
    mgr.send_with_recovery(&app, db, &json)
}

pub fn ws_kill_agent(mgr: &AgentManager, id: String) -> Result<(), String> {
    let cmd = serde_json::json!({ "type": "destroy_session", "agentId": id });
    let json = serde_json::to_string(&cmd).map_err(|e| e.to_string())?;
    let _ = mgr.send_to_sidecar(&json);
    let mut agents = mgr.agents.lock().map_err(|e| e.to_string())?;
    agents.remove(&id);
    drop(agents);
    let mut map = mgr.session_map.lock().map_err(|e| e.to_string())?;
    map.remove(&id);
    drop(map);
    mgr.remove_live_entry(&id);
    Ok(())
}

pub fn ws_load_session_context(
    mgr: &AgentManager,
    db: &Arc<Database>,
    agent_id: String,
    source_session_id: String,
) -> Result<(), String> {
    let app = mgr.get_app_handle()?;
    let messages = db.get_messages_with_ancestry(&source_session_id)?;
    if messages.is_empty() { return Ok(()); }
    let msg_array: Vec<serde_json::Value> = messages.iter()
        .filter(|m| m.role == "user" || m.role == "assistant" || m.role == "toolResult")
        .map(|m| serde_json::json!({ "role": m.role, "content": m.content, "model": m.model }))
        .collect();
    let cmd = serde_json::json!({ "type": "load_session", "agentId": agent_id, "messages": msg_array });
    let json = serde_json::to_string(&cmd).map_err(|e| e.to_string())?;
    mgr.send_with_recovery(&app, db, &json)
}

pub fn ws_new_agent_session(
    mgr: &AgentManager,
    db: &Arc<Database>,
    agent_id: String,
    new_session_id: String,
    parent_session_id: Option<String>,
) -> Result<(), String> {
    let app = mgr.get_app_handle()?;
    let old_session_id = {
        let map = mgr.session_map.lock().map_err(|e| e.to_string())?;
        map.get(&agent_id).cloned()
    };
    if let Some(old_sid) = &old_session_id {
        let _ = db.update_session_internal(old_sid, None, None, None, Some(&chrono_now()));
    }
    let agent_state = {
        let agents = mgr.agents.lock().map_err(|e| e.to_string())?;
        agents.get(&agent_id).cloned()
    };
    let (model, provider) = agent_state.map(|s| (s.model.clone(), s.provider.clone())).unwrap_or((None, None));
    db.ensure_agent_exists_internal(&AgentRow {
        id: agent_id.clone(), name: agent_id.clone(), project_id: None,
        shadow_name: None, shadow_title: None, shadow_grade: None,
        provider: provider.clone(), model: model.clone(), thinking_level: None,
        cwd: None, custom_prompt: None, context_window: None, created_at: chrono_now(), updated_at: chrono_now(),
    })?;
    let valid_parent = match parent_session_id {
        Some(pid) if db.session_exists_internal(&pid)? => Some(pid),
        _ => None,
    };
    db.create_session_internal(&crate::db::SessionRow {
        id: new_session_id.clone(), agent_id: agent_id.clone(), pi_session_file: None,
        model, provider, started_at: chrono_now(), ended_at: None,
        message_count: 0, total_tokens: 0, total_cost: 0.0, parent_session_id: valid_parent,
    })?;
    {
        let mut map = mgr.session_map.lock().map_err(|e| e.to_string())?;
        map.insert(agent_id.clone(), new_session_id);
    }
    let cmd = serde_json::json!({ "type": "new_session", "agentId": agent_id });
    let json = serde_json::to_string(&cmd).map_err(|e| e.to_string())?;
    mgr.send_with_recovery(&app, db, &json)
}

pub fn ws_switch_agent_session(
    mgr: &AgentManager,
    db: &Arc<Database>,
    agent_id: String,
    session_id: String,
) -> Result<(), String> {
    let app = mgr.get_app_handle()?;
    if !db.session_exists_internal(&session_id)? {
        return Err(format!("Session not found: {}", session_id));
    }
    let old_session_id = {
        let map = mgr.session_map.lock().map_err(|e| e.to_string())?;
        map.get(&agent_id).cloned()
    };
    if let Some(old_sid) = &old_session_id {
        if old_sid != &session_id {
            let _ = db.update_session_internal(old_sid, None, None, None, Some(&chrono_now()));
        }
    }
    {
        let mut map = mgr.session_map.lock().map_err(|e| e.to_string())?;
        map.insert(agent_id.clone(), session_id.clone());
    }
    {
        let mut agents = mgr.agents.lock().map_err(|e| e.to_string())?;
        if let Some(agent) = agents.get_mut(&agent_id) {
            agent.session_id = session_id.clone();
        }
    }
    let cmd = serde_json::json!({ "type": "new_session", "agentId": agent_id });
    let json = serde_json::to_string(&cmd).map_err(|e| e.to_string())?;
    mgr.send_with_recovery(&app, db, &json)
}

pub fn ws_respond_extension_ui(
    mgr: &AgentManager,
    db: &Arc<Database>,
    agent_id: String,
    request_id: String,
    value: serde_json::Value,
) -> Result<(), String> {
    let app = mgr.get_app_handle()?;
    let cmd = serde_json::json!({
        "type": "extension_ui_response",
        "agentId": agent_id,
        "requestId": request_id,
        "value": value,
    });
    let json = serde_json::to_string(&cmd).map_err(|e| e.to_string())?;
    mgr.send_with_recovery(&app, db, &json)
}

pub fn ws_detect_project(db: &Arc<Database>, cwd: String) -> Result<Option<serde_json::Value>, String> {
    let cwd_path = Path::new(&cwd);
    let root = match find_project_root(cwd_path) {
        Some(r) => r,
        None => return Ok(None),
    };
    let root_str = root.to_string_lossy().to_string();
    let name = root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| root_str.clone());
    let existing = db.get_project_by_path_internal(&root_str)?;
    let file_instructions = read_instructions_from_root(&root);
    Ok(Some(serde_json::json!({
        "rootPath": root_str,
        "name": existing.as_ref().map(|p| p.name.as_str()).unwrap_or(&name),
        "projectId": existing.as_ref().map(|p| p.id.as_str()),
        "hasInstructions": existing.as_ref()
            .and_then(|p| p.instructions.as_ref())
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false) || file_instructions.is_some(),
    })))
}

pub fn ws_read_project_instructions(cwd: String) -> Result<Option<String>, String> {
    let cwd_path = Path::new(&cwd);
    let root = match find_project_root(cwd_path) {
        Some(r) => r,
        None => return Ok(None),
    };
    Ok(read_instructions_from_root(&root))
}
