# MON-27 — Complete tokio migration: async write path + tokio-rusqlite

Linear: https://linear.app/monarch-commander/issue/MON-27
Parent: MON-14 Phase 1 (merged) + Wave 0–3 cleanup train (MON-29 → MON-39)
PR: https://github.com/whoislikemiha/monarch/pull/38

## What was implemented

MON-14 Phase 1 left `src-tauri/` partially migrated: the sidecar **stdout**
reader and per-agent state assembly were already async, but every Tauri
command handler was sync, the sidecar **stdin** write path went through an
mpsc-bridged writer task, and every SQLite write was wrapped in
`tauri::async_runtime::spawn_blocking` because `rusqlite` is blocking. MON-27
closes both shortcuts together so the backend is fully tokio-native end to
end.

Two logically distinct migrations shipped in one PR because they are
entangled through the command-handler signatures and the persistence
consumer:

**Part A — async sidecar write path**

- `SidecarProcess` now holds `tokio::sync::Mutex<Option<ChildStdin>>`
  directly. The MON-14 mpsc-bridged writer task and the unbounded channel
  are deleted; `write_command` is `async fn` and does
  `stdin.write_all(line.as_bytes()).await` + `flush().await` under the
  async mutex.
- `send_to_sidecar`, `send_with_recovery`, and every `AgentManager`
  lifecycle method (`spawn`, `send_command`, `kill`, `load_session_context`,
  `new_session`, `switch_session`, `respond_extension_ui`) become
  `async fn`. The former `tauri::async_runtime::block_on(recover_sidecar)`
  bridge inside `send_with_recovery` is now a direct `.await`.
- Every `#[tauri::command]` in `agent.rs` is `async fn`. tauri-specta's
  `collect_commands!` already supports async commands (proven pre-MON-27
  by `get_agent_state` and `rebuild_agent_state_from_session`).
- `ws::dispatch_command` arms gain `.await` with no signature change at
  the WS layer.
- `shutdown_sidecar` stays sync because Tauri's `RunEvent::ExitRequested`
  hook is sync. It uses `tauri::async_runtime::block_on` on a small async
  helper that takes the `ChildStdin` out of the `Option` and drops it
  (the async equivalent of dropping the pre-MON-27 mpsc sender). The
  bounded `try_wait` poll + hard-kill fallback are unchanged.

**Part B — tokio-rusqlite migration**

- Bumped `rusqlite` to `0.37` and added `tokio-rusqlite = "0.7"` (both
  with `bundled`). `Database` now owns a `tokio_rusqlite::Connection`
  directly; the `std::sync::Mutex<Connection>` and ~40 `lock_poisoned("db")`
  sites are gone.
- Every method on `Database` is `async fn`. The body of each method moves
  verbatim into a `self.conn.call(move |c| { ... }).await` closure. The
  closures are `FnOnce(&mut rusqlite::Connection) -> Result<T, rusqlite::Error>
  + Send + 'static`, so borrowed `&str` / `&[T]` arguments are cloned
  up-front to satisfy the `Send + 'static` bound.
- `Database::new` and `new_in_memory` are `async fn`. `lib.rs::run` drives
  construction via `tauri::async_runtime::block_on(Database::new())`.
- All ~30 `db_*` Tauri command wrappers are `async fn`. Row-mapping
  helpers that previously lived inline in the command bodies (`db_create_session`,
  `db_save_message`, `db_get_project_by_path`, `db_log_event`,
  `db_get_ui_state`, `db_set_ui_state`) folded into proper `Database`
  methods so the `conn.call` scaffolding lives in one place per logical
  operation.
