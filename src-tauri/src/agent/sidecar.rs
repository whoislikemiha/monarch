//! Sidecar process layer — spawn, stdin/stdout wiring, crash recovery.
//!
//! MON-27: the full write path is async. `SidecarProcess` owns the sidecar's
//! `tokio::process::ChildStdin` directly behind a `tokio::sync::Mutex`, and
//! callers `.await` on `write_command`. The MON-14 Phase 1 mpsc-bridged
//! writer task and the dedicated drain loop are gone — they existed only to
//! give sync Tauri command handlers a non-blocking handoff to an async
//! writer, a premise removed when every command handler became `async fn`.
//!
//! Shutdown still closes stdin to trigger the sidecar's graceful
//! `rl.on("close")` path: dropping the `ChildStdin` taken out of the
//! `Option` is the async equivalent of dropping the mpsc sender. The
//! `child` field stays behind a `std::sync::Mutex<TokioChild>` because
//! `shutdown_sidecar` is invoked from Tauri's sync `RunEvent::ExitRequested`
//! hook and needs to observe liveness without acquiring a tokio lock —
//! `tokio::process::Child::try_wait` itself is sync, so a
//! `std::sync::Mutex` guard suffices.

use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::AppHandle;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as TokioBufReader};
use tokio::process::{Child as TokioChild, ChildStdin, Command as TokioCommand};
use tokio::sync::Mutex as TokioMutex;

use crate::agent::state::{display_items_from_messages, DisplayItem};
use crate::db::Database;
use crate::error::MonarchError;
use crate::sidecar_protocol::{LoadSessionMessage, SessionRole, SidecarCommand};

use super::event_handler::{emit_state_event, handle_sidecar_event};
use super::AgentManager;

#[allow(dead_code)]
pub(super) struct SidecarProcess {
    /// Kept so we can observe liveness via `try_wait()` and kill on shutdown.
    /// Sync mutex because the shutdown hook is sync; `try_wait` does not
    /// require a runtime context.
    pub(super) child: Mutex<TokioChild>,
    /// Async-owned stdin. Wrapped in `Mutex<Option<_>>` so the shutdown path
    /// can `take()` and drop the `ChildStdin`, closing the pipe and firing
    /// the sidecar's graceful `rl.on("close")` path.
    pub(super) stdin: TokioMutex<Option<ChildStdin>>,
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
        let Ok(child) = self.child.get_mut() else {
            return;
        };
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        if let Err(e) = child.start_kill() {
            eprintln!("[monarch] SidecarProcess Drop: start_kill failed: {}", e);
        }
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
        std::env::var("MONARCH_SIDECAR_PATH")
            .ok()
            .map(std::path::PathBuf::from),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("sidecar/dist/index.js"))),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("../sidecar/dist/index.js"))),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("../../sidecar/dist/index.js"))),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("../../../sidecar/dist/index.js"))),
    ];

    candidates
        .into_iter()
        .flatten()
        .find(|p| p.exists())
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| MonarchError::not_found("sidecar/dist/index.js"))
}

impl AgentManager {
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

    pub(super) fn ensure_sidecar(
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
        // MON-100: also clones `dispatch_tx` (for trigger enqueue) and `db`
        // (for the `keeper_result` arm's `current_objective_id` lookup).
        let app_clone = app.clone();
        let inner_clone = self.inner.clone();
        let live_states_clone = self.live_states.clone();
        let ws_tx = self.ws_broadcast.clone();
        let persist_tx = self.persist_tx.clone();
        let dispatch_tx = self.dispatch_tx.clone();
        let db_clone = self.db.clone();
        let memory_index_clone = self.memory_index.clone();
        tauri::async_runtime::spawn(async move {
            let mut lines = TokioBufReader::new(stdout).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) if !line.is_empty() => {
                        handle_sidecar_event(
                            &app_clone,
                            &persist_tx,
                            &dispatch_tx,
                            &db_clone,
                            &memory_index_clone,
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

    pub(super) async fn send_to_sidecar(&self, json: &str) -> Result<(), MonarchError> {
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
                        session_role: SessionRole::Executor,
                        messages: messages
                            .iter()
                            .filter(|m| {
                                m.role == "user" || m.role == "assistant" || m.role == "toolResult"
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
    pub(super) async fn send_with_recovery(
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
}
