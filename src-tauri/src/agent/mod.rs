use dashmap::DashMap;
use parking_lot::Mutex as PlMutex;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::{broadcast, mpsc, RwLock};

type TaskHandle = tauri::async_runtime::JoinHandle<()>;

use crate::agent_state::{display_items_from_messages, DisplayItem, LiveAgentState};
use crate::db::{AgentRow, Database};
use crate::error::MonarchError;
use crate::persistence::read_agent_prompt_file;
use crate::sidecar_protocol::{LoadSessionMessage, ShadowConfig, SidecarCommand};
use crate::util::chrono_now;

pub mod commands;
mod event_handler;
mod persist;
mod sidecar;

// DTOs re-exported at the module root so `crate::agent::SpawnAgentRequest`
// etc. keep working for ws.rs. Tauri command fns themselves stay addressed
// as `agent::commands::X` because `#[tauri::command]` emits a paired
// `__cmd__<name>` helper that must share the fn's module.
pub use commands::{ExtensionUiResponseRequest, SpawnAgentRequest};

use event_handler::emit_state_event;
use persist::{run_persist_consumer, PersistCommand};
use sidecar::SidecarProcess;

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

#[derive(Debug, Clone)]
pub struct AgentState {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub session_id: String,
    /// The original create_session command, replayed on sidecar crash
    /// recovery. Typed `SidecarCommand::CreateSession` since MON-32 — the
    /// recovery resend path serializes once via `serde_json::to_string`.
    pub create_cmd: SidecarCommand,
}

/// MON-34: consolidated sync-path state for `AgentManager`. The two agent
/// maps (`agents` and `session_map`) used to live behind separate
/// `std::sync::Mutex` fields, which made lock ordering between them an
/// implicit invariant that call sites could (and did) violate. Folding them
/// into one struct behind a single `parking_lot::Mutex` makes the ordering
/// question structurally impossible.
#[derive(Default)]
struct AgentManagerInner {
    agents: HashMap<String, AgentState>,
    /// agentId → sessionId mapping, shared with the reader task via an
    /// `Arc<PlMutex<AgentManagerInner>>` clone.
    session_map: HashMap<String, String>,
}


// ---- Agent Manager (manages sidecar + agent state) ----

/// Per-agent live state entry. Holds the assembled `LiveAgentState` plus the
/// debounce coalescing state used by the reader task. Kept separate from the
/// wire type so `LiveAgentState` stays purely data.
///
/// MON-30: `cancel_generation` lives outside the inner `RwLock` so sync kill
/// paths (`remove_live_entry`, reached from the sync `kill_agent` Tauri
/// command) can invalidate an in-flight debounce without trying to acquire a
/// tokio lock from a sync context. Any debounce task captures the generation
/// at arm time and bails after the lock handoff if it no longer matches.
pub struct AgentStateEntry {
    pub inner: RwLock<AgentStateInner>,
    pub cancel_generation: AtomicU64,
    /// Cached `agent-state-{id}` topic string. Built once at entry creation
    /// so the ~six reader-side emit sites don't `format!` per event. MON-39
    /// item 8.
    pub topic: String,
}

impl AgentStateEntry {
    pub fn new(agent_id: &str) -> Self {
        Self {
            inner: RwLock::new(AgentStateInner::default()),
            cancel_generation: AtomicU64::new(0),
            topic: format!("agent-state-{}", agent_id),
        }
    }
}

#[derive(Default)]
pub struct AgentStateInner {
    pub state: LiveAgentState,
    /// Set true by `message_update`; cleared when the debounce task fires and
    /// emits. Gives the debounce task a way to skip redundant emits if a
    /// terminal event already flushed since the timer was armed.
    pub dirty: bool,
    /// In-flight debounce task, if any. Aborted + taken by terminal events
    /// so they can flush immediately without racing the timer.
    pub debounce_handle: Option<TaskHandle>,
}