- `run_persist_consumer` (MON-37's single-consumer pipeline) drops the
  `tauri::async_runtime::spawn_blocking(move || cmd.apply(&db)).await`
  hop and calls `cmd.apply(&db).await` directly. `PersistCommand::apply`
  is `async fn`. FIFO ordering is still preserved by the loop awaiting
  each command before pulling the next.

**Ancillary scope**

- `persistence.rs` flips to `tokio::fs`. `read_agent_prompt_file`,
  `write_agent_prompt_file`, `prompts_dir`, `monarch_dir`,
  `prompts_dir_string`, and the three `#[tauri::command]` entry points
  are all `async fn`. The `path.exists()` probe is replaced with an
  `ErrorKind::NotFound` match on the read result.
- `project.rs::resolve_project` and `detect_project` are `async fn` for
  their DB calls.
- `MonarchError` gains a `From<tokio_rusqlite::Error>` shim that unwraps
  `Error::Error(rusqlite::Error)` back into `MonarchError::Db(..)` so the
  existing `ErrorDto { kind, message, details }` wire shape is preserved
  byte-for-byte.
- `.github/workflows/rust-test.yml`: Ubuntu, `cargo test --lib` in
  `src-tauri/`, Swatinem cache, Tauri Linux deps installed. Activates the
  MON-33 `kill_agent_round_trip_funnels_through_shared_method` test on
  CI without depending on the Windows dev machine.
- `ONBOARDING.md` §5.2 and §5.5 rewritten — the Phase 1/2 and
  `spawn_blocking`-interim language is gone.

## Key decisions

- **Library: `tokio-rusqlite`, not `sqlx`.** As recommended in the plan.
  The `.call(|c| { ... })` closure pattern is a near-mechanical wrap of
  the existing method bodies; `sqlx` would have required rewriting every
  query call site to macro syntax with zero benefit for this codebase's
  shape (~50 hand-rolled queries, hand-written migrations). Stuck with
  the recommendation.
- **Versions: rusqlite 0.37 + tokio-rusqlite 0.7.** The plan said
  "tokio-rusqlite 0.6 or current". The user asked for "latest compatible".
  `tokio-rusqlite 0.7` requires `rusqlite ^0.37` and `libsqlite3-sys 0.35`,
  which forced bumping `rusqlite` from `0.33`. No API-surface drift
  observed: `params!`, `query_row`, `query_map`, `unchecked_transaction`,
  `last_insert_rowid` all unchanged in the 0.33 → 0.37 window.
- **`Database::new` async + sync `run()`.** `Database::new` needs to
  return a future, and `lib.rs::run()` is sync (the Tauri builder isn't
  itself async at the construction site, and `Database` must exist before
  `AgentManager::new` so the persistence consumer can be spawned). Used
  `tauri::async_runtime::block_on` at the construction site instead of
  pushing the whole startup chain into Tauri's async `setup` closure —
  smaller diff, no semantic difference.
- **`tokio_rusqlite::Error` mapping.** Rather than mint a new
  `MonarchError` variant and risk disrupting the `ErrorDto` contract, the
  `From` impl unwraps `Error::Error(inner)` straight into
  `MonarchError::Db(inner)`. The other variants (`ConnectionClosed`,
  `Close`) map to `Persistence(String)` — they're edge cases that should
  never fire during normal operation, and giving them a generic
  persistence kind keeps the frontend's existing `kind` discriminator
  unchanged.
- **`shutdown_sidecar` `block_on` bridge stays.** The Tauri
  `ExitRequested` hook is sync — that is a Tauri API shape, not something
  MON-27 can change. `block_on` from a Tauri worker thread (not the tokio
  runtime worker) is safe; the bounded close timeout (1.5s) bounds
  worst-case shutdown latency.
- **Lock-hierarchy discipline.** The critical correctness invariant is
  "never hold a `parking_lot::MutexGuard` across an `.await`". `parking_lot`
  guards are `Send`, so the compiler **does not** enforce this. I audited
  every lifecycle method by hand: `spawn`, `new_session`, `switch_session`
  required scoping the `inner.lock()` reads to drop the guard before any
  DB await; the existing pre-MON-34 cloning patterns made this mostly
  mechanical. The lock-hierarchy doc comment on `AgentManager` was
  rewritten to make the rule explicit.
- **Folded inline DB helpers into `Database` methods.** The pre-MON-27
  `db_create_session`, `db_save_message`, `db_get_project_by_path`,
  `db_log_event`, `db_get_ui_state`, `db_set_ui_state` Tauri commands
  inlined their SQL at the command site instead of routing through a
  `Database` method. Migrating them this way would have required
  duplicating the `conn.call` scaffolding in two sites per query. Folded
  them into proper methods (`create_session_internal`,
  `save_message_internal`, `get_project_by_path_internal`,
  `log_event_internal`, new `get_ui_state_internal` /
  `set_ui_state_internal`) so the closure lives in one place per logical
  operation. Minor cleanup, but worth including.
- **Row mappers extracted as free functions.** `map_project`, `map_agent`,
  `map_session`, `map_message`, `map_memory`, `map_agent_template`. The
  pre-MON-27 closures were duplicated across methods; lifting them out
  keeps the `conn.call` closures small and consistent.
- **`models.rs` untouched.** Per the plan and the out-of-scope reminder.
  `MonarchError::Lock` and the `lock_poisoned` helper remain in `error.rs`
  for the four `models.rs` sites. The "no `db.rs` sites" cleanup means
  `lock_poisoned` is now a `models.rs`-only helper.
- **`persistence.rs` to `tokio::fs`, even though `tokio::fs` isn't truly
  async for regular files.** It dispatches each `std::fs` call through
  the blocking thread pool. The cost on prompt files is negligible; the
  consistency win — "every I/O boundary in the backend is async-native"
  — is the point.
- **CI workflow scoped to `cargo test --lib`.** Per the plan. `--bin
  monarch` still needs the WebView runtime at link time; the Linux Tauri
  dev deps satisfy that for the library target. Activates the MON-33
  round-trip test (now updated to `.await` the kill path) and any future
  MON-27 regression tests.
- **No new regression tests.** The plan flagged a concurrent-sends-no-deadlock
  test as optional. Skipped per pre-impl agreement with the user — the
  CI workflow primarily exists to activate the MON-33 test that was
  already written but couldn't run on Windows.

## Files touched

- `src-tauri/Cargo.toml` — bump `rusqlite` to 0.37, add
  `tokio-rusqlite = "0.7"` (both with `bundled`).
- `src-tauri/Cargo.lock` — dependency resolution.
- `src-tauri/src/db.rs` — full rewrite. `Database` owns
  `tokio_rusqlite::Connection`; every method is `async fn` with body
  inside a `conn.call` closure. New row mapper helpers. New
  `get_ui_state_internal` / `set_ui_state_internal`. All `db_*` Tauri
  commands `async fn`.
- `src-tauri/src/agent.rs` — `SidecarProcess` shape change (drop mpsc,
  hold `tokio::sync::Mutex<Option<ChildStdin>>`). All lifecycle methods
  + Tauri commands `async fn`. `recover_sidecar` and
  `rebuild_state_from_session` now `.await` the DB. `shutdown_sidecar`
  bridges the stdin close via `block_on`. Persistence consumer drops
  `spawn_blocking`. Lock-hierarchy doc comment rewritten. Test updated
  to `.await` `Database::new_in_memory()` and `mgr.kill(..)`.
- `src-tauri/src/error.rs` — new `From<tokio_rusqlite::Error>` impl.
  `MonarchError::Lock` + `lock_poisoned` retained for `models.rs`.
- `src-tauri/src/persistence.rs` — flipped to `tokio::fs`; all functions
  + Tauri commands `async fn`.
- `src-tauri/src/project.rs` — `resolve_project` and `detect_project`
  `async fn`.
- `src-tauri/src/lib.rs` — `Database::new()` driven via
  `tauri::async_runtime::block_on` in `run()`. No collect_commands or
  generate_handler shape change (async commands are transparent).
- `src-tauri/src/ws.rs` — every dispatch arm that calls a DB method or
  an `AgentManager` lifecycle method gains `.await`. Persistence-prompts
  arms gain `.await`. No transport-level signature change.
- `.github/workflows/rust-test.yml` — new file. Ubuntu runner,
  `cargo test --lib` in `src-tauri/`, Swatinem cache, Tauri Linux deps.
- `ONBOARDING.md` — §5.2 (Run loop) and §5.5 (formerly "Phased tokio
  migration") rewritten to describe the fully tokio-native posture.
- `thoughts/plan/MON-27.md` — committed alongside (per the durable
  "always commit thoughts/plan and thoughts/impl" rule).

## What was left out

- **`models.rs` lock refactor.** Out of scope per the plan; different
  concern (user memory state, not DB I/O).
- **`tracing` migration.** `agent.rs` is still 100% `eprintln!`. Same
  parking-lot item flagged in MON-37's impl notes.
- **New regression tests.** Skipped by agreement with the user. The CI
  workflow activates the existing MON-33 round-trip test, which is the
  representative end-to-end coverage for the async lifecycle path.
- **Async Tauri `setup` closure.** Could push `Database::new` into
  Tauri's async setup form. Not worth the diff: the current `block_on`
  in `run()` is one line and runs on a worker thread before the Tauri
  builder fires.
- **Removing the `_internal` suffix from `db.rs` methods.** The plan
  noted this as an open question; default was "keep, no rename churn".
  Stuck with that — every call site already uses the suffix and renaming
  ~30 methods + ~30 callers for cosmetic reasons would add noise without
  changing behaviour.
