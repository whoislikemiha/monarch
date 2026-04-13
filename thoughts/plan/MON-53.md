# MON-53 — Split `agent.rs` into focused modules

## Summary

`src-tauri/src/agent.rs` is a ~1976-line monolith that owns five unrelated concerns behind one file: the sidecar child process (spawn / stdin-stdout / shutdown / crash recovery), the `AgentManager` with its live-state maps and high-level methods, the sidecar event dispatcher (including debounce and desync handling), the single-consumer persistence pipeline (`PersistCommand` + SQL dispatch), and all the `#[tauri::command]` wrappers plus request DTOs. These concerns already have clean seams in the code today — the goal is to promote those seams into separate submodules under `src-tauri/src/agent/` so future work on the Agent loop project (orchestration, memory, cost tracking) lands in focused files instead of wedging into a single 2k-line module. No behavior, wire format, or external API surface changes.

## Relevant files and areas

### The file being split

- `src-tauri/src/agent.rs` (1976 lines). Natural sections in current order:
  - **Imports + constants** (lines 1–32) — `DEBOUNCE_MILLIS = 16`, tokio + dashmap + parking_lot imports.
  - **`WsBroadcast` struct** (34–39).
  - **`AgentState` / `AgentManagerInner`** (41–66) — tracking structs for live agents and session mapping.
  - **`SidecarProcess` struct, `write_command`, `Drop`** (68–149) — the async stdin-owning process wrapper.
  - **`AgentStateEntry` / `AgentStateInner`** (151–203) — per-agent live-state container with debounce handle and cancel generation.
  - **`AgentManager` struct** (205–228) — the top-level manager with field-level lock-hierarchy doc.
  - **`AgentManager::new` / `set_app_handle` / `shutdown_sidecar`** (230–333).
  - **`ensure_sidecar`** (335–435) — spawns Node sidecar, wires stdout/stderr reader tasks, stores `SidecarProcess`.
  - **`live_entry` / `remove_live_entry`** (437–463) — live-state map helpers.
  - **`send_to_sidecar` / `recover_sidecar` / `send_with_recovery`** (465–599) — sidecar write path + crash recovery replay.
  - **`rebuild_state_from_session`** (609–662) — rebuild `LiveAgentState` from SQLite ancestry.
  - **`spawn` / `send_command` / `kill` / `load_session_context` / `new_session` / `switch_session` / `respond_extension_ui`** (664–1003) — high-level manager methods.
  - **`resolve_sidecar_path` / `get_session_id`** (1005–1028) — free helpers.
  - **`emit_event` / `emit_state_event`** (1030–1065) — dual-channel event emit helpers with the MON-38 invariant doc.
  - **`handle_sidecar_event`** (1067–1226) — top-level dispatcher over `SidecarEvent` variants.
  - **`try_consume_debounce_snapshot` / `apply_and_maybe_emit` / `mark_agent_desynced`** (1228–1367) — debounce body, per-event apply+emit, desync flag flip.
  - **`PersistCommand` enum + `apply`** (1369–1472) — typed persistence command variants and their SQL dispatch.
  - **`build_persist_commands` / `inner_event_tag`** (1474–1631) — translate typed `InnerEvent` → zero-to-N `PersistCommand`s, plus snake_case tagging.
  - **`run_persist_consumer`** (1633–1669) — single-consumer mpsc drain loop.
  - **`chrono_now` / `uuid_v4_simple`** (1671–1684) — RFC3339 + uuid helpers, also used by `project.rs` and `db.rs`.
  - **Tauri command wrappers + request DTOs** (1685–1972) — `detect_project`, `read_project_instructions`, `ShadowSpec`, `SpawnAgentRequest`, `ExtensionUiResponseRequest`, `spawn_agent`, `send_command`, `kill_agent`, `get_agent_state`, `rebuild_agent_state_from_session`, `load_session_context`, `new_agent_session`, `switch_agent_session`, `respond_extension_ui`.
  - **Tests** (1716–1797) — `#[cfg(test)]` block with `kill_agent_round_trip_funnels_through_shared_method`.

### Consumers that must keep compiling unchanged

- `src-tauri/src/lib.rs` — imports `agent::AgentManager` and registers every `agent::<cmd>` handler in `tauri::generate_handler!` / `specta_builder` (lines 15, 46–54, 88–89, 183–220). Any symbol it references must remain `crate::agent::<name>`.
- `src-tauri/src/ws.rs` — `use crate::agent::{AgentManager, WsBroadcast}` (line 11) plus `crate::agent::SpawnAgentRequest` (185) and `crate::agent::ExtensionUiResponseRequest` (237).
- `src-tauri/src/project.rs` — `use crate::agent::{chrono_now, uuid_v4_simple}` (line 9).
- `src-tauri/src/db.rs` — calls `crate::agent::chrono_now()` inline (line 636).

