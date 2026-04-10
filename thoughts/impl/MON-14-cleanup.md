# MON-14 cleanup tracker

Post-review execution log for the MON-14 Phase 1 cleanup wave. One parent
issue (MON-14) with 11 sub-issues filed on Linear after the three-way agentic
review. This file is the single source of truth while the work is in flight —
status, open questions, and handoff notes between steps live here, not in
scratchpads or PR descriptions.

## Layout

The tracker is organized into **waves**. Each wave is a set of issues that
can ship together (or in parallel) without blocking each other. Waves run
strictly in order — do not start wave _n+1_ until every issue in wave _n_ is
merged.

Each issue entry has:

- **Checkbox** — flip to `[x]` only after the PR is merged into `master`, not
  at PR-open time. If an issue is split mid-flight, note the split in the
  wave's notes block and add the new checkboxes inline.
- **Linear ID + title** — clickable via the Linear plugin; the branch name on
  each issue follows the repo convention.
- **Priority + labels** — copied from Linear at the time of filing. If
  priorities shift, update here too so this doc stays the authoritative view.
- **One-line hook** — the "what does done look like" for that issue. Full
  problem statements live on Linear.

Between each wave is a **Notes** block. Use it for:

- Surprises encountered while implementing (API quirks, test failures,
  cross-issue interactions).
- Decisions that deviate from the Linear description.
- Things the _next_ wave's executor needs to know but that do not belong in
  any single issue's handoff.
- Links to the merged PRs, so the next person can diff against a known-good
  checkpoint.

Keep notes terse — a few bullets per wave is the target. If a wave's notes
start to look like a design doc, it probably means a new sub-issue is hiding
inside and should be filed rather than inlined here.

At the bottom there is a **Parking lot** section for findings that came out
of implementation but do not fit into any existing sub-issue. Anything added
there should either (a) get filed as a new Linear sub-issue and removed, or
(b) get consciously declined with a one-line reason.

## Execution plan

Order comes from the agentic-debate review recommendation:

> Ship MON-29 now (unblocks merge), then do MON-38 / MON-30 / MON-36 / MON-37
> in any order (or parallel), then the typed-protocol track
> MON-31 → MON-32 → MON-35 → MON-33 → MON-34, then MON-39 as a sweep.

### Wave 0 — Unblock merge

The one-line fix that the review identified as a true merge blocker.

- [ ] **MON-29** — Fix double JSON encoding on `agent-state-{id}` channel  · Urgent · `refactor`, `Bug` — _In review (merged to Phase 1; awaits master)_
  - Done when: `emit_event` stops wrapping already-serialized JSON in another
    JSON envelope; frontend drops its `JSON.parse` of the inner payload;
    one-frame round trip verified in dev.

**Notes (Wave 0):**

- MON-29 PR: https://github.com/whoislikemiha/monarch/pull/18 — fix landed as a new `emit_state_event` helper alongside
  the existing `emit_event`; the shared helper is untouched, so Wave 1's
  MON-38 is free to move serialization out of the write lock without
  refactoring a shared signature. State-emit call sites now clone
  `LiveAgentState` out of the guard and pass `&state` to the new helper, so
  MON-38 only has to swap the `.clone()` for the deferred-serialize pattern.
  Nothing else surprising; `cargo check`, `cargo clippy` (no new warnings),
  and `svelte-check` all clean.

---

### Wave 1 — Independent correctness and hygiene fixes

These four can run in any order or in parallel. They touch different files
and have no cross-dependencies beyond Wave 0.

- [ ] **MON-38** — Serialize `LiveAgentState` outside the write lock  · High · `performance` — _In review (merged to Phase 1; awaits master)_
  - Done when: `apply_and_maybe_emit` + `rebuild_state_from_session` + the
    debounce task all `clone` → `drop(guard)` → `to_string`. Reader throughput
    is no longer O(history) per event under load.
- [ ] **MON-30** — Fix debounce cancellation race on agent kill/destroy  · High · `Bug` — _In review (branch based on Phase 1)_
  - Done when: a killed or destroyed agent cannot emit a stale snapshot from
    a still-running debounce task. Generation / cancelled flag or `Notify`
    drop — whichever is cheaper.
