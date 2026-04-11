# MON-27 — Complete tokio migration: async write path + tokio-rusqlite

## Summary

MON-14 Phase 1 (merged to master, commit `a9986a9`) plus the Wave 0–3 cleanup train (MON-29 through MON-39) left `src-tauri/` in a partially-migrated tokio state: the sidecar **stdout** reader and all per-agent state assembly are async-native, but the **write path** (sidecar stdin + every `#[tauri::command]` in `agent.rs`) is still synchronous, and every SQLite write still goes through `tokio::task::spawn_blocking` because `rusqlite` is blocking. MON-27 closes both interim shortcuts together so the Monarch backend is fully tokio-native end to end. The motivation is not performance on its own — it is eliminating the mixed sync/async posture that makes it awkward to write background tokio tasks that touch shared state (timeline, audit trail, loop inspector features that the roadmap has queued behind this migration).

The scope is two logically distinct migrations that must ship together because they are entangled through the command-handler signatures and the persistence consumer: **Part A** makes sidecar writes, `send_with_recovery`, and every Tauri command async; **Part B** replaces `rusqlite` with `tokio-rusqlite` (preferred) or `sqlx` and makes `Database` methods `async fn`. Phase 1 deliberately wrote its async code so that both migrations can happen at the boundaries without touching `apply_event`, `LiveAgentState`, the sidecar protocol, or the SQLite schema. MON-27 is a pure refactor — zero new features, zero UI work, zero wire-shape changes.

## Relevant files and areas

### Rust — sidecar write path and command boundary

- **`src-tauri/src/agent.rs:60-106`** — `SidecarProcess` and `write_command`. Currently `stdin_tx` is a `std::sync::Mutex<Option<mpsc::UnboundedSender<String>>>` and a dedicated tokio writer task drains the mpsc into the real `tokio::process::ChildStdin`. `write_command` is sync, called from every command handler. This whole mpsc-bridged writer task is the interim that MON-27 collapses: once the write path is async, callers can `.await` directly on a `tokio::sync::Mutex<tokio::process::ChildStdin>` and the writer task + channel go away.
- **`src-tauri/src/agent.rs:276-311`** — `shutdown_sidecar`. Deliberately sync so it can run from Tauri's sync `RunEvent::ExitRequested` closure (see MON-36 notes in the cleanup tracker). The teardown protocol (take `Arc` → drop stdin sender → bounded `try_wait` poll → `start_kill`) must still work after the mpsc goes away; the replacement is dropping/closing the `ChildStdin` itself. This is the one sync path that cannot become async without a `block_on` shim, and the tradeoff has to stay the same.
- **`src-tauri/src/agent.rs:108-127`** — `impl Drop for SidecarProcess`. Uses `Mutex::get_mut()` on the `std::sync::Mutex` around `child`. When `child` moves behind a `tokio::sync::Mutex`, `get_mut()` is still available (it takes `&mut self` — the semantics are preserved) but the lock type churn is mechanical.
- **`src-tauri/src/agent.rs:462-474, 494-528, 577-595`** — `send_to_sidecar`, the persistence consumer's sidecar echo calls, and `send_with_recovery`. These become `async fn`. `send_with_recovery`'s current shape (call `send_to_sidecar`, if error then `block_on(recover_sidecar)`, retry) already bridges to the async `recover_sidecar` via `tauri::async_runtime::block_on` — MON-34 notes explicitly flag this as "MON-27 will turn back into a direct `.await`".
- **`src-tauri/src/agent.rs:655-1010`** — every `impl AgentManager` lifecycle method (`spawn`, `send_command`, `kill`, `load_session_context`, `new_session`, `switch_session`, `respond_extension_ui`). Each one calls `send_to_sidecar` or `send_with_recovery` synchronously and is called from two places: the Tauri command body and the WS adapter in `ws::dispatch_command`. They become `async fn`; both callers `.await`.
- **`src-tauri/src/agent.rs:1591-1890`** — all `#[tauri::command]` entry points (`detect_project`, `read_project_instructions`, `spawn_agent`, `send_command`, `kill_agent`, `load_session_context`, `new_agent_session`, `switch_agent_session`, `respond_extension_ui`, plus the already-async `get_agent_state` and `rebuild_agent_state_from_session`). Every sync one becomes `async fn`. Tauri v2 supports this natively; tauri-specta's `collect_commands!` covers async signatures already (the existing `get_agent_state` command proves this).
- **`src-tauri/src/ws.rs:177-230`** — `dispatch_command`. Already `async fn`; each arm that calls an `AgentManager` lifecycle method switches from `mgr.spawn(..)?` to `mgr.spawn(..).await?`. Zero signature change at the WS boundary — the arms just gain `.await`.
- **`src-tauri/src/lib.rs:31-34, 160-163`** — the `collect_commands!` specta registration and the `tauri::generate_handler!` runtime dispatch. Both must list the same set of commands with new async signatures. The post-processing `Value = unknown; Vec<T> = T[]` workaround at `export_bindings` stays (MON-35 confirmed it is not a MON-27 responsibility).
- **`src-tauri/Cargo.toml:15-33`** — tokio features already include `process`, `io-util`, `sync`, `macros`, `rt-multi-thread`. No new tokio features required for Part A. Part B adds one DB crate (see below).

