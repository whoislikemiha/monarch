# Monarch — Current Architecture

## Stack

- Tauri v2 desktop shell
- Svelte 5 frontend
- Rust backend for SQLite, lifecycle, and IPC
- Node sidecar embedding Pi SDK as the runtime engine
- SQLite as the canonical source of truth for agents, sessions, messages, memories, and events

## Runtime Model

- Monarch owns agent identity, session history, restore behavior, and persistence.
- Pi is used as an in-memory execution engine inside the Node sidecar.
- Rust talks to the sidecar over Monarch-owned JSONL commands and events.
- Session restore and continuation come from SQLite snapshots, not Pi session files.

## Sidecar Protocol

Commands Rust sends to the sidecar:

- `create_session`
- `destroy_session`
- `prompt`
- `abort`
- `set_model`
- `set_thinking_level`
- `new_session`
- `compact`
- `load_session`
- `extension_ui_response`

Events the sidecar emits back:

- `session_ready`
- `session_destroyed`
- `event` for forwarded Pi SDK runtime events
- `extension_ui_request`
- `error`

## Session Semantics

- Every live conversation has a Monarch session row in SQLite.
- Continuing a prior conversation creates a new session with `parent_session_id`.
- Message restore and UI history both walk session ancestry, so continued chats survive app restarts.
- Sidecar crash recovery respawns the runtime, recreates every tracked session, and rehydrates live context from SQLite before retrying the failed command.

## Current Product Capabilities

- Multi-agent command center with named shadow identities
- SQLite-backed restore and history browsing
- Session continuation with ancestry
- Council mode broadcasting to multiple live agents
- Extension UI round-trips through Monarch
- Prompt file overrides stored under `~/.config/monarch/prompts/`
- Real-time audit trail and session stats

## Legacy Notes

- `pi_session_file` still exists in the SQLite schema as a compatibility column, but it is no longer part of runtime behavior.
- The old Pi CLI/RPC subprocess architecture has been removed from the live app path.
