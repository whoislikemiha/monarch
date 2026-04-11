# MON-34 — Unify concurrency primitives across AgentManager

## Summary

`AgentManager` in `src-tauri/src/agent.rs` runs on two different concurrency primitives at once: `std::sync::Mutex` on the sync-path fields (`sidecar`, `agents`, `session_map`, `app_handle`) and `tokio::sync::RwLock` on each `live_states` entry (the MON-14 per-agent assembled state). The std::Mutex sites are hand-wrapped with `lock_poisoned("label")` from `src-tauri/src/error.rs` at ~15 call sites. Lock ordering between `agents` and `session_map` is an implicit invariant that the existing code already gets inconsistent (`spawn_agent` takes session_map → agents; `recover_sidecar` takes agents → session_map; `kill` takes agents → session_map). The MON-14 recovery path silently drops state snapshots whenever the per-agent `try_write()` races with the reader task.

Per the MON-14-cleanup Wave 2 note (Option B, `thoughts/impl/MON-14-cleanup.md:628`), the task is to **consolidate**: fold `agents` and `session_map` into a single struct guarded by one `parking_lot::Mutex`, which structurally kills the lock-ordering question. `sidecar` and `app_handle` also move onto `parking_lot::Mutex` individually (they each guard independent resources, not shared map state, so they don't need to join the inner struct). `live_states` is untouched — its tokio RwLock is async-correct and owned by MON-14. `recover_sidecar` becomes `async fn` so the per-entry `try_write()` bail-out turns into a blocking `.write().await` that no longer drops snapshots.

## Relevant files and areas

- `src-tauri/src/agent.rs`
  - `AgentManager` struct (lines ~157–179). In scope: `sidecar`, `agents`, `session_map`, `app_handle`. Out of scope: `live_states`, `persist_tx`, `ws_broadcast`.
  - `AgentSessionMap` type alias (line ~65): `Arc<std::sync::Mutex<HashMap<String, String>>>`. Removed — the map moves inside the new consolidated struct. The async reader task / `handle_sidecar_event` path that takes `&AgentSessionMap` needs a new shared handle.
  - `AgentManager::new` (lines ~181–214). Constructor wiring.
  - `set_app_handle` / `get_app_handle` (lines ~217–229). `app_handle` primitive swap.
  - `send_to_sidecar` (lines ~437–443) and every `self.sidecar.lock().map_err(lock_poisoned("sidecar"))?` sibling site.
  - `recover_sidecar` (lines ~453–547). Currently sync. Becomes `async fn`. Internal `try_write()` bail-out at lines ~524–531 → `.write().await`. The snapshot-then-release pattern over `agents` and `session_map` collapses into one lock acquire against the consolidated inner.
  - `send_with_recovery` (lines ~549–568). Caller of `recover_sidecar`. Becomes `async fn` or uses `tauri::async_runtime::block_on` at the sync-command boundary — see "What needs to change" for the chosen shape.
  - `remove_live_entry` (lines ~426–435). Its `try_write()` is documented (MON-30) as a best-effort cleanup; `cancel_generation` is the correctness path. **Not touched.**
  - `SidecarProcess.stdin_tx`, `SidecarProcess.child` (lines ~80–90). **Not touched** — part of the sidecar graceful-shutdown protocol, explicitly out of scope per the issue.
  - Free functions the reader task calls: `get_session_id` (line ~287 / ~956 after MON-37) and `handle_sidecar_event` / `apply_and_maybe_emit` / `mark_agent_desynced` (they take `&AgentSessionMap` today). Their signatures change to take a shared handle to the new inner struct (an `Arc<parking_lot::Mutex<AgentManagerInner>>` clone).
  - Tests module at the bottom of `agent.rs` — the MON-33 round-trip test `kill_agent_round_trip_funnels_through_shared_method` around line ~1630 seeds `mgr.agents` and `mgr.session_map` through `std::Mutex.lock().unwrap()`. Per the MON-14-cleanup note, the seeding path updates; assertion shape (kill-clears-both) stays.
- `src-tauri/src/error.rs`
  - `MonarchError::Lock` and `lock_poisoned` helper (lines ~32, ~178–184). **Stay.** `db.rs` uses them at ~40 sites and `models.rs` at 4 — they are load-bearing outside `agent.rs`. Only the `agent.rs` call sites drop away.
- `src-tauri/Cargo.toml`
  - `parking_lot` is a transitive dep only. Add it as a direct dep.
- `src-tauri/src/agent.rs` tests — follow the MON-14-cleanup note: preserve the assertion shape, rewrite the seed path.

## What needs to change

At the module level:

1. **New `AgentManagerInner` struct.** A plain sync struct holding `agents: HashMap<String, AgentState>` and `session_map: HashMap<String, String>`. Guarded by a single `parking_lot::Mutex<AgentManagerInner>` on `AgentManager`. The old `AgentSessionMap` type alias goes away; the reader task takes `Arc<parking_lot::Mutex<AgentManagerInner>>` (or a dedicated `Arc<...>` handle to just the inner) so it can look up session ids for incoming events.
2. **`sidecar` and `app_handle` become `parking_lot::Mutex`.** They don't join the inner struct — `sidecar` is accessed on its own in command paths, and `app_handle` is independently readable by the persistence consumer. Keeping them as separate `parking_lot::Mutex` fields is smaller-diff than consolidating everything, and the lock-ordering problem only existed between `agents` and `session_map`.
3. **Drop every `.map_err(lock_poisoned(...))` call inside `agent.rs`.** parking_lot doesn't poison, so `.lock()` returns the guard directly. The `lock_poisoned` helper and `MonarchError::Lock` variant stay in `error.rs` because `db.rs` and `models.rs` still use them. A future ticket (or MON-27's wake) can delete them when nothing else calls in.
4. **Reader-task + helper signature update.** `handle_sidecar_event`, `get_session_id`, `apply_and_maybe_emit`, `mark_agent_desynced`, and `persist_event` currently take `session_map: &AgentSessionMap`. They switch to taking a shared handle into the consolidated inner — cheapest shape is an `Arc<parking_lot::Mutex<AgentManagerInner>>` that gets cloned into the reader task at construction time, same spot `session_map_clone` is minted today (line ~365).
5. **`recover_sidecar` becomes `async fn`.** Reasons: (a) the per-entry `try_write()` at lines ~524–531 must turn into `.write().await` so recovery snapshots stop being silently dropped under contention — the issue's final acceptance bullet; (b) it's called by `send_with_recovery`, which is already called off the command thread from crash paths — making it async is propagation-contained. The snapshot-then-release dance at lines ~461–468 that currently takes both `agents` and `session_map` separately collapses into a single `let snapshot = { let g = inner.lock(); g.clone() };` acquire against the consolidated inner, dropping the guard before any `.await`.
6. **`send_with_recovery` stays sync at the command boundary.** Option: make it sync but block on `recover_sidecar` via `tauri::async_runtime::block_on`. Tauri's sync `#[tauri::command]` handlers call `send_with_recovery` from a worker thread, so a `block_on` there is safe (not inside the runtime thread). Alternative: make `send_with_recovery` also `async fn` and let MON-27 finish the job. Default is the `block_on` shim to keep MON-34's blast radius contained to `agent.rs` — no `#[tauri::command]` signatures change.
7. **Lock hierarchy comment.** After consolidation the only hierarchy that remains is `sidecar` vs the consolidated inner (they're independent, so no ordering required) and each is taken independently. A short module-level comment on `AgentManager` records: "`AgentManagerInner` owns the two agent maps under one lock; never hold this lock across an `.await`; `sidecar` and `app_handle` are independent and can be taken in any order." That satisfies the "lock hierarchy documented or structurally impossible" acceptance bullet — structurally impossible is the load-bearing half.
8. **Cargo.toml:** add `parking_lot = "0.12"` (or whichever version matches the transitive one in `Cargo.lock` to avoid a second copy) as a direct dep.
9. **Tests.** Rewrite the MON-33 round-trip test's seed path to insert into the new inner struct through the `parking_lot::Mutex` lock. Keep the assertion shape intact.

PR targets `markocvijanovic1998/mon-14-phase-1-rust-state-ownership`, not master. After MON-14 lands on master, the base will flip automatically.

## Out of scope

- MON-27: converting `#[tauri::command]` handlers to `async fn` and moving the manager onto `tokio::sync`. MON-14-cleanup explicitly rules this out of MON-34's scope (line ~659): "keeping sync bodies with one unified sync lock is the sized-right answer for this ticket."
- `live_states` and its inner `tokio::sync::RwLock`. Async-correct, owned by MON-14 Phase 1.
- `SidecarProcess.stdin_tx` / `SidecarProcess.child` mutexes — part of the sidecar graceful-shutdown protocol.
- `remove_live_entry`'s `try_write()` — documented as intentional best-effort cleanup; correctness comes from MON-30's `cancel_generation`.
- Deleting `MonarchError::Lock` / `lock_poisoned` — still used heavily by `db.rs` (~40 sites) and `models.rs`. Leave them alone.
- Sidecar protocol changes, SQLite changes, frontend work.
