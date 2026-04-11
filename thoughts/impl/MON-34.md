# MON-34 — Unify concurrency primitives across AgentManager

## What was implemented

`AgentManager`'s sync-path concurrency is now uniformly `parking_lot::Mutex`.
Before this change the manager mixed `std::sync::Mutex` on four fields
(`sidecar`, `agents`, `session_map`, `app_handle`) with `tokio::sync::RwLock`
on the per-agent `live_states` entries — so every sync site had to hand-wrap
`.lock().map_err(lock_poisoned(...))`, and the ordering between `agents` and
`session_map` was an implicit invariant that call sites had already violated
(`spawn` took session_map → agents; `kill` took agents → session_map;
`new_session` went session_map → agents → session_map).

This refactor:

* Folds `agents` + `session_map` into a new `AgentManagerInner` struct behind
  a single `parking_lot::Mutex`. The lock-ordering question between the two
  maps is now **structurally impossible**, not documented.
* Flips `sidecar` and `app_handle` to their own independent
  `parking_lot::Mutex` fields. They don't need to join the inner struct —
  they guard independent resources, and no path ever held both simultaneously
  with the agent maps.
* Drops every `.map_err(lock_poisoned(...))` inside `agent.rs` except the two
  on `SidecarProcess.{child, stdin_tx}`, which the ticket explicitly scopes
  out as part of the graceful-shutdown protocol.
* Turns `recover_sidecar` into an `async fn` so the per-agent `live_states`
  write acquire becomes `.write().await` instead of the pre-MON-34
  `try_write()` bail-out that silently dropped recovery snapshots under
  contention (the final acceptance bullet).
* Updates the reader-task helpers (`get_session_id`, `handle_sidecar_event`)
  to take a shared `Arc<parking_lot::Mutex<AgentManagerInner>>` handle
  instead of the removed `AgentSessionMap` type alias.

`MonarchError::Lock` and the `lock_poisoned` helper stay in `error.rs` —
`db.rs` (~40 sites) and `models.rs` (4 sites) still use them. Deleting them
is MON-27's / MON-39's call, not this ticket's.

## Key decisions

* **Option B (consolidation) over Option A (parking_lot + doc comment).**
  The plan's default was Option A: migrate the four existing `std::sync::Mutex`
  sites to `parking_lot::Mutex` and pin the lock order in a module-level
  comment. Option B folds the two agent maps into one struct behind one lock,
  which kills the ordering class of bug rather than documenting it. Diff size
  is almost identical — every call site that touched both maps now takes one
  acquire instead of two, and the reader task needs one handle instead of
  two. The MON-14-cleanup Wave 2 handoff already flagged Option B as the
  better shape; this PR commits to it.
* **`send_with_recovery` stays sync at the command boundary.** Making
  `recover_sidecar` async means the call chain touches async, but the
  `#[tauri::command]` handlers are still sync fns — propagating async all
  the way out is MON-27's job. The shim uses `tauri::async_runtime::block_on`
  to bridge into the now-async recovery. This is safe because Tauri's sync
  command handlers run on worker threads, not the runtime thread, so
  `block_on` can't deadlock the executor. Blast radius is contained to
  `agent.rs`.
* **`sidecar` and `app_handle` don't join `AgentManagerInner`.** They're
  independent resources: `sidecar` is touched on its own in command paths
  and in `shutdown_sidecar`; `app_handle` is read from the persistence
  consumer task, independent of the agent maps. Keeping them as separate
  `parking_lot::Mutex` fields is smaller-diff than consolidating everything
  and doesn't introduce new interference.
* **`switch_session` keeps two acquires around the DB call.** I took a short
  read lock for the old-session lookup, released it across the DB
  `update_session_internal` call, then a short write lock for the
  session_map insert + agent_state session_id update. One long acquire
  would have been simpler but would hold the inner lock across a blocking
  DB call. `kill` does collapse into one acquire because its DB-side work
  is already out of the lock scope.
* **Test seed path rewritten as `!contains_key(..)` on assertions** to
  satisfy `clippy::unnecessary_get_then_check`, which warns on the original
  `.get(..).is_none()` shape. Functionally identical; clippy preference.

## Files touched

* `src-tauri/Cargo.toml` — added `parking_lot = "0.12"` as a direct dep
  (already transitively present as 0.12.5).
* `src-tauri/src/agent.rs` — all of the above. Lock hierarchy doc comment
  added above `AgentManager`; `AgentSessionMap` type alias removed;
  `AgentManagerInner` added; every command method rewritten to go through
  `self.inner.lock()`; reader task spawn clones `inner` instead of
  `session_map`; `run_persist_consumer`'s `app_handle` slot flips to
  parking_lot; MON-33 round-trip test's seed path rewritten.

## What was left out

* **`SidecarProcess.{child, stdin_tx}`** stay as `std::sync::Mutex`. The
  ticket explicitly scopes them out as part of the graceful-shutdown
  protocol. The two `lock_poisoned` call sites that survive in `agent.rs`
  are both on these fields.
* **`MonarchError::Lock` / `lock_poisoned`** stay in `error.rs`. `db.rs`
  and `models.rs` still call them heavily. Deleting them is MON-27 / MON-39.
* **`live_states` inner `tokio::sync::RwLock`** is untouched — async-correct
  and owned by the MON-14 event-assembly path, explicitly out of scope.
* **`#[tauri::command]` handler signatures** stay sync. MON-27 flips them to
  `async fn` and unwinds the `block_on` shim.
* **`remove_live_entry`'s `try_write()`** stays — documented (MON-30) as
  best-effort cleanup; correctness comes from `cancel_generation`.

## Verification

* `cargo check` — clean.
* `cargo clippy --all-targets` — clean (zero warnings).
* `cargo test kill_agent_round_trip` — hits the Windows
  `STATUS_ENTRYPOINT_NOT_FOUND` Tauri DLL quirk documented in
  `thoughts/impl/MON-14-cleanup.md` and `thoughts/impl/MON-33.md`. The
  round-trip test compiles and its seed path now goes through the new
  inner lock; CI or a Linux harness is needed to actually run it.
* `bindings.ts` — no diff. No Tauri command surface changed.
