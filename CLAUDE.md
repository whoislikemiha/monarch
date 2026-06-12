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

| Layer    | File                                                  | Role                                                                        |
| -------- | ----------------------------------------------------- | --------------------------------------------------------------------------- |
| Rust     | `src-tauri/src/agent/mod.rs`                          | Module facade; re-exports + `DEBOUNCE_MILLIS`, `WsBroadcast`                |
| Rust     | `src-tauri/src/agent/manager.rs`                      | `AgentManager`, live-state types, high-level lifecycle                      |
| Rust     | `src-tauri/src/agent/sidecar.rs`                      | Sidecar spawn, stdin/stdout I/O, crash recovery                             |
| Rust     | `src-tauri/src/agent/event_handler.rs`                | Inbound sidecar event dispatch + snapshot emission                          |
| Rust     | `src-tauri/src/agent/persist/`                        | Single-consumer persistence pipeline (MON-37); split into messages.rs, objectives.rs, util.rs |
| Rust     | `src-tauri/src/agent/keeper.rs`                       | Keeper trigger/result handling + memory-slice rendering                     |
| Rust     | `src-tauri/src/agent/objective_prompt.rs`                 | Objective-prompt heuristics + `rehydrate_user_content`                          |
| Rust     | `src-tauri/src/agent/commands.rs`                     | Tauri command wrappers + request DTOs                                       |
| Rust     | `src-tauri/src/agent/state.rs`                        | Event-to-state assembly (`LiveAgentState`)                                  |
| Rust     | `src-tauri/src/db/`                                   | SQLite persistence (`tokio-rusqlite`), split by domain; schema/migrations in `db/schema.rs` |
| Rust     | `src-tauri/src/sidecar_protocol/`                     | JSONL wire protocol types; split into config.rs, commands.rs, events.rs, types.rs |
| Rust     | `src-tauri/src/models.rs`                             | Provider auth, model cache                                                  |
| Rust     | `src-tauri/src/persistence.rs`                        | Prompt/avatar/attachment file I/O                                           |
| Rust     | `src-tauri/src/project/`                              | Project detection + instruction file commands                               |
| Rust     | `src-tauri/src/config/thinking.rs`                    | Per-model thinking-level defaults (`thinking.toml`)                         |
| Rust     | `src-tauri/src/websocket/`                            | WebSocket bridge (mirrors Tauri commands); dispatch.rs + handlers/ per domain |
| Rust     | `src-tauri/src/error.rs`                              | `MonarchError` unified error type                                           |
| Rust     | `src-tauri/src/ui/zoom.rs`                            | Window zoom command                                                         |
| Sidecar  | `sidecar/src/runtime-manager.ts`                      | Pi SDK session host                                                         |
| Sidecar  | `sidecar/src/protocol.ts`                             | Command + event type definitions                                            |
| Sidecar  | `sidecar/src/shadow-oath.ts`                          | Shadow identity + system prompt builder                                     |
| Sidecar  | `sidecar/src/ui-bridge.ts`                            | Pi extension UI request/response routing                                    |
| Sidecar  | `sidecar/src/model-resolver.ts`                       | Dynamic model registration + thinking-level resolution                      |
| Sidecar  | `sidecar/src/stored-content.ts`                       | Stored message content parsing + normalization helpers                      |
| Sidecar  | `sidecar/src/memory-tools.ts`                         | Pi tool definitions for memory search/inject                                |
| Frontend | `src/App.svelte`                                      | App shell, restore flow, agent creation                                     |
| Frontend | `src/lib/AgentView.svelte`                            | Live agent UI, event handling, session continuation                         |
| Frontend | `src/lib/AgentRoster.svelte`                          | Left-rail agent list (portraits + status)                                   |
| Frontend | `src/lib/ChatInput.svelte`                            | Composer: textarea, attachments, @-mention autocomplete                     |
| Frontend | `src/lib/api.ts`                                      | Unified IPC (Tauri webview or WebSocket fallback)                           |
| Frontend | `src/lib/bindings.ts`                                 | Auto-generated Tauri command types (**do not edit**)                        |
| Frontend | `src/lib/toolbox/liveAgentStore.svelte.ts`            | Per-agent reactive state (SvelteMap + `$state`)                             |
| Frontend | `src/lib/toolbox/objectiveStore.svelte.ts`                | Per-agent objective tree + event-log slice (MON-83)                             |
| Frontend | `src/lib/toolbox/tools/ObjectiveTimelineTool.svelte`      | Read-only objective timeline + manual create form (MON-83)                      |
| Frontend | `src/lib/workspace/timelineStore.svelte.ts`               | Paged per-agent execution-timeline feed + live head refresh (MON-124)           |
| Frontend | `src/lib/workspace/TimelinePane.svelte`                   | Workspace timeline: NOW strip, segments, action cards, infinite scroll (MON-124)|
| Frontend | `src/lib/workspace/timelineModel.ts`                      | Timeline view-model: payload parsing, action grouping, live tool merge (MON-124)|
| Frontend | `src/lib/classifierStore.svelte.ts`                   | Per-agent user-turn complexity classifications (MON-82)                     |
| Frontend | `src/lib/ClassificationPill.svelte`                   | Read-only complexity pill shown beside each user message (MON-82)           |
| Frontend | `src/lib/toolbox/tools/ClassifierSettingsTool.svelte` | Global classifier config: primary/fallback models, timeout, prompt (MON-82) |
| Rust     | `src-tauri/src/config/classifier.rs`                  | `classifier.toml` loader + Tauri commands (MON-82)                          |
| Sidecar  | `sidecar/src/classifier.ts`                           | One-shot Haiku/LM Studio classifier invoked on every user turn (MON-82)     |
| Frontend | `src/lib/stores/agentStore.svelte.ts`                 | Active/saved agent list + selection state                                   |
| Frontend | `src/lib/stores/notificationsStore.svelte.ts`         | App-wide error/warning toasts (MON-51)                                      |
| Frontend | `src/lib/NotificationStack.svelte`                    | Top-right overlay rendering notifications (MON-51)                          |
| Frontend | `src/lib/thinking.ts`                                 | Thinking-level UI catalogue + per-provider labels                           |
| Frontend | `src/lib/ui/`                                         | Design system — tokens (`src/global.css`), atoms (`styles/atoms.css`), Svelte primitives, `Catalog.svelte` (`?catalog`). See `src/lib/ui/README.md` |

