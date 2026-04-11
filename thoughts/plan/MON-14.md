# MON-14 — Lift event assembly into Rust, emit pre-assembled per-agent state

## Summary

Today, Rust's `handle_sidecar_event` in `src-tauri/src/agent.rs` is a thin forwarder: it parses a sidecar JSONL line, persists terminal events to SQLite via `persist_event`, and re-emits the raw inner event on `agent-event-{agent_id}`. All turn assembly (streaming message stitching, tool-group scoping, `lastUsage`, `activityStatus`, `toolExecutions` map, `items[]`) happens in Svelte — `AgentView.svelte` owns the handler that writes into `liveAgentStore.svelte.ts`. After **MON-26 (prerequisite)** removes Council mode entirely, `AgentView.svelte` is the only consumer of the raw `agent-event-{id}` channel, which narrows this refactor's frontend blast radius to one file. This refactor moves that assembly into Rust, introduces a new per-agent `LiveAgentState` owned by `AgentManager`, emits assembled snapshots on a new `agent-state-{agent_id}` channel keyed by a monotonic `state_version`, and rewrites `liveAgentStore` as a passive receiver that seeds from a new `get_agent_state` Tauri command on mount and then applies incoming snapshots. `AgentView.svelte`'s event handler is deleted. The toolbox `AgentContext.live` contract is frozen — zero diff to tool component files — per the abstraction MON-12/MON-13 put in place explicitly to absorb this swap.

**Wire-type generation is locked to `specta` + `tauri-specta`.** See the "Wire types" section below — this is a fleet-wide decision, not a MON-14-scoped one. All existing Tauri commands migrate to typed command wrappers as part of this issue.

## Relevant files and areas

### Rust — sidecar event path and agent manager
- `src-tauri/src/agent.rs:68-90` — `AgentManager` struct and constructor. Holds `sidecar: Mutex<Option<Arc<SidecarProcess>>>`, `agents: Mutex<HashMap<String, AgentState>>`, `session_map: Arc<Mutex<HashMap<String, String>>>`, `ws_broadcast: broadcast::Sender<WsBroadcast>`, `app_handle: Mutex<Option<AppHandle>>`. The new per-agent live-state map is added here.
- `src-tauri/src/agent.rs:150-180` — sidecar stdout reader thread. Calls `handle_sidecar_event(&app_clone, &db_clone, &session_map_clone, &ws_tx, &line)` per JSONL line. This is the single write path and will mutate `LiveAgentState` after this change.
- `src-tauri/src/agent.rs:195-244` — `recover_sidecar`. Replays `create_session` + `load_session` from SQLite ancestry on sidecar crash. Must also rebuild `LiveAgentState.items` from that ancestry, reset `turn` to `Idle`, bump `state_version`, and emit one snapshot.
- `src-tauri/src/agent.rs:247-266` — `send_with_recovery`. Unchanged structurally; downstream side-effects of `recover_sidecar` extend to live state.
- `src-tauri/src/agent.rs:291-298` — `emit_event` helper. Fans out to Tauri webview and WS broadcast. The new `agent-state-{id}` channel uses the same fan-out shape.
- `src-tauri/src/agent.rs:300-374` — `handle_sidecar_event`. The match arms for `session_ready`, `session_destroyed`, `event`, `extension_ui_request`, `error`. Assembly logic and the new emit live here. `extension_ui_request` / `sidecar_error` / `session_ready` stay on `agent-event-{id}` and are **not** folded into state.
- `src-tauri/src/agent.rs:376-499` (approx) — `persist_event`. Already handles SQLite persistence; untouched by this refactor except insofar as it runs alongside the new in-memory mutation.
- `src-tauri/src/agent.rs:720-1100` — Tauri command handlers (`create_session`, `send_message`, etc.) that call `send_with_recovery`. A new `get_agent_state` command is added alongside these.
- `src-tauri/src/lib.rs` — Tauri command registration. Registers the new `get_agent_state` command.
- `src-tauri/Cargo.toml` — new deps: `dashmap` or `parking_lot` (TBD — see open questions), plus `specta`, `tauri-specta`, and their proc macros (locked in — see "Wire types" below).

### Rust — DB read path reused in recovery
- `src-tauri/src/db.rs` — `get_messages_with_ancestry` already exists and is the right primitive for rebuilding `LiveAgentState.items` on recovery. No schema change.

