use dashmap::DashMap;
use parking_lot::Mutex as PlMutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::AppHandle;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as TokioBufReader};
use tokio::process::{Child as TokioChild, ChildStdin, Command as TokioCommand};
use tokio::sync::{broadcast, mpsc, Mutex as TokioMutex, RwLock};

type TaskHandle = tauri::async_runtime::JoinHandle<()>;

use crate::agent_state::{display_items_from_messages, DisplayItem, LiveAgentState};
use crate::db::{AgentRow, Database};
use crate::error::MonarchError;
use crate::persistence::read_agent_prompt_file;
use crate::sidecar_protocol::{LoadSessionMessage, ShadowConfig, SidecarCommand};
use crate::util::chrono_now;

mod event_handler;
mod persist;

use event_handler::{emit_state_event, handle_sidecar_event};
use persist::{run_persist_consumer, PersistCommand};

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

// ---- Sidecar process management ----
//
// MON-27: the full write path is async. `SidecarProcess` owns the sidecar's
// `tokio::process::ChildStdin` directly behind a `tokio::sync::Mutex`, and
// callers `.await` on `write_command`. The MON-14 Phase 1 mpsc-bridged
// writer task and the dedicated drain loop are gone — they existed only to
// give sync Tauri command handlers a non-blocking handoff to an async
// writer, a premise removed when every command handler became `async fn`.
//
// Shutdown still closes stdin to trigger the sidecar's graceful `rl.on("close")`
// path: dropping the `ChildStdin` taken out of the `Option` is the async
// equivalent of dropping the mpsc sender. The `child` field stays behind a
// `std::sync::Mutex<TokioChild>` because `shutdown_sidecar` is invoked from
// Tauri's sync `RunEvent::ExitRequested` hook and needs to observe liveness
// without acquiring a tokio lock — `tokio::process::Child::try_wait` itself
// is sync, so a `std::sync::Mutex` guard suffices.

#[allow(dead_code)]
struct SidecarProcess {
    /// Kept so we can observe liveness via `try_wait()` and kill on shutdown.
    /// Sync mutex because the shutdown hook is sync; `try_wait` does not
    /// require a runtime context.
    child: Mutex<TokioChild>,
    /// Async-owned stdin. Wrapped in `Mutex<Option<_>>` so the shutdown path
    /// can `take()` and drop the `ChildStdin`, closing the pipe and firing
    /// the sidecar's graceful `rl.on("close")` path.
    stdin: TokioMutex<Option<ChildStdin>>,
}

impl SidecarProcess {
    async fn write_command(&self, json: &str) -> Result<(), MonarchError> {
        let mut line = json.to_string();
        if !line.ends_with('\n') {
            line.push('\n');
        }
        let mut guard = self.stdin.lock().await;
        let stdin = guard
            .as_mut()
            .ok_or_else(MonarchError::sidecar_process_down)?;
        stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| MonarchError::sidecar_stdin_write(e.to_string()))?;
        stdin
            .flush()
            .await
            .map_err(|e| MonarchError::sidecar_stdin_write(e.to_string()))?;
        Ok(())
    }
}

impl Drop for SidecarProcess {
    /// Panic-unwind safety net. If the child is still running when the last
    /// `Arc<SidecarProcess>` drops — e.g. on a Rust panic unwind that bypasses
    /// the normal `ExitRequested` shutdown path — best-effort `start_kill()`
    /// so we don't orphan the Node process.
    ///
    /// `start_kill()` is synchronous and does not await the reaper, so it is
    /// safe to call from `Drop` even when the tokio runtime is mid-teardown.
    /// We use `Mutex::get_mut` rather than `.lock()` because `&mut self` in
    /// `drop` gives us exclusive access without risking a poisoned-lock no-op.
    fn drop(&mut self) {
        let Ok(child) = self.child.get_mut() else { return };
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        if let Err(e) = child.start_kill() {
            eprintln!("[monarch] SidecarProcess Drop: start_kill failed: {}", e);
        }
    }
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