- [ ] **MON-36** — Sidecar process lifecycle: `Drop` impl and `ExitRequested` hook  · High · `Bug` — _In review (branch based on Phase 1)_
  - Done when: Tauri shutdown or panic unwind terminates the Node sidecar.
    Verified by killing the app mid-stream and observing no orphan `node`
    process.
- [ ] **MON-37** — Ordered persistence pipeline via single-consumer mpsc  · High · `refactor`, `Bug` — _In review (merged to Phase 1; awaits master)_
  - Done when: sidecar → DB writes are serialized through one mpsc consumer,
    `JoinHandle`s are observed, DB errors are logged (not silently dropped),
    and `message_end` cannot land after its own `tool_execution_end`.

**Notes (Wave 1):**

- MON-38 PR: https://github.com/whoislikemiha/monarch/pull/19 — audit confirmed MON-29's refactor already moved
  `serde_json::to_string` out from under the write guard (all emit sites now
  use `emit_state_event(&LiveAgentState)` which serializes on the caller's
  stack, not in the guard). The MON-38 PR codifies the invariant by
  standardizing on the explicit `let snap = guard.state.clone(); drop(guard);
  emit_state_event(.., &snap);` shape at every site and documenting the
  constraint on `emit_state_event`. Sites touched: sidecar recovery emit,
  `rebuild_state_from_session`, `apply_and_maybe_emit` (EmitNow branch),
  `mark_agent_desynced`. Debounce task + `session_destroyed` handler already
  had the explicit form and were left alone. `cargo check`, `cargo clippy`
  (no new warnings), and `svelte-check` all clean.
- Parking-lot items (a) and (b) confirmed during MON-38 setup audit and
  filed as **MON-40** (isStreaming flag no longer toggles) and **MON-41**
  (coarse reactivity in `applyUpdate`). Removed from the parking lot.
- Parking-lot item (c) — duplicate emit in `rebuild_agent_state_from_session`
  — audited and declined for folding into MON-38. The Tauri caller dedupes
  via the `state_version` guard in `applyUpdate`; WS clients legitimately
  need the emit; making it conditional would require `emit_state_event`
  signature churn that the briefing explicitly forbade. Left in the
  parking lot with a declined note.
- MON-30 PR: https://github.com/whoislikemiha/monarch/pull/20 — fix structure: split `AgentStateEntry` into an outer
  struct holding `AtomicU64 cancel_generation` (lock-free) + inner
  `RwLock<AgentStateInner>` for `state` / `dirty` / `debounce_handle`. The
  debounce closure in `apply_and_maybe_emit` now snapshots `cancel_generation`
  at arm time; the task body — factored into `try_consume_debounce_snapshot`
  — re-reads the counter after taking the inner write lock and bails
  (without clearing `dirty`) if it changed. The three kill/reset call sites
  bump before acquiring the inner lock: `remove_live_entry` (sync —
  `fetch_add` runs before `try_write`), `session_destroyed` handler, and
  `rebuild_state_from_session`. Acquire/Release ordering pairs the sync
  bump with the task's post-lock check. `cargo check`, `cargo clippy` (no
  new warnings — same 3 pre-existing: MON-35's `too_many_arguments` ×2 and
  MON-37's `non-binding let`). Regression test gap logged in the parking
  lot — no Rust test harness exists in the repo yet.

- Audit note for the next agent reading the MON-30 diff: this changes the
  lock topology around `AgentStateEntry`. Anywhere you see
  `entry.write().await` / `entry.try_write()` historically, it is now
  `entry.inner.write().await` / `entry.inner.try_write()`. The outer
  `AgentStateEntry` is lock-free and exposes `cancel_generation` directly;
  the three `guard.state` / `guard.dirty` / `guard.debounce_handle` fields
  now live on the inner struct but with identical names, so the body of
  each guarded block is unchanged.