/// MON-27 lock hierarchy:
///
/// * `inner` (`parking_lot::Mutex<AgentManagerInner>`) — the only lock that
///   protects both `agents` and `session_map`, so the two can never deadlock
///   against each other. **Never hold this lock across an `.await`** — the
///   manager's async methods use it and `parking_lot` will block a runtime
///   thread if you do. `!Send` guards make the compiler enforce this at
///   any call site that straddles an `.await`.
/// * `sidecar` (`parking_lot::Mutex<Option<Arc<SidecarProcess>>>`) —
///   independent of `inner`; can be taken in either order. Guards must
///   also not cross an `.await`.
/// * `app_handle` (`Arc<parking_lot::Mutex<Option<AppHandle>>>`) —
///   independent of both above; taken briefly in `get_app_handle` and in
///   the persistence consumer's desync path.
/// * `live_states` entries' inner `tokio::sync::RwLock` — async-only,
///   owned by the MON-14 event-assembly path. Never taken under any of the
///   locks above.
/// * `SidecarProcess.stdin` — `tokio::sync::Mutex<Option<ChildStdin>>`;
///   MON-27 collapses the former mpsc writer task into a direct async
///   write from command handlers.
/// * `SidecarProcess.child` — `std::sync::Mutex<TokioChild>`; sync because
///   `shutdown_sidecar` runs from Tauri's sync `ExitRequested` hook and
///   `try_wait` is itself sync.
pub struct AgentManager {
    sidecar: PlMutex<Option<Arc<SidecarProcess>>>,
    /// MON-34: `agents` + `session_map` consolidated under one lock.
    /// The reader task holds a clone of this `Arc` and resolves session
    /// ids through it.
    inner: Arc<PlMutex<AgentManagerInner>>,
    /// Per-agent assembled state, owned by this Rust process and emitted on
    /// `agent-state-{id}`. Outer DashMap is sync-friendly; inner RwLock is
    /// tokio-native because the reader task is async. Entries are lazily
    /// created on first event for an agent.
    live_states: Arc<DashMap<String, Arc<AgentStateEntry>>>,
    /// Broadcast channel for forwarding events to WebSocket clients
    pub ws_broadcast: broadcast::Sender<WsBroadcast>,
    /// Stored AppHandle for WS-initiated commands that need sidecar access.
    /// Arc-wrapped so the persistence consumer task (MON-37) can share access
    /// without needing a back-reference to the manager.
    app_handle: Arc<PlMutex<Option<AppHandle>>>,
    /// MON-37: producer handle for the single-consumer persistence pipeline.
    /// The reader task clones this and `send().await`s one command per
    /// effect (event log + optional message write). A single consumer drains
    /// the channel so writes land in FIFO order; bounded capacity of 256
    /// provides back-pressure if SQLite stalls. Cheap to clone.
    persist_tx: mpsc::Sender<PersistCommand>,
}

impl AgentManager {
    pub fn new(db: Arc<Database>) -> Self {
        let (ws_broadcast, _) = broadcast::channel(256);
        // MON-37: bounded channel feeding the single-consumer persistence
        // task. 256 is well above the sidecar's human-scale event rate; if
        // the DB falls behind, back-pressure stalls the reader before we
        // queue unbounded memory. Not load-bearing — can be tuned.
        let (persist_tx, persist_rx) = mpsc::channel::<PersistCommand>(256);
        let live_states: Arc<DashMap<String, Arc<AgentStateEntry>>> =
            Arc::new(DashMap::new());
        let app_handle: Arc<PlMutex<Option<AppHandle>>> = Arc::new(PlMutex::new(None));

        // MON-37: manager-lifetime persistence consumer. Spawned once in
        // `new()`, not per sidecar respawn — we do not want to lose enqueued
        // commands when the sidecar crashes. Exits naturally when all
        // senders drop (process exit).
        tauri::async_runtime::spawn(run_persist_consumer(
            persist_rx,
            db,
            live_states.clone(),
            ws_broadcast.clone(),
            app_handle.clone(),
        ));

        Self {
            sidecar: PlMutex::new(None),
            inner: Arc::new(PlMutex::new(AgentManagerInner::default())),
            live_states,
            ws_broadcast,
            app_handle,
            persist_tx,
        }
    }

    /// Store the AppHandle after Tauri setup so WS commands can use it
    pub fn set_app_handle(&self, handle: AppHandle) {
        *self.app_handle.lock() = Some(handle);
    }

    pub fn get_app_handle(&self) -> Result<AppHandle, MonarchError> {
        self.app_handle
            .lock()
            .clone()
            .ok_or_else(|| MonarchError::invalid_input("AppHandle not initialized"))
    }


    /// Get or lazily create the live-state entry for an agent.
    fn live_entry(&self, agent_id: &str) -> Arc<AgentStateEntry> {
        self.live_states
            .entry(agent_id.to_string())
            .or_insert_with(|| Arc::new(AgentStateEntry::new(agent_id)))
            .clone()
    }