### Frontend — live store and consumers
- `src/lib/toolbox/liveAgentStore.svelte.ts:1-57` — store shape today (per-agent `$state` entries inside a `SvelteMap`). Rewritten to drop the `emptyLiveState` seed shape and instead accept a full snapshot from Rust. `ensureLiveState` becomes `seedFromSnapshot(agentId, snapshot)`; `resetLiveState` is replaced by the Rust-driven reset path.
- `src/lib/toolbox/types.ts:15-25` — TS `LiveAgentState` interface. Either becomes a re-export of a `ts-rs`-generated type, or is regenerated from Rust via a build step. `AgentContext.live` shape is frozen at the component boundary; see open question 1 for how.
- `src/lib/AgentView.svelte:52-54, 722-727, 784-795` — the raw-event listener, its unlisten bookkeeping, and the handler call into `handleEvent`. The listener subscription, the handler function (`handleEvent`), and all assembly logic it reaches into (`streamingMessage` stitching, tool-group scoping, `lastUsage`, `activityStatus`, `eventCount`, `items[]` mutation) are deleted. The bind/unbind path is rewritten to: (a) call `get_agent_state(target.id)` for an initial seed, (b) subscribe to `agent-state-{target.id}` for updates, (c) rely entirely on `liveAgentStore` reads for render.
- `src/lib/AgentView.svelte:797-825` — the `agent-exit-{id}` and `agent-stderr-{id}` listeners. These channels are independent of `agent-event-{id}` and stay as-is.
- `src/App.svelte:354-359, 392-394` — `agent-exit-{id}` tracking. Unchanged. Council-mode wiring is gone after MON-26, so `AgentView.svelte` is the only caller of the new store on this branch.
- `src/lib/api.ts` — `invoke`/`listen` indirection with WS fallback for the browser dev mode. No structural change; the new `get_agent_state` command and `agent-state-{id}` channel flow through the existing `invoke` and `listen` primitives. The WS re-subscription logic at `api.ts:44-96` already covers the new channel for free, because it re-sends all `listen` keys on reconnect.

### Sidecar (unchanged)
- `sidecar/src/runtime-manager.ts` — still emits raw Pi events. Out of scope for this issue.

### Docs
- `ONBOARDING.md` §5 "Agent lifecycle" and §6 "Sidecar protocol" — updated prose: Rust owns assembled state; `agent-state-{id}` is the canonical state channel; `agent-event-{id}` is narrowed to UI requests and error pings; the pull-then-subscribe reload pattern is documented.

### Prior-art plan notes
- `thoughts/plan/MON-12.md`, `thoughts/impl/MON-12.md`, `thoughts/plan/MON-13.md`, `thoughts/impl/MON-13.md` — these are the abstraction work this issue depends on. MON-13 explicitly froze `AgentContext.live` in anticipation of MON-14.

## What needs to change

At the module / concept level.

### Rust side

1. **New module `src-tauri/src/agent_state.rs`.** Defines the state domain as types, not as a bag of fields:
   - `LiveAgentState` with fields `items`, `tool_executions` (keyed by tool call id), `turn: TurnState`, `activity_status`, `event_count`, `desynced: bool`, `state_version: u64`.
   - `TurnState` as an enum with variants `Idle`, `Streaming { msg, tool_group }`, `ToolsRunning { tool_group, last_usage }`. Invalid combinations (streaming message without a turn, tool group without a phase) are unrepresentable.
   - Mirror types for `DisplayItem`, `StreamingMessage`, `ToolExecution`, `ToolGroup`, `Usage` at whatever granularity the frontend needs. Today these live only as TS shapes (`src/lib/types.ts` + toolbox `types.ts`); they must now be authored in Rust.
   - A single `apply_event` method (or a small pure-ish state machine) that takes a parsed inner event and mutates `self`, bumping `state_version`, handling desync, and returning a lightweight outcome (e.g. "emit now", "debounce", "no-op") so the caller can decide whether to flush.

