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

<!-- HANDOFF: filled in at end of Phase 1 -->