    /// Drop an agent's live-state entry entirely (on kill).
    ///
    /// MON-30: bump `cancel_generation` unconditionally **before** the
    /// best-effort abort. The bump is lock-free and synchronously visible
    /// (`Release` ordering pairs with the debounce task's `Acquire` load
    /// after it takes the inner write lock), so any in-flight debounce that
    /// races past this point observes the new generation and bails on its
    /// gen check. The `try_write` abort still runs as a cleanup path for the
    /// common case where no debounce is queued.
    fn remove_live_entry(&self, agent_id: &str) {
        if let Some((_, entry)) = self.live_states.remove(agent_id) {
            entry.cancel_generation.fetch_add(1, Ordering::Release);
            if let Ok(mut guard) = entry.inner.try_write() {
                if let Some(h) = guard.debounce_handle.take() {
                    h.abort();
                }
            }
        }
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
    ) -> Result<LiveAgentState, MonarchError> {
        let items: Vec<DisplayItem> = match session_id {
            Some(sid) => {
                let messages = db.get_messages_with_ancestry(sid).await.unwrap_or_default();
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
        // MON-30: bump before acquiring the inner write lock. This replaces
        // the assembled state wholesale, so any debounce task armed against
        // the pre-rebuild state must bail once the lock hands over. The
        // `Release` bump pairs with the debounce task's `Acquire` load after
        // it takes the lock.
        entry.cancel_generation.fetch_add(1, Ordering::Release);
        let mut guard = entry.inner.write().await;
        if let Some(h) = guard.debounce_handle.take() {
            h.abort();
        }
        guard.dirty = false;
        guard.state.reset_with_items(items);
        // MON-38: clone + explicit drop before emit_state_event so the write
        // guard is released before any serialization runs.
        let snapshot = guard.state.clone();
        drop(guard);

        emit_state_event(app, &self.ws_broadcast, &entry.topic, &snapshot);

        Ok(snapshot)
    }

    // ---- Shared agent-lifecycle methods (MON-33) ----
    //
    // Each method owns the full business logic for one agent lifecycle
    // operation. The `#[tauri::command]` entry points and the `ws::dispatch_command`
    // arms are thin adapters that only translate transport-specific arguments
    // and delegate here. `ensure_sidecar` is called inside the method, not the
    // adapter, so neither transport can forget it.

    pub async fn spawn(
        &self,
        app: &AppHandle,
        db: &Arc<Database>,
        req: SpawnAgentRequest,
    ) -> Result<(), MonarchError> {
        self.ensure_sidecar(app)?;

        let SpawnAgentRequest {
            id,
            session_id,
            provider,
            model,
            thinking_level,
            cwd,
            shadow: shadow_spec,
            context_window,
        } = req;

        let now = chrono_now();
        let effective_cwd = cwd.as_deref().unwrap_or(".").to_string();
        let (project_id, project_instructions) =
            crate::project::resolve_project(db, &effective_cwd).await?;

        let shadow_name = shadow_spec.as_ref().and_then(|s| s.shadow_name.clone());
        let shadow_title = shadow_spec.as_ref().and_then(|s| s.shadow_title.clone());
        let shadow_grade = shadow_spec.as_ref().and_then(|s| s.shadow_grade.clone());

        // If the caller didn't supply a context window (restore flow), reuse
        // the one persisted on the agent row so we don't silently lose it.
        let effective_context_window = match context_window {
            Some(cw) => Some(cw),
            None => db
                .get_agent_context_window_internal(&id)
                .await
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
            provider: provider.clone(),
            model: model.clone(),
            thinking_level: thinking_level.clone(),
            cwd: cwd.clone(),
            custom_prompt: None,
            context_window: effective_context_window,
            created_at: now.clone(),
            updated_at: now.clone(),
            archived_at: None,
        })
        .await?;

        if !db.session_exists_internal(&session_id).await? {
            db.create_session_internal(&crate::db::SessionRow {
                id: session_id.clone(),
                agent_id: id.clone(),
                pi_session_file: None,
                model: model.clone(),
                provider: provider.clone(),
                started_at: now.clone(),
                ended_at: None,
                message_count: 0,
                total_tokens: 0,
                total_cost: 0.0,
                parent_session_id: None,
            })
            .await?;
            // MON-63: track session count
            db.increment_agent_sessions(&id).await?;
        }

        {
            let mut inner = self.inner.lock();
            inner.session_map.insert(id.clone(), session_id.clone());
        }

        let shadow = shadow_spec.as_ref().map(|_| ShadowConfig {
            name: shadow_name.clone().unwrap_or_else(|| "Shadow".to_string()),
            title: shadow_title.clone().unwrap_or_else(|| "Shadow Soldier".to_string()),
            grade: shadow_grade.clone().unwrap_or_else(|| "Knight".to_string()),
            id: id.clone(),
        });

        let custom_prompt = read_agent_prompt_file(&id)
            .await?
            .filter(|p| !p.trim().is_empty());

        let cmd = SidecarCommand::CreateSession {
            agent_id: id.clone(),
            cwd: effective_cwd,
            provider: provider.clone().unwrap_or_else(|| "anthropic".to_string()),
            model: model.clone().unwrap_or_else(|| "claude-sonnet-4-5".to_string()),
            thinking_level: thinking_level.clone().unwrap_or_else(|| "medium".to_string()),
            shadow,
            custom_prompt,
            project_instructions,
            context_window: effective_context_window,
        };

        self.send_to_sidecar(&serde_json::to_string(&cmd)?).await?;

        {
            let mut inner = self.inner.lock();
            inner.agents.insert(
                id,
                AgentState {
                    provider,
                    model,
                    session_id,
                    create_cmd: cmd,
                },
            );
        }

        Ok(())
    }

    pub async fn send_command(
        &self,
        app: &AppHandle,
        db: &Arc<Database>,
        id: String,
        command_json: String,
    ) -> Result<(), MonarchError> {
        // MON-32: narrow typed passthrough. Parse the frontend's payload as a
        // Value, inject `agentId`, then re-deserialize into SidecarCommand so
        // the shape is validated against the canonical wire contract.
        let mut value: serde_json::Value = serde_json::from_str(&command_json)?;
        if let Some(obj) = value.as_object_mut() {
            obj.insert("agentId".to_string(), serde_json::Value::String(id));
        }
        let cmd: SidecarCommand = serde_json::from_value(value)?;
        self.send_with_recovery(app, db, &serde_json::to_string(&cmd)?)
            .await
    }

    pub async fn kill(&self, id: &str) -> Result<(), MonarchError> {
        let cmd = SidecarCommand::DestroySession { agent_id: id.to_string() };
        let _ = self.send_to_sidecar(&serde_json::to_string(&cmd)?).await;

        {
            let mut inner = self.inner.lock();
            inner.agents.remove(id);
            inner.session_map.remove(id);
        }
        self.remove_live_entry(id);
        Ok(())
    }

    pub async fn load_session_context(
        &self,
        app: &AppHandle,
        db: &Arc<Database>,
        agent_id: String,
        source_session_id: String,
    ) -> Result<(), MonarchError> {
        let messages = db.get_messages_with_ancestry(&source_session_id).await?;
        if messages.is_empty() {
            return Ok(());
        }

        let cmd = SidecarCommand::LoadSession {
            agent_id,
            messages: messages
                .iter()
                .filter(|m| m.role == "user" || m.role == "assistant" || m.role == "toolResult")
                .map(|m| LoadSessionMessage {
                    role: m.role.clone(),
                    content: m.content.clone(),
                    model: m.model.clone(),
                })
                .collect(),
        };

        self.send_with_recovery(app, db, &serde_json::to_string(&cmd)?)
            .await
    }

    pub async fn new_session(
        &self,
        app: &AppHandle,
        db: &Arc<Database>,
        agent_id: String,
        new_session_id: String,
        parent_session_id: Option<String>,
    ) -> Result<(), MonarchError> {
        // One acquire covers both reads. Cloned out so the subsequent DB
        // calls (which are `.await` points) run without the lock held.
        let (old_session_id, agent_state) = {
            let inner = self.inner.lock();
            (
                inner.session_map.get(&agent_id).cloned(),
                inner.agents.get(&agent_id).cloned(),
            )
        };
        if let Some(old_sid) = &old_session_id {
            let _ = db
                .update_session_internal(old_sid, None, None, None, Some(&chrono_now()))
                .await;
        }

        let (model, provider) = agent_state
            .map(|s| (s.model.clone(), s.provider.clone()))
            .unwrap_or((None, None));

        // Recreate a minimal agent row if the DB entry was pruned or never
        // persisted so the new session insert doesn't trip the FK.
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
            archived_at: None,
        })
        .await?;