- MON-37 PR: https://github.com/whoislikemiha/monarch/pull/22 — replaces the per-event `spawn_blocking`
  fire-and-forget in `handle_sidecar_event` with a bounded
  `tokio::sync::mpsc::channel::<PersistCommand>(256)` drained by a single
  manager-lifetime consumer task spawned in `AgentManager::new`. FIFO
  ordering restored (one consumer), DB errors surface via `eprintln!` +
  `mark_agent_desynced` (dev indicator flips like it does for parser
  failures), and bounded back-pressure stalls the reader before unbounded
  memory growth. `PersistCommand` enum (`LogEvent`,
  `SaveAssistantMessage`, `SaveToolResult`) resolves `session_id` on the
  producer side so ordering is meaningful even if the session map
  mutates between enqueue and apply. `AgentManager::new` now takes
  `Arc<Database>`; `lib.rs` passes `database.clone()`. `ensure_sidecar`
  no longer takes `db` (reader task captures `persist_tx` clone instead).
  `app_handle` slot is now `Arc<Mutex<Option<AppHandle>>>` so the
  consumer can read it for the desync path without a back-reference to
  the manager. `tracing` not added — `eprintln!` is consistent with the
  rest of `agent.rs`; parking-lot item to migrate logging. `cargo check`,
  `cargo clippy` (no new warnings — same 2 pre-existing
  `too_many_arguments` on `spawn_agent` / `ws_spawn_agent`) clean.
  Manual smoke pending.

- MON-36 PR: https://github.com/whoislikemiha/monarch/pull/21 — wires sidecar teardown into the
  Tauri exit path. Key observation that shrank the diff: the sidecar's
  `index.ts` already has `rl.on("close", shutdown)` wired to
  `manager.disposeAll()` + `process.exit(0)`, so closing stdin *is* the
  graceful-shutdown protocol. No new `SidecarCommand::Shutdown` wire type
  added (would have pre-touched the types Wave 2 / MON-32 is refactoring).
  Structure: (1) `stdin_tx` moved to `Mutex<Option<UnboundedSender>>` so
  the shutdown path can drop it from outside the `Arc`; `write_command`
  now returns a clean `"sidecar stdin closed"` error instead of a raw
  `SendError` if it races shutdown. (2) `impl Drop for SidecarProcess`
  via `Mutex::get_mut()` → `try_wait` → `start_kill` covers panic unwind.
  (3) `AgentManager::shutdown_sidecar(timeout)` is a sync
  graceful-then-hard teardown: take sidecar `Arc` out of the slot, drop
  stdin sender, `std::thread::sleep(25ms)` poll `try_wait` up to 1500ms,
  `start_kill` on deadline. Sync so the Tauri `RunEvent::ExitRequested`
  closure in `lib.rs` calls it directly without `block_on`. `lib.rs`
  switches `.run(ctx)` → `.build(ctx).expect(..).run(closure)` to get the
  `RunEvent` callback. `cargo check`, `cargo clippy` (no new warnings —
  same 3 pre-existing from MON-30), `cargo build` clean. Manual smoke
  pending.

---

### Wave 2 — Typed protocol + error domain track

Strictly sequential: each step depends on the previous one having landed,
because they keep tightening the type system around the same call sites.

- [ ] **MON-31** — Introduce `MonarchError` domain error type  · High · `refactor` — **PR #24 merged to phase-1 (2026-04-11)**
  - Done when: Every Tauri command returns `Result<T, MonarchError>` (or an
    intentional alias); `thiserror` chains replace the `.to_string()` churn;
    poisoned-lock paths surface distinct variants.
  - _Why first in this wave:_ subsequent refactors (MON-32, MON-35, MON-33)
    all touch command signatures — converting the error type once up-front
    avoids three passes over the same lines.
- [ ] **MON-32** — Typed `SidecarEvent` and `SidecarCommand` enums  · High · `refactor` — **PR #25 merged to phase-1 (2026-04-11)**
  - Done when: `apply_event` dispatches on a `#[serde(tag = "type")]` enum
    instead of `get("type").as_str()`; outbound command JSON sites are
    replaced with a typed `SidecarCommand` serialized once. No more
    `unwrap_or("")` on event fields.
  - _Depends on MON-31_: handler returns will thread the new error type.
- [ ] **MON-35** — Re-enable specta coverage for `spawn_agent`  · High · `refactor`
  - Done when: `spawn_agent` collapses its shadow + model fields into
    `SpawnAgentRequest`, fits under the 10-arg specta cap, is re-added to the
    command collection, and the `type Value = unknown; type Vec<T> = T[]`
    post-processing hack in `lib.rs` is deleted.
  - _Depends on MON-32_: the typed command surface is cleaner once the
    sidecar protocol is typed first.
