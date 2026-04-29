# Monarch

Multi-agent desktop command center built on Tauri v2, Svelte 5, Rust, SQLite, and a Node sidecar that embeds Pi SDK.

Monarch manages a fleet of AI coding agents called shadows. SQLite is the canonical source of truth. A long-lived Node sidecar hosts in-memory Pi SDK sessions and streams runtime events back to Rust. Pi is the execution engine, not the session authority.

**Mental model:** Rust owns state, the frontend displays it, the sidecar operates on it. When in doubt, read `src-tauri/src/agent/` and `sidecar/src/runtime-manager.ts` side by side — they're the contract.

For the product vision, see [VISION.md](./VISION.md). For the full architecture walkthrough, data model, lifecycle details, and protocol reference, see [ONBOARDING.md](./ONBOARDING.md). For ongoing design work on shadow cognition (memory, attention, distillation, interaction flows), see [`thoughts/design/shadow-cognition/`](./thoughts/design/shadow-cognition/) — start with the README in that folder, then see [`roadmap.md`](./thoughts/design/shadow-cognition/roadmap.md) for the implementation phasing (each phase is a tangible testable result; existing tickets MON-91/82/83/84/93/94/95/96/97 are mapped to phases).

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

# regenerate Tauri bindings after Rust type changes
cargo run -- --export-bindings   # from src-tauri/

