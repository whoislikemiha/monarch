# Monarch

Multi-agent desktop command center built on Tauri v2, Svelte 5, Rust, SQLite, and a Node sidecar that embeds Pi SDK.

Monarch manages a fleet of AI coding agents called shadows. SQLite is the canonical source of truth. A long-lived Node sidecar hosts in-memory Pi SDK sessions and streams runtime events back to Rust. Pi is the execution engine, not the session authority.

**Mental model:** Rust owns state, the frontend displays it, the sidecar operates on it. When in doubt, read `src-tauri/src/agent.rs` and `sidecar/src/runtime-manager.ts` side by side — they're the contract.

For the product vision, see [VISION.md](./VISION.md). For the full architecture walkthrough, data model, lifecycle details, and protocol reference, see [ONBOARDING.md](./ONBOARDING.md).

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

# regenerate Tauri bindings after Rust type changes
cargo run -- --export-bindings   # from src-tauri/
```

### Where things land

- Frontend dev server: `http://localhost:1420`
- Sidecar compiled: `sidecar/dist/index.js`
- SQLite DB: `~/.config/monarch/monarch.db`
- Prompt files: `~/.config/monarch/prompts/{agent_id}.md`

## Workflow

Linear is the source of truth for work items. GitHub is for code and PRs. Every non-trivial change starts with a Linear ticket and ends with a PR linked back to it.

### Linear-first development

- **No ticket, no work.** If a task doesn't have a Linear issue, create one before starting. Use `/linear-to-plan` for the full flow, or create one directly for smaller items.
- **One ticket = one branch = one PR.** This is the atomic unit of work. If a ticket's scope grows beyond a single coherent PR, split it — create sub-tasks or new tickets for the spun-off work rather than bloating the original.
- **Keep tickets alive.** Update the Linear issue when reality diverges from the plan: scope changes, things get descoped, blockers surface, acceptance criteria shift. The ticket should reflect what's actually happening, not what was originally imagined.
### Branches

Named `{github-username}/mon-{N}-{slug}`. One branch per Linear issue. Branch off `master`.

### Commits

Conventional commits scoped to the Linear issue: `type(mon-N): description`

Types: `feat`, `fix`, `refactor`, `perf`, `chore`, `docs`

Commit often — each commit should be a single logical change. Rebase onto `master` for clean history before merging.

### Plans & implementation notes

- Research plans go in `thoughts/plan/MON-{N}.md` before implementation.
- Implementation notes go in `thoughts/impl/MON-{N}.md` after completion.
- First commit on a task branch is typically `docs(mon-N): research plan`.
- Last commit before PR is typically `docs(mon-N): implementation notes`.

### Keep docs alive

When your changes affect architecture, conventions, data model, protocol, or lifecycle flows, update the relevant docs in the same PR:

- **CLAUDE.md** — rules, gotchas, key files, build commands, code patterns.
- **ONBOARDING.md** — deep architecture, data model, protocol reference, component tree, lifecycle walkthroughs.

If you add a new table, command, event channel, or convention — it belongs in the docs, not just the code. Stale docs are worse than no docs.

## Start Here (key files)

| Layer | File | Role |
|-------|------|------|
| Rust | `src-tauri/src/agent.rs` | Sidecar lifecycle, spawn, commands, crash recovery |
| Rust | `src-tauri/src/agent_state.rs` | Event-to-state assembly (`LiveAgentState`) |
| Rust | `src-tauri/src/db.rs` | SQLite schema and persistence |
| Rust | `src-tauri/src/sidecar_protocol.rs` | JSONL wire protocol types |
| Rust | `src-tauri/src/models.rs` | Provider auth, model cache |
| Rust | `src-tauri/src/ws.rs` | WebSocket bridge (mirrors Tauri commands) |
| Sidecar | `sidecar/src/runtime-manager.ts` | Pi SDK session host |
| Sidecar | `sidecar/src/protocol.ts` | Command + event type definitions |
| Sidecar | `sidecar/src/shadow-oath.ts` | Shadow identity + system prompt builder |
| Frontend | `src/App.svelte` | App shell, restore flow, agent creation |
| Frontend | `src/lib/AgentView.svelte` | Live agent UI, event handling, session continuation |
| Frontend | `src/lib/api.ts` | Unified IPC (Tauri webview or WebSocket fallback) |
| Frontend | `src/lib/bindings.ts` | Auto-generated Tauri command types (**do not edit**) |
| Frontend | `src/lib/liveAgentStore.svelte.ts` | Per-agent reactive state (SvelteMap + `$state`) |