### Adjacent context (read, don't touch)

- `src-tauri/src/agent_state.rs` — `LiveAgentState`, `ApplyOutcome`, `DisplayItem`, `display_items_from_messages`. Out of scope.
- `src-tauri/src/sidecar_protocol.rs` — `SidecarCommand`, `SidecarEvent`, `InnerEvent`, `apply_event`, `LoadSessionMessage`, `ShadowConfig`. Out of scope.
- `src-tauri/src/db.rs` — `AgentRow`, `MessageRow`, `Database`. The persistence submodule will keep calling `db.*_internal` methods exactly as today.

## What needs to change

The output is a new `src-tauri/src/agent/` directory with a `mod.rs` root and focused submodules. Nothing outside the `agent` module should change other than re-export resolution.

Proposed module layout (all inside `src-tauri/src/agent/`):

1. **`mod.rs`** — the new entrypoint. Declares the submodules, re-exports every symbol currently imported as `crate::agent::<name>` from elsewhere in the crate (see the consumer list above). Also carries the `DEBOUNCE_MILLIS` constant and the `TaskHandle` type alias if they end up needed by multiple submodules. Goal: keep `mod.rs` a thin facade — under ~100 lines.

2. **`sidecar.rs`** — owns the child-process abstraction end-to-end:
   - `SidecarProcess` struct + `write_command` + `Drop` impl.
   - `resolve_sidecar_path`.
   - The `ensure_sidecar`, `shutdown_sidecar`, `send_to_sidecar`, `send_with_recovery`, and `recover_sidecar` methods. Because these are currently `impl AgentManager` methods that read `self.sidecar`, `self.inner`, `self.live_states`, `self.ws_broadcast`, `self.persist_tx`, the cleanest shape is to *leave the methods on `AgentManager`* but move them into a separate `impl` block in this file via `impl crate::agent::manager::AgentManager { ... }`. Rust allows multiple `impl` blocks across submodules of the same crate module tree.
   - `WsBroadcast` may also live here since it represents "what the sidecar layer broadcasts out".
   - The two reader-task closures (stdout/stderr) stay here and call into the event-handler submodule via a `pub(super)` entry point.