        let valid_parent_session_id = match parent_session_id {
            Some(parent_id) if db.session_exists_internal(&parent_id).await? => Some(parent_id),
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
        })
        .await?;
        // MON-63: track session count
        db.increment_agent_sessions(&agent_id).await?;

        {
            let mut inner = self.inner.lock();
            inner.session_map.insert(agent_id.clone(), new_session_id);
        }

        let cmd = SidecarCommand::NewSession { agent_id };
        self.send_with_recovery(app, db, &serde_json::to_string(&cmd)?)
            .await
    }

    pub async fn switch_session(
        &self,
        app: &AppHandle,
        db: &Arc<Database>,
        agent_id: String,
        session_id: String,
    ) -> Result<(), MonarchError> {
        if !db.session_exists_internal(&session_id).await? {
            return Err(MonarchError::not_found(format!("session {}", session_id)));
        }

        // One acquire for the read, a second (short) one for the write-side
        // updates. Splitting read from write lets `update_session_internal`
        // run without the sync `inner` lock held across its `.await`.
        let old_session_id = {
            let inner = self.inner.lock();
            inner.session_map.get(&agent_id).cloned()
        };
        if let Some(old_sid) = &old_session_id {
            if old_sid != &session_id {
                let _ = db
                    .update_session_internal(old_sid, None, None, None, Some(&chrono_now()))
                    .await;
            }
        }

        {
            let mut inner = self.inner.lock();
            inner
                .session_map
                .insert(agent_id.clone(), session_id.clone());
            if let Some(agent) = inner.agents.get_mut(&agent_id) {
                agent.session_id = session_id.clone();
            }
        }

        let cmd = SidecarCommand::NewSession { agent_id };
        self.send_with_recovery(app, db, &serde_json::to_string(&cmd)?)
            .await
    }

    pub async fn respond_extension_ui(
        &self,
        app: &AppHandle,
        db: &Arc<Database>,
        req: ExtensionUiResponseRequest,
    ) -> Result<(), MonarchError> {
        let ExtensionUiResponseRequest {
            agent_id,
            request_id,
            value,
        } = req;
        let cmd = SidecarCommand::ExtensionUiResponse {
            agent_id,
            request_id,
            value,
        };
        self.send_with_recovery(app, db, &serde_json::to_string(&cmd)?)
            .await
    }
}