2. **Phase 1 tokio-native reader (this issue).** The sidecar-stdout reader moves from a `std::thread::spawn` blocking loop to a `tokio::task::spawn` async loop. Concretely:
   - Sidecar spawn moves from `std::process::Command` to `tokio::process::Command`. The resulting `tokio::process::Child` exposes `stdout` as an `AsyncRead`.
   - Ownership split: the reader task takes `child.stdout` (async). `SidecarProcess` keeps `child.stdin` as a `std::process::ChildStdin` (sync) so the existing synchronous Tauri command handlers and `send_with_recovery` path do **not** have to change in this issue. `tokio::process::Child` allows taking each pipe independently; this split is the enabling trick that keeps MON-14's blast radius on the read side only.
   - Reader loop: `let mut lines = tokio::io::BufReader::new(stdout).lines(); while let Some(line) = lines.next_line().await? { handle_sidecar_event(..).await }`.
   - The `AgentManager` already holds `broadcast::Sender<WsBroadcast>` (tokio-aware). It gets a runtime handle from the Tauri `AppHandle` (`tauri::async_runtime::spawn` or `Handle::current()`) to spawn the reader task.
   - Write path (`SidecarProcess::write_command`, every `#[tauri::command]` in `agent.rs`) stays synchronous. This is deferred to the follow-up issue — see "Out of scope".

3. **Per-agent state ownership inside `AgentManager`.** `DashMap<String, Arc<tokio::sync::RwLock<LiveAgentState>>>`. Read-mostly outer map, fully tokio-native inner lock. No `parking_lot`, no `std::sync` for state — the reader is now async and can `.await` the lock naturally. Entry creation is lazy on first event, or eager on `session_ready`. Entry reset on `session_destroyed`. Entry removal on the existing agent-removal path. The outer `DashMap` is sync and works from both async (reader task) and sync (Tauri command handlers calling `get_agent_state`) contexts uniformly.

4. **`handle_sidecar_event` becomes an async single write + emit site.** Signature changes to `async fn`. For `event`-typed lines:
   - Parse the inner event (as today).
   - Persist the event via `tokio::task::spawn_blocking(move || persist_event(...))` — see point 5.
   - `let state = map.get(agent_id).cloned();` (DashMap read, no await). Then `let mut guard = state.write().await;`, call `apply_event`, bump `state_version`, clone a snapshot, drop the guard.
   - Decide whether to emit now or schedule a debounced emit (see point 6).
   - When emitting: serialize snapshot, call `emit_event(app, ws_tx, "agent-state-{id}", payload)`.
   - **No guard is held across the emit or across `spawn_blocking`.**
   - Parse failures and out-of-order events set `desynced: true`, log once, emit once with the flag set, and reset the turn to `Idle` on the next `message_start`. The desync branch never panics the reader task.
   - `extension_ui_request`, `sidecar_error`, `session_ready` still emit on `agent-event-{id}` and are not folded into state.

5. **DB writes via `spawn_blocking` — interim only.** `rusqlite` is blocking, and calling it directly from a tokio task blocks a runtime worker. Wrap `persist_event`'s DB calls in `tokio::task::spawn_blocking(move || db.log_event_internal(..))`. This is explicitly an **interim** for Phase 1 — the follow-up issue migrates `db.rs` to `tokio-rusqlite`, at which point these `spawn_blocking` wrappers are removed. Mark each wrapper with a `// TODO(MON-27): remove after tokio-rusqlite migration` comment so the cleanup is mechanical.

6. **New Tauri command `get_agent_state(agent_id) -> (LiveAgentState, u64)`.** Read-only. Looks up the entry in `DashMap`, acquires a read guard (`state.read().await`), clones, returns `(snapshot, state_version)`. The command stays `async fn` because it `.await`s the lock; Tauri v2 supports async commands natively. This is the pull half of pull-then-subscribe.

7. **Streaming-chunk coalescing — `tokio::time::sleep` debounce.** `message_update` events arrive at token rate; cloning + emitting a full snapshot on every chunk is wasted work. The tokio-native reader unlocks the clean option: a per-agent coalescing actor.
   - Each entry has a `dirty: bool` and an optional `debounce_handle: Option<JoinHandle<()>>`.
   - On a `message_update`: set `dirty = true`. If no debounce task is in flight, `tokio::spawn` a task that `tokio::time::sleep(Duration::from_millis(16)).await`, reacquires the lock, checks `dirty`, emits the snapshot, clears `dirty`.
   - **Terminal events (`message_end`, `tool_execution_end`, `session_ready`, errors)** cancel any pending debounce task (`handle.abort()`) and flush immediately. Users perceive these as "done" transitions and latency matters.
   - 16ms is a starting value; one line of constant to tune. Document the choice in a comment on the emit site.