3. **`event_handler.rs`** — owns the inbound event dispatch:
   - `handle_sidecar_event` (the top-level `match typed_event { .. }`).
   - `apply_and_maybe_emit` and `try_consume_debounce_snapshot` (the debounce mechanics).
   - `mark_agent_desynced`.
   - `emit_event` and `emit_state_event` (including the MON-38 invariant doc comment — it's load-bearing context for anyone touching the emit path).
   - `get_session_id` (the one-liner that reads `AgentManagerInner.session_map`).
   - Pure free functions — no `impl AgentManager` here. `AgentManagerInner` and `AgentStateEntry` are passed in as `&Arc<PlMutex<_>>` / `&Arc<DashMap<_,_>>` the same way they are today, so this submodule only needs to import the two types and not couple to `AgentManager` at all.

4. **`persist.rs`** — owns the persistence pipeline:
   - `PersistCommand` enum + `agent_id()` + `apply()` impl.
   - `build_persist_commands` + `inner_event_tag`.
   - `run_persist_consumer`.
   - Re-exports `PersistCommand` at `pub(super)` visibility so `manager.rs` and `event_handler.rs` can send into the channel.

5. **`manager.rs`** — owns the `AgentManager` value type and its high-level methods:
   - `AgentState`, `AgentManagerInner`, `AgentStateEntry`, `AgentStateInner`.
   - `AgentManager` struct definition + `new` + `set_app_handle` + `get_app_handle` + `live_entry` + `remove_live_entry`.
   - High-level command methods: `spawn`, `send_command`, `kill`, `new_session`, `switch_session`, `load_session_context`, `respond_extension_ui`, `rebuild_state_from_session`.
   - Keeps `AgentManager`'s public fields/methods accessible so `sidecar.rs` can attach more `impl` blocks.

6. **`commands.rs`** — owns the Tauri command layer:
   - `ShadowSpec`, `SpawnAgentRequest`, `ExtensionUiResponseRequest` request DTOs.
   - All `#[tauri::command]` wrappers: `spawn_agent`, `send_command`, `kill_agent`, `get_agent_state`, `rebuild_agent_state_from_session`, `load_session_context`, `new_agent_session`, `switch_agent_session`, `respond_extension_ui`, `detect_project`, `read_project_instructions`.
   - The bindings.ts auto-generator picks these up through `lib.rs`'s `specta_builder.commands(…)` list — no change needed there, only import paths.

7. **`util.rs`** *(optional)* — `chrono_now` and `uuid_v4_simple`. Small enough that they could also live in `mod.rs`; the decision point is whether `project.rs` and `db.rs` keep importing them via `crate::agent::{chrono_now, uuid_v4_simple}` (yes — preserved through `mod.rs` re-export).

8. **Tests** — the existing `#[cfg(test)]` `kill_agent_round_trip_funnels_through_shared_method` goes with the method it exercises (`manager.rs`, inside a `#[cfg(test)] mod tests { ... }` block).

### Execution sequence

The refactor is best done as a single commit (or a tight series of commits that each compile) because splitting a file and re-exporting from `mod.rs` is atomic — partial splits leave the tree in a non-compiling state. Suggested approach:

1. Create `src-tauri/src/agent/` directory and move `agent.rs` → `agent/mod.rs` as-is. Verify `cargo check` still passes.
2. Move the five concern groups into submodules one at a time, each followed by `cargo check`:
   1. `persist.rs` (most self-contained; only depends on `db`, `InnerEvent`, `chrono_now`).
   2. `event_handler.rs` (depends on `persist` via channel sender, plus `AgentStateEntry` / `AgentManagerInner` from what will become `manager.rs`).
   3. `sidecar.rs` (depends on `event_handler` for the stdout reader closure, on `manager` for `AgentManager` fields).
   4. `commands.rs` (depends on `manager` for `AgentManager` methods).
   5. `manager.rs` (what remains after the others are extracted).
3. Trim `mod.rs` to a facade: submodule declarations and `pub use` re-exports only.
4. Regenerate `bindings.ts` (`cargo run -- --export-bindings` from `src-tauri/`) and confirm byte-for-byte identical to pre-refactor — any change means a `#[specta::specta]` decorator or request DTO drifted.
5. Run `cargo clippy` and `svelte-check`, then smoke-test with `npm run build:sidecar && npm run tauri dev`.

### What deliberately stays exactly the same

- Every `#[tauri::command]` function signature and its `#[specta::specta]` attribute.
- All `pub use` / `pub(crate)` visibility at the `crate::agent::*` level.
- The lock hierarchy, channel capacities (256), debounce window (16ms), sidecar resolution order, and the crash-recovery replay sequence.
- Field names of `AgentManager`, `AgentState`, `AgentStateEntry`, `AgentStateInner` — internal but touched by enough code that a rename is a distraction here.
- Doc comments, especially the MON-14 / MON-27 / MON-30 / MON-32 / MON-34 / MON-37 / MON-38 notes — they're load-bearing context, not trivia. Moving them with their referents is part of the refactor.

## Open questions

1. **`detect_project` / `read_project_instructions` placement.** They live in `agent.rs` today but are filesystem-scope project metadata, not agent-loop concerns. The acceptance criteria keep them in the `agent` module (re-exported from `commands.rs`). Follow-up ticket to move them into `project.rs` proper, or leave alone?
2. **`WsBroadcast` placement.** It's used by every submodule (sidecar broadcasts, event-handler emits, ws.rs consumes it). Best home: `mod.rs` or `sidecar.rs`? My inclination is `mod.rs` so it's available to all submodules without an explicit `sidecar::` prefix.
3. **`chrono_now` scope.** `db.rs` and `project.rs` both import it from `agent`. Should this be moved to a neutral `util.rs` at the crate root as a small cleanup, or is that scope creep for this ticket? (My vote: scope creep — leave it in `agent` for MON-53, move it in a follow-up.)
4. **`impl AgentManager` split across files.** Rust allows multiple `impl` blocks in different files within a module tree — this is the cleanest way to keep `sidecar.rs` methods as methods on `AgentManager` without the indirection of free functions. Confirm this is acceptable stylistically, or prefer free functions taking `&AgentManager`?
5. **Commit granularity.** One atomic "move everything" commit (reviewable as a big diff but easier to read as "this block went there"), or five smaller commits (each moves one concern, each compiles, full bisectability)? My preference: staged commits, one per submodule, so review can be done concern-by-concern.

## Out of scope

- Any behavior change, logic change, or optimization. Pure code-move only.
- Adding new tests (MON-54 covers this).
- Refactoring `agent_state.rs` or `sidecar_protocol.rs`.
- Moving `detect_project` / `read_project_instructions` / `chrono_now` / `uuid_v4_simple` out of the `agent` module namespace.
- Touching the lock hierarchy, debounce window, channel capacity, or any protocol types.
- Renaming fields on `AgentManager` / `AgentState` / related structs.
- Any changes to `lib.rs`, `ws.rs`, `db.rs`, `project.rs` beyond what the compiler requires (should be zero).
