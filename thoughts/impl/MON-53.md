# MON-53 — Split `agent.rs` into focused modules

## What was implemented

`src-tauri/src/agent.rs` (1976 lines) was split into a `src-tauri/src/agent/` module with a thin facade and five focused submodules. Pure code-move refactor — no logic or behaviour changed. `chrono_now` / `uuid_v4_simple` were additionally promoted to a new crate-root `src-tauri/src/util.rs` so `db.rs` and `project.rs` no longer depend on the `agent` namespace for timestamp helpers.

Final layout (lines):

- `agent/mod.rs` — 47 (facade: submodule decls, re-exports, `WsBroadcast`, `DEBOUNCE_MILLIS`, `TaskHandle`)
- `agent/manager.rs` — 701 (`AgentManager` + state types + high-level lifecycle methods + round-trip test)
- `agent/sidecar.rs` — 417 (`SidecarProcess` + process-layer `impl AgentManager` block)
- `agent/event_handler.rs` — 358 (inbound dispatch, debounce, emission)
- `agent/persist.rs` — 332 (MON-37 persistence pipeline)
- `agent/commands.rs` — 216 (Tauri command wrappers + request DTOs)
- `util.rs` — 21 (`chrono_now`, `uuid_v4_simple`)

## Key decisions

- **Staged commits (seven).** Each commit compiles clean in isolation; easy to bisect.
- **Multiple `impl AgentManager` blocks across submodules.** `manager.rs` owns the primary impl; `sidecar.rs` attaches a second impl block with the process-layer methods (`ensure_sidecar`, `shutdown_sidecar`, `send_to_sidecar`, `send_with_recovery`, `recover_sidecar`). Several `AgentManager` fields became `pub(super)` so the cross-file impl can see them.
- **`commands` is `pub mod commands`, not a re-export.** `#[tauri::command]` emits a paired `__cmd__<name>` helper that must share the command fn's module. `pub use commands::spawn_agent` doesn't forward the helper, so `tauri::generate_handler![agent::spawn_agent]` fails to resolve. Solution: leave command fns at `agent::commands::X` and update `lib.rs`'s 11 handler registrations. DTOs (`SpawnAgentRequest`, `ExtensionUiResponseRequest`) and `AgentManager`, `WsBroadcast` are still re-exported at `crate::agent::*`, so `ws.rs`, `project.rs`, `db.rs` remained unchanged.
- **`util.rs` created instead of leaving `chrono_now` in `agent`.** Plan originally had this as "leave alone"; on review we chose to promote since `db.rs` and `project.rs` were both reaching into `agent::` for it — a reverse dependency that disappears cleanly with the promotion.
- **Follow-up MON-69 created** for moving `detect_project` / `read_project_instructions` into a real `project` module. Blocked by MON-53; groundwork for the worktree/project-first-class work.

## Files touched

Created:

- `src-tauri/src/util.rs`
- `src-tauri/src/agent/mod.rs` (facade — file previously was the monolith)
- `src-tauri/src/agent/manager.rs`
- `src-tauri/src/agent/sidecar.rs`
- `src-tauri/src/agent/event_handler.rs`
- `src-tauri/src/agent/persist.rs`
- `src-tauri/src/agent/commands.rs`

Modified:

- `src-tauri/src/lib.rs` — added `mod util;`, rewrote agent command-handler registrations from `agent::X` to `agent::commands::X`.
- `src-tauri/src/db.rs` — `crate::agent::chrono_now` → `crate::util::chrono_now`.
- `src-tauri/src/project.rs` — import `chrono_now` / `uuid_v4_simple` from `crate::util`.

Deleted:

- `src-tauri/src/agent.rs` (became `agent/mod.rs`).

## What was left out

- `detect_project` / `read_project_instructions` stay in `agent/commands.rs` — moving them is MON-69.
- No new tests (MON-54 owns that scope).
- No changes to lock hierarchy, debounce window, channel capacity, protocol types, or any runtime behaviour.
- Pre-existing `clippy::match_like_matches_macro` warning in `sidecar_protocol.rs:742` is unrelated and untouched.