Full file reference: [ONBOARDING.md](./ONBOARDING.md) section 12.

## Code Patterns

- **Svelte 5 runes only** — `$state()`, `$derived()`, `$effect()`. No legacy `$:` reactive statements or writable stores.
- **Tauri commands** are registered via `tauri::generate_handler![]` in `lib.rs`. Types auto-export to `bindings.ts` via tauri-specta. Max 10 args per command (Specta limit) — use request structs beyond that.
- **IPC abstraction** — all frontend `invoke`/`listen` calls go through `src/lib/api.ts`, never import `@tauri-apps/api` directly. This keeps the WebSocket fallback working for browser-mode dev.
- **Sidecar protocol** — JSONL over stdin/stdout. Commands: Rust-to-sidecar enums in `sidecar_protocol.rs`. Events: sidecar-to-Rust in `protocol.ts`.
- **Event channels** — `agent-state-{id}` (Rust-assembled snapshots, canonical), `agent-event-{id}` (out-of-band signals only), `agent-exit-{id}`, `agent-stderr-{id}`.
- **State flow** — Rust assembles `LiveAgentState` from sidecar events, emits snapshots on `agent-state-{id}` with 16ms debounce. Frontend pulls initial state via `get_agent_state()`, then subscribes. Reconcile by `stateVersion` — drop stale updates.
- **Frontend never writes conversation history.** All `messages`/`sessions` writes happen inside Rust's sidecar event handler.

## Rules & Gotchas

- **Do not edit `src/lib/bindings.ts`** — auto-generated by tauri-specta. Regenerate with `cargo run -- --export-bindings` from `src-tauri/`.
- **Build sidecar before `tauri dev`** — it fails if `sidecar/dist/index.js` doesn't exist.
- **Session ancestry is canonical** — continuing a conversation creates a new session row with `parent_session_id`. `get_messages_with_ancestry` is the only correct way to load history.
- **Sidecar is singleton** — one Node process hosts many agents, keyed by `agentId`. Not one process per agent.
- **Legacy columns** — `sessions.pi_session_file` and `agents.custom_prompt` exist in the schema but are inert. Don't build on them.
- **Prompt overrides are files** — stored at `~/.config/monarch/prompts/{agent_id}.md`, not in the DB. Editable externally.
- **Thinking levels are Pi-canonical on the wire** — `off` / `minimal` / `low` / `medium` / `high` / `xhigh`. `off` is a first-class value (pi-agent-core maps it to `undefined` reasoning). Per-provider display labels and per-model supported subsets live in `src/lib/thinking.ts`. Per-model defaults come from `~/.config/monarch/thinking.toml` (see `src-tauri/src/thinking_config.rs`); absence of a matching entry falls back to a conservative built-in table.
- **Toolbox tools stay mounted across agent switches** — if your tool keeps per-agent state, key it by `agentContext.agentId`.

## Adding a Toolbox Tool

1. Create component at `src/lib/toolbox/tools/YourTool.svelte` — must accept `{ agentContext }: ToolProps`.
2. Register it in `src/lib/toolbox/registry.ts` with a stable `id`, `title`, SVG `icon`, and optional `order`.
3. (Optional) Add backend: Tauri command in `src-tauri/src/toolbox/`, register in `lib.rs` handler + `ws.rs` dispatch.

Full guide: [ONBOARDING.md](./ONBOARDING.md) section 7.
