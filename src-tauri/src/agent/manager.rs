//! `AgentManager` and the sync-path state structs.
//!
//! The high-level business-logic methods (`spawn`, `send_command`, `kill`,
//! `new_session`, `switch_session`, `load_session_context`,
//! `respond_extension_ui`, `rebuild_state_from_session`) live here. The
//! process-layer methods (`ensure_sidecar`, `shutdown_sidecar`,
//! `send_to_sidecar`, `send_with_recovery`, `recover_sidecar`) are attached
//! to `AgentManager` in `sidecar.rs` — Rust allows `impl` blocks across
//! submodules of the same module tree.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use parking_lot::Mutex as PlMutex;
use tauri::AppHandle;
use tokio::sync::{broadcast, mpsc, RwLock};

use crate::agent_state::{display_items_from_messages, DisplayItem, LiveAgentState};
use crate::db::{AgentRow, Database, MemoryRow, MessageRow};
use crate::error::MonarchError;
use crate::memory_index::MemoryIndex;
use crate::persistence::read_agent_prompt_file;
use crate::sidecar_protocol::{KeeperConfig, LoadSessionMessage, ShadowConfig, SidecarCommand};
use crate::util::chrono_now;

use super::commands::{ExtensionUiResponseRequest, SpawnAgentRequest};
use super::event_handler::{emit_event, emit_state_event};
use super::persist::{run_persist_consumer, PersistCommand};
use super::sidecar::SidecarProcess;
use super::{TaskHandle, WsBroadcast};

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
pub(super) struct AgentManagerInner {
    pub(super) agents: HashMap<String, AgentState>,
    /// agentId → sessionId mapping, shared with the reader task via an
    /// `Arc<PlMutex<AgentManagerInner>>` clone.
    pub(super) session_map: HashMap<String, String>,
}

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

/// MON-100: internal dispatch channel for tasks that need `Arc<AgentManager>`
/// (keeper runs, etc.) but originate from the event-handler path which only
/// has access to `inner` + `live_states`. The event handler enqueues; a
/// manager-owned consumer task spawned by `start_dispatcher` drains.
#[derive(Debug)]
pub(crate) enum InternalDispatch {
    /// Trigger one continuous-compaction Keeper run for the agent. No-op if
    /// the Keeper config is empty or a run is already in flight.
    KeeperRun {
        agent_id: String,
        trigger: KeeperRunTrigger,
    },
    /// Send a manager-originated command to the sidecar. Used by inbound
    /// request/response bridges that originate in `event_handler.rs`.
    SendSidecarCommand { command: SidecarCommand },
}

/// MON-100 / MON-103: Keeper runs share plumbing but differ in why they
/// fired and which message slice should feed the model.
#[derive(Debug, Clone)]
pub(crate) enum KeeperRunTrigger {
    Continuous,
    QuestClose {
        quest_id: String,
        since: Option<String>,
    },
}

impl KeeperRunTrigger {
    fn label(&self) -> &'static str {
        match self {
            Self::Continuous => "continuous",
            Self::QuestClose { .. } => "quest_close",
        }
    }

    fn quest_id(&self) -> Option<&str> {
        match self {
            Self::Continuous => None,
            Self::QuestClose { quest_id, .. } => Some(quest_id.as_str()),
        }
    }
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
    pub(super) sidecar: PlMutex<Option<Arc<SidecarProcess>>>,
    /// MON-34: `agents` + `session_map` consolidated under one lock.
    /// The reader task holds a clone of this `Arc` and resolves session
    /// ids through it.
    pub(super) inner: Arc<PlMutex<AgentManagerInner>>,
    /// Per-agent assembled state, owned by this Rust process and emitted on
    /// `agent-state-{id}`. Outer DashMap is sync-friendly; inner RwLock is
    /// tokio-native because the reader task is async. Entries are lazily
    /// created on first event for an agent.
    pub(super) live_states: Arc<DashMap<String, Arc<AgentStateEntry>>>,
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
    pub(super) persist_tx: mpsc::Sender<PersistCommand>,
    /// MON-100: producer handle for the internal-dispatch task. Cloned into
    /// the reader's `handle_sidecar_event` so trigger checks can enqueue a
    /// Keeper run without needing `Arc<AgentManager>` themselves. Drained by
    /// the consumer spawned in `start_dispatcher`.
    pub(super) dispatch_tx: mpsc::Sender<InternalDispatch>,
    /// MON-100: stash for the dispatcher's receiver, taken once by
    /// `start_dispatcher` after `Arc<Self>` is constructed.
    dispatch_rx_slot: PlMutex<Option<mpsc::Receiver<InternalDispatch>>>,
    /// MON-100: shared `Arc<Database>` so the reader-task spawn (in
    /// `ensure_sidecar`) can pass a db handle to `handle_sidecar_event`
    /// without threading db through every call site of every Tauri command.
    pub(super) db: Arc<Database>,
    /// Shared memory index used by Keeper writes and MON-101 retrieval.
    pub(super) memory_index: Arc<MemoryIndex>,
}