### Rust — persistence layer (Part B scope)

- **`src-tauri/src/db.rs`** (1115 lines) — every method on `Database` is currently `pub fn X_internal(&self, ...) -> Result<T, MonarchError>` and takes a `std::sync::Mutex<Connection>` lock. ~50 internal methods and ~30 `#[tauri::command]` wrappers (`db_upsert_agent`, `db_get_agents`, `db_save_message`, `db_get_messages_with_ancestry`, `db_log_event`, `db_upsert_project`, `db_save_memory`, etc.) at lines 857–1115. Each becomes async; each command becomes `async fn`. `Database::new` becomes `async fn` if the chosen library's `Connection::open` returns a future (`tokio-rusqlite::Connection::open` is async).
- **`src-tauri/src/db.rs:1-50`** — `Database` struct and constructors (`new`, `new_in_memory`). With `tokio-rusqlite`, the `Mutex<Connection>` wrapper goes away entirely — `tokio_rusqlite::Connection` is itself `Send + Sync + Clone` and dispatches work onto a dedicated background thread that owns the raw `rusqlite::Connection`. Every public method becomes `pub async fn X(&self, ...)` and closes over `|conn| { ...rusqlite... }` passed to `self.conn.call(...).await`. The `_internal` naming convention (kept through MON-33 for parity with the sync helpers) is no longer load-bearing and can be dropped or kept — see open question 2.
- **`src-tauri/src/models.rs`** — uses the `lock_poisoned` helper from 4 sites around its own state locks. The cleanup tracker notes this is "MON-27's call" for whether to also flip `models.rs` to a tokio-native posture or leave it alone. Most likely leave alone (different concern — user memory state, not DB), and the `lock_poisoned` helper + `MonarchError::Lock` variant stay in `error.rs` for those remaining sites.
- **`src-tauri/src/error.rs`** — holds `MonarchError::Lock` and the `lock_poisoned(label)` helper, still referenced from `db.rs` (~40 sites) and `models.rs` (~4 sites). After Part B, `db.rs` references drop to zero; `models.rs` keeps them. The variant and helper stay.
- **`src-tauri/src/persistence.rs`** — pure filesystem operations on prompt files. Uses blocking `std::fs::read_to_string` / `std::fs::write` / `std::fs::create_dir_all`. Flipped to `tokio::fs` equivalents in this issue. `tokio::fs` is not "real" async filesystem I/O (OSes don't expose that for regular files) — it dispatches the `std::fs` call through the blocking thread pool — but it prevents the async command handler's runtime worker from parking during the syscall and keeps the "every I/O boundary is async-native" invariant consistent. The cost on prompt files is negligible; the consistency is the point.
- **`src-tauri/src/agent.rs:1324-1580`** — the persistence consumer (MON-37's work). Currently `run_persist_consumer` drains the mpsc and calls `tauri::async_runtime::spawn_blocking(move || cmd.apply(&db_for_cmd)).await`. Each `PersistCommand::apply` call wraps the sync DB methods. After Part B, `cmd.apply(&db).await` is a direct await — no `spawn_blocking` hop, no `db_for_cmd` clone (the `Arc<Database>` clone stays, but it is not the interesting one; `Database` is now cheap-Clone or `Arc`-wrapped). Every `// TODO(MON-27)` marker in this file (grep: currently one near line 1330) is deleted.
- **`src-tauri/src/toolbox/placeholder.rs`** — has a small DB-touching command. Signature ripples the same way. Minor.

### Rust — library choice (Part B)

- **`tokio-rusqlite`** (preferred) — thin wrapper around `rusqlite` that owns a single-threaded connection on a blocking thread and exposes an async `Connection::call<F>(&self, f: F)` API. Zero schema migration, zero SQL rewrite, same feature flags (`bundled`). The one real constraint is that every closure passed to `conn.call` is `FnOnce(&mut rusqlite::Connection) -> Result<T, rusqlite::Error> + Send + 'static`, which means the body of every `Database::X_internal` moves verbatim into a closure with minor ownership adjustments (`params!` macro needs `Send` args).
- **`sqlx`** (alternative) — fully async, compile-time-checked queries, but requires rewriting every SQL call site to macro syntax and has a different schema migration story. Strictly bigger refactor for zero benefit in this project's shape; noted only because MON-27's Linear description mentions it as a call to make during planning.

**Recommendation: `tokio-rusqlite`.** Rationale: the `rusqlite` → `tokio-rusqlite` diff is mechanical (wrap each method body in `self.conn.call(|c| { ... }).await`), the existing SQL is ~50 hand-rolled queries that work today, schema migrations are already hand-written in `Database::new`, and MON-27's explicit goal is "library swap, not storage swap". `sqlx` costs more and buys nothing for this codebase.

### Frontend

- **Zero intended changes.** The acceptance criterion "no schema changes, no wire-shape changes" means `bindings.ts` regenerates to the same shape: async `#[tauri::command]`s serialize identically to sync ones under tauri-specta. The one thing that might change is the generated return type wrapping (typed error envelope) — verify during implementation that no frontend caller relies on a specific `Promise` vs. non-`Promise` assumption (they all already `await invoke` / generated wrappers, so this should be a no-op).
- **`src/lib/bindings.ts`** regenerates via `cargo run -- --export-bindings` as usual. Diff should be empty or limited to comment/signature cosmetics.

### Docs

- **`ONBOARDING.md` §5 "Agent lifecycle" and §6 "Sidecar protocol"** — the phased-tokio paragraph that MON-14 added (Phase 1 = async reader + sync write path + `spawn_blocking` for DB; Phase 2/3 = this issue) collapses to "the backend is fully tokio-native; all I/O boundaries are async". Remove the phasing language; drop any reference to `spawn_blocking` as an interim.
- **`thoughts/plan/MON-14.md` §"Out of scope reminders"** — a historical reference. Not edited (plan docs are the record of what was decided at the time), but the impl note on the follow-up branch can cross-reference that MON-27 closed the shortcut.

## What needs to change

At the module / concept level.

### Part A — async write path

1. **`SidecarProcess` lock topology.** Replace `Mutex<Option<mpsc::UnboundedSender<String>>>` with `tokio::sync::Mutex<tokio::process::ChildStdin>`. Drop the dedicated writer task and the unbounded mpsc channel — they were a workaround for "sync caller wants to hand a string to an async writer", and that premise is gone. The ChildStdin is owned directly; `write_command` becomes `async fn write_command(&self, json: &str) -> Result<(), MonarchError>` and does `self.stdin.lock().await.write_all(line.as_bytes()).await?`. The `Option` wrapper stays on the stdin field because `shutdown_sidecar` still needs to be able to drop/close the writer half to trigger the sidecar's graceful `rl.on("close")` path — taking the `ChildStdin` out of the Option and letting it drop is the async equivalent of the current "drop the mpsc sender" trick.

2. **`shutdown_sidecar` stays sync, bridges via `block_on`.** The Tauri `RunEvent::ExitRequested` callback that calls it is still sync — that is a Tauri API shape, not something MON-27 can change. The teardown protocol becomes: take the `Arc<SidecarProcess>` out of the manager slot (sync), then `tauri::async_runtime::block_on` a small async helper that acquires the `tokio::sync::Mutex<ChildStdin>`, takes the `Option`, drops it (triggering EOF on the sidecar's stdin), and polls `try_wait` on the `tokio::process::Child` with bounded deadline. `block_on` on a sync thread is safe in the Tauri run loop context (MON-34 established this pattern). The `Drop` impl on `SidecarProcess` uses the same shape or degrades to a best-effort `start_kill` via `child.get_mut()` as today.

3. **`send_to_sidecar` and `send_with_recovery` become `async fn`.** The `block_on(recover_sidecar)` bridge at the current `agent.rs:591` becomes a direct `.await self.recover_sidecar(app, db).await?`. The persistence consumer's sidecar echo calls at `agent.rs:494-528` (persistence consumer calling back into the sidecar to stream effects) stay inside the persistence consumer's own async task and pick up `.await` on `send_to_sidecar`.

4. **Every `impl AgentManager` lifecycle method becomes `async fn`.** `spawn`, `send_command`, `kill`, `load_session_context`, `new_session`, `switch_session`, `respond_extension_ui`. Internal lock work keeps using `parking_lot::Mutex` on `AgentManagerInner` per MON-34's resolution — **the critical invariant is that `parking_lot` guards are never held across an `.await`**, which MON-34's lock-hierarchy doc comment already spells out. Every existing method drops its `inner.lock()` guard before the sidecar send and before any `.await`; MON-27 must preserve that discipline and, where necessary, restructure a method to release the lock before the send. This is the subtlest correctness point in the PR and deserves a dedicated audit pass (see open question 1).

5. **Every `#[tauri::command]` entry point in `agent.rs` becomes `async fn`.** Mechanical: add `async` to the signature, add `.await` at the body's call into the lifecycle method. Tauri v2 dispatches async commands on the runtime automatically. `tauri-specta`'s `collect_commands!` already handles async — proof point is the existing `get_agent_state` and `rebuild_agent_state_from_session` commands.

6. **`ws::dispatch_command` arms gain `.await`.** Each `mgr.X(..)?` becomes `mgr.X(..).await?`. Zero signature or dispatch change at the WS layer; it is already async.

7. **Delete the intermediary writer task, the unbounded mpsc for stdin, and any imports/uses that existed only to support them.** Verify nothing else depended on the "sync from outside" entry point.

8. **Audit for `std::sync::Mutex` / `std::sync::RwLock` in `src-tauri/src`.** Grep for both. Per the acceptance criterion, the remaining ones after MON-27 should be limited to `models.rs` (deliberate, out of scope), `SidecarProcess`'s internal sync primitives if any are left (ideally none), and the `error.rs` helper's references. Document any survivors with an inline comment explaining why.

### Part B — tokio-rusqlite migration

9. **Add `tokio-rusqlite = "0.6"` (or current) to `src-tauri/Cargo.toml`.** Keep the `bundled` feature equivalent so the SQLite library is still statically linked — `tokio-rusqlite` re-exports `rusqlite`'s feature flags. Confirm at implementation time that `bundled` is the right feature name on the chosen version.

10. **Rewrite `Database` to own a `tokio_rusqlite::Connection`.** Drop `std::sync::Mutex<rusqlite::Connection>`. `Database::new` and `new_in_memory` become `async fn` and return a `Database` holding the async connection. The schema-creation + migration logic inside `new` runs via `conn.call(|c| { c.execute_batch(...) }).await?` — same SQL, same transactional shape, just wrapped.

11. **Mechanically rewrite every `Database::X_internal` method.** Pattern:
    ```
    pub async fn log_event_internal(&self, ...) -> Result<(), MonarchError> {
        self.conn.call(move |conn| {
            conn.execute(...)?;  // unchanged rusqlite code
            Ok(())
        }).await?;
        Ok(())
    }
    ```
    The closure body is 1:1 with the existing sync body. Argument captures need to be `Send + 'static`; `&str` becomes `String`, `&[T]` becomes `Vec<T>` as needed. `MonarchError::from(rusqlite::Error)` already exists (MON-31).

12. **Rewrite every `#[tauri::command]` DB wrapper in `db.rs` (~30 functions, `db_*` prefix) to `async fn` and `.await` the underlying `Database` method.** Bindings regenerate; no shape change. Each `tauri::State<'_, Arc<Database>>` continues to work in async command signatures.

13. **Delete the `spawn_blocking` persistence bridge in `agent.rs`.** `run_persist_consumer`'s `tauri::async_runtime::spawn_blocking(move || cmd.apply(&db_for_cmd)).await` becomes `cmd.apply(&db).await`. `PersistCommand::apply` becomes `async fn` and its internal DB calls `.await`. Delete every `// TODO(MON-27)` comment in `agent.rs`. Delete the `db_for_cmd` clone if it was only there for the blocking closure; keep it if the consumer still needs a cheap clone for ownership (likely does).

14. **Call-site sweep everywhere else.** Grep for direct `Database::X_internal` callers outside `db.rs` and `agent.rs` — `persistence.rs`, `project.rs` (if any DB calls), `lib.rs` setup, `toolbox/placeholder.rs`, `models.rs`. Each call site picks up `.await` or is itself promoted to async. The setup path in `lib.rs` runs `Database::new().await` inside the Tauri `setup` closure — Tauri v2 supports async setup via `tauri::Builder::setup(|app| async { ... })`.

15. **Drop the `Mutex<Connection>` lock-poisoning error paths in `db.rs`.** ~40 sites that currently do `.lock().map_err(lock_poisoned("..."))?`. With `tokio_rusqlite::Connection`, there is no user-visible lock, so the error variant disappears from `db.rs`. `MonarchError::Lock` stays in `error.rs` because `models.rs` still uses it.

16. **Delete every `// TODO(MON-27)` marker in the codebase.** One-line mechanical sweep at the end.

### Docs

17. **`persistence.rs` — flip to `tokio::fs`.** `read_agent_prompt_file`, `write_agent_prompt_file`, `monarch_dir`, `prompts_dir` all become `async fn` and use `tokio::fs::{read_to_string, write, create_dir_all}`. The three `#[tauri::command]` entry points (`get_agent_prompt`, `save_agent_prompt`, `get_prompts_dir`) become `async fn`. Bindings regenerate to the same wire shape (the `typedError<string, ErrorDto>` envelope from MON-39 item 6 stays). Call sites elsewhere (e.g. `agent.rs::spawn` loading the prompt file) pick up `.await`. Mechanical.

18. **CI cargo-test workflow** (`.github/workflows/rust-test.yml`, new file). Minimal stopgap for the Rust-on-Windows `STATUS_ENTRYPOINT_NOT_FOUND` harness gap that has been open as a parking-lot item since Wave 1. Ubuntu runner, `cargo test --lib` in `src-tauri/`, ~25 lines of YAML. Shape:
    ```yaml
    name: rust-test
    on:
      push:
        branches: [master]
      pull_request:
    jobs:
      test:
        runs-on: ubuntu-latest
        defaults:
          run:
            working-directory: src-tauri
        steps:
          - uses: actions/checkout@v4
          - uses: dtolnay/rust-toolchain@stable
          - uses: Swatinem/rust-cache@v2
            with:
              workspaces: src-tauri
          - name: Install Tauri Linux deps
            run: |
              sudo apt-get update
              sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev
          - run: cargo test --lib
    ```
    Ubuntu-only; no Windows/macOS matrix. Runs `--lib` specifically because `--bin monarch` still needs the WebView runtime at link time and the Linux Tauri dev deps satisfy that for the library target. This unblocks the MON-33 `kill_agent_round_trip` test and any MON-27 regression tests (e.g. a concurrent-sends-no-deadlock test) that the implementation wants to add. If MON-27 doesn't end up writing new Rust tests, the workflow still activates the existing MON-33 test.

19. **`ONBOARDING.md` §5 and §6 rewrite.** Replace the "Phase 1/Phase 2" and `spawn_blocking`-interim paragraphs with the final fully-tokio-native description. One-paragraph edit per section; the surrounding prose about sidecar protocol and agent lifecycle stays.

20. **`thoughts/impl/MON-27.md` post-merge.** Document the library choice, the `shutdown_sidecar` `block_on` bridge rationale, the final `SidecarProcess` shape, any surprises in the `tokio-rusqlite` `.call` closure capture constraints, and the CI workflow's first-run results. Reference this plan.

## Open questions

None blocking implementation. Resolutions from the review pass:

- **Library choice** → `tokio-rusqlite`. Revisit only if implementation-time investigation hits a hard blocker (unlikely — the `.call(|c| { ... })` closure pattern is a near-mechanical wrap of existing method bodies).
- **`persistence.rs`** → flip to `tokio::fs`. Consistency win; cost is negligible; the "every I/O boundary async-native" criterion is the point of this issue.
- **CI test workflow** → folded into scope as item 18. Ubuntu-only, `cargo test --lib`, ~25 lines of YAML. Activates the existing MON-33 round-trip test that currently can't run on the Windows dev machine.
- **MON-14 Phase 2** → **already shipped.** MON-14 is `Done` on Linear (`completedAt: 2026-04-10`); PR #30 "Phase 1 + Wave 0/1/2 cleanup (train merge)" folded in the frontend cutover. `AgentView.svelte` already listens on `agent-state-{id}` and has no `handleEvent`. The "Phase 1 / Phase 2" split was an internal planning artifact in `thoughts/plan/MON-14-*.md`, not a Linear-tracked split. MON-27 does not interact with any frontend code.

Minor confirmations to make during implementation (not blockers — the kind of thing you notice while writing the code):

- Never hold a `parking_lot::MutexGuard` across an `.await`. The `!Send` bound will make the compiler enforce this, but methods that today call `send_to_sidecar` under a guard via RAII will need their scopes tightened. `switch_session` and `spawn` are the two to watch.
- Whether to keep the `_internal` suffix on `db.rs` method names after the sync-lock boundary dissolves. Default: keep it, no rename churn.
- `tokio-rusqlite` transaction closures must not interleave external `.await` points inside the `conn.call(|c| { ... })` closure (they can't anyway — the closure is sync by construction).
- `Database::new` moves into Tauri v2's async `setup` closure; verify no managed-state reader fires before setup completes.

## Out of scope reminders

- **No new features, no new UI, no new observability tools.** Pure infrastructure migration, per the Linear description.
- **No changes to the sidecar protocol or `sidecar/src/runtime-manager.ts`.** MON-32's typed `SidecarCommand` / `SidecarEvent` enums are the final wire contract; MON-27 only changes how bytes move, not what the bytes are.
- **No schema changes to SQLite.** `tokio-rusqlite` is a transport swap; the schema, migrations, and `strftime` timestamp convention from MON-39 all stay.
- **No changes to the toolbox tool contract (`ToolDefinition`, `AgentContext`, `ToolProps`).** Zero diff to tool component files — the MON-12/MON-13 abstraction layer that MON-14 preserved continues to hold.
- **No changes to `LiveAgentState`, `apply_event`, or the `agent-state-{id}` wire shape.** MON-27 only migrates the I/O primitives underneath the event-assembly path; the assembled output is byte-identical.
- **No `sqlx` evaluation.** Library choice is `tokio-rusqlite` per the recommendation above; if implementation-time investigation finds a blocker, stop and ask before pivoting.
- **No `models.rs` lock refactor.** `models.rs` still uses `std::sync::Mutex` for its own state; that is a separate concern (user memory, not DB I/O) and stays as-is. The `MonarchError::Lock` variant and `lock_poisoned` helper remain in `error.rs` for `models.rs` and any other survivor.
- **No `SidecarProcess.{child}` mutex change beyond what the async write path requires.** The graceful-shutdown protocol ownership model stays.
- **No MON-14 Phase 2 frontend work.** That is tracked separately; MON-27 is backend-only.
- **No deletion of the legacy `agent-event-{id}` stream-event forwarding.** MON-39 already removed the Event-arm forward; the remaining `SessionReady`/`ExtensionUiRequest`/`Error` emits are permanent and stay.
- **No CI workflow scaffold** unless open question 6 is resolved in favor of folding it in.
