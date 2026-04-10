# MON-10: WebSocket Bridge for Browser-Mode IPC

## What was implemented

A WebSocket server that runs alongside the Tauri app, enabling the Svelte frontend to work fully in a plain browser. A frontend wrapper (`$lib/api.ts`) auto-detects Tauri vs browser and routes all `invoke()`/`listen()` calls accordingly.

## Key decisions

- **Dedicated Tokio runtime on its own thread** — Tauri's `.setup()` runs outside a Tokio context, so the WS server spawns on `std::thread` with its own `tokio::runtime`.
- **No command refactoring** — Existing `#[tauri::command]` functions stay untouched. Thin `ws_*` wrappers call the same `_internal` methods. Adding a new command = one match arm in `ws.rs`.
- **Arc-wrapped managed state** — `AgentManager` and `ModelCache` wrapped in `Arc` before `.manage()` so both Tauri commands and WS server share the same instances.
- **Stored AppHandle** — WS-initiated agent commands need the sidecar, which needs `AppHandle` for the reader thread. Stored in `AgentManager` during `.setup()`.
- **Broadcast channel for events** — `handle_sidecar_event` pushes to both `app.emit()` (Tauri) and `broadcast::Sender` (WS) via `emit_event()`.

## Files touched

- `src-tauri/src/ws.rs` (new) — WS server, dispatch, event forwarding
- `src/lib/api.ts` (new) — Frontend invoke/listen wrapper
- `src-tauri/src/agent.rs` — Broadcast channel, ws_* wrappers, emit_event()
- `src-tauri/src/db.rs` — ws_* wrappers for all DB commands
- `src-tauri/src/models.rs` — ws_* wrappers
- `src-tauri/src/persistence.rs` — ws_* wrappers
- `src-tauri/src/lib.rs` — Arc state, WS server startup
- `src-tauri/Cargo.toml` — tokio-tungstenite, futures-util
- All 7 frontend Svelte files — import updates
- `tsconfig.json`, `vite.config.ts` — $lib path alias
- `skills/agent-browser/SKILL.md` (new) — agent-browser CLI skill

## What was left out

- Cross-instance sync (agent created in browser doesn't appear in Tauri desktop without restart)
- Authentication/security for WS endpoint (local dev only)
- Static file serving for production browser mode