Full file reference: [ONBOARDING.md](./ONBOARDING.md) section 12.

## Code Patterns

- **Svelte 5 runes only** — `$state()`, `$derived()`, `$effect()`. No legacy `$:` reactive statements or writable stores.
- **Tauri commands** are registered via `tauri::generate_handler![]` in `lib.rs`. Types auto-export to `bindings.ts` via tauri-specta. Max 10 args per command (Specta limit) — use request structs beyond that.
- **IPC abstraction** — all frontend `invoke`/`listen` calls go through `src/lib/api.ts`, never import `@tauri-apps/api` directly. This keeps the WebSocket fallback working for browser-mode dev.
- **Sidecar protocol** — JSONL over stdin/stdout. Commands: Rust-to-sidecar enums in `sidecar_protocol/commands.rs`. Events: sidecar-to-Rust in `protocol.ts`.
- **Event channels** — `agent-state-{id}` (Rust-assembled snapshots, canonical), `agent-event-{id}` (out-of-band signals only), `agent-exit-{id}`, `agent-stderr-{id}`, `agent-classification-{id}` (MON-82 per-turn classifier output).
- **State flow** — Rust assembles `LiveAgentState` from sidecar events, emits snapshots on `agent-state-{id}` with 16ms debounce. Frontend pulls initial state via `get_agent_state()`, then subscribes. Reconcile by `stateVersion` — drop stale updates.
- **Frontend never writes conversation history.** All `messages`/`sessions` writes happen inside Rust's sidecar event handler.

## Design System (visual language) — use this for ALL new UI

Monarch is mid-migration to a flat, token-driven visual language. **New surfaces are built with the design system; legacy components are not the template.** Full guide: [`src/lib/ui/README.md`](./src/lib/ui/README.md). Live reference: run dev and open `http://localhost:1420/?catalog`.

