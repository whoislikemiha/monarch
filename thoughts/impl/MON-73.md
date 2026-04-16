# MON-73: Agent Avatar Presets + Edit Agent View

## What was implemented

Full agent editing flow: a dedicated `EditAgentDialog` accessible via right-click context menu on any agent in the sidebar. Covers all configurable fields — name, shadow identity (shadow name/title/grade), provider, model, thinking level, CWD, and avatar.

Avatar system extended to support both animated Rive files and static images. Two built-in presets ship out of the box: the existing animated Shadow (`.riv`) and a new static shadow silhouette (`.svg`). Users can also upload any image from disk.

## Key decisions

**base64 data URL over Tauri asset protocol** — `asset://` URLs require explicit capability scope config in `capabilities/default.json` that wasn't present and would need per-path allowlisting. Simpler to add a `read_avatar_data_url` Rust command that reads the file and returns `data:image/...;base64,...`. One command, works everywhere including WS fallback mode.

**`uploadedPath`/`uploadedDataUrl` independent of current selection in AvatarPicker** — naively storing just `avatarType`/`avatarPath` meant clicking another preset overwrote the uploaded path, losing the custom image. Separating "what's uploaded" from "what's selected" lets the custom card persist visually while the user browses built-ins, with `selectCustom()` restoring it as the active selection.

**`db_update_agent` with `AgentUpdatePayload` struct** — follows the same pattern as `SpawnAgentRequest` (MON-35) to stay under Specta's 10-arg cap. All editable fields in one atomic update.

## Files touched

- `src-tauri/src/db.rs` — `AgentRow` + `AgentUpdatePayload`, migration, `db_update_agent` command
- `src-tauri/src/persistence.rs` — `save_avatar_image`, `read_avatar_data_url`, `avatars_dir()`
- `src-tauri/src/lib.rs` — command registration
- `src-tauri/src/ws.rs` — WS bridge dispatch for `db_update_agent`
- `src-tauri/src/agent/manager.rs` — `avatar_type`/`avatar_path` fields in two `AgentRow` initializers
- `src-tauri/Cargo.toml` — `base64 = "0.22"`
- `src/lib/types.ts` — `avatarType`/`avatarPath` on `Agent`
- `src/lib/stores/agentStore.svelte.ts` — mapping + `saveAgentEdits()`
- `src/lib/avatar/ShadowAvatar.svelte` — image/rive branching, filesystem image loading
- `src/lib/avatar/AvatarPicker.svelte` — new component
- `src/lib/EditAgentDialog.svelte` — new component
- `src/lib/Sidebar.svelte` — "Edit agent" context menu entry, avatar image sizing fix
- `src/lib/AgentHeader.svelte` — passes avatar props to ShadowAvatar
- `src/App.svelte` — wires EditAgentDialog
- `static/avatars/shadow_silhouette.svg` — new built-in static preset

## What was left out

- Multiple custom `.riv` file uploads (only built-in rive presets; user uploads are images only)
- Avatar cropping / resize UI
- Drag-to-reorder presets
