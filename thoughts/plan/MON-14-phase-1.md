# MON-14 Phase 1 — Rust-side state ownership + wire types

Parent plan: [`MON-14.md`](./MON-14.md). Phase 2: [`MON-14-phase-2.md`](./MON-14-phase-2.md).

## Goal

Move turn assembly into Rust and publish assembled snapshots on a new `agent-state-{id}` channel, **without touching the frontend consumer**. The existing `agent-event-{id}` forwarding continues unchanged so `AgentView.svelte` keeps rendering from its current handler. Phase 2 flips the frontend over.

This is shippable on its own: Phase 1 ends with dual emission (legacy raw events + new assembled state) and an unused-but-exercisable `get_agent_state` command. Nothing in the UI observes the new channel yet.

## Scope (what Phase 1 ships)

1. **New module `src-tauri/src/agent_state.rs`** — `LiveAgentState`, `TurnState` enum, nested types (`DisplayItem`, `StreamingMessage`, `ToolExecution`, `ToolGroup`, `Usage`), and an `apply_event` state machine. All types derive `specta::Type`. Invalid turn-state combinations are unrepresentable.
2. **Per-agent state map on `AgentManager`** — `DashMap<String, Arc<tokio::sync::RwLock<LiveAgentState>>>`. Entry creation lazy on first event or eager on `session_ready`. Reset on `session_destroyed`. Removal on agent-removal path.
3. **Tokio-native sidecar reader.** Sidecar spawn moves to `tokio::process::Command`. `child.stdout` is taken by an async reader task (`tokio::task::spawn` via `tauri::async_runtime::spawn`). `child.stdin` stays sync on `SidecarProcess` so existing `#[tauri::command]` handlers and `send_with_recovery` do not change. Verify the `tokio::process::Child` stdin/stdout ownership split lets the Child still handle kill/wait cleanly (minor research step noted in parent plan).
4. **`handle_sidecar_event` becomes `async fn`** — parses inner event, persists via `tokio::task::spawn_blocking(move || persist_event(...))` (marked `// TODO(MON-27)`), acquires the per-agent write guard, calls `apply_event`, bumps `state_version`, clones a snapshot, drops the guard, then emits. No guard held across `.await` points or `spawn_blocking`. **Legacy `agent-event-{id}` forwarding is preserved unchanged** — Phase 1 emits on both channels. `extension_ui_request`, `sidecar_error`, `session_ready` continue to route only on `agent-event-{id}` and are not folded into `LiveAgentState`.
5. **New Tauri command `get_agent_state(agent_id) -> (LiveAgentState, u64)`** — async, reads through `DashMap` + `state.read().await`, clones and returns. Registered in `lib.rs`.
6. **Streaming coalescing** — per-entry `dirty: bool` + `debounce_handle: Option<JoinHandle<()>>`. `message_update` sets `dirty` and spawns a 16ms `tokio::time::sleep` task if none in flight. Terminal events (`message_end`, `tool_execution_end`, `session_ready`, errors) `handle.abort()` the pending debounce and flush immediately. Debounce interval in a named constant with a comment explaining the choice.
7. **Recovery** — `recover_sidecar` rebuilds `LiveAgentState.items` from `db.get_messages_with_ancestry(session_id)`, resets `turn` to `Idle`, clears `tool_executions`, bumps `state_version`, and emits one snapshot per recovered agent. Comment inline why mid-stream assembly is intentionally dropped.
8. **Fleet-wide `specta` + `tauri-specta` migration.**
   - New deps in `src-tauri/Cargo.toml`: `specta`, `tauri-specta`, their proc-macro crates, plus `dashmap`.
   - Single `tauri-specta` command collection in `src-tauri/src/lib.rs` registering every existing `#[tauri::command]` in `agent.rs` plus the new `get_agent_state`. Each command is annotated with the specta attribute.
   - Every type crossing the Tauri boundary derives `specta::Type` — `LiveAgentState` and nested types, `AgentRow`, `MessageRow`, `ProjectRow`, anything referenced from a command signature or event payload.
   - A `cargo test` target exports the bindings to `src/lib/bindings.ts`. CI runs the test + `git diff --exit-code` on that path to catch staleness. (If no CI is configured for that yet, leave a note in the impl doc — Phase 2 doesn't block on it.)
   - Frontend is **not** switched to the generated wrappers in Phase 1 (that happens in Phase 2 alongside the store rewrite). The generated file exists and compiles, but `api.ts` continues to call `invoke` directly. This keeps Phase 1's frontend diff to zero.

## Out of Phase 1 (deferred to Phase 2)

- Rewriting `liveAgentStore.svelte.ts` as a passive receiver.
- Deleting `AgentView.svelte`'s `handleEvent`, streaming stitching, `activityStatus`/`eventCount`/`lastUsage` tracking.
- Pull-then-subscribe wire-up (`get_agent_state` + `agent-state-{id}` listener) in `AgentView`.
- Frontend migration to generated command wrappers from `bindings.ts`.
- `ONBOARDING.md` §5/§6 prose updates.
- Dev-only desync indicator (`VITE_MONARCH_DEBUG_DESYNC`).
- Deletion of legacy `agent-event-{id}` message/tool forwarding (that's the follow-up issue tracked in the parent plan).

## Files touched (expected)

- **New:** `src-tauri/src/agent_state.rs`, `src/lib/bindings.ts` (generated).
- **Modified:** `src-tauri/src/agent.rs` (sidecar spawn, reader task, `handle_sidecar_event`, `AgentManager`, `recover_sidecar`, `get_agent_state` command), `src-tauri/src/lib.rs` (command registration, tauri-specta builder), `src-tauri/Cargo.toml` (deps), `src-tauri/src/db.rs` (derive `specta::Type` on boundary types), possibly `src-tauri/src/toolbox/mod.rs` if any types cross the boundary.
- **Untouched:** all frontend files, sidecar, SQLite schema, prompt persistence, toolbox tool components.

## Acceptance criteria for Phase 1

- Rust compiles (`cargo check`) and unit tests (if any) pass.
- Sidecar spawns and reader task runs under tokio; agent creation + send-message still works end-to-end through the existing UI (frontend still assembles from legacy channel, but server-side tokio path must not regress behavior).
- `get_agent_state` command is invocable and returns a non-empty snapshot after a turn, with `state_version > 0`.
- `agent-state-{id}` events can be observed (e.g. via a temporary `console.log` listener during smoke test, or DevTools) — this is the qualitative check that the new channel fires on message updates and flushes immediately on terminal events.
- `specta`-generated `bindings.ts` exists, compiles, and the generated `LiveAgentState` shape matches the hand-written TS shape closely enough that Phase 2's swap will be mechanical.
- Zero diff to any frontend file.
- Zero diff to sidecar, SQLite schema, and toolbox tool components.

## Handoff notes to Phase 2

The handoff section in this file will be **updated at the end of Phase 1** with:

- Final shape of `LiveAgentState` and `TurnState` as they landed in `agent_state.rs`, including any field renames from what the parent plan sketched.
- Exact import path and export name of the generated `LiveAgentState` type in `bindings.ts`.
- Whether the debounce constant ended up at 16ms or was tuned.
- Any surprises in the tokio-process stdin/stdout split that Phase 2 should know about.
- Any TODOs or `// TODO(MON-27)` markers left in the code, so Phase 2 doesn't trip on them.
- The exact `agent-state-{id}` payload shape (`{ snapshot, version }` vs. flattened) so Phase 2's listener signature matches on day one.
- Whether `bindings.ts` is wired into CI or still manual.

## HANDOFF (filled in at end of Phase 1)

**Phase 1 merged state — read this before starting Phase 2.**

### Wire shape

`LiveAgentState` (in `src-tauri/src/agent_state.rs`) is a flat struct, not the `TurnState` enum the parent plan sketched. The plan's "invalid states unrepresentable" goal was traded for "frontend contract frozen so Phase 2 is a mechanical swap." The invariants (no streaming message without a turn, etc.) are enforced inside `apply_event` rather than via the type system. Noted in the module doc comment.

Generated TS shape is in `src/lib/bindings.ts` as `export type LiveAgentState`. Import via:

```ts
import type { LiveAgentState } from "$lib/bindings";
```

Field-level gotchas worth knowing:

- **Option fields serialize as `null`**, not as omitted keys. Phase 1 had to drop `#[serde(skip_serializing_if = "Option::is_none")]` throughout — specta rc.24's unified mode can't represent conditional omission. Phase 2's store will see `streamingMessage: null`, `lastUsage: null`, etc. Adjust reads accordingly.
- **`toolExecutions` serializes as a JS object**, not a `Map`. Phase 2's `seedFromSnapshot` needs to wrap it in `new Map(Object.entries(snapshot.toolExecutions))` before assigning into the store so the `AgentContext.live` shape seen by tool components stays frozen (they expect `Map<string, ToolExecution>`).
- **`currentToolGroupIdx`** is `#[serde(skip)]` — it's an internal Rust index into `items`, not sent on the wire. Phase 2 should derive `currentToolGroup` from `items` if it needs one (scan for the last `ToolGroup` with `turnComplete: false`), or leave it out entirely since nothing reads it on the frontend right now.
- **`DisplayItem` kind tags** are kebab-case (`"tool-group"`), field names inside variants are camelCase (including `turnComplete`). This matches `src/lib/types.ts` exactly.
- **`ContentBlocks`** is typed as `Vec<serde_json::Value>` on the Rust side → renders in bindings.ts as a tagged-union-of-tagged-unions through the `Value` alias we inject. Functionally equivalent to `any[]` for the frontend. The existing `src/lib/types.ts` `ContentBlock` union is stricter and better for the UI; Phase 2 can keep importing that for tool-component types and only use the generated `LiveAgentState` for the store seed.

### Wire channel

Snapshots are emitted on `agent-state-{id}` as a **flat JSON-serialized `LiveAgentState`** — not a `{ snapshot, version }` envelope. Phase 2's listener is:

```ts
listen<string>(`agent-state-${target.id}`, (event) => {
  const snapshot: LiveAgentState = JSON.parse(event.payload);
  applyUpdate(target.id, snapshot, snapshot.stateVersion);
});
```

Note the payload is a **string** (the reader serializes, then emits the JSON string — consistent with the legacy `agent-event-{id}` channel). `get_agent_state` the Tauri command returns a structured `LiveAgentState | null`, not a string — that's the "pull" side, not the "listen" side. Keep them distinct in Phase 2.

### Debounce constant

Landed at 16ms (`DEBOUNCE_MILLIS` in `src-tauri/src/agent.rs`). Applied only to `message_update`. All other events flush immediately. Terminal events (`message_end`, `tool_execution_end`, `agent_end`, etc.) abort any pending debounce before flushing. Constant has a comment explaining the tradeoff.

### Dual emission in place

Phase 1 emits on **both** `agent-state-{id}` (new, assembled) and `agent-event-{id}` (legacy, raw) for `event`-typed sidecar lines. The frontend still subscribes to the legacy channel in `AgentView.svelte` and assembles state in JS. Phase 2 **must**:

1. Remove the `listen(`agent-event-${id}`, ...)` subscription and the entire `handleEvent` function from `AgentView.svelte`.
2. Replace with pull-then-subscribe on `agent-state-{id}`.
3. **Do not remove** the Rust-side legacy emit (`agent.rs` `handle_sidecar_event` `"event"` arm). That's the follow-up issue tracked at the bottom of the parent plan — it's easier to verify nothing else subscribes after Phase 2 lands, then remove the Rust emit as a separate one-line change.

`extension_ui_request`, `sidecar_error`, and `session_ready` still route **only** on `agent-event-{id}` and are not folded into `LiveAgentState`. Phase 2's `AgentView.svelte` needs two listeners: one for `agent-state-{id}` (messages, tool execution, activity) and one for `agent-event-{id}` narrowed to extension UI + error pings + session_ready. The `agent-exit-{id}` and `agent-stderr-{id}` channels are untouched and stay.

### Sidecar / tokio split

- `SidecarProcess` now holds `child: Mutex<tokio::process::Child>` and an `mpsc::UnboundedSender<String>` for stdin.
- A dedicated tokio task drains the mpsc and writes to `child.stdin`. This keeps the write path synchronous from the POV of every `#[tauri::command]` in `agent.rs` — none of them were converted to async in Phase 1.
- stdout/stderr readers are tokio tasks spawned via `tauri::async_runtime::spawn`.
- `tokio::process::Child::try_wait` is sync and works from sync contexts — used for the liveness check in `ensure_sidecar`.
- No research into `ChildStdin::into_std()` was needed; the mpsc-bridged writer task pattern worked first try.

### spawn_agent exclusion

`agent::spawn_agent` is **not** in the specta command collection because it has 13 parameters and specta's `SpectaFn` trait caps at 10. Runtime dispatch for it goes through `tauri::generate_handler!` in `run()` alongside everything else — it is callable at runtime, just not typed in `bindings.ts`.

**Phase 2 action:** refactor `spawn_agent`'s signature before wiring up typed callers. Collapse the three shadow fields (`shadow_name`, `shadow_title`, `shadow_grade`) into a `ShadowIdentity` struct, and the three model fields (`provider`, `model`, `thinking_level`) into a `ModelConfig` struct. That drops the arg count to 9 and fits under specta's limit. This is a breaking change to the frontend `invoke("spawn_agent", ...)` call site, so it should land as part of the Phase 2 frontend cutover.

### Bindings generation

- **Trigger:** `cargo run -- --export-bindings` from `src-tauri/`. The main binary checks for the flag before starting Tauri and writes `src/lib/bindings.ts`, then exits.
- **Why not a cargo test:** test binaries on Windows fail to start with `STATUS_ENTRYPOINT_NOT_FOUND` because of Tauri runtime DLL resolution issues. The main `monarch.exe` has the right DLL neighbours (WebView2Loader etc.) so running it with a pre-startup flag is the most reliable path.
- **CI wiring:** **not done in Phase 1**. Document TODO: add a CI step that runs `cargo run -- --export-bindings` and then `git diff --exit-code src/lib/bindings.ts`. If Phase 2 touches `.github/workflows/` anyway, add it there.
- **Post-processing workaround:** `export_bindings()` reads the generated file and injects two type aliases (`type Value = unknown; type Vec<T> = T[];`) to work around specta rc.24 emitting raw Rust type names for `serde_json::Value` fields (`detect_project` return, `respond_extension_ui` param, `DisplayItem::Assistant.content`). Remove this workaround once specta fixes the `serde_json` feature's TS emission. Phase 2 can also consider narrowing the `detect_project` / `respond_extension_ui` signatures to proper typed structs.

### TODO(MON-27) markers

All DB writes inside the async reader path are wrapped in `tauri::async_runtime::spawn_blocking`. Search `src-tauri/src/agent.rs` for `TODO(MON-27)` — there's one marker in `handle_sidecar_event` above the `spawn_blocking` call for `persist_event`. The follow-up issue removes these wrappers after `db.rs` migrates to `tokio-rusqlite`.

### Things left in `agent_state.rs` that Phase 2 will use

- `mark_desynced(&mut self)` — has `#[allow(dead_code)]` in Phase 1 because nothing calls it yet. Phase 2's desync indicator will call it from the reader task whenever a parse failure or unexpected state occurs.
- `LiveAgentState::new()` — unused convenience constructor, also `#[allow(dead_code)]`. Phase 2 can remove it if still unused.
- `display_items_from_messages()` — used by `recover_sidecar` to rebuild items on crash. Phase 2 should also use it if the frontend ever needs a client-side rebuild, but the preferred path is calling `get_agent_state` which already includes rebuilt items.

### Dev-flag name

`VITE_MONARCH_DEBUG_DESYNC` was chosen (no existing `VITE_` flag convention in the repo). Phase 2 adds it to `vite.config.ts` / dev env config and gates the UI indicator on `import.meta.env.VITE_MONARCH_DEBUG_DESYNC === "true"`.