- **Tokens are global, always available.** Use the design-system custom properties in any component: elevation `--bg-sink/-base/-panel/-raised/-overlay`; `--status-success/-warning/-error/-info`; `--accent`, `--accent-2`, `--accent-ink`, `--border`, `--focus`; grade ramp `--grade-e…--grade-s`; spacing `--s1…--s8`; radius `--r-sm/-md/-lg`. Defined in `src/global.css` (aliased onto the themed tokens in `src/lib/themes/*`).
- **Atoms are opt-in.** Component classes (`.btn`, `.badge`, `.chip`, `.sdot`, `.avatar`, `.meter`, `.drow`, `.tree`, `.popover`, `.codeblock`, event icons) live in `src/lib/ui/styles/atoms.css`. They are **not loaded app-wide** — import them only in the new surface that uses them, or (preferred) use the Svelte primitives in `src/lib/ui/`.
- **Prefer Svelte primitives.** Wrap atoms as typed components in `src/lib/ui/` (built on demand). Add new ones there as surfaces need them — don't scatter raw class strings.
- **House style (non-negotiable):** NO shadows / glows / blurs — depth is elevation + 1px border + space. Small radius only (`--r-sm/-md/-lg`, circle for dots/avatars). **Inter for everything a human reads; JetBrains Mono (`.mono`) ONLY for ids, metrics, paths, timestamps, code.** Status is never color-alone (shape + label). Stay themeable — token vars only, no hardcoded hex.

## Rules & Gotchas

- **Do not edit `src/lib/bindings.ts`** — auto-generated by tauri-specta. Regenerate with `cargo run -- --export-bindings` from `src-tauri/`.
- **Don't copy legacy styling into new UI.** Most existing components predate the design system (scoped ad-hoc styles, legacy token names, occasional shadows). When building or restyling a surface, follow the Design System section above — don't mimic the nearest old component. Legacy styles get deleted as surfaces are rebuilt, not propagated.
- **Build sidecar before `tauri dev`** — it fails if `sidecar/dist/index.js` doesn't exist.
- **Session ancestry is canonical** — continuing a conversation creates a new session row with `parent_session_id`. `get_messages_with_ancestry` is the only correct way to load history.
- **Sidecar is singleton** — one Node process hosts many agents, keyed by `agentId`. Not one process per agent.
- **Legacy columns** — `sessions.pi_session_file` and `agents.custom_prompt` exist in the schema but are inert. Don't build on them.
- **Schema evolves via `ALTER TABLE` migrations** — `db::init_schema` applies idempotent `ALTER TABLE` / `CREATE TABLE IF NOT EXISTS` blocks at the end of init. Never rewrite the base `CREATE TABLE` for columns added post-launch — add a new migration block. Current post-launch columns: `sessions.parent_session_id`, `agents.project_id`, `agents.context_window`, `agents.archived_at`, `agents.avatar_type`, `agents.avatar_path`, `agents.current_objective_id`, `agents.identity_version_id`, `messages.duration_ms`, `messages.objective_id`, `objective_nodes.scope`, `objective_nodes.current_direction`, `objective_nodes.rationale`, `objective_nodes.fork_parent_id`, `objective_nodes.kind` (P1), `projects.root_objective_id` (P1). Post-launch tables: `projects`, `agent_templates`, `ui_state`, `agent_stats`, `agent_tool_usage`, `message_attachments`, `objective_nodes`, `objective_events`, `objective_plan_items`, `objective_refs`, `objective_reports`, `classifications`, `captain`, `captain_identity_versions`, `shadow_identity_versions`, `agent_working_memory`, `memories`, `memories_fts`, `memory_keeper_runs`.
- **Classifier is advisory, per-user-turn** (MON-82) — the sidecar fires a one-shot `complete()` against Haiku (default) or LM Studio in parallel with every user turn, emits `agent-classification-{id}`, and annotates the forwarded user `message_end` with `classification_id` so Rust can backfill the FK. Failures log a "failed" pill but never block the turn. No consumer of the label in Slice 1 — Slice 3 (Architect, MON-84) is the first reader. Config is global at `~/.config/monarch/classifier.toml`; the system prompt lives in `config/classifier.rs` (read-only in the settings UI).

