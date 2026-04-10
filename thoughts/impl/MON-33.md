# MON-33 — Collapse ws_* duplication behind a shared service layer

## What was implemented

Every backend operation with a `#[tauri::command]` + `ws_*` twin now
delegates to a single shared implementation. The Tauri command bodies
and the `ws::dispatch_command` match arms are one-line adapters; the
~500 lines of verbatim mirrored logic that used to sit in
`agent.rs`, `db.rs`, and `persistence.rs` are gone.

The refactor covers three subsystems:

1. **Agent lifecycle (`agent.rs`)** — `spawn`, `send_command`, `kill`,
   `load_session_context`, `new_session`, `switch_session`,
   `respond_extension_ui` all live on `impl AgentManager`. Each method
   takes `&AppHandle`, `&Arc<Database>`, and a typed argument list.
   `ensure_sidecar` is called inside the shared methods, not the
   adapters, so neither transport can forget it. The entire `ws_*` free
   function block at the bottom of `agent.rs` is deleted.
2. **Database (`db.rs`)** — every `db_*` Tauri command that was still
   inlining SQL now delegates to an `_internal` method on
   `impl Database`. The `ws_*` free function block is deleted entirely;
   WS dispatch arms call the `_internal` methods directly.
3. **Persistence (`persistence.rs`)** — the byte-identical
   `ws_save_agent_prompt` / `ws_get_prompts_dir` wrappers are gone. Both
   transports call `write_agent_prompt_file` and `prompts_dir_string`
   helpers.

Project detection (`find_project_root`, `read_instructions_from_root`,
`resolve_project`, plus `detect_project` and `read_project_instructions`)
moved to a new `src-tauri/src/project.rs` module so the spawn path and
the two standalone commands share one implementation.

`respond_extension_ui` was folded in as part of the scope expansion: it
now takes a typed `ExtensionUiResponseRequest` struct collapsing the
three scattered `agent_id` / `request_id` / `value` args. The inner
`value` stays `serde_json::Value` because the extension UI contract is
intentionally open-ended.

## Key decisions

- **Explicit `&AppHandle` parameter** on every shared method. The WS
  adapter always calls `state.agent_mgr.get_app_handle()?` and passes
  it through. Uniform across all lifecycle methods — no mix of
  explicit-vs-implicit handle acquisition.
- **`_internal` suffix convention preserved.** Every newly-extracted
  `impl Database` method uses it, matching the existing naming on
  `upsert_agent_internal` / `create_session_internal` / etc. Kept the
  suffix to minimise diff churn; naming cleanup was out of scope.
- **`ensure_sidecar` moved inside shared methods** rather than staying
  in the adapters. Uniform guarantee; future new transport adapters
  inherit it automatically.
- **`respond_extension_ui` typed via a struct, not a newtype around
  `Value`.** Folding the three args into a struct gives bindings.ts a
  nameable type (`ExtensionUiResponseRequest`). Specta's emission of
  the inner `value: serde_json::Value` field is the same inline tagged
  union it already produced for the pre-refactor command-arg case, so
  there is no wire-shape regression; it's cosmetically unchanged.
- **`chrono_now` / `uuid_v4_simple` bumped to `pub(crate)`** so the new
  `project.rs` module can reuse them without a duplicate body. No new
  util module — the existing definitions in `agent.rs` stayed put.
- **`dispatch_command` bumped from private to `pub(crate)`** so the
  round-trip test in `agent::tests` can drive the full WS adapter path
  without going through the websocket transport.

## Drift bugs closed

- `db::ws_get_agents` was hardcoding `context_window: None` while
  `db::db_get_agents` was selecting the real column. With a single
  implementation (`Database::get_agents_internal` selects the column
  for both), the drift cannot recur. Latent bug — invisible from the
  frontend today because no WS consumer polls the agent list, but it
  was waiting for a caller.

## Files touched

- `src-tauri/src/agent.rs` — lifecycle methods added to
  `impl AgentManager`; Tauri command bodies collapsed to one-liners;
  `ws_*` free functions deleted; `ExtensionUiResponseRequest` added;
  `chrono_now` / `uuid_v4_simple` bumped to `pub(crate)`; project
  helper functions removed (moved to `project.rs`); round-trip test
  added under `#[cfg(test)] mod tests`.
- `src-tauri/src/db.rs` — 14 new `_internal` methods on `impl Database`;
  every `db_*` Tauri command body shrunk to one-line delegation; the
  entire `ws_*` block deleted; `new_in_memory` constructor under
  `#[cfg(test)]`.
- `src-tauri/src/project.rs` — **new file**. Houses all project
  detection logic, shared between `AgentManager::spawn` and the
  `detect_project` / `read_project_instructions` Tauri commands plus
  their WS dispatch arms.
- `src-tauri/src/persistence.rs` — `ws_save_agent_prompt` /
  `ws_get_prompts_dir` deleted; `write_agent_prompt_file` and
  `prompts_dir_string` helpers added; Tauri commands shrunk to
  delegations.
- `src-tauri/src/ws.rs` — every dispatch arm updated to call the
  shared method directly (via `state.agent_mgr.<method>` or
  `state.db.<method_internal>`). `dispatch_command` visibility bumped
  to `pub(crate)` for test access.
- `src-tauri/src/lib.rs` — `mod project;` added.
- `src/lib/bindings.ts` — regenerated. One changed signature
  (`respondExtensionUi` now takes `req: ExtensionUiResponseRequest`)
  and one added struct; every other command is byte-identical.
- `src/lib/AgentView.svelte` — both `respond_extension_ui` call sites
  now go through `commands.respondExtensionUi(...)` instead of raw
  `invoke("respond_extension_ui", ...)`. `as any` cast on the request
  object because specta's Value type signature is inline-tagged and
  doesn't match the free-form runtime shape.

## What was left out

- **Reshaping the other flat `String` args into typed structs.**
  `send_command`, `new_agent_session`, `switch_agent_session`, and
  `load_session_context` still take flat args on both transports. This
  was explicit out-of-scope per the plan; MON-33 is dedup, not arg
  redesign.
- **Removing the `Value = unknown; Vec<T> = T[]` textual patch in
  `lib.rs::export_bindings`.** Still parked under MON-14 Wave 2;
  `LiveAgentState`, `StreamingMessage`, and `ToolExecution` all still
  reference `serde_json::Value` fields, so the patch remains
  load-bearing.
- **Test harness fix for Windows.** The new `kill_agent_round_trip`
  test compiles (`cargo check --tests` passes) but cannot be run on
  this Windows dev machine — the same `STATUS_ENTRYPOINT_NOT_FOUND`
  Tauri DLL quirk documented in `lib.rs::export_bindings` blocks the
  test binary. Running the test needs a CI workflow on Linux or a
  sub-crate split that puts lock-free helpers in something that
  doesn't link Tauri. Both tracked under the existing
  "No working Rust test harness" parking-lot item in MON-14-cleanup.md.
- **`send_to_sidecar` inside `AgentManager::kill`.** `kill` still
  best-efforts the sidecar destroy command and swallows the error —
  unchanged from the pre-refactor `kill_agent` / `ws_kill_agent`
  behaviour. Deliberate: local state cleanup must run even if the
  sidecar is already dead.