8. **Crash recovery path.** `recover_sidecar` already clones the agents snapshot and replays `create_session` + `load_session` from SQLite ancestry. Extend it to, for each tracked agent:
   - Rebuild `LiveAgentState.items` from `db.get_messages_with_ancestry(session_id)`.
   - Reset `turn` to `Idle` and `tool_executions` to empty. Document in a comment why mid-stream assembly is intentionally dropped.
   - Bump `state_version`.
   - Emit one `agent-state-{id}` snapshot so the frontend store picks up the rebuilt items without needing a manual refresh.

9. **Wire types — `specta` + `tauri-specta`, fleet-wide.** This is the locked-in choice and it is deliberately fleet-wide because a split style (some commands typed, some not) is strictly worse than either pure option.
   - Add `specta`, `tauri-specta`, and their proc-macro crates to `src-tauri/Cargo.toml`.
   - Define a single `tauri-specta` command collection in `src-tauri/src/lib.rs` that registers every existing Tauri command (`create_session`, `send_message`, `broadcast_prompt` *(gone after MON-26)*, `kill_agent`, `get_agent_state` *(new)*, and the rest in `agent.rs`). Each command gets `#[tauri::command]` + the specta attribute.
   - `LiveAgentState`, `TurnState`, and the nested types in `agent_state.rs` derive `specta::Type`. Same for any existing types that cross the Tauri boundary (`AgentRow`, message rows, etc.) so the generated TS file is self-consistent.
   - A `cargo test` target exports the generated TS bindings to a fixed path (e.g. `src/lib/bindings.ts`). CI runs the test + `git diff --exit-code` to catch staleness.
   - The frontend imports typed command wrappers from `bindings.ts` instead of calling `invoke` directly. `src/lib/api.ts` either wraps the generated wrappers with the existing WS-fallback logic, or the generated wrappers are configured to route through the same `invoke` shim. Pick whichever keeps the WS fallback intact — the browser dev mode still needs it.
   - `src/lib/toolbox/types.ts`'s `LiveAgentState` becomes a re-export of the generated type. Other TS types that duplicate Rust shapes (`src/lib/types.ts` — `AgentRow`, `MessageRow`, etc.) are migrated to re-exports as part of the same pass where they cross the Tauri boundary. Pure-frontend types (UI-only view state) stay hand-written.
   - Per-agent event channels (`agent-state-{id}`, `agent-event-{id}`, `agent-exit-{id}`) still use raw `listen<T>` with an imported type param — tauri-specta's typed events only cover literal channel names, and the per-agent id interpolation rules that out. Using the generated `LiveAgentState` as the `T` is the win.

### Frontend side

10. **Rewrite `liveAgentStore.svelte.ts` as a passive receiver.** Drop `emptyLiveState` as a frontend-owned seed; the seed comes from `get_agent_state`. Keep `SvelteMap<string, LiveAgentState>` for per-key reactivity. New operations:
   - `seedFromSnapshot(agentId, snapshot, version)` — creates or replaces the entry.
   - `applyUpdate(agentId, snapshot, version)` — reconciles by `state_version`: if incoming `version <= entry.version`, drop; otherwise replace.
   - `removeLiveState(agentId)` — unchanged.
   - No write path that builds state from individual events. No imports from raw event shapes.

11. **Delete `AgentView.svelte`'s raw-event handler.** Remove `handleEvent`, the `streamingMessage` tracking, the tool-group assembly, the `lastUsage`/`activityStatus`/`eventCount` writes — everything the `unlistenEvent` closure reaches. Replace with:
   - On bind: `invoke("get_agent_state", { agentId })` → `seedFromSnapshot`.
   - `listen("agent-state-{id}", payload => applyUpdate(id, payload.snapshot, payload.version))`.
   - Render continues to read from `liveAgentStore` via the shared path established in MON-12/MON-13.
   - The `agent-exit-{id}` and `agent-stderr-{id}` listeners stay untouched.

12. **Freeze the toolbox tool contract.** `ToolDefinition`, `AgentContext`, `ToolProps` are unchanged. Tool component files (context inspector, placeholder) have a literal zero diff. This is the acceptance gate that proves the MON-12/MON-13 abstraction held.

### Docs

13. **`ONBOARDING.md` §5 and §6 updates.** Rewrite the two sections to describe:
    - `agent-state-{id}` as the canonical assembled-state channel.
    - `agent-event-{id}` as reduced to UI requests and error pings; message/tool event forwarding on this channel is deprecated and will be removed in MON-15.
    - The pull-then-subscribe reload pattern and the `state_version` reconciliation rule.
    - Where `LiveAgentState` is authored (Rust) and how the TS shape is generated.
    - The phased tokio migration: Phase 1 in MON-14 (async reader, sync write path, `spawn_blocking` for DB), Phase 2/3 in the follow-up (async write path + command handlers + `tokio-rusqlite`).