    /// Graceful-then-hard sidecar teardown, invoked from the Tauri
    /// `RunEvent::ExitRequested` hook on window close.
    ///
    /// Sequence:
    /// 1. Take the sidecar `Arc` out of the manager slot.
    /// 2. Close the `ChildStdin` (bridged via `block_on` because the
    ///    async mutex requires it) — `ChildStdin` drops → sidecar's
    ///    `rl.on("close")` fires graceful `disposeAll()` + `process.exit(0)`.
    /// 3. Poll `try_wait()` until the child reports exit or the deadline
    ///    elapses.
    /// 4. If still alive at the deadline, `start_kill()` as the hard-kill
    ///    fallback. `SidecarProcess::drop` running later is then a no-op.
    ///
    /// Sync by design so it can be called directly from Tauri's sync
    /// `RunEvent` closure. The `block_on` in step (2) is safe because the
    /// `ExitRequested` hook runs on a Tauri worker thread, not the tokio
    /// runtime worker itself. Worst-case close latency is bounded by
    /// `timeout` (typically 1.5s), acceptable during shutdown.
    pub fn shutdown_sidecar(&self, timeout: Duration) {
        let sidecar = self.sidecar.lock().take();
        let Some(sc) = sidecar else { return };

        // (2) Close stdin to trigger the sidecar's graceful-shutdown path.
        // `take()` inside the async mutex drops `ChildStdin`, closing the
        // pipe — the async equivalent of dropping the pre-MON-27 mpsc sender.
        let sc_for_close = sc.clone();
        tauri::async_runtime::block_on(async move {
            let mut guard = sc_for_close.stdin.lock().await;
            *guard = None;
        });

        // (3) Bounded wait for the child to exit on its own.
        let deadline = std::time::Instant::now() + timeout;
        let poll_interval = Duration::from_millis(25);
        loop {
            let exited = match sc.child.lock() {
                Ok(mut c) => matches!(c.try_wait(), Ok(Some(_))),
                // Poisoned lock — treat as terminal so we don't spin.
                Err(_) => true,
            };
            if exited {
                return;
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(poll_interval);
        }

        // (4) Hard-kill fallback.
        if let Ok(mut c) = sc.child.lock() {
            if let Err(e) = c.start_kill() {
                eprintln!("[monarch] shutdown_sidecar: start_kill failed: {}", e);
            }
        };
        // `sc` drops here; `SidecarProcess::drop` sees the already-killed
        // child via `try_wait()` and no-ops.
    }

    fn ensure_sidecar(
        &self,
        app: &AppHandle,
    ) -> Result<Arc<SidecarProcess>, MonarchError> {
        let mut sidecar_lock = self.sidecar.lock();

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

        let mut child = cmd.spawn()?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| MonarchError::persistence("Failed to capture sidecar stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| MonarchError::persistence("Failed to capture sidecar stderr"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| MonarchError::persistence("Failed to capture sidecar stdin"))?;

        let sc = Arc::new(SidecarProcess {
            child: Mutex::new(child),
            stdin: TokioMutex::new(Some(stdin)),
        });

        // Stdout reader task: async loop, one line → one handle_sidecar_event.
        // Owns clones of everything the handler needs; no `self` captured.
        // MON-37: captures `persist_tx` instead of `db_clone` — the reader
        // enqueues PersistCommands rather than running blocking SQL inline.
        let app_clone = app.clone();
        let inner_clone = self.inner.clone();
        let live_states_clone = self.live_states.clone();
        let ws_tx = self.ws_broadcast.clone();
        let persist_tx = self.persist_tx.clone();
        tauri::async_runtime::spawn(async move {
            let mut lines = TokioBufReader::new(stdout).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) if !line.is_empty() => {
                        handle_sidecar_event(
                            &app_clone,
                            &persist_tx,
                            &inner_clone,
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

    async fn send_to_sidecar(&self, json: &str) -> Result<(), MonarchError> {
        // Clone the Arc out while briefly holding the sync mutex so the
        // guard is dropped before the `.await` below. Holding a
        // `parking_lot::MutexGuard` across `.await` would park a runtime
        // worker (see lock-hierarchy doc comment on `AgentManager`).
        let sc = {
            self.sidecar
                .lock()
                .as_ref()
                .ok_or_else(MonarchError::sidecar_process_down)?
                .clone()
        };
        sc.write_command(json).await
    }

    /// Recover from a dead sidecar: respawn it and recreate all tracked agent sessions
    /// with their full config and session context.
    ///
    /// MON-14: also rebuilds each agent's `LiveAgentState.items` from SQLite
    /// ancestry and emits one snapshot per recovered agent on `agent-state-{id}`.
    /// Mid-stream assembly (partial streaming message, in-flight tool group)
    /// is intentionally dropped — we cannot reconstruct it from persisted rows
    /// and showing a frozen partial state would be worse than a clean reset.
    async fn recover_sidecar(
        &self,
        app: &AppHandle,
        db: &Arc<Database>,
    ) -> Result<(), MonarchError> {
        self.ensure_sidecar(app)?;

        // MON-34: single snapshot of the consolidated inner state. One lock
        // acquire, cloned out, guard dropped before anything else — no
        // ordering question, no lock held across the awaits below.
        let (agents_snapshot, session_snapshot) = {
            let guard = self.inner.lock();
            (guard.agents.clone(), guard.session_map.clone())
        };

        for (agent_id, state) in &agents_snapshot {
            // Replay the original create_session command (includes cwd, shadow, etc.)
            if let Ok(json) = serde_json::to_string(&state.create_cmd) {
                let _ = self.send_to_sidecar(&json).await;
            }

            // Replay session context from SQLite
            let messages_opt = if let Some(session_id) = session_snapshot.get(agent_id) {
                db.get_messages_with_ancestry(session_id).await.ok()
            } else {
                None
            };

            if let Some(messages) = &messages_opt {
                if !messages.is_empty() {
                    let load_cmd = SidecarCommand::LoadSession {
                        agent_id: agent_id.clone(),
                        messages: messages
                            .iter()
                            .filter(|m| {
                                m.role == "user"
                                    || m.role == "assistant"
                                    || m.role == "toolResult"
                            })
                            .map(|m| LoadSessionMessage {
                                role: m.role.clone(),
                                content: m.content.clone(),
                                model: m.model.clone(),
                            })
                            .collect(),
                    };
                    if let Ok(json) = serde_json::to_string(&load_cmd) {
                        let _ = self.send_to_sidecar(&json).await;
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
            // MON-34: now that recover_sidecar is async we block on the
            // write lock instead of the pre-MON-34 `try_write()` bail-out.
            // Recovery is rare and single-threaded per agent, so contention
            // is effectively zero; the old try_write path silently dropped
            // snapshots under races, which is the bug this fixes.
            let mut guard = entry.inner.write().await;
            if let Some(h) = guard.debounce_handle.take() {
                h.abort();
            }
            guard.dirty = false;
            guard.state.reset_with_items(items);
            // MON-38: clone + explicit drop before emit_state_event so the
            // write guard is released before any serialization runs.
            let snapshot = guard.state.clone();
            drop(guard);

            emit_state_event(app, &self.ws_broadcast, &entry.topic, &snapshot);
        }

        Ok(())
    }

    /// Send a command to the sidecar, recovering from crash if needed.
    ///
    /// MON-27: fully async. Command handlers are `async fn` so the
    /// former `block_on(recover_sidecar)` bridge is a direct `.await`.
    async fn send_with_recovery(
        &self,
        app: &AppHandle,
        db: &Arc<Database>,
        json: &str,
    ) -> Result<(), MonarchError> {
        // Fast path
        match self.send_to_sidecar(json).await {
            Ok(()) => return Ok(()),
            Err(_) => {
                eprintln!("[monarch] Send failed, attempting sidecar recovery...");
            }
        }

        self.recover_sidecar(app, db).await?;

        // Retry the original command
        self.send_to_sidecar(json).await
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

/// Resolve the sidecar script path.
///
/// The probes are rooted at `current_exe()` so resolution works the same
/// in `cargo tauri dev` (where the exe lives at `target/debug/monarch.exe`
/// and the project-root sidecar sits at `../../../sidecar/dist/index.js`)
/// and any packaged layout that keeps the sidecar next to the binary.
/// `std::env::current_dir` was used pre-MON-39 but is undefined for a
/// packaged Tauri build. `MONARCH_SIDECAR_PATH` remains a manual override
/// for unusual layouts and tests.
///
/// Packaged Tauri builds that bundle the sidecar via `externalBin` are not
/// wired up yet — a dedicated packaging ticket owns that.
fn resolve_sidecar_path() -> Result<String, MonarchError> {
    let candidates = [
        std::env::var("MONARCH_SIDECAR_PATH").ok().map(std::path::PathBuf::from),
        std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("sidecar/dist/index.js"))),
        std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("../sidecar/dist/index.js"))),
        std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("../../sidecar/dist/index.js"))),
        std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("../../../sidecar/dist/index.js"))),
    ];

    candidates
        .into_iter()
        .flatten()
        .find(|p| p.exists())
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| MonarchError::not_found("sidecar/dist/index.js"))
}



#[tauri::command]
#[specta::specta]
pub async fn detect_project(
    db: tauri::State<'_, Arc<Database>>,
    cwd: String,
) -> Result<Option<serde_json::Value>, MonarchError> {
    crate::project::detect_project(&db, &cwd).await
}

#[tauri::command]
#[specta::specta]
pub fn read_project_instructions(cwd: String) -> Result<Option<String>, MonarchError> {
    Ok(crate::project::read_project_instructions(&cwd))
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

// ---- Tauri Commands ----

/// Shadow identity block carried inside `SpawnAgentRequest`. Mirrors the
/// frontend's nested `config.shadow` object (name/title/grade), which the
/// backend then maps into the sidecar-facing `ShadowConfig` by injecting the
/// synthesized agent id at command-build time.
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ShadowSpec {
    pub shadow_name: Option<String>,
    pub shadow_title: Option<String>,
    pub shadow_grade: Option<String>,
}

/// Request payload for the `spawn_agent` Tauri command. Collapsing the ten
/// per-field params into a struct keeps the command under specta's 10-arg
/// `SpectaFn` cap so it can participate in typed binding generation — see
/// `lib.rs::specta_builder` for the registration site.
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SpawnAgentRequest {
    pub id: String,
    pub session_id: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking_level: Option<String>,
    pub cwd: Option<String>,
    pub shadow: Option<ShadowSpec>,
    pub context_window: Option<i32>,
}

/// Request payload for the `respond_extension_ui` Tauri command. MON-33 folds
/// the three scattered `agent_id` / `request_id` / `value` args into a single
/// typed struct so both the IPC and WS transports decode the same shape.
/// `value` stays `serde_json::Value` because the extension UI contract is
/// intentionally open-ended — different widget kinds return different payloads
/// and the sidecar is the ultimate authority on shape validation.
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionUiResponseRequest {
    pub agent_id: String,
    pub request_id: String,
    pub value: serde_json::Value,
}

#[tauri::command]
#[specta::specta]
pub async fn spawn_agent(
    app: AppHandle,
    state: tauri::State<'_, Arc<AgentManager>>,
    db: tauri::State<'_, Arc<Database>>,
    req: SpawnAgentRequest,
) -> Result<(), MonarchError> {
    state.spawn(&app, &db, req).await
}

#[tauri::command]
#[specta::specta]
pub async fn send_command(
    app: AppHandle,
    state: tauri::State<'_, Arc<AgentManager>>,
    db: tauri::State<'_, Arc<Database>>,
    id: String,
    command_json: String,
) -> Result<(), MonarchError> {
    state.send_command(&app, &db, id, command_json).await
}

#[tauri::command]
#[specta::specta]
pub async fn kill_agent(
    state: tauri::State<'_, Arc<AgentManager>>,
    id: String,
    _graceful: Option<bool>,
) -> Result<(), MonarchError> {
    state.kill(&id).await
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
) -> Result<Option<LiveAgentState>, MonarchError> {
    let entry = match state.live_states.get(&agent_id) {
        Some(e) => e.clone(),
        None => return Ok(None),
    };
    let guard = entry.inner.read().await;
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
) -> Result<LiveAgentState, MonarchError> {
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
pub async fn load_session_context(
    app: AppHandle,
    state: tauri::State<'_, Arc<AgentManager>>,
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
    source_session_id: String,
) -> Result<(), MonarchError> {
    state
        .load_session_context(&app, &db, agent_id, source_session_id)
        .await
}

/// Create a new session for an existing agent.
/// Creates a DB row, updates the agent→session mapping, and tells the sidecar to reset.
#[tauri::command]
#[specta::specta]
pub async fn new_agent_session(
    app: AppHandle,
    state: tauri::State<'_, Arc<AgentManager>>,
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
    new_session_id: String,
    parent_session_id: Option<String>,
) -> Result<(), MonarchError> {
    state
        .new_session(&app, &db, agent_id, new_session_id, parent_session_id)
        .await
}

/// Switch an agent to an existing persisted session instead of creating a new one.
/// Resets the sidecar's in-memory conversation and updates DB/session routing so
/// subsequent messages are appended to the selected session.
#[tauri::command]
#[specta::specta]
pub async fn switch_agent_session(
    app: AppHandle,
    state: tauri::State<'_, Arc<AgentManager>>,
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
    session_id: String,
) -> Result<(), MonarchError> {
    state.switch_session(&app, &db, agent_id, session_id).await
}

/// Forward extension UI response from frontend to sidecar
#[tauri::command]
#[specta::specta]
pub async fn respond_extension_ui(
    app: AppHandle,
    state: tauri::State<'_, Arc<AgentManager>>,
    db: tauri::State<'_, Arc<Database>>,
    req: ExtensionUiResponseRequest,
) -> Result<(), MonarchError> {
    state.respond_extension_ui(&app, &db, req).await
}

