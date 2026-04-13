# MON-69 — Implementation notes

## What was implemented

Moved the `detect_project` and `read_project_instructions` Tauri command wrappers out of the `agent` module and into a dedicated `project::commands` submodule. The underlying free functions already lived in `project.rs` (from MON-33); only the thin `#[tauri::command]` adapters were still dangling in `agent/commands.rs` for historical reasons.

Pure code move. No behavior change. Auto-generated `bindings.ts` regenerated with zero diff, confirming the refactor preserves the public IPC surface exactly.

## Key decisions

- **Option B (directory) over Option A (append to single file).** Promoted `project.rs` → `project/mod.rs` + `project/commands.rs`, mirroring the `agent/{mod.rs, commands.rs, …}` shape. Motivation: the ticket's stated purpose is "room to grow" for the upcoming Git & Worktree Integration work; pre-structuring the directory now avoids a second refactor when worktree lifecycle / project-registry files land.
- **Left `ws.rs` untouched.** It was already calling `crate::project::detect_project` / `read_project_instructions` (the free functions, not the command wrappers), so the WebSocket dispatch path needs no change.

## Files touched

- `src-tauri/src/project.rs` → `src-tauri/src/project/mod.rs` (git-renamed, added `pub mod commands;` + module note)
- `src-tauri/src/project/commands.rs` — new, holds the two Tauri wrappers
- `src-tauri/src/agent/commands.rs` — removed the two wrappers
- `src-tauri/src/lib.rs` — repointed specta `collect_commands!` + `tauri::generate_handler!` entries

## What was left out

- No worktree, project-registry, or per-agent worktree work — all explicitly out of scope per the ticket; belongs to downstream tickets under the Git & Worktree Integration project.
