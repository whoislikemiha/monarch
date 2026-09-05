
# Monarch

Multi-agent desktop command center built on Tauri v2, Svelte 5, Rust, SQLite, and a Node sidecar that embeds Pi SDK.

Monarch manages a fleet of AI coding agents. SQLite is the canonical source of truth. A long-lived Node sidecar hosts in-memory Pi SDK sessions and streams runtime events back to Rust. Pi is the execution engine, not the session authority.

**Mental model:** Rust owns state, the frontend displays it, the sidecar operates on it. When in doubt, read `src-tauri/src/agent/` and `sidecar/src/runtime-manager.ts` side by side — they're the contract.


## Build & Dev

```bash
# first-time setup
npm install
npm install --prefix sidecar

# development (build sidecar first — tauri dev fails without it)
npm run build:sidecar
npm run tauri dev

# production
npm run build            # chains sidecar + web builds
npm run tauri build      # package desktop binary

# type-checking
npx svelte-check         # frontend
cargo check              # backend (from src-tauri/)

# frontend tests (Vitest — runs store unit tests)
npm test                 # single run
npm run test:watch       # watch mode

# backend tests (from src-tauri/)
cargo test

# regenerate Tauri bindings after Rust type changes
cargo run -- --export-bindings   # from src-tauri/

# bump Pi packages to latest (run before each release; Pi releases ~daily)
npm run pi:upgrade               # bumps pi-ai + pi-coding-agent in root + sidecar, rebuilds sidecar
```

After `pi:upgrade`, sanity-check `sidecar/node_modules/@mariozechner/pi-ai/dist/models.generated.js` for the
latest model IDs and update the curated lists in `src-tauri/src/models.rs` (`anthropic_curated`,
`openai_codex_curated`) plus the subscription-tagging sets in the same file. Pi catalog drift is the
main reason curated lists go stale.

### Runtime and first-run requirements

- `node` must be on `PATH` at runtime; Rust spawns the Node sidecar as a child process.
- The initial Rust build downloads ONNX Runtime binaries, and first memory use downloads the `bge-small-en-v1.5` embedding model from HuggingFace. These steps require network access until the artifacts are cached.
- Provider API keys must be available in the environment launching the app; the sidecar inherits them. Pi subscription credentials are read from `~/.pi/agent/auth.json`. See [README.md](./README.md#authentication--api-keys) for setup.

### Where things land

- Frontend dev server: `http://localhost:1420`
- Sidecar compiled: `sidecar/dist/index.js`
- SQLite DB: `~/.config/monarch/monarch.db`

## Start Here

- **Agent lifecycle and state:** `src-tauri/src/agent/`
- **Database and migrations:** `src-tauri/src/db/` (schema in `schema.rs`)
- **Execution runtime:** `sidecar/src/runtime-manager.ts`
- **Wire contract:** `src-tauri/src/sidecar_protocol/` + `sidecar/src/protocol.ts`
- **Frontend IPC:** `src/lib/api.ts`
- **UI conventions:** [src/lib/ui/README.md](./src/lib/ui/README.md) — read before UI changes.

## Code Patterns

- **Svelte 5 runes only** — `$state()`, `$derived()`, `$effect()`. No legacy `$:` reactive statements or writable stores.
- **Tauri commands** must be registered in both `src-tauri/src/lib.rs` and the WebSocket bridge (`src-tauri/src/websocket/handlers/` + `dispatch.rs`) so browser-mode dev works. Max 10 args per command (Specta limit) — use request structs beyond that.
- **Generated bindings:** never edit `src/lib/bindings.ts` manually. After Rust command/type changes, regenerate with `cargo run -- --export-bindings` from `src-tauri/`.
- **IPC abstraction** — all frontend `invoke`/`listen` calls go through `src/lib/api.ts`, never import `@tauri-apps/api` directly. This keeps the WebSocket fallback working for browser-mode dev.
- **Sidecar protocol** — JSONL over stdin/stdout. Commands: Rust-to-sidecar enums in `sidecar_protocol/commands.rs`. Events: sidecar-to-Rust in `protocol.ts`.
- **Event channels** — `agent-state-{id}` (Rust-assembled snapshots, canonical), `agent-event-{id}` (out-of-band signals only), `agent-exit-{id}`, `agent-stderr-{id}`, `agent-classification-{id}` (MON-82 per-turn classifier output).
- **State flow** — Rust assembles `LiveAgentState` from sidecar events, emits snapshots on `agent-state-{id}` with 16ms debounce. Frontend pulls initial state via `get_agent_state()`, then subscribes. Reconcile by `stateVersion` — drop stale updates.
- **Rust owns conversation persistence and turn assembly.** The frontend renders Rust-assembled state; sidecar event persistence goes through `src-tauri/src/agent/persist/`.
- **Preserve persistence ordering.** Enqueue sidecar event persistence through the single-consumer `PersistCommand` pipeline. It awaits each database operation in FIFO order; independent spawned writes can reorder events.
- **Async locking:** never hold a `parking_lot::MutexGuard` across `.await`. Copy the needed state and release the guard before async work; database methods already run through `tokio-rusqlite`.
- **History compatibility:** use `normalizeStoredUserContent` / `normalizeStoredAssistantContent` in `sidecar/src/stored-content.ts` when replaying stored messages. Older user rows may contain plain text instead of JSON content blocks.
- **Session history:** use `get_messages_with_ancestry` for runtime rehydration and active-conversation history. The session browser uses `get_session_display_items` for that session alone. Fresh sessions have no parent; continuing in place and waking a stopped agent reuse the existing session row. Set `parent_session_id` only for explicit continuation with ancestry.
- **Dock panels:** register in `src/lib/layout/panelRegistry.ts`. DB-backed panels must work when `agentContext.live` is null. Panels stay mounted across agent switches, so key per-agent local state by `agentContext.agentId`.