impl AgentManager {
    pub fn new(db: Arc<Database>, memory_index: Arc<MemoryIndex>) -> Self {
        let (ws_broadcast, _) = broadcast::channel(256);
        // MON-37: bounded channel feeding the single-consumer persistence
        // task. 256 is well above the sidecar's human-scale event rate; if
        // the DB falls behind, back-pressure stalls the reader before we
        // queue unbounded memory. Not load-bearing — can be tuned.
        let (persist_tx, persist_rx) = mpsc::channel::<PersistCommand>(256);
        let live_states: Arc<DashMap<String, Arc<AgentStateEntry>>> = Arc::new(DashMap::new());
        let app_handle: Arc<PlMutex<Option<AppHandle>>> = Arc::new(PlMutex::new(None));

        // MON-37: manager-lifetime persistence consumer. Spawned once in
        // `new()`, not per sidecar respawn — we do not want to lose enqueued
        // commands when the sidecar crashes. Exits naturally when all
        // senders drop (process exit).
        // MON-100: thread MemoryIndex through so InsertMemory can embed
        // before insert and RebuildHnsw can call into the index.
        tauri::async_runtime::spawn(run_persist_consumer(
            persist_rx,
            db.clone(),
            memory_index.clone(),
            live_states.clone(),
            ws_broadcast.clone(),
            app_handle.clone(),
        ));

        // MON-100: dispatcher channel. Bounded — if the Keeper trigger
        // saturates somehow, back-pressure stalls the reader rather than
        // queuing unbounded work.
        let (dispatch_tx, dispatch_rx) = mpsc::channel::<InternalDispatch>(32);

        Self {
            sidecar: PlMutex::new(None),
            inner: Arc::new(PlMutex::new(AgentManagerInner::default())),
            live_states,
            ws_broadcast,
            app_handle,
            persist_tx,
            dispatch_tx,
            dispatch_rx_slot: PlMutex::new(Some(dispatch_rx)),
            db,
            memory_index,
        }
    }