14. **Dev-only desync debug indicator.** The `desynced: bool` flag is new state that's invisible in prod today. Gate a small UI indicator behind a build-time flag: `VITE_MONARCH_DEBUG_DESYNC` (or the project's existing equivalent — pick whichever pattern the codebase already uses). The indicator renders somewhere visible inside `AgentView` when `agentContext.live.desynced` is true — a small badge or corner marker, not a blocking overlay. Default the flag to `true` in the dev/debug build config, leave it unset in prod builds. Rationale: it's the first time this state is observable at all, and surfacing it under a flag in dev means we'll actually notice if desync starts happening during development without imposing UX cost on users.

## Resolved decisions

1. **Wire-type generation: `specta` + `tauri-specta`, fleet-wide.** All existing Tauri commands migrate to typed wrappers in this issue. Every type crossing the Tauri boundary is Rust-authored and generated into `src/lib/bindings.ts`; pure-frontend view-state types stay hand-written.
2. **CouncilView migration** — obsolete. Council mode is deleted wholesale in **MON-26** as a prerequisite; `AgentView.svelte` is the only consumer of the raw event channel on this branch.
3. **Concurrency primitive: `tokio::sync::RwLock` per entry + `DashMap` outer map.** Enabled by the tokio-native reader (see Rust side §2). No `parking_lot`, no `std::sync` for state. Pure tokio-native; no bridging primitives.
4. **Coalescing: `tokio::time::sleep(16ms)` debounce with immediate-flush on terminal events.** The clean tokio-native option, unlocked by the async reader. See Rust side §7.
5. **Type port scope: full boundary sweep.** Every Rust type that crosses `#[tauri::command]` or an event payload becomes Rust-authored via `specta::Type`. This is a larger diff to `src/lib/types.ts` than the narrower alternative, but aligns the type surface in a single pass rather than leaving half-converted drift.
6. **`desynced` recovery UX: silent in prod, dev-only debug indicator gated behind a flag.** See Rust side §14. Default on in dev, off in prod.
7. **Phased tokio migration: Phase 1 only in MON-14.** Async reader + `tokio::sync::RwLock` + `DashMap` + `tokio::time::sleep` debounce + DB writes via `tokio::task::spawn_blocking` as an explicit interim. Phases 2/3 (async write path + command handler conversion + `tokio-rusqlite` migration) tracked in the follow-up issue.

## Open questions

None blocking implementation. Minor confirmations to make during the work:

- Exact env-flag name for the dev desync indicator — confirm the project's existing dev-flag convention at implementation time.
- The `tokio::process::Child` stdin-stdout ownership split — verify the exact API (whether `child.stdout.take()` + keeping `child.stdin` works cleanly with the Child handle still owning the process for kill/wait). Small research step during implementation.

## Out of scope reminders

- Council-mode deletion is **MON-26**, not MON-14. MON-14 assumes MON-26 has landed.
- **Async write path + Tauri command handler conversion** — the ~15 `#[tauri::command]` handlers in `agent.rs` stay sync in MON-14. Conversion is tracked in the follow-up tokio-migration issue.
- **`tokio-rusqlite` migration** — `persist_event` uses `tokio::task::spawn_blocking` as an interim in MON-14. Full migration to `tokio-rusqlite` (or equivalent) tracked in the same follow-up issue. Each `spawn_blocking` wrapper is tagged with a `TODO(MON-27)` comment for mechanical cleanup.
- No new UI surfaces or toolbox tools — this is a refactor.
- No persisting `LiveAgentState` to SQLite. SQLite already owns finalized messages via the ancestry path; live state is in-memory.
- No changes to the sidecar protocol. `sidecar/src/runtime-manager.ts` is untouched.
- No cross-agent observability views (timeline, audit trail, loop inspector). Those are the *beneficiaries* of this refactor and ship separately.
- No deletion of the legacy `agent-event-{id}` message/tool forwarding. MON-15 is the follow-up that removes it once nothing subscribes.
- No changes to the toolbox tool contract (`ToolDefinition`, `AgentContext`, `ToolProps`). Tool component files must have zero diff.
- No changes to `agent-exit-{id}` or `agent-stderr-{id}` — those are independent channels and stay as-is.
- No schema changes to the SQLite DB.
