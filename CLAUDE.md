# Monarch

Multi-agent desktop command center built on Tauri v2, Svelte 5, Rust, SQLite, and a Node sidecar that embeds Pi SDK.

## Project Overview

Monarch manages a fleet of AI coding agents called shadows.

- Monarch owns agent identity, persistence, restore rules, and session history.
- SQLite is the canonical source of truth.
- A long-lived Node sidecar hosts in-memory Pi SDK sessions and streams runtime events back to Rust.
- Pi is the execution engine, not the session authority.

See [VISION.md](/home/miha/pro/monarch/VISION.md) for the broader concept.

## High-Level Architecture

```text
Svelte UI
  -> Tauri IPC
Rust backend
  -> SQLite persistence
  -> Node sidecar process
Node sidecar
  -> Pi SDK in-memory agent sessions
```

## Important Paths

- [src/App.svelte](/home/miha/pro/monarch/src/App.svelte): app shell, restore flow, agent creation
- [src/lib/AgentView.svelte](/home/miha/pro/monarch/src/lib/AgentView.svelte): live agent UI, session continuation, event handling
- [src-tauri/src/agent.rs](/home/miha/pro/monarch/src-tauri/src/agent.rs): sidecar lifecycle, crash recovery, runtime commands
- [src-tauri/src/db.rs](/home/miha/pro/monarch/src-tauri/src/db.rs): SQLite schema and persistence APIs
- [sidecar/src/runtime-manager.ts](/home/miha/pro/monarch/sidecar/src/runtime-manager.ts): Pi SDK runtime host
- [src-tauri/src/persistence.rs](/home/miha/pro/monarch/src-tauri/src/persistence.rs): prompt file storage

## Session Model

- Every live chat is a SQLite `sessions` row.
- Continued chats create a new session row with `parent_session_id`.
- Message replay uses session ancestry from SQLite for both UI display and runtime rehydration.
- Sidecar recovery rebuilds sessions from persisted Monarch state.

## Current Conventions

- Rust owns persistence and runtime recovery.
- Frontend renders state and invokes Tauri commands; it is not the canonical writer for conversation history.
- Use the sidecar protocol rather than Pi CLI commands.
- Prompt overrides are stored as files under `~/.config/monarch/prompts/`.

## Legacy Compatibility

- `pi_session_file` remains in the database schema as a compatibility column only.
- Prompt files remain filesystem-based.