- [ ] **MON-33** — Collapse `ws_*` duplication behind a shared service layer  · High · `refactor`
  - Done when: `#[tauri::command]` handlers and `ws_*` WebSocket counterparts
    both delegate to a single service-layer function per operation. Drift
    risk is eliminated; net ~500 lines of dup removed.
  - _Depends on MON-35_: collapsing the layer is simpler when every command
    already has a clean typed signature.
- [ ] **MON-34** — Unify concurrency primitives across `AgentManager`  · Medium · `refactor`
  - Done when: the `std::sync::Mutex` / `tokio::sync::RwLock` split is
    resolved one way or the other — either `parking_lot::Mutex` everywhere
    sync remains sync, or the whole thing is fully async. No more
    `.lock().map_err(|e| e.to_string())` boilerplate.
  - _Depends on MON-33_: easier to rewrite lock acquisition once the service
    layer is the only caller site.

**Notes (Wave 2):**

<!-- Wave 2 is the longest and most interdependent — leave a checkpoint note
after each issue merges, including any assumptions the next step is making
about the shape left behind. -->

- **MON-31 — PR #24 (2026-04-11).** Full Tauri command surface now
  returns `Result<T, MonarchError>` — every `#[tauri::command]` +
  `ws_*` twin across `agent.rs`, `db.rs`, `persistence.rs`,
  `models.rs`, `toolbox/placeholder.rs` + internal helpers
  (`write_command`, `ensure_sidecar`, `Database::*_internal`,
  `resolve_project`, `lib::export_bindings`). `.map_err(|e|
  e.to_string())?` collapses to `?` via `From` impls; lock sites tag
  via `lock_poisoned(label)`. The error module is at
  `src-tauri/src/error.rs` and surfaces a flat DTO
  `{ kind, message, details }` with sidecar sub-kinds flattened to
  the top-level `kind` (`sidecarProcessDown | sidecarStdinWrite |
  sidecarReplyError | sidecarParse`). Bindings regenerated — every
  `typedError<T, E>` wrapper now has `ErrorDto` in the `E` position.
  Frontend acceptance site: `App.svelte` spawn handler branches on
  `err.kind` via a new `formatSpawnError` helper; AgentView's
  try/catch sites intentionally left on the opaque path for
  incremental consumer adoption.
  - **Deviations from plan:** no `thiserror` dep (manual `Display` +
    `Error` impls, ~30 lines, keeps DTO projection colocated);
    `Result<T>` alias lives in `error.rs` but is not re-exported
    crate-wide — call sites use the fully-qualified
    `Result<T, MonarchError>` form to avoid shadowing in files that
    collect `rusqlite::Result<Vec<_>>`; `PersistCommand::apply` **was**
    migrated (optional consistency win per the Wave 2 handoff), while
    `run_persist_consumer` stringifies only at the log/desync
    boundary; `ws::make_response` adds a new top-level `errorData`
    (DTO) field alongside the existing `error` (string) to preserve
    WS-client backwards compat rather than nesting under `error.data`.
  - **Starting point for MON-32:** command surface is uniform now —
    `MonarchError::Serde` (from `serde_json::Error`) is the natural
    landing for `sidecar_parse`-style errors once the typed
    `SidecarEvent` lands. The two pre-existing `too_many_arguments`
    clippy warnings on `spawn_agent` / `ws_spawn_agent` still stand,
    waiting on MON-35. `resolve_sidecar_path`'s
    `MonarchError::not_found("sidecar/dist/index.js")` is a good
    template for the MON-32 branch that needs to distinguish config
    issues from runtime failures.

