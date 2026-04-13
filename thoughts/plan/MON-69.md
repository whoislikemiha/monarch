# MON-69 — Extract project/cwd into first-class module (out of agent.rs)

## Summary

The project-detection logic (`find_project_root`, `read_instructions_from_root`, `resolve_project`, `detect_project`, `read_project_instructions`) already lives in `src-tauri/src/project.rs` — MON-33 moved the free functions out. What still pollutes the `agent` namespace are the two **Tauri command wrappers** (`detect_project`, `read_project_instructions`), which sit in `src-tauri/src/agent/commands.rs` and merely delegate one-liners into `crate::project`. This ticket relocates those command wrappers into the `project` module so that the `agent` module contains only agent-loop concerns, and so the project layer has a proper home (with room for worktree/project-registry work downstream).

This is a pure code-move refactor. No behavior change, no new features.

## Relevant files and areas

- `src-tauri/src/project.rs` — current home of the free functions (`find_project_root`, `read_instructions_from_root`, `resolve_project`, `detect_project`, `read_project_instructions`). The command wrappers will join them here (or inside a `project/commands.rs` submodule — see open question).
- `src-tauri/src/agent/commands.rs:19–32` — the two `#[tauri::command]` wrappers to move out. Nothing else in this file is project-related; the rest is agent-loop request DTOs and commands.
- `src-tauri/src/lib.rs:89–90, 220–221` — two registration sites for the commands: the specta `collect_commands!` list and the `tauri::generate_handler!` list. Both currently reference `agent::commands::detect_project` and `agent::commands::read_project_instructions`. Both need updating to the new module path.
- `src-tauri/src/ws.rs:245–253` — WebSocket dispatch. Already calls `crate::project::detect_project` / `crate::project::read_project_instructions` directly (the free functions, not the command wrappers), so no change needed here — but we must preserve those free-function signatures when reshaping the module.
- `src/lib/bindings.ts` — auto-generated; will regenerate identically (command names are unchanged, types are unchanged). No frontend call-site changes.

## What needs to change

1. **Move the two command wrappers out of `agent/commands.rs`** into the `project` module. Two concrete layouts are viable:
   - **Option A (minimal):** append the two `#[tauri::command]` wrappers to the existing `project.rs`, next to the functions they wrap.
   - **Option B (scalable):** promote `project.rs` → `project/mod.rs` and add `project/commands.rs` that holds the Tauri wrappers, mirroring the `agent/{mod.rs, commands.rs, …}` shape.
2. **Update `lib.rs`** — change the two paths in both the specta `collect_commands!` list and the `tauri::generate_handler!` list from `agent::commands::…` to `project::…` (or `project::commands::…` under Option B).
3. **Remove the now-empty wrappers from `agent/commands.rs`** along with any imports that become unused (notably `Arc`, `tauri::State`, `Database`). Leave the agent-related DTOs and commands untouched.
4. **Regenerate bindings** via `cargo run -- --export-bindings` from `src-tauri/`; commit the result only if it diverges (it should be a no-op since command names and signatures are identical).
5. **Verify no leftover `agent::commands::detect_project` / `agent::commands::read_project_instructions` references** anywhere (grep the tree).

## Open questions

- **Module shape — Option A vs Option B?** The ticket hints at `project/` as a possible layout ("`src-tauri/src/project.rs` extended, or `src-tauri/src/project/`"). Option A is the smallest diff today; Option B pre-structures the directory for the worktree/registry work called out in the ticket's context. My recommendation: **Option B**, because the whole motivation for this ticket is "room to grow" — making the directory now saves a second refactor when the worktree work lands. But it's a judgment call on timing; if you'd rather defer until the first downstream ticket actually needs more files, Option A is defensible.

## Out of scope

- Any worktree logic, project registry, per-agent worktree allocation, or branch isolation — all downstream tickets under the "Git & Worktree Integration" project.
- Moving `chrono_now` / `uuid_v4_simple` — handled under MON-53 → `util.rs`.
- Any DB schema changes (e.g. expanding `projects` table) for project tracking.
- Any change to the free-function API (`resolve_project`, `find_project_root`, `read_instructions_from_root`) — those stay as-is; only the Tauri command wrappers relocate.
- Frontend changes — bindings are auto-regenerated, no call-site edits needed.