- **The workspace timeline is a feed projection** (MON-124) — `db_list_agent_timeline` pages on top-level `objective_events` (`parent_event_id IS NULL`, `(created_at, id)` cursor, newest-first) joined through `objective_nodes.assignee_shadow_id` with `kind='objective'`; children and objective metadata ride along per page. Tool-call payloads carry a normalized `target` extracted at record time (previews are truncated — don't parse `args_preview`). Captain-opened scoped chats record a `chat_spawned` child event under the action. Chat panes never render tool tables — `ToolActivityChip` links a turn's tool group to its timeline card by `tool_call_id`. Keep action-card children heterogeneous (tools, decisions, chats, future delegated runs) — that's the Arc II seam.
- **Objectives are orthogonal to sessions** (MON-83) — an objective can span multiple sessions, a session can span multiple objectives. Aggregation key for "what happened on this objective" is `objective_id`, not `session_id`. Slice 2 creates objectives manually via the toolbox tool; Slice 3 (MON-84) adds automatic decomposition via the Architect. Don't couple the two concepts.
- **Campaign root is a typed node** (P1, roadmap-v2) — each project has exactly one *campaign*: an `objective_nodes` row with `kind='campaign'`, `parent_id=NULL`, `root_id=self`, linked from `projects.root_objective_id`. It's a never-closed container, not work — never assign/grade/close/report it; filter it out with `WHERE kind='objective'` when you mean real work. Created on project detect via `ensure_campaign_root_internal` (idempotent, one per project). All real work is `kind='objective'`.
- **Meaningful turns branch under the campaign** (P1) — `auto_create_current_objective_internal` creates the turn's objective as a branch (`parent_id = root_id = campaign root`), not a fresh per-turn root. **Project-less agents stay ephemeral**: no project → no campaign → no auto-objective (the seam where a future per-captain scratch campaign hooks in). The objective timeline groups by `root_id` (the campaign), so the store fetches the campaign tree, not per-objective roots; `db_get_campaign_root_for_agent` resolves the placement target for capture before any work exists.
- **Archive lifecycle** — `agents.archived_at IS NULL` means active; non-null means archived. Use `db_archive_agent` / `db_unarchive_agent`, not hard delete, unless the user explicitly asks.
- **Attachments live on disk** — `message_attachments` is just an ordered reference; bytes go under `~/.config/monarch/attachments/{uuid}.{ext}`. Same pattern as prompts and avatars.
- **Prompt overrides are files** — stored at `~/.config/monarch/prompts/{agent_id}.md`, not in the DB. Editable externally.
- **Avatars are image-only** — uploaded images live under `~/.config/monarch/avatars/`; the DB holds `avatar_type` (`"image"`, or NULL = monogram fallback) + `avatar_path` only. Rendered by `src/lib/ui/Avatar.svelte`; the Rive animation system was removed (stale `'rive'` rows are cleared by a startup migration).
- **Projects are the grouping unit** — `projects` table + `agents.project_id` FK, keyed by git-root path (`find_project_root` walks up to `.git`). Project instructions come from `projects.instructions` (DB, editable in UI) and fall back to reading `AGENTS.md` / `CLAUDE.md` at the project root; the DB value takes precedence, the file value seeds the DB on first detect. The resolved string is sent to the sidecar as `projectInstructions` in `create_session` and appended to the system prompt by `buildSystemPrompt`.
- **Thinking levels are Pi-canonical on the wire** — `off` / `minimal` / `low` / `medium` / `high` / `xhigh`. `off` is a first-class value (pi-agent-core maps it to `undefined` reasoning). Per-provider display labels and per-model supported subsets live in `src/lib/thinking.ts`. Per-model defaults come from `~/.config/monarch/thinking.toml` (see `src-tauri/src/config/thinking.rs`); absence of a matching entry falls back to a conservative built-in table.
- **Toolbox tools stay mounted across agent switches** — if your tool keeps per-agent state, key it by `agentContext.agentId`.

## Adding a Toolbox Tool

1. Create component at `src/lib/toolbox/tools/YourTool.svelte` — must accept `{ agentContext }: ToolProps`.
2. Register it in `src/lib/toolbox/registry.ts` with a stable `id`, `title`, SVG `icon`, and optional `order`.
3. (Optional) Add backend: Tauri command in `src-tauri/src/toolbox/`, register in `lib.rs` handler + `websocket/dispatch.rs` dispatch.

Existing tools to crib from: `PlaceholderTool.svelte` (full store + backend path), `ContextInspectorTool.svelte` (reads `agentContext.live`), `ShadowStatsTool.svelte` (pulls from `db_get_agent_stats`).

Full guide: [ONBOARDING.md](./ONBOARDING.md) section 7.