- **Handoff from Wave 1 → Wave 2 starting point.** Wave 1 is merged
  into `markocvijanovic1998/mon-14-phase-1-rust-state-ownership` (all
  five PRs: MON-29 #18, MON-38 #19, MON-30 #20, MON-36 #21, MON-37 #22).
  None have reached `master` yet — the whole Phase 1 train merges
  together later, which is why the Wave 1 checkboxes stay `[ ]` per
  the tracker rules. Base all Wave 2 branches on the Phase 1 branch,
  not on `master`.

- **Next task: MON-35 — Re-enable specta coverage for `spawn_agent`.**
  Third in Wave 2. MON-32 is merged (PR #25, commit `c3f35ce` on the
  phase-1 base), so the typed `SidecarCommand` surface is in place
  and `SidecarCommand::CreateSession` is the obvious landing site
  for the collapsed shape. Full ticket:
  https://linear.app/monarch-commander/issue/MON-35

  _Linear state note:_ the merge auto-flipped MON-32 to **Done**
  even though phase-1 is not `master`. Per the durable rule, revert
  MON-32 back to **In Review** manually — the actual ship happens
  when the phase-1 train lands on master.

  _Scope reminder from the ticket:_ collapse `spawn_agent`'s shadow
  + model + thinking_level + context_window fields into a
  `SpawnAgentRequest` struct so the Tauri command fits under the
  10-arg specta cap. Re-add it to the `collect_commands!` macro in
  `lib.rs::specta_builder()` (currently omitted — see the doc
  comment at `lib.rs:31-34`). Delete the
  `type Value = unknown; type Vec<T> = T[]` post-processing hack
  in `lib.rs::export_bindings` at line 116.

  _Heads-up on the hack-deletion bullet:_ see the "MON-32 residue
  MON-35 will encounter" bullets below. The `Value = unknown` hack
  is **not** a pure spawn_agent problem — it workarounds a specta
  rc.24 bug that bites every `serde_json::Value` reference in the
  command surface, and `LiveAgentState` + MON-32's new typed
  protocol added more of those, not fewer. Surface the issue
  early, and negotiate with the reviewer whether to split the hack
  deletion into a follow-up sub-issue or descope that acceptance
  bullet from MON-35.

- **Wave 2 starting state for MON-35.** Branch from
  `markocvijanovic1998/mon-14-phase-1-rust-state-ownership` at
  `c3f35ce` (MON-32 merge commit). `cargo check` / `cargo clippy`
  clean modulo the two pre-existing `too_many_arguments` warnings
  on `spawn_agent` (13 args) and `ws_spawn_agent` (11 args) —
  MON-35 fixes both. `svelte-check` clean (269 files). No Rust
  test harness — manual verification only.

- **Wave 2 residue MON-32 will encounter (from MON-31).**
  - `MonarchError::Serde` (from `serde_json::Error`) is the natural
    landing for `SidecarEvent` / `SidecarCommand` parse failures —
    the `From<serde_json::Error>` impl is already wired, so
    `serde_json::from_str::<SidecarEvent>(line)?` just works. If
    MON-32 wants richer context (e.g. which event type failed to
    parse), prefer `MonarchError::sidecar_parse(raw)` over inventing
    a new variant — that sub-kind already exists and flattens to
    `sidecarParse` on the wire.
  - The sidecar event reader in `agent.rs::handle_sidecar_event`
    currently logs parse errors via `eprintln!` and bails; MON-32
    should not migrate that log to `tracing` (parking-lot item) but
    **should** make the error path surface via
    `mark_agent_desynced` the same way malformed-envelope errors
    already do.
  - `build_persist_commands` takes `event: &serde_json::Value`. If
    MON-32 types the event shape, this function's signature wants to
    change to `&SidecarEvent` at the same time so the `.get("X")`
    chains collapse to field access. Fair game — it's all internal
    to the persistence consumer.
  - `ws::dispatch_command` deserializes inbound args via
    `serde_json::from_value(args.get("agent")…)` with
    `MonarchError::invalid_input(...)` wraps. If MON-32 introduces
    typed inbound `SidecarCommand`s on the WS side too, that
    invalid-input path is the correct landing site — do not add a
    new variant for it.
  - The two pre-existing `too_many_arguments` clippy warnings on
    `spawn_agent` / `ws_spawn_agent` still stand. **Do not** fix
    them in MON-32 — MON-35 collapses the signatures via
    `SpawnAgentRequest`, and pre-doing it here steps on that diff.
  - `PersistCommand::apply` was migrated to `MonarchError` during
    MON-31 (optional consistency win). `run_persist_consumer` still
    stringifies at its log/desync boundary — that's intentional and
    MON-32 should not touch it.

- **MON-32 — PR #25 (2026-04-11).** Typed Rust ↔ sidecar JSONL
  protocol shipped in three commits on the PR branch (`c8a7994`
  outbound → `d8af6be` inbound + `apply_event` move → `52d901d`
  typed `send_command` passthrough). New module
  `src-tauri/src/sidecar_protocol.rs` (~680 lines) mirrors
  `sidecar/src/protocol.ts`: `SidecarCommand` (Serialize +
  Deserialize, 11 variants), `SidecarEvent` / `InnerEvent`
  (Deserialize with custom impls routing unknown tags through
  explicit `Unknown { raw }` variants while propagating errors on
  malformed known-tag payloads), plus `Message` / `ShadowConfig` /
  `LoadSessionMessage` helpers. A free `apply_event(&mut
  LiveAgentState, &InnerEvent)` function replaces the inherent
  method; `LiveAgentState` loses `streaming_from_json` /
  `parse_usage` (derived `Deserialize` + `streaming_from` in the
  new module) and `Usage` / `Cost` pick up `#[serde(default)]` at
  struct level to preserve per-field defaulting. Every outbound
  sidecar-command site in `agent.rs` migrated to typed
  `SidecarCommand`; `AgentState.create_cmd_json: String` →
  `create_cmd: SidecarCommand`; `handle_sidecar_event` parses each
  line twice (`Value` for byte-fidelity `LogEvent.data` +
  `from_value::<SidecarEvent>` for dispatch); `build_persist_commands`
  takes `&InnerEvent` + raw `Option<&Value>`; `send_command` /
  `ws_send_command` validate payload shape via
  `from_value::<SidecarCommand>` after injecting `agentId` into
  the raw `Value`. Full impl notes: `thoughts/impl/MON-32.md`.
  - **Deviations from plan:** `InnerEvent::Unknown` handling moved
    from `apply_event` to `handle_sidecar_event` (cleaner match
    exhaustiveness in `build_persist_commands`, raw-payload
    logging at the reader boundary where `agent_id` is in scope).
    `send_command` uses `Value`-inject-then-`from_value` instead
    of a `SidecarCommand::set_agent_id` helper + per-variant
    `#[serde(default)]` (one-shot validation, zero type
    pollution). `Usage` / `Cost` struct-level `#[serde(default)]`
    preserves pre-MON-32 per-field `unwrap_or(0)` behavior
    without tightening the schema against Pi SDK events that
    omit fields. `ToolExecutionEnd.tool_name` is
    `Option<String>` with a `"unknown"` fallback in
    `build_persist_commands` to preserve the pre-MON-32 stored
    `toolResult` row shape byte-for-byte. `CompactionStart.reason`
    / `CompactionEnd.aborted` stay `Option<T>` with display-time
    fallbacks in `apply_event` — the Option is the schema
    commitment, the fallback is only for status-item text.
  - **Starting point for MON-35:** the typed command surface is
    now the canonical wire contract, and
    `SidecarCommand::CreateSession` is the obvious landing site
    for `SpawnAgentRequest`'s inner representation — the typed
    shape already collapses shadow + model + thinking_level +
    context_window into one struct. MON-35 can build a
    `SpawnAgentRequest { id, session_id, create_command:
    SidecarCommand::CreateSession {...}}` or just reuse
    `CreateSession` directly on the Tauri command and plumb
    `id` + `session_id` as sibling fields. Either shape gets
    `spawn_agent` under the 10-arg specta cap.

- **MON-32 residue MON-35 will encounter.**
  - The two `too_many_arguments` clippy warnings on `spawn_agent`
    (13 args) and `ws_spawn_agent` (11 args) are the only
    non-clean warnings on the branch. MON-32 intentionally did
    not touch them; they are MON-35's core deliverable.
  - `spawn_agent` is currently omitted from the specta
    `collect_commands!` macro in `lib.rs::specta_builder()` — see
    the doc comment at `lib.rs:31-34` and `lib.rs:160-163` for
    the runtime `tauri::generate_handler!` (which *does* include
    it because specta is only used for type export). MON-35 re-
    adds it to `collect_commands!` once the arg count is ≤ 10.
  - The `type Value = unknown; type Vec<T> = T[]` post-processing
    hack in `lib.rs::export_bindings` (line 116) is **not a pure
    spawn_agent problem** — it workarounds a specta rc.24 bug in
    TS emission of `serde_json::Value` references. MON-32
    actually adds *more* `Value` references (via
    `SidecarCommand::ExtensionUiResponse.value`,
    `ToolExecutionStart.args`, `ToolExecutionEnd.result`,
    `InnerEvent::Unknown.raw`), and `LiveAgentState` already
    exposes `ContentBlocks` / `ToolArgs` / `ToolResult` as
    `serde_json::Value`. Deleting the hack per MON-35's
    acceptance bullet may require either a specta upgrade or
    wrapping those fields in specta-aware newtypes — this is
    likely *not* inside MON-35's spawn_agent refactor scope.
    Flag early and negotiate with the reviewer: either split
    the hack deletion into a follow-up sub-issue, or descope
    that bullet from MON-35.
  - The `#[tauri::command]` + `ws_spawn_agent` duplication is
    MON-33's territory, not MON-35's. MON-35 should leave the
    twin arrangement in place — collapse the arg list on both
    sides in parallel, but do not merge the two functions into
    one shared service layer.
  - `SidecarCommand::CreateSession` has required `cwd`,
    `provider`, `model`, `thinking_level` fields (no Options).
    The Tauri `spawn_agent` currently passes `Option<String>` for
    each and defaults to literals (`"anthropic"`,
    `"claude-sonnet-4-5"`, `"medium"`). If `SpawnAgentRequest`
    exposes these as `Option<String>` to preserve the call-site
    shape, the struct-to-`CreateSession` conversion needs to
    apply the same defaults (the current implementation at
    `agent.rs::spawn_agent` does this inline — MON-35 should
    lift it into a `From<SpawnAgentRequest>` or helper).
  - The `send_command` / `ws_send_command` typed passthrough
    uses a `Value`-inject + `from_value::<SidecarCommand>` round
    trip. If MON-35 adds any new frontend call shapes (unlikely
    — it's a pure signature refactor) they'll go through the
    same validation. Do not add `#[serde(default)]` to
    `SidecarCommand` agent_id fields — the `Value`-inject
    handles it cleanly and pollutes nothing.

- **Wave 1 residue MON-31 encountered.** (Historical — MON-31 shipped
  2026-04-11, PR #24. Keeping the list for context on what MON-32 and
  beyond inherit.)
  - MON-37 added `Result<(), String>` in `PersistCommand::apply` and
    `run_persist_consumer` inside `agent.rs`. Those are **not** Tauri
    command returns — they are internal to the persistence consumer
    loop — so they are not in scope for the MON-31 "zero
    `Result<T, String>` on `#[tauri::command]`" acceptance bullet.
    Leave them `Result<(), String>` or convert them to
    `Result<(), MonarchError>` as a consistency win, but do not
    conflate them with the boundary migration.
  - MON-37 added `eprintln!("[monarch] persist failed: ..")` on the
    consumer's error path. The parking lot has a "migrate `agent.rs`
    logging from `eprintln!` to `tracing`" item — MON-31 should not
    touch that; it is Wave 2 parking-lot territory, not a MON-31 task.
  - MON-37 changed `app_handle` to `Arc<Mutex<Option<AppHandle>>>`
    (`std::sync::Mutex`). MON-34 is the issue that unifies the
    `std::sync::Mutex` / `tokio::sync::RwLock` split — MON-31 should
    leave the lock topology alone and only carve out the poisoned-lock
    error variant (`MonarchError::Lock`).
  - Wave 1 left two pre-existing clippy warnings on `spawn_agent` /
    `ws_spawn_agent` (`too_many_arguments`, 13 and 11 args
    respectively). MON-35 fixes them by collapsing shadow + model
    fields into `SpawnAgentRequest`. MON-31 should **not** fix them —
    touching those signatures in a pure error-type migration adds
    diff noise that MON-35 will rewrite anyway.


- **Plan-then-impl workflow is the expectation.** Previous Wave 1
  issues each landed a `thoughts/plan/MON-XX.md` (research/plan phase)
  and a `thoughts/impl/MON-XX.md` (notes after implementation), both
  committed on the feature branch per the durable "always commit
  thoughts/plan and thoughts/impl" rule. Open a MON-31 plan first,
  get it reviewed, then implement.

---

### Wave 3 — Sweep

Final tidying pass. Runs after every other wave is merged so it does not
have to re-verify anything.

- [ ] **MON-39** — Phase 1 cleanup: remove dead code and legacy channels  · Medium · `chore`
  - Done when: dead `AgentLifecycleState` is either driven or deleted;
    legacy `agent-event-{id}` message/tool forward is removed now that
    `liveAgentStore` consumes the new channel; `uuid_v4_simple` is replaced
    with the `uuid` crate; `chrono_now` timestamps stop colliding with
    SQLite `datetime('now')` defaults (pick one format, parse both during
    a migration window, then converge).

**Notes (Wave 3):**

<!-- Final sweep notes: what got deleted, what got kept despite the review
flag, and why. -->

---

## Parking lot

Findings from the in-editor review pass that are **not yet filed as Linear
sub-issues**. Each bullet is a candidate — decide during implementation
whether to file, fold into an existing issue, or decline.

- ~~**`agent.isStreaming` no longer flips during normal operation.**~~
  Confirmed during MON-38 setup. Filed as **MON-40** (High, Bug).
  Removed from parking lot.

- ~~**Coarse reactivity in `liveAgentStore.applyUpdate`.**~~ Confirmed
  during MON-38 setup. Filed as **MON-41** (High, performance). Removed
  from parking lot.

- **`rebuild_agent_state_from_session` both returns the snapshot and emits
  on `agent-state-{id}`.** Callers seed via `seedFromSnapshot` _and_ the
  event fires, producing a dropped duplicate via the `state_version` guard.
  Functionally correct, one wasted serialize+emit per rebuild. Minor.
  → **Declined during MON-38 implementation.** The Tauri caller dedupes via
  the `state_version` guard in `applyUpdate` (drops equal version); WS
  clients legitimately need the emit. A conditional-emit path would require
  `emit_state_event` signature churn that the MON-38 briefing explicitly
  forbade ("other Wave 1 issues are not expecting churn there"). Leaving
  the wasted one-emit-per-rebuild as the better tradeoff.

- **Migrate `agent.rs` logging from `eprintln!` to `tracing`.** MON-37
  surfaced this: the acceptance bullet "logs errors via `tracing`" was
  satisfied in spirit with `eprintln!("[monarch] persist failed: ..")`
  because the entire rest of `agent.rs` uses `eprintln!`. Introducing
  `tracing` just for the persist path would have created two logging
  conventions in the same file and been out of scope for a Wave 1
  cleanup. Worth doing as a file-wide sweep, not a one-site carve-out.
  Candidate Wave 2 item.

- **No working Rust test harness.** Discovered during MON-30 — the repo
  has zero `#[test]` or `#[tokio::test]` functions and no CI workflow. I
  added a `#[cfg(test)] mod tests` to `src-tauri/src/agent.rs` exercising
  `try_consume_debounce_snapshot` (arm → bump gen → assert no emit and
  `dirty` preserved, plus the happy path and `remove_live_entry`
  invalidation case), but the test binary fails to start on Windows with
  `STATUS_ENTRYPOINT_NOT_FOUND` (`0xc0000139`) — a Tauri-on-Windows DLL
  symbol mismatch that only bites non-GUI binaries. Gating the tests on
  `#[cfg(not(target_os = "windows"))]` was rejected as dead code (no CI,
  dev machine is Windows), so the test module was removed and the helper
  `try_consume_debounce_snapshot` was kept in place as a pure async
  function that a future harness can call with no `AppHandle` dependency.
  The MON-30 acceptance bullet for a regression test is unmet pending
  this. Candidate fixes: add a GitHub Actions workflow running `cargo
  test --lib` on `ubuntu-latest`, or restructure so lock-free helpers
  live in a sub-crate that does not link Tauri. Worth filing before Wave
  2 kicks off — Wave 2 is the refactor-heavy track and will want tests.

---

## How to update this file

- Check a box only after the PR is merged (`git log master --oneline` has
  the commit).
- Add a bullet to the wave's notes block on every merge. One line minimum:
  PR link + "nothing surprising" or "watch out for X in next step."
- If a finding shows up mid-implementation, add it to the parking lot first,
  then decide whether to file. Never let a finding live only in a PR
  description.
- If an issue is renumbered or split on Linear, update the checkbox line
  here in the same commit that pushes the Linear change.
