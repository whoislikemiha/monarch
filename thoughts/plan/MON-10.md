# MON-10: Add WebSocket bridge for browser-mode IPC

## Summary

The Monarch frontend communicates with the Rust backend exclusively through Tauri's `invoke()` IPC, which only exists inside Tauri's webview. When the Svelte app loads in a plain browser via Vite's dev server (`localhost:1420`), all 24+ invoke call sites fail silently — the UI renders but nothing works. The goal is to add a WebSocket server to the Rust backend and a thin frontend wrapper so the same app works fully in any browser. This unblocks interactive UI testing with `agent-browser` and is a stepping stone toward mobile/remote access.

## Relevant files and areas

### Rust backend

- **`src-tauri/src/lib.rs`** (lines 11-67): App builder — manages state (`AgentManager`, `ModelCache`, `Arc<Database>`) and registers all commands via `tauri::generate_handler![]`. The WebSocket server needs to start here alongside the Tauri app and have access to the same managed state.

- **`src-tauri/src/agent.rs`**: Agent lifecycle commands (`spawn_agent`, `kill_agent`, `send_command`, `broadcast_prompt`, `new_agent_session`, `switch_agent_session`, `load_session_context`, `respond_extension_ui`, `detect_project`, `read_project_instructions`). Also contains `handle_sidecar_event()` (line ~260) which is the critical event emission path — it calls `app.emit()` to push events to the frontend. This function needs a parallel WebSocket broadcast path.

- **`src-tauri/src/db.rs`**: All database commands (~15 commands: `db_upsert_agent`, `db_get_agents`, `db_create_session`, `db_get_sessions`, `db_get_messages`, `db_get_messages_with_ancestry`, `db_save_agent_template`, etc.). These are pure data access — straightforward to expose over WebSocket.

- **`src-tauri/src/models.rs`**: `get_models` (async, uses `ModelCache` state) and `get_provider_auth_status`. Two commands.

- **`src-tauri/src/persistence.rs`**: `get_agent_prompt`, `save_agent_prompt`, `get_prompts_dir`. Three filesystem commands, no state dependencies.

- **`src-tauri/Cargo.toml`**: Current deps include `tokio` (rt-multi-thread, macros) and `serde_json`. No WebSocket library yet — needs `tokio-tungstenite` added.

### Frontend

- **`src/App.svelte`** (lines 3-4, 186-502): Imports `invoke` from `@tauri-apps/api/core` and `listen` from `@tauri-apps/api/event`. ~9 invoke calls + 1 listen registration for agent-exit events.

- **`src/lib/AgentView.svelte`** (lines 3-4, 323-956): Heaviest IPC consumer. ~6 invoke calls + 3 `listen()` subscriptions (`agent-event-{id}`, `agent-exit-{id}`, `agent-stderr-{id}`). The event listeners are the most complex part — they drive the real-time streaming UI.

- **`src/lib/SpawnDialog.svelte`** (line 3, 200-210): 2 invoke calls (templates). Already has `__TAURI_INTERNALS__` detection with browser fallbacks for model fetching.

- **`src/lib/ProjectEditor.svelte`** (line 2, 49-85): 3 invoke calls (project rename, instructions, send_command).

- **`src/lib/PromptEditor.svelte`** (line 3, 97-125): 4 invoke calls (prompt save/load + send_command).

- **`src/lib/CouncilView.svelte`** (lines 3-4, 197-234): 1 invoke + multiple listen registrations per agent.

- **`src/lib/HistoryPanel.svelte`** (line 2): invoke import, session/message loading.

## What needs to change

### 1. New Rust module: WebSocket server (`src-tauri/src/ws.rs`)

A new module that starts a `tokio-tungstenite` WebSocket server on a configurable port (default 3001). It needs access to the same three state objects (`AgentManager`, `ModelCache`, `Arc<Database>`) and the Tauri `AppHandle` (for event emission compatibility).

The server accepts JSON-RPC style messages: `{ "id": 1, "cmd": "spawn_agent", "args": {...} }` and returns `{ "id": 1, "result": ... }` or `{ "id": 1, "error": "..." }`.

For events, the server pushes unsolicited messages: `{ "event": "agent-event-abc123", "payload": "..." }` to subscribed clients. Clients subscribe by sending `{ "cmd": "listen", "event": "agent-event-{id}" }`.

### 2. Command dispatch layer

Currently each `#[tauri::command]` function takes `tauri::State<'_, T>` extractors that only work within Tauri's invoke handler. The core logic needs to be factored into plain functions that accept the state types directly (not via Tauri extractors). Both the Tauri command handlers and the WebSocket dispatcher call these shared functions.

This is the bulk of the refactor — touching all ~24 command functions to split "extract state → call logic" from "the logic itself."

### 3. Event broadcast to WebSocket clients

`handle_sidecar_event()` in `agent.rs` currently calls `app.emit()`. It needs to also broadcast to connected WebSocket clients. The simplest approach: maintain a `tokio::sync::broadcast` channel. The sidecar event handler pushes events into the channel; each WebSocket connection subscribes to the channel and forwards matching events to its client.

### 4. Frontend invoke wrapper (`src/lib/api.ts`)

A new module that exports `invoke()` and `listen()` functions matching Tauri's API signatures. Internally:
- If `__TAURI_INTERNALS__` exists: delegate to `@tauri-apps/api/core` invoke and `@tauri-apps/api/event` listen.
- Otherwise: use a singleton WebSocket connection to `ws://localhost:3001`, send JSON-RPC requests, and register event callbacks.

### 5. Update all frontend import sites

Replace `import { invoke } from "@tauri-apps/api/core"` with `import { invoke } from "$lib/api"` across all 7 files. Same for `listen` imports (3 files). This is mechanical find-and-replace.

### 6. App startup wiring (`lib.rs`)

Start the WebSocket server in a `tokio::spawn` during Tauri app setup (inside `.setup()` callback), passing clones of the managed state. The server runs alongside the Tauri app for the lifetime of the process.

## Open questions

1. **Port configuration**: Hardcode 3001, use an env var, or auto-detect a free port and inject it into the Vite dev server somehow? Hardcoded is simplest but could conflict.

2. **Multiple WebSocket clients**: Should we support multiple browser tabs connecting simultaneously, or is single-client sufficient? Multiple is more robust but adds complexity around event routing.

3. **AppHandle in WebSocket context**: Some commands use `AppHandle` to emit events. In the WebSocket path, we don't have a real `AppHandle` — we'd use the broadcast channel instead. Need to verify all `AppHandle` usage is limited to event emission (it appears to be).

4. **Startup order**: The WebSocket server needs state that's created during Tauri's `.manage()` phase. Using `.setup()` callback should work, but need to verify we can clone/access managed state there.

5. **Should this also serve the frontend?**: Currently Vite serves the frontend in dev. In production browser mode, something needs to serve the HTML/JS. Could add static file serving to the WebSocket server, or keep it as a separate concern.

## Out of scope

- Authentication or security for the WebSocket endpoint (local dev only for now)
- HTTP REST API (WebSocket covers both request/response and streaming)
- Changes to the Node sidecar protocol
- Mobile-specific UI adaptations
- Deploying as a hosted web service
- `agent-browser` installation/configuration (separate task)