    /// MON-100: spawn the dispatcher task that owns `Arc<Self>` so the
    /// event-handler path can enqueue work like Keeper runs without holding
    /// a `Self` reference. Idempotent — the receiver is taken once; further
    /// calls are no-ops. Call once after wrapping the manager in `Arc`.
    pub fn start_dispatcher(self: &Arc<Self>, db: Arc<Database>) {
        let Some(rx) = self.dispatch_rx_slot.lock().take() else {
            return;
        };
        let mgr = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut rx = rx;
            while let Some(d) = rx.recv().await {
                match d {
                    InternalDispatch::KeeperRun { agent_id, trigger } => {
                        if let Err(e) = mgr.dispatch_keeper_run(&db, &agent_id, trigger).await {
                            eprintln!("[monarch] keeper dispatch failed for {}: {:?}", agent_id, e);
                        }
                    }
                    InternalDispatch::SendSidecarCommand { command } => {
                        match serde_json::to_string(&command) {
                            Ok(json) => {
                                if let Err(e) = mgr.send_to_sidecar(&json).await {
                                    eprintln!("[monarch] sidecar response send failed: {:?}", e);
                                }
                            }
                            Err(e) => eprintln!("[monarch] sidecar response encode failed: {}", e),
                        }
                    }
                }
            }
            eprintln!("[monarch] dispatcher exited");
        });
    }

    /// MON-100: assemble the slice + ship a `KeeperRun` command.
    ///
    /// Silent no-op when `memory.toml` has no Keeper model configured (the
    /// captain hasn't opted in) OR when a run is already in flight for this
    /// agent (debounces concurrent threshold crossings while the model is
    /// answering). Errors propagate so the dispatcher logs them.
    pub(crate) async fn dispatch_keeper_run(
        &self,
        db: &Arc<Database>,
        agent_id: &str,
        trigger: KeeperRunTrigger,
    ) -> Result<Option<i64>, MonarchError> {
        let cfg = crate::memory_config::resolved().await;
        let Some(km) = cfg.keeper.clone() else {
            return Ok(None);
        };

        // Guard against double dispatch. The flag is set inside this method
        // and cleared by the event handler when `keeper_result` lands.
        let entry = self.live_entry(agent_id);
        {
            let g = entry.inner.read().await;
            if g.state.keeper_in_flight {
                return Ok(None);
            }
        }

        let model_id = format!("{}/{}", km.provider, km.model);

        // Slice anchor. Continuous compaction resumes after the last
        // successful Keeper run; quest-close distillation scopes to the
        // quest's own lifetime.
        let last_run = db
            .last_successful_keeper_run_internal(agent_id)
            .await
            .ok()
            .flatten();
        let last_completed_at: Option<String> =
            last_run.as_ref().and_then(|r| r.completed_at.clone());
        let prior_summary: Option<String> =
            last_run.as_ref().and_then(|r| r.output_summary.clone());
        let since: Option<String> = match &trigger {
            KeeperRunTrigger::Continuous => last_completed_at,
            KeeperRunTrigger::QuestClose { since, .. } => since.clone(),
        };
        let quest_id = match &trigger {
            KeeperRunTrigger::QuestClose { quest_id, .. } => Some(quest_id.clone()),
            KeeperRunTrigger::Continuous => db
                .get_agent_current_quest_id_internal(agent_id)
                .await
                .ok()
                .flatten(),
        };
        let trigger_label = trigger.label().to_string();

        let messages = db
            .list_agent_messages_since_internal(agent_id, since.as_deref())
            .await
            .unwrap_or_default();

        // Cap the slice to the most recent ~30k tokens to keep first-run
        // distillations within the model's context window. Confirmed scope
        // with the captain — first-time setup runs against fresh
        // conversations, so worst-case clipping is rare.
        let mut budget: i64 = 30_000;
        let mut newest_first: Vec<MessageRow> = Vec::new();
        for m in messages.into_iter().rev() {
            budget = budget.saturating_sub(m.tokens.max(0) as i64);
            newest_first.push(m);
            if budget <= 0 {
                break;
            }
        }
        newest_first.reverse();
        let kept = newest_first;

        // BM25 top-K=5 over `memories_fts` keyed on the captain's most
        // recent user prompt. Cheapest signal that lets the Keeper avoid
        // re-claiming what's already known. Vector retrieval lands in
        // MON-101.
        let related_query = kept
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| extract_text_from_stored_content(&m.content))
            .unwrap_or_default();
        let related = if related_query.trim().is_empty() {
            Vec::new()
        } else {
            let hits = db
                .fts_search_memories_internal(agent_id, &related_query, 5)
                .await
                .unwrap_or_default();
            let mut out = Vec::new();
            for h in hits {
                if let Ok(Some(m)) = db.get_memory_internal(h.memory_id).await {
                    out.push(m);
                }
            }
            out
        };

        // P6 Slice D (MON-122): fold the first-person quest report into the
        // slice on quest-close runs so the Keeper sees the executor's own
        // framing alongside the raw stream. Continuous runs never include a
        // report even when a current quest happens to be set.
        let quest_close_report: Option<String> = match &trigger {
            KeeperRunTrigger::QuestClose { quest_id, .. } => db
                .get_quest_report_by_quest_internal(quest_id)
                .await
                .ok()
                .flatten()
                .map(|row| row.payload),
            KeeperRunTrigger::Continuous => None,
        };

        let slice = render_keeper_slice(
            prior_summary.as_deref(),
            &related,
            &kept,
            quest_close_report.as_deref(),
        );

        let run_id = db
            .insert_keeper_run_internal(
                agent_id,
                &trigger_label,
                quest_id.as_deref().or_else(|| trigger.quest_id()),
                &model_id,
            )
            .await?;

        // Mark in-flight before shipping the command so any threshold
        // crossing that lands while the model is answering observes the flag
        // and skips dispatch.
        {
            let mut g = entry.inner.write().await;
            g.state.keeper_in_flight = true;
            g.state.state_version = g.state.state_version.saturating_add(1);
        }

        let cmd = SidecarCommand::KeeperRun {
            agent_id: agent_id.to_string(),
            run_id,
            trigger: trigger_label,
            slice,
            config: KeeperConfig {
                provider: km.provider,
                model: km.model,
                system_prompt: cfg.keeper_system_prompt,
            },
        };
        if let Err(e) = self.send_to_sidecar(&serde_json::to_string(&cmd)?).await {
            // Roll back the in-flight flag so a future trigger can retry.
            let mut g = entry.inner.write().await;
            g.state.keeper_in_flight = false;
            return Err(e);
        }
        Ok(Some(run_id))
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
    pub(super) fn live_entry(&self, agent_id: &str) -> Arc<AgentStateEntry> {
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

        // MON-100: seed the compaction-trigger counter from the DB so the
        // running token sum survives Monarch restarts. Soft/hard threshold
        // dispatch keeps working without needing the in-memory counter to
        // outlive the process. Failures are non-fatal — an unseeded counter
        // just means the next restart starts fresh.
        let seeded_tokens = db
            .tokens_since_last_keeper_run_internal(agent_id)
            .await
            .unwrap_or(0);

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
        guard.state.tokens_since_last_compaction = seeded_tokens;
        guard.state.keeper_in_flight = false;
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
            avatar_type: None,
            avatar_path: None,
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
            title: shadow_title
                .clone()
                .unwrap_or_else(|| "Shadow Soldier".to_string()),
            grade: shadow_grade.clone().unwrap_or_else(|| "Knight".to_string()),
            id: id.clone(),
        });

        let custom_prompt = read_agent_prompt_file(&id)
            .await?
            .filter(|p| !p.trim().is_empty());

        let effective_provider = provider.clone().unwrap_or_else(|| "anthropic".to_string());
        let effective_model = model
            .clone()
            .unwrap_or_else(|| "claude-sonnet-4-5".to_string());
        let effective_thinking = match thinking_level.clone() {
            Some(v) => v,
            None => {
                crate::thinking_config::default_for(&effective_provider, &effective_model).await
            }
        };

        let captain_payload = db
            .get_captain_identity_payload_internal()
            .await
            .ok()
            .flatten();
        let shadow_payload = db
            .get_shadow_identity_payload_internal(&id)
            .await
            .ok()
            .flatten();

        let cmd = SidecarCommand::CreateSession {
            agent_id: id.clone(),
            cwd: effective_cwd,
            provider: effective_provider,
            model: effective_model,
            thinking_level: effective_thinking,
            shadow,
            custom_prompt,
            project_instructions,
            context_window: effective_context_window,
            captain_identity_payload: captain_payload,
            shadow_identity_payload: shadow_payload,
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
            obj.insert("agentId".to_string(), serde_json::Value::String(id.clone()));
        }
        let mut cmd: SidecarCommand = serde_json::from_value(value)?;
        // MON-82: on Prompt, attach the resolved classifier config + a minted
        // classification id so the sidecar can fire the classifier in
        // parallel with the Pi turn and Rust can backfill the FK when the
        // user message row lands.
        if let SidecarCommand::Prompt { classifier, .. } = &mut cmd {
            if classifier.is_none() {
                let resolved = crate::classifier_config::resolved().await;
                if resolved.enabled {
                    *classifier = Some(crate::sidecar_protocol::ClassifierInvocation {
                        id: crate::util::uuid_v4_simple(),
                        config: crate::sidecar_protocol::ClassifierInvocationConfig {
                            enabled: true,
                            primary: crate::sidecar_protocol::ClassifierProvider {
                                provider: resolved.primary.provider,
                                model: resolved.primary.model,
                            },
                            fallback: resolved.fallback.map(|f| {
                                crate::sidecar_protocol::ClassifierProvider {
                                    provider: f.provider,
                                    model: f.model,
                                }
                            }),
                            timeout_ms: resolved.timeout_ms,
                            system_prompt: resolved.system_prompt,
                        },
                    });
                }
            }
        }
        if let SidecarCommand::Prompt { message, .. } = &cmd {
            self.maybe_auto_create_quest_for_prompt(app, db, &id, message)
                .await;
        }
        self.send_with_recovery(app, db, &serde_json::to_string(&cmd)?)
            .await
    }

    async fn maybe_auto_create_quest_for_prompt(
        &self,
        app: &AppHandle,
        db: &Arc<Database>,
        agent_id: &str,
        message: &serde_json::Value,
    ) {
        let text = prompt_text(message);
        if !is_meaningful_quest_prompt(&text) {
            return;
        }
        let title = quest_title_from_prompt(&text)
            .unwrap_or_else(|| format!("Task from {}", crate::util::chrono_now()));
        let description = quest_description_from_prompt(&text);
        match db
            .auto_create_current_quest_internal(agent_id, &title, description.as_deref())
            .await
        {
            Ok(Some(quest_id)) => {
                let payload = serde_json::json!({ "id": quest_id, "agentId": agent_id });
                emit_event(
                    app,
                    &self.ws_broadcast,
                    &format!("quest-created-{}", quest_id),
                    &payload.to_string(),
                );
                emit_event(
                    app,
                    &self.ws_broadcast,
                    &format!("quest-created-for-agent-{}", agent_id),
                    &payload.to_string(),
                );
            }
            Ok(None) => {}
            Err(e) => {
                eprintln!(
                    "[monarch] auto quest creation failed for {}: {:?}",
                    agent_id, e
                );
            }
        }
    }

    /// MON-98: Push an updated captain identity payload to all live agent
    /// sessions. Each agent's `setCustomPrompt` will update its stored captain
    /// payload and rebuild the system prompt so the next turn uses the new
    /// identity. `payload = None` clears the captain section.
    pub async fn refresh_captain_identity(
        &self,
        payload: Option<String>,
    ) -> Result<(), MonarchError> {
        let agent_ids: Vec<String> = {
            let inner = self.inner.lock();
            inner.agents.keys().cloned().collect()
        };
        for agent_id in agent_ids {
            let cmd = SidecarCommand::SetCustomPrompt {
                agent_id: agent_id.clone(),
                prompt: None,
                project_instructions: None,
                captain_identity_payload: Some(payload.clone().unwrap_or_default()),
                shadow_identity_payload: None,
            };
            let _ = self.send_to_sidecar(&serde_json::to_string(&cmd)?).await;
        }
        Ok(())
    }

    /// MON-98: Push an updated shadow identity payload to a single live agent
    /// session. `payload = None` clears the shadow section.
    pub async fn refresh_shadow_identity(
        &self,
        agent_id: &str,
        payload: Option<String>,
    ) -> Result<(), MonarchError> {
        let cmd = SidecarCommand::SetCustomPrompt {
            agent_id: agent_id.to_string(),
            prompt: None,
            project_instructions: None,
            captain_identity_payload: None,
            shadow_identity_payload: Some(payload.unwrap_or_default()),
        };
        let _ = self.send_to_sidecar(&serde_json::to_string(&cmd)?).await;
        Ok(())
    }

    pub async fn kill(&self, id: &str) -> Result<(), MonarchError> {
        let cmd = SidecarCommand::DestroySession {
            agent_id: id.to_string(),
        };
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

        // MON-75: user messages can carry attachments (image bytes on
        // disk). Before replaying history into the sidecar we need to
        // splice those bytes back into the content array as image blocks
        // so the LLM sees the same multimodal context it did originally.
        let mut load_messages: Vec<LoadSessionMessage> = Vec::with_capacity(messages.len());
        for m in &messages {
            if m.role != "user" && m.role != "assistant" && m.role != "toolResult" {
                continue;
            }
            let content = if m.role == "user" && !m.attachments.is_empty() {
                rehydrate_user_content(&m.content, &m.attachments).await
            } else {
                m.content.clone()
            };
            load_messages.push(LoadSessionMessage {
                role: m.role.clone(),
                content,
                model: m.model.clone(),
            });
        }

        let cmd = SidecarCommand::LoadSession {
            agent_id,
            messages: load_messages,
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
            avatar_type: None,
            avatar_path: None,
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

/// MON-75: reassemble a user message's content JSON with its persisted
/// image attachments spliced back in as `{type:"image"}` blocks, so the
/// sidecar (and the LLM behind it) see the same multimodal payload they
/// saw when the message was first sent. The `content` argument is the
/// raw JSON string stored in `messages.content` — after MON-75 persist,
/// this is always text-only (image blocks were stripped and written to
/// disk). If a block fails to read from disk we drop it silently and
/// keep going, rather than aborting the whole replay over one missing
/// file; a gap is better than a broken restore.
async fn rehydrate_user_content(
    content: &str,
    attachments: &[crate::db::MessageAttachmentRow],
) -> String {
    // Parse the stored content as JSON. Strings and arrays are both
    // valid shapes depending on how the turn was captured; treat
    // anything else defensively as empty text.
    let parsed: serde_json::Value =
        serde_json::from_str(content).unwrap_or_else(|_| serde_json::Value::Array(Vec::new()));
    let mut blocks: Vec<serde_json::Value> = match parsed {
        serde_json::Value::Array(arr) => arr,
        serde_json::Value::String(s) => {
            if s.is_empty() {
                Vec::new()
            } else {
                vec![serde_json::json!({ "type": "text", "text": s })]
            }
        }
        other => vec![other],
    };

    for att in attachments {
        let bytes_b64 = match crate::persistence::read_attachment_bytes_base64(&att.path).await {
            Ok(b) => b,
            Err(e) => {
                eprintln!(
                    "[monarch] skipping attachment {} during replay: {}",
                    att.path, e
                );
                continue;
            }
        };
        blocks.push(serde_json::json!({
            "type": "image",
            "data": bytes_b64,
            "mimeType": att.mime_type,
        }));
    }

    serde_json::to_string(&serde_json::Value::Array(blocks)).unwrap_or_else(|_| String::new())
}

/// MON-100: render the Keeper's input slice as plain text. The Keeper
/// system prompt teaches the model the section structure (PRIOR SUMMARY /
/// RELATED MEMORIES / RECENT ACTIVITY); this helper produces that layout
/// verbatim so the prompt text and the rendering stay in lockstep.
fn render_keeper_slice(
    prior_summary: Option<&str>,
    related: &[MemoryRow],
    messages: &[MessageRow],
    quest_report: Option<&str>,
) -> String {
    let mut s = String::new();
    if let Some(p) = prior_summary {
        let trimmed = p.trim();
        if !trimmed.is_empty() {
            s.push_str("## PRIOR SUMMARY (last compaction tick)\n\n");
            s.push_str(trimmed);
            s.push_str("\n\n");
        }
    }
    if !related.is_empty() {
        s.push_str("## RELATED MEMORIES (already known — do not re-claim)\n\n");
        for m in related {
            s.push_str(&format!("- {}: {}\n", m.title, m.summary));
        }
        s.push('\n');
    }
    // P6 Slice D (MON-122): the executor's first-person report on the closing
    // quest, included only for quest-close runs. Placed before the raw stream
    // so the Keeper reads the executor's own framing first; the JSON shape is
    // kept verbatim (the report tool already trims field sizes) and the LLM
    // is told via the section header that this is first-person.
    if let Some(report) = quest_report {
        let trimmed = report.trim();
        if !trimmed.is_empty() {
            s.push_str("## QUEST REPORT (first-person from the executor)\n\n");
            s.push_str(trimmed);
            s.push_str("\n\n");
        }
    }
    s.push_str("## RECENT ACTIVITY\n\n");
    for m in messages {
        let body = extract_text_from_stored_content(&m.content);
        let body = if body.trim().is_empty() {
            m.content.clone()
        } else {
            body
        };
        s.push_str(&format!("[{} @ {}]\n{}\n\n", m.role, m.timestamp, body));
    }
    s
}

fn prompt_text(message: &serde_json::Value) -> String {
    match message {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(parts) => parts
            .iter()
            .filter_map(|p| {
                if p.get("type").and_then(|t| t.as_str()) == Some("text") {
                    p.get("text").and_then(|t| t.as_str()).map(String::from)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn is_meaningful_quest_prompt(text: &str) -> bool {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = compact.to_lowercase();
    if compact.len() < 18 {
        return false;
    }
    let chitchat = [
        "hi",
        "hello",
        "hey",
        "thanks",
        "thank you",
        "ok",
        "okay",
        "cool",
        "nice",
        "what's up",
        "how are you",
        "status",
        "ping",
    ];
    if chitchat
        .iter()
        .any(|p| lower == *p || lower == format!("{p}!"))
    {
        return false;
    }
    let task_markers = [
        "add ",
        "build ",
        "change ",
        "check ",
        "create ",
        "debug ",
        "design ",
        "fix ",
        "implement ",
        "investigate ",
        "look ",
        "make ",
        "plan ",
        "refactor ",
        "review ",
        "run ",
        "set up ",
        "update ",
        "write ",
        "pr ",
        "ticket",
        "linear",
        "roadmap",
        "branch",
        "commit",
        "test",
        "error",
        "bug",
        "file",
        "code",
    ];
    text.lines().filter(|line| !line.trim().is_empty()).count() > 1
        || compact.len() >= 80
        || task_markers.iter().any(|marker| lower.contains(marker))
}

fn quest_title_from_prompt(text: &str) -> Option<String> {
    let first = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?
        .trim_start_matches(['#', '-', '*', '>', ' '])
        .trim();
    if first.is_empty() {
        return None;
    }
    let mut title = first
        .split_whitespace()
        .take(16)
        .collect::<Vec<_>>()
        .join(" ");
    if title.len() > 90 {
        title.truncate(87);
        title.push_str("...");
    }
    Some(title)
}

fn quest_description_from_prompt(text: &str) -> Option<String> {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.is_empty() {
        return None;
    }
    let mut excerpt = compact;
    if excerpt.len() > 500 {
        excerpt.truncate(497);
        excerpt.push_str("...");
    }
    Some(format!("Auto-created from user turn:\n\n{}", excerpt))
}

/// MON-100: pull plain text out of a stored `messages.content` value, which
/// may be a JSON-encoded array of content blocks (assistant), a plain string
/// (user, free-form), or a tool-result JSON blob. The Keeper benefits from
/// reading text fluently; image data and binary blobs are skipped.
fn extract_text_from_stored_content(stored: &str) -> String {
    let trimmed = stored.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if !(trimmed.starts_with('[') || trimmed.starts_with('{') || trimmed.starts_with('"')) {
        return stored.to_string();
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(stored) else {
        return stored.to_string();
    };
    match value {
        serde_json::Value::String(s) => s,
        serde_json::Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| match b.get("type").and_then(|t| t.as_str()) {
                Some("text") => b.get("text").and_then(|t| t.as_str()).map(String::from),
                Some("thinking") => b.get("thinking").and_then(|t| t.as_str()).map(String::from),
                Some("toolCall") => {
                    let name = b.get("name").and_then(|n| n.as_str()).unwrap_or("tool");
                    let args = b
                        .get("arguments")
                        .map(|a| serde_json::to_string(a).unwrap_or_default())
                        .unwrap_or_default();
                    Some(format!("<toolCall name=\"{}\">{}</toolCall>", name, args))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        serde_json::Value::Object(obj) => {
            // Tool-result JSON: surface `result` + `toolName` so the Keeper
            // can claim "tool X returned Y" without tripping on the JSON
            // wrapper.
            let name = obj
                .get("toolName")
                .and_then(|n| n.as_str())
                .unwrap_or("tool");
            let result = obj
                .get("result")
                .map(|r| {
                    if let Some(s) = r.as_str() {
                        s.to_string()
                    } else {
                        serde_json::to_string(r).unwrap_or_default()
                    }
                })
                .unwrap_or_default();
            format!("<toolResult name=\"{}\">{}</toolResult>", name, result)
        }
        _ => stored.to_string(),
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
    use crate::memory_index::MemoryIndex;
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

    // ---- P6 Slice D (MON-122): render_keeper_slice quest-report wiring ----

    #[test]
    fn render_keeper_slice_includes_quest_report_section_when_present() {
        let slice = render_keeper_slice(
            None,
            &[],
            &[],
            Some("{\"summary\":\"shipped slice D\",\"outcome\":\"done\"}"),
        );
        assert!(slice.contains("## QUEST REPORT (first-person from the executor)"));
        assert!(slice.contains("shipped slice D"));
        // The raw stream marker stays present so the Keeper can still find it.
        assert!(slice.contains("## RECENT ACTIVITY"));
    }

    #[test]
    fn render_keeper_slice_omits_quest_report_section_when_absent() {
        let slice = render_keeper_slice(None, &[], &[], None);
        assert!(!slice.contains("## QUEST REPORT"));
        assert!(slice.contains("## RECENT ACTIVITY"));
    }

    #[test]
    fn render_keeper_slice_omits_quest_report_section_when_whitespace() {
        // Defensive: an upstream caller that handed us an empty payload
        // string should not produce a header above nothing.
        let slice = render_keeper_slice(None, &[], &[], Some("   \n   "));
        assert!(!slice.contains("## QUEST REPORT"));
    }

    #[test]
    fn auto_quest_heuristic_ignores_chitchat() {
        assert!(!is_meaningful_quest_prompt("thanks"));
        assert!(!is_meaningful_quest_prompt("how are you?"));
        assert!(!is_meaningful_quest_prompt("ok"));
    }

    #[test]
    fn auto_quest_heuristic_accepts_task_prompts() {
        assert!(is_meaningful_quest_prompt(
            "fix the failing memory retrieval test"
        ));
        assert!(is_meaningful_quest_prompt(
            "let's set up the auto quest ticket first"
        ));
        assert!(is_meaningful_quest_prompt(
            "Please inspect the Rust sidecar protocol and update the roadmap notes."
        ));
    }

    #[tokio::test]
    async fn kill_agent_round_trip_funnels_through_shared_method() {
        let db = Arc::new(Database::new_in_memory().await.expect("in-memory db"));
        let memory_index_for_mgr = Arc::new(MemoryIndex::new(std::env::temp_dir()));
        let mgr = Arc::new(AgentManager::new(db.clone(), memory_index_for_mgr));
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
            inner
                .session_map
                .insert("ipc-kill".to_string(), "s1".to_string());
            inner
                .session_map
                .insert("ws-kill".to_string(), "s2".to_string());
        }

        // IPC side: the Tauri command body is `state.kill(&id)`. Call the
        // shared method directly. send_to_sidecar will fail silently because
        // no sidecar is running; the local state cleanup runs regardless.
        mgr.kill("ipc-kill").await.expect("ipc kill");

        // WS side: full dispatch path — decode args, delegate to mgr.kill.
        let memory_index = Arc::new(MemoryIndex::new(std::env::temp_dir()));
        let ws_state = WsState {
            db,
            agent_mgr: mgr.clone(),
            model_cache,
            memory_index,
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