# bump Pi packages to latest (run before each release; Pi releases ~daily)
npm run pi:upgrade               # bumps pi-ai + pi-coding-agent in root + sidecar, rebuilds sidecar
```

After `pi:upgrade`, sanity-check `sidecar/node_modules/@mariozechner/pi-ai/dist/models.generated.js` for the
latest model IDs and update the curated lists in `src-tauri/src/models.rs` (`anthropic_curated`,
`openai_codex_curated`) plus the subscription-tagging sets in the same file. Pi catalog drift is the
main reason curated lists go stale.

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

- **AGENTS.md** — rules, gotchas, key files, build commands, code patterns.
- **ONBOARDING.md** — deep architecture, data model, protocol reference, component tree, lifecycle walkthroughs.

If you add a new table, command, event channel, or convention — it belongs in the docs, not just the code. Stale docs are worse than no docs.

## Start Here (key files)

| Layer | File | Role |
|-------|------|------|
| Rust | `src-tauri/src/agent/mod.rs` | Module facade; re-exports + `DEBOUNCE_MILLIS`, `WsBroadcast` |
| Rust | `src-tauri/src/agent/manager.rs` | `AgentManager`, live-state types, high-level lifecycle |
| Rust | `src-tauri/src/agent/sidecar.rs` | Sidecar spawn, stdin/stdout I/O, crash recovery |
| Rust | `src-tauri/src/agent/event_handler.rs` | Inbound sidecar event dispatch + snapshot emission |
| Rust | `src-tauri/src/agent/persist.rs` | Single-consumer persistence pipeline (MON-37) |
| Rust | `src-tauri/src/agent/commands.rs` | Tauri command wrappers + request DTOs |
| Rust | `src-tauri/src/agent_state.rs` | Event-to-state assembly (`LiveAgentState`) |
| Rust | `src-tauri/src/db.rs` | SQLite schema and persistence (`tokio-rusqlite`) |
| Rust | `src-tauri/src/sidecar_protocol.rs` | JSONL wire protocol types |
| Rust | `src-tauri/src/models.rs` | Provider auth, model cache |
| Rust | `src-tauri/src/persistence.rs` | Prompt/avatar/attachment file I/O |
| Rust | `src-tauri/src/project/` | Project detection + instruction file commands |
| Rust | `src-tauri/src/thinking_config.rs` | Per-model thinking-level defaults (`thinking.toml`) |
| Rust | `src-tauri/src/ws.rs` | WebSocket bridge (mirrors Tauri commands) |
| Rust | `src-tauri/src/error.rs` | `MonarchError` unified error type |
| Rust | `src-tauri/src/zoom.rs` | Window zoom command |
| Sidecar | `sidecar/src/runtime-manager.ts` | Pi SDK session host |
| Sidecar | `sidecar/src/protocol.ts` | Command + event type definitions |
| Sidecar | `sidecar/src/shadow-oath.ts` | Shadow identity + system prompt builder |
| Sidecar | `sidecar/src/ui-bridge.ts` | Pi extension UI request/response routing |
| Frontend | `src/App.svelte` | App shell, restore flow, agent creation |
| Frontend | `src/lib/AgentView.svelte` | Live agent UI, event handling, session continuation |
| Frontend | `src/lib/AgentRoster.svelte` | Left-rail agent list (portraits + status) |
| Frontend | `src/lib/ChatInput.svelte` | Composer: textarea, attachments, @-mention autocomplete |
| Frontend | `src/lib/api.ts` | Unified IPC (Tauri webview or WebSocket fallback) |
| Frontend | `src/lib/bindings.ts` | Auto-generated Tauri command types (**do not edit**) |
| Frontend | `src/lib/toolbox/liveAgentStore.svelte.ts` | Per-agent reactive state (SvelteMap + `$state`) |
| Frontend | `src/lib/toolbox/questStore.svelte.ts` | Per-agent quest tree + event-log slice (MON-83) |
| Frontend | `src/lib/toolbox/tools/QuestTimelineTool.svelte` | Read-only quest timeline + manual create form (MON-83) |
| Frontend | `src/lib/classifierStore.svelte.ts` | Per-agent user-turn complexity classifications (MON-82) |
| Frontend | `src/lib/ClassificationPill.svelte` | Read-only complexity pill shown beside each user message (MON-82) |
| Frontend | `src/lib/toolbox/tools/ClassifierSettingsTool.svelte` | Global classifier config: primary/fallback models, timeout, prompt (MON-82) |
| Rust | `src-tauri/src/classifier_config.rs` | `classifier.toml` loader + Tauri commands (MON-82) |
| Sidecar | `sidecar/src/classifier.ts` | One-shot Haiku/LM Studio classifier invoked on every user turn (MON-82) |
| Frontend | `src/lib/stores/agentStore.svelte.ts` | Active/saved agent list + selection state |
| Frontend | `src/lib/stores/notificationsStore.svelte.ts` | App-wide error/warning toasts (MON-51) |
| Frontend | `src/lib/NotificationStack.svelte` | Top-right overlay rendering notifications (MON-51) |
| Frontend | `src/lib/thinking.ts` | Thinking-level UI catalogue + per-provider labels |

Full file reference: [ONBOARDING.md](./ONBOARDING.md) section 12.

## Code Patterns

- **Svelte 5 runes only** — `$state()`, `$derived()`, `$effect()`. No legacy `$:` reactive statements or writable stores.
- **Tauri commands** are registered via `tauri::generate_handler![]` in `lib.rs`. Types auto-export to `bindings.ts` via tauri-specta. Max 10 args per command (Specta limit) — use request structs beyond that.
- **IPC abstraction** — all frontend `invoke`/`listen` calls go through `src/lib/api.ts`, never import `@tauri-apps/api` directly. This keeps the WebSocket fallback working for browser-mode dev.
- **Sidecar protocol** — JSONL over stdin/stdout. Commands: Rust-to-sidecar enums in `sidecar_protocol.rs`. Events: sidecar-to-Rust in `protocol.ts`.
- **Event channels** — `agent-state-{id}` (Rust-assembled snapshots, canonical), `agent-event-{id}` (out-of-band signals only), `agent-exit-{id}`, `agent-stderr-{id}`, `agent-classification-{id}` (MON-82 per-turn classifier output).
- **State flow** — Rust assembles `LiveAgentState` from sidecar events, emits snapshots on `agent-state-{id}` with 16ms debounce. Frontend pulls initial state via `get_agent_state()`, then subscribes. Reconcile by `stateVersion` — drop stale updates.
- **Frontend never writes conversation history.** All `messages`/`sessions` writes happen inside Rust's sidecar event handler.

## Rules & Gotchas

- **Do not edit `src/lib/bindings.ts`** — auto-generated by tauri-specta. Regenerate with `cargo run -- --export-bindings` from `src-tauri/`.
- **Build sidecar before `tauri dev`** — it fails if `sidecar/dist/index.js` doesn't exist.
- **Session ancestry is canonical** — continuing a conversation creates a new session row with `parent_session_id`. `get_messages_with_ancestry` is the only correct way to load history.
- **Sidecar is singleton** — one Node process hosts many agents, keyed by `agentId`. Not one process per agent.
- **Legacy columns** — `sessions.pi_session_file` and `agents.custom_prompt` exist in the schema but are inert. Don't build on them.
- **Schema evolves via `ALTER TABLE` migrations** — `db::init_schema` applies idempotent `ALTER TABLE` / `CREATE TABLE IF NOT EXISTS` blocks at the end of init. Never rewrite the base `CREATE TABLE` for columns added post-launch — add a new migration block. Current post-launch columns: `sessions.parent_session_id`, `agents.project_id`, `agents.context_window`, `agents.archived_at`, `agents.avatar_type`, `agents.avatar_path`, `agents.current_quest_id`, `messages.duration_ms`, `messages.quest_id`, plus nested quest-event columns `quest_events.parent_event_id`, `quest_events.author`, `quest_events.surface_override`, and `quest_events.payload_schema_version`. Post-launch tables: `projects`, `agent_templates`, `ui_state`, `agent_stats`, `agent_tool_usage`, `message_attachments`, `quest_nodes`, `quest_events`, `agent_working_memory`, `classifications`. Roadmap P4b adds `quest_plan_items` and plan-action links.
- **Classifier is advisory, per-user-turn** (MON-82) — the sidecar fires a one-shot `complete()` against Haiku (default) or LM Studio in parallel with every user turn, emits `agent-classification-{id}`, and annotates the forwarded user `message_end` with `classification_id` so Rust can backfill the FK. Failures log a "failed" pill but never block the turn. No consumer of the label in Slice 1 — Slice 3 (Architect, MON-84) is the first reader. Config is global at `~/.config/monarch/classifier.toml`; the system prompt lives in `classifier_config.rs` (read-only in the settings UI).

- **Quests are orthogonal to sessions** (MON-83) — a quest can span multiple sessions, a session can span multiple quests. Aggregation key for "what happened on this quest" is `quest_id`, not `session_id`. Quests are Monarch's canonical work object; external trackers can be attachments/refs, not the authority.
- **Quest / plan / timeline are distinct** (roadmap P4/P4b) — quest = what/why, durable execution plan = intended how, timeline coherent actions = what actually happened. Don't treat coherent actions as plan items, and don't store future plan fields until P4b has writers.
- **Executor narration is semantic, not chatty** (roadmap P4) — narration tools should produce `coherent_action`, `action_outcome`, and `executor_decision` events. Hide the narration tool calls themselves from timeline tool-call rows. Do not persist raw model thinking as timeline content; if rationale matters, record an explicit decision.
- **Archive lifecycle** — `agents.archived_at IS NULL` means active; non-null means archived. Use `db_archive_agent` / `db_unarchive_agent`, not hard delete, unless the user explicitly asks.
- **Attachments live on disk** — `message_attachments` is just an ordered reference; bytes go under `~/.config/monarch/attachments/{uuid}.{ext}`. Same pattern as prompts and avatars.
- **Prompt overrides are files** — stored at `~/.config/monarch/prompts/{agent_id}.md`, not in the DB. Editable externally.
- **Avatars are files** — rive / image uploads live under `~/.config/monarch/avatars/`; the DB holds `avatar_type` + `avatar_path` only.
- **Projects are the grouping unit** — `projects` table + `agents.project_id` FK, keyed by git-root path (`find_project_root` walks up to `.git`). Project instructions come from `projects.instructions` (DB, editable in UI) and fall back to reading `AGENTS.md` / `AGENTS.md` at the project root; the DB value takes precedence, the file value seeds the DB on first detect. The resolved string is sent to the sidecar as `projectInstructions` in `create_session` and appended to the system prompt by `buildSystemPrompt`.
- **Thinking levels are Pi-canonical on the wire** — `off` / `minimal` / `low` / `medium` / `high` / `xhigh`. `off` is a first-class value (pi-agent-core maps it to `undefined` reasoning). Per-provider display labels and per-model supported subsets live in `src/lib/thinking.ts`. Per-model defaults come from `~/.config/monarch/thinking.toml` (see `src-tauri/src/thinking_config.rs`); absence of a matching entry falls back to a conservative built-in table.
- **Toolbox tools stay mounted across agent switches** — if your tool keeps per-agent state, key it by `agentContext.agentId`.

## Adding a Toolbox Tool

1. Create component at `src/lib/toolbox/tools/YourTool.svelte` — must accept `{ agentContext }: ToolProps`.
2. Register it in `src/lib/toolbox/registry.ts` with a stable `id`, `title`, SVG `icon`, and optional `order`.
3. (Optional) Add backend: Tauri command in `src-tauri/src/toolbox/`, register in `lib.rs` handler + `ws.rs` dispatch.

Existing tools to crib from: `PlaceholderTool.svelte` (full store + backend path), `ContextInspectorTool.svelte` (reads `agentContext.live`), `ShadowStatsTool.svelte` (pulls from `db_get_agent_stats`).

Full guide: [ONBOARDING.md](./ONBOARDING.md) section 7.