#[cfg(test)]
mod tests {
    //! Round-trip tests for MON-33's shared service layer. The goal is to
    //! prove that both the Tauri command path and the WS dispatch path
    //! funnel into the same `AgentManager` method — so a future refactor
    //! that only touches one transport cannot silently drift.
    //!
    //! `kill_agent` is the representative operation: it exercises state
    //! cleanup (agents map, session map, live entries) without requiring a
    //! live sidecar process, so the test runs purely in-process.

    use super::*;
    use crate::db::Database;
    use crate::models::ModelCache;
    use crate::sidecar_protocol::SidecarCommand;
    use crate::ws::{self, WsState};
    use tokio::sync::broadcast;

    fn seeded_agent_state(agent_id: &str, session_id: &str) -> AgentState {
        AgentState {
            provider: None,
            model: None,
            session_id: session_id.to_string(),
            create_cmd: SidecarCommand::NewSession {
                agent_id: agent_id.to_string(),
            },
        }
    }

    #[tokio::test]
    async fn kill_agent_round_trip_funnels_through_shared_method() {
        let db = Arc::new(Database::new_in_memory().await.expect("in-memory db"));
        let mgr = Arc::new(AgentManager::new(db.clone()));
        let model_cache = Arc::new(ModelCache::new());
        let (broadcast_tx, _rx) = broadcast::channel(16);

        // Seed two agents: one killed via the IPC path, one via the WS
        // path. MON-34: both maps live inside `AgentManagerInner` behind a
        // single `parking_lot::Mutex`, so one acquire covers both seeds.
        {
            let mut inner = mgr.inner.lock();
            inner
                .agents
                .insert("ipc-kill".to_string(), seeded_agent_state("ipc-kill", "s1"));
            inner
                .agents
                .insert("ws-kill".to_string(), seeded_agent_state("ws-kill", "s2"));
            inner.session_map.insert("ipc-kill".to_string(), "s1".to_string());
            inner.session_map.insert("ws-kill".to_string(), "s2".to_string());
        }

        // IPC side: the Tauri command body is `state.kill(&id)`. Call the
        // shared method directly. send_to_sidecar will fail silently because
        // no sidecar is running; the local state cleanup runs regardless.
        mgr.kill("ipc-kill").await.expect("ipc kill");

        // WS side: full dispatch path — decode args, delegate to mgr.kill.
        let ws_state = WsState {
            db,
            agent_mgr: mgr.clone(),
            model_cache,
            broadcast_rx: broadcast_tx,
        };
        ws::dispatch_command(
            &ws_state,
            "kill_agent",
            serde_json::json!({ "id": "ws-kill" }),
        )
        .await
        .expect("ws dispatch kill");

        let inner = mgr.inner.lock();
        assert!(
            !inner.agents.contains_key("ipc-kill"),
            "ipc path did not clear agent"
        );
        assert!(
            !inner.agents.contains_key("ws-kill"),
            "ws path did not clear agent"
        );
        assert!(
            !inner.session_map.contains_key("ipc-kill"),
            "ipc path did not clear session"
        );
        assert!(
            !inner.session_map.contains_key("ws-kill"),
            "ws path did not clear session"
        );
    }
}


