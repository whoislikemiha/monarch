# Monarch — Contributor Onboarding

Welcome. This doc is the long version of "everything you need to know to be productive in this repo." It walks through the architecture, data model, lifecycle of an agent, the sidecar protocol, the frontend, conventions, build flow, and what is and isn't implemented yet.

For the one-paragraph pitch, read [README.md](./README.md). For the product vision, read [VISION.md](./VISION.md). For the tight architectural summary, read [CLAUDE.md](./CLAUDE.md). This document is the place that ties them all together.

---

## 1. Top-level layout

```
monarch/
├── src/                # Svelte 5 frontend (TypeScript + .svelte components)
├── src-tauri/          # Rust backend (Tauri v2)
│   ├── src/            # Rust source: main.rs, lib.rs, agent.rs, db.rs, models.rs, persistence.rs
│   ├── tauri.conf.json # Window, dev URL, frontend dist path
│   ├── Cargo.toml      # Rust deps
│   └── build.rs        # Tauri build hook
├── sidecar/            # Long-lived Node.js process hosting the Pi SDK runtime
│   ├── src/            # index.ts, runtime-manager.ts, protocol.ts, shadow-oath.ts, ui-bridge.ts
│   ├── tsconfig.json
│   └── package.json
├── prompts/            # Legacy bundled prompt templates (not used by the current flow)
├── package.json        # Root frontend deps + workspace scripts
├── vite.config.ts
├── svelte.config.js
├── tsconfig.json
├── index.html
├── README.md
├── CLAUDE.md
├── VISION.md
└── FEATURES.md
```

---

## 2. Tech stack

### Frontend (`package.json`)
- **Svelte 5** — runes-based reactivity (`$state`, `$derived`, `$effect`).
- **Vite 6** — dev server on port 1420, builds to `dist/`.
- **TypeScript 5**.
- **@tauri-apps/api 2** — IPC (`invoke`, `listen`, `emit`) to Rust.
- **@tauri-apps/plugin-dialog 2.6.0** — file/folder dialogs.
- **marked 17** — Markdown rendering for agent output.
- **@mariozechner/pi-ai 0.65.2** — Pi SDK core: providers, models, auth.
- **@mariozechner/pi-coding-agent 0.65.2** — Pi agent runtime + session manager.

### Sidecar (`sidecar/package.json`)
- **TypeScript 5**, compiled via `tsc` to `sidecar/dist/index.js`.
- **@mariozechner/pi-ai**, **@mariozechner/pi-coding-agent** — same versions as the frontend.
- Targets Node.js ESM (NodeNext).

### Backend (`src-tauri/Cargo.toml`)
- **tauri 2** — desktop shell, IPC, window management.
- **rusqlite 0.33** (bundled) — SQLite.
- **tokio 1** (`rt-multi-thread`, `macros`) — async runtime.
- **reqwest 0.12** (`json`, `rustls-tls`) — HTTP for model discovery.
- **serde 1 / serde_json 1** — JSON over IPC.
- **dirs 6** — cross-platform config directory resolution.

### Tauri config (`src-tauri/tauri.conf.json`)
- Window: 1200×800, resizable.
- Dev URL: `http://localhost:1420`.
- Frontend dist: `../dist`.
- App identifier: `com.monarch.app`.

---

## 3. Architecture — three processes talking in JSON

```
┌──────────────────────────────────────────────────────┐
│  Svelte 5 UI                                         │
│  • agents[], activeId, event listeners               │
│  • renders streaming messages & tool calls           │
└──────────────────┬───────────────────────────────────┘
                   │  Tauri invoke() / listen()
                   ▼
┌──────────────────────────────────────────────────────┐
│  Rust backend (Tauri v2)                             │
│  • AgentManager  — spawns + routes sidecar           │
│  • Database      — SQLite (source of truth)          │
│  • ModelCache    — provider discovery                │
│  • PersistenceMgr — prompt files on disk             │
└──────────────────┬───────────────────────────────────┘
                   │  JSONL over sidecar stdin/stdout
                   ▼
┌──────────────────────────────────────────────────────┐
│  Node sidecar (long-lived)                           │
│  • RuntimeManager — one Pi AgentSession per agent    │
│  • Extension UI bridge — routes Pi dialogs to Rust   │
└──────────────────┬───────────────────────────────────┘
                   │  in-process API calls
                   ▼
┌──────────────────────────────────────────────────────┐
│  Pi SDK                                              │
│  • LLM loop, tool execution, thinking, extensions    │
└──────────────────────────────────────────────────────┘
```

Three processes at runtime: the Tauri window (Rust + embedded WebView for Svelte), and a single long-lived Node sidecar. The sidecar is **not** one process per agent — it hosts many Pi sessions in memory and routes them by `agentId`.

### How Rust finds the sidecar

At startup, Rust looks for `sidecar/dist/index.js` in this order (`src-tauri/src/agent.rs`):

1. `$MONARCH_SIDECAR_PATH` env var.
2. `./sidecar/dist/index.js` relative to cwd.
3. `../sidecar/dist/index.js` (one level up).
4. Relative to the Tauri binary location (for packaged builds).

**Gotcha:** `npm run tauri dev` will fail if `sidecar/dist/index.js` doesn't exist. Always `npm run build:sidecar` first.

---

## 4. Data model

SQLite lives at `~/.config/monarch/monarch.db` (XDG-ish — `dirs::config_dir()` on each OS). Schema is created in [`src-tauri/src/db.rs`](./src-tauri/src/db.rs).

### Tables

```sql
CREATE TABLE agents (
  id            TEXT PRIMARY KEY,
  name          TEXT NOT NULL,
  shadow_name   TEXT,
  shadow_title  TEXT,
  shadow_grade  TEXT,
  provider      TEXT,
  model         TEXT,
  thinking_level TEXT,
  cwd           TEXT,
  custom_prompt TEXT,              -- legacy, not used at runtime
  created_at    TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE sessions (
  id                TEXT PRIMARY KEY,
  agent_id          TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
  pi_session_file   TEXT,           -- legacy compatibility column, never read
  model             TEXT,
  provider          TEXT,
  started_at        TEXT NOT NULL DEFAULT (datetime('now')),
  ended_at          TEXT,
  message_count     INTEGER DEFAULT 0,
  total_tokens      INTEGER DEFAULT 0,
  total_cost        REAL    DEFAULT 0.0,
  parent_session_id TEXT REFERENCES sessions(id)  -- ancestry chain
);

CREATE TABLE messages (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  role       TEXT NOT NULL,         -- "user" | "assistant" | "toolResult"
  content    TEXT NOT NULL,         -- JSON, structure depends on role
  model      TEXT,
  tokens     INTEGER DEFAULT 0,
  cost       REAL    DEFAULT 0.0,
  timestamp  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE memories (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  agent_id      TEXT REFERENCES agents(id) ON DELETE CASCADE,
  layer         TEXT NOT NULL DEFAULT 'hot',   -- hot | warm | cold
  category      TEXT NOT NULL DEFAULT 'general',
  content       TEXT NOT NULL,
  relevance     REAL DEFAULT 1.0,
  created_at    TEXT NOT NULL DEFAULT (datetime('now')),
  last_accessed TEXT,
  access_count  INTEGER DEFAULT 0
);

CREATE TABLE events (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  agent_id   TEXT,
  session_id TEXT,
  event_type TEXT NOT NULL,
  data       TEXT,                  -- JSON dump of the full event
  timestamp  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_sessions_agent    ON sessions(agent_id);
CREATE INDEX idx_messages_session  ON messages(session_id);
CREATE INDEX idx_memories_agent    ON memories(agent_id);
CREATE INDEX idx_memories_layer    ON memories(layer);
CREATE INDEX idx_events_agent      ON events(agent_id);
CREATE INDEX idx_events_type       ON events(event_type);
```

### Session ancestry — the key concept

When the user **continues** a conversation, Monarch creates a **new session row** with `parent_session_id` pointing at the old one. `db::get_messages_with_ancestry(session_id)` walks the parent chain to the root and returns the full flattened history. This is used for:

- **UI display** — the agent view shows the full conversation across ancestor sessions.
- **Sidecar rehydration** — on restore or crash recovery, Rust replays all ancestor messages into the Pi session with a `load_session` command.

```
session-A ──► session-B ──► session-C          (parent_session_id chain)
                    │
             get_messages_with_ancestry(C)
                    ▼
         [A.msgs..., B.msgs..., C.msgs...]
```

### Legacy fields

- `agents.custom_prompt` — historical column; current code stores prompt overrides as files, not in the DB. See §8.
- `sessions.pi_session_file` — kept because Pi used to own session files; Monarch is now the session authority, so it's inert.

---

## 5. Agent (shadow) lifecycle

There are four lifecycle transitions worth knowing: **spawn**, **run**, **restore**, and **recover-from-crash**. They all converge on the same `create_session` + `load_session` sidecar command pair.

### 5.1 Spawn (new agent)

Frontend ([`src/App.svelte`](./src/App.svelte) `createAgent()`):

1. Builds an in-memory `Agent` object with a new id.
2. Calls `db_upsert_agent` and `db_create_session` (persist **before** anything else — FK safety).
3. Calls `spawn_agent` on the Rust backend.
4. Attaches listeners for `agent-event-{id}` and `agent-exit-{id}`.

Rust ([`src-tauri/src/agent.rs`](./src-tauri/src/agent.rs) `spawn_agent`):

1. Ensures the sidecar process is alive (`ensure_sidecar`).
2. Re-persists agent/session rows (defensive, in case frontend skipped it).
3. Records the `agent_id → session_id` mapping in `state.session_map`.
4. Builds a `create_session` JSON command including shadow identity and the custom prompt override (read from disk).
5. Sends it to the sidecar.
6. **Caches the create command JSON** in `AgentState.create_cmd_json` — this is replayed during crash recovery.

Sidecar ([`sidecar/src/runtime-manager.ts`](./sidecar/src/runtime-manager.ts) `createSession`):

1. Picks the system prompt: `customPrompt` if non-empty, otherwise `buildSystemPrompt(shadow, cwd)` from [`shadow-oath.ts`](./sidecar/src/shadow-oath.ts).
2. Creates a `DefaultResourceLoader` with `systemPromptOverride: () => promptRef.current` — Pi calls this closure, so runtime prompt edits are picked up automatically.
3. Creates an in-memory Pi `AgentSession` via `createAgentSession`.
4. Registers custom providers (LM Studio, OpenRouter) against the session.
5. Resolves and sets the model.
6. Binds Pi extensions to the extension UI bridge so Pi dialogs route to Rust → Svelte.
7. Subscribes to Pi events and forwards each one as `{ type: "event", agentId, event }`.
8. Emits `session_ready`.

### 5.2 Run (user sends a message)

1. `ChatInput.svelte` → `AgentView.svelte` → `invoke("send_command", { id, commandJson })`.
2. Rust injects `agentId` into the command and calls `send_with_recovery()`.
3. Sidecar routes by command type (`prompt`, `abort`, `set_model`, `set_thinking_level`, `compact`, `new_session`, `set_custom_prompt`, ...).
4. Pi runs the LLM loop, streaming events (`message_start` → `message_update` → `message_end`, `tool_execution_*`, `turn_*`).
5. Sidecar forwards every Pi event to Rust.
6. Rust's async event handler (`handle_sidecar_event`) does three things on each `event`-typed line:
   - Enqueues persistence effects on the bounded single-consumer mpsc pipeline (`PersistCommand`), which applies them in FIFO order by awaiting the async `Database` methods directly — no `spawn_blocking` hop, since `db.rs` runs on `tokio-rusqlite`.
   - Feeds the event into the per-agent `LiveAgentState::apply_event` state machine (`src-tauri/src/agent_state.rs`) — Rust owns turn assembly: streaming messages, tool-group stitching, `lastUsage`, `activityStatus`, etc.
   - Emits the assembled snapshot on `agent-state-{agent_id}` as a JSON-encoded string, with a 16ms debounce coalescing streaming `message_update`s (terminal events flush immediately).
7. Legacy `agent-event-{agent_id}` forwarding is still present for out-of-band signals only: `session_ready`, `sidecar_error`, and `extension_ui_request`. **Message and tool events are not consumed from this channel by the frontend anymore.** The raw `event` forward on this topic is pending removal (MON-14 follow-up).
8. `AgentView.svelte` uses a **pull-then-subscribe** pattern: on bind, `invoke("get_agent_state", { agentId })` seeds `liveAgentStore`, then `listen("agent-state-{id}")` applies incremental snapshots. Snapshots are reconciled by `stateVersion` — any incoming snapshot with `version <= entry.stateVersion` is dropped.

**Important:** the frontend **never** writes to the DB for conversation history and **never** assembles turn state. Rust owns persistence and the authoritative live view. The frontend is a passive receiver of Rust-assembled snapshots.

### 5.3 Restore (pick up a saved agent on startup)

1. App startup: `App.svelte` calls `db_get_agents` and populates `savedAgents[]`, shows the restore bar.
2. User clicks "Restore All" (or one specifically). `restoreAgent()` calls `createAgent()` with `reuseExistingSession: true` and a `sourceSessionId` pointing at the session to replay.
3. The normal spawn flow runs (steps 5.1), but using the existing session row instead of creating a new one.
4. `AgentView.bindAgent` calls `rebuild_agent_state_from_session` with the `sourceSessionId`, which loads messages from SQLite (following parent-session ancestry), rebuilds `LiveAgentState.items` via `display_items_from_messages`, replaces the in-memory entry, and returns the new snapshot. The frontend seeds its store from the returned value (and Rust also emits the same snapshot on `agent-state-{id}`).
5. When the sidecar emits `session_ready`, `AgentView` notices `sourceSessionId` was pending and calls `load_session_context` — Rust walks ancestry and sends a `load_session` command to inject messages into the sidecar's LLM context.
6. Sidecar's `loadSession` reconstructs messages (user / assistant / toolResult) and pushes them into `session.agent.state.messages` plus the session manager's persisted log.

### 5.4 Recover from sidecar crash

If the sidecar dies, Rust's async stdout reader task hits EOF. The next command triggers `send_with_recovery()`, which falls into `recover_sidecar()`:

1. Respawn the sidecar process.
2. Snapshot `agents` + `session_map`.
3. For each tracked agent:
   - Replay the cached `create_cmd_json` (same command that spawned it originally).
   - Walk ancestry in SQLite and replay the full message history via `load_session`.
   - Call `LiveAgentState::reset_with_items` with the rebuilt display items and emit a single snapshot on `agent-state-{id}`. Mid-stream state (partial streaming message, in-flight tool group) is intentionally dropped.
4. Retry the original failing command.

From the frontend's perspective, nothing happened: the `agent-state-{id}` listener picks up the rebuilt snapshot automatically, and the UI reflects it without a manual refresh.

### 5.5 Fully tokio-native backend (MON-14 + MON-27)

The backend is fully tokio-native. The sidecar runs on `tokio::process`, every `#[tauri::command]` in `agent.rs` is `async fn`, the write path is a direct `.await` into a `tokio::sync::Mutex<ChildStdin>` (no mpsc bridge, no dedicated writer task), and SQLite runs on `tokio-rusqlite` so every `Database` method is `async fn` and dispatches work onto the connection's dedicated background thread. `persistence.rs` prompt-file IO uses `tokio::fs`. The one remaining sync bridge is `AgentManager::shutdown_sidecar`, which is called from Tauri's sync `RunEvent::ExitRequested` hook and uses `tauri::async_runtime::block_on` to close the async-owned `ChildStdin` — an unavoidable compromise, since the Tauri hook itself is sync. The critical invariant for any new code is that `parking_lot::MutexGuard` must never be held across an `.await` (`inner`, `sidecar`, `app_handle`).

---

## 6. Sidecar protocol (JSONL)

One JSON object per line, both directions. The full schema lives in [`sidecar/src/protocol.ts`](./sidecar/src/protocol.ts).

### Rust → sidecar (commands)

| Command                   | Purpose |
|---------------------------|---------|
| `create_session`          | Spawn a Pi session for an agent. Includes provider, model, cwd, shadow identity, custom prompt. |
| `destroy_session`         | Clean up a session. |
| `prompt`                  | Send a user message. If already streaming, becomes a `followUp`. |
| `abort`                   | Cancel the in-flight turn. |
| `set_model`               | Switch model/provider at runtime. |
| `set_thinking_level`      | Pi-canonical `off` / `minimal` / `low` / `medium` / `high` / `xhigh`. The UI surfaces only the subset the current model supports and maps the wire value to the provider-native label (e.g. `xhigh` → "max" on Opus 4.6, uppercase on Gemini). See `src/lib/thinking.ts` and `~/.config/monarch/thinking.toml` for the per-model default table. |
| `new_session`             | Clear the conversation in-memory but keep the sidecar session alive. |
| `compact`                 | Ask Pi to compress the context. |
| `load_session`            | Inject an array of messages into the session (used on restore and recovery). |
| `set_custom_prompt`       | Replace the active system prompt; also updates the `promptRef` closure. |
| `extension_ui_response`   | Reply to a pending Pi extension UI request. |

### Sidecar → Rust (events)

| Event                     | Meaning |
|---------------------------|---------|
| `session_ready`           | Pi session is initialized and ready for `prompt`. |
| `session_destroyed`       | Session has been cleaned up. |
| `event`                   | Wrapper around a raw Pi SDK runtime event (`message_*`, `tool_execution_*`, `turn_*`, `queue_update`, `compaction_*`, …). |
| `extension_ui_request`    | Pi needs user input (select / confirm / input / editor). Includes a `requestId` for the eventual `extension_ui_response`. |
| `error`                   | Sidecar-level error (parse failure, model setup failure, ...). |

### Rust → frontend (Tauri events)

| Channel                   | Payload | Meaning |
|---------------------------|---------|---------|
| `agent-state-{id}`        | JSON-encoded `LiveAgentState` (string) | **Canonical assembled state.** Rust-owned turn assembly; streaming updates debounced to ~60fps, terminal events flush immediately. The frontend pulls via `get_agent_state` on bind and then subscribes for incremental snapshots, reconciling by `stateVersion`. |
| `agent-event-{id}`        | Varies (JSON) | Out-of-band signals only: `session_ready`, `sidecar_error`, `extension_ui_request`. **Message and tool forwarding on this channel is deprecated** — a follow-up issue removes the Rust-side emit after verifying no frontend subscribers remain. |
| `agent-exit-{id}`         | `number \| null` | Pi process exit code. |
| `agent-stderr-{id}`       | `string` | Per-line sidecar stderr, buffered into `agent.stderrLines`. |

`LiveAgentState` is defined in Rust at `src-tauri/src/agent_state.rs`; the TypeScript shape is generated via `specta` + `tauri-specta` into `src/lib/bindings.ts`. To regenerate after a Rust change, run `cargo run -- --export-bindings` from `src-tauri/` — the generated file is post-processed to route through `$lib/api` so the WS fallback still works in non-Tauri environments.

### Example message shapes

```json
{"type":"create_session","agentId":"agent-1","cwd":"/home/me/proj","provider":"anthropic","model":"claude-sonnet-4-5","thinkingLevel":"medium","shadow":{"name":"Aurora","title":"Scout","grade":"Knight","id":"agent-1"},"customPrompt":null}
```

```json
{"type":"prompt","agentId":"agent-1","message":"What does this function do?"}
```

```json
{"type":"event","agentId":"agent-1","event":{"type":"message_end","message":{"role":"assistant","content":[{"type":"text","text":"..."}],"model":"claude-sonnet-4-5","usage":{"input":150,"output":42,"cost":{"total":0.00159}},"stopReason":"stop","timestamp":1712761234000}}}
```

### Providers

Subscription-backed, auth from `~/.pi/agent/auth.json` (checked by `get_provider_auth_status`):
- `anthropic` — Claude (Opus, Sonnet, Haiku). Also works with `ANTHROPIC_API_KEY`.
- `openai-codex` — GPT via Pi's OpenAI Codex login; the model picker locks to the single supported ID.

Dynamic providers built in the sidecar by `buildDynamicModel` in [`sidecar/src/runtime-manager.ts`](./sidecar/src/runtime-manager.ts):
- `openrouter` — any model routed via `https://openrouter.ai/api/v1`. Requires `OPENROUTER_API_KEY` in the sidecar's environment.
- `lmstudio` — local server at `http://127.0.0.1:1234` (override with `LMSTUDIO_BASE_URL`; a trailing `/v1` on the env var is accepted for backward compatibility). The sidecar registers the provider on first use with a dummy API key, since LM Studio accepts any. The per-agent context window is auto-detected from LM Studio's native `/api/v0/models` endpoint — each listed model's `loaded_context_length` is surfaced read-only in the spawn dialog, then persisted on the `agents.context_window` column and forwarded to the sidecar as `CreateSessionCommand.contextWindow`. Only models LM Studio reports as currently loaded are listed, so every picker entry is something you can actually talk to right now. If the native endpoint isn't available (older LM Studio), discovery falls back to `/v1/models` and the sidecar's 32k default is used instead.

Model discovery lives in [`src-tauri/src/models.rs`](./src-tauri/src/models.rs) and is exposed as `get_models` and `get_provider_auth_status` Tauri commands. For `openrouter`, `get_models` hits the provider's `/models` endpoint directly. For `lmstudio`, it prefers the native `/api/v0/models` endpoint (filtering to `state == "loaded"` entries and threading `loaded_context_length` onto `ModelInfo.contextWindow`) and falls back to the OpenAI-compatible `/v1/models` path on any failure — which is already the loaded-models view, so no extra filtering is needed there. The LM Studio arm returns `Err(...)` when neither endpoint responds so the UI can show a distinct "provider unreachable" state.

---

## 7. Frontend layout

### Component tree

```
App.svelte                       — root: agents[], activeId, session restore, keybindings
├── Sidebar.svelte               — active + saved agents
├── SpawnDialog.svelte           — modal chrome for the new-agent flow
│   └── SpawnForm.svelte         — form body (shadow identity, cwd, save-as-template)
│       ├── TemplateSelector.svelte — load/apply/delete AgentTemplateRow chips
│       └── ModelSelector.svelte    — provider, model picker, LM Studio ctx, thinking
├── HistoryPanel.svelte          — session browser for a saved agent
├── AgentView.svelte             — main workspace per active agent
│   ├── AgentHeader.svelte       — name, model, shadow grade
│   ├── AgentControls.svelte     — thinking level, token/cost counter, abort
│   ├── MessageList.svelte       — rendered display items
│   │   ├── AssistantMessage.svelte  — text / thinking / tool call blocks
│   │   ├── ToolGroup.svelte         — groups of tool executions
│   │   └── ToolCallCard.svelte      — individual tool call + result
│   ├── ChatInput.svelte         — textarea, auto-resize, Enter to send
│   ├── MentionAutocomplete.svelte — @-mention file/folder dropdown (sibling to a textarea)
│   ├── PromptEditor.svelte      — modal to edit system prompt override
│   └── ExtensionDialog.svelte   — handles Pi extension UI requests
├── toolbox/ToolPanelStack.svelte — vertically stacked tool panels (resizable)
└── toolbox/ToolRail.svelte      — right-edge vertical icon strip
```

Shared types live in [`src/lib/types.ts`](./src/lib/types.ts). Toolbox types and
the live-state store live in [`src/lib/toolbox/`](./src/lib/toolbox/).

### State flow (Svelte 5 runes)

Top-level (`App.svelte`):

```ts
let agents:       Agent[]              = $state([]);
let activeId:     string | null        = $state(null);
let savedAgents:  SavedAgentInfo[]     = $state([]);
let openToolIds:  string[]             = $state(restoreOpenIds());
let toolboxWidth: number               = $state(restoreWidth());
```

Per-agent live conversation state (items, tool executions, streaming message,
etc.) lives in [`src/lib/toolbox/liveAgentStore.ts`](./src/lib/toolbox/liveAgentStore.ts)
— a module-level `$state({ byAgent: Map })`. AgentView writes to and reads
from the store exclusively, so the state survives the `{#key activeAgent.viewKey}`
remount and is visible to toolbox tools via `AgentContext.live`.

Per-agent `AgentView.svelte` keeps only genuinely UI-local state (streaming
flag, extension request, showStderr, modal open flags, listener handles).

Event flow from backend to UI:

1. Svelte calls `invoke("send_command", …)`.
2. Rust → sidecar → Pi → Pi events → Rust.
3. Rust persists to SQLite and emits `agent-event-{agentId}` on the Tauri event bus.
4. `AgentView` listens on that topic and writes to `liveAgentStore.byAgent.get(id)`.
5. Svelte's reactivity re-renders `AgentView` and any open toolbox tool.

### Adding a toolbox tool

The toolbox is a pluggable registry. Adding a tool = editing one registry file
plus creating one Svelte component. If the tool needs backend access, a typed
Tauri command is added alongside.

1. **Create the component** at `src/lib/toolbox/tools/YourTool.svelte`. It must
   accept exactly `{ agentContext }: ToolProps` — the import is
   `import type { ToolProps } from "../types";`. Derive display from
   `agentContext` reactively. `agentContext.live` exposes `items`,
   `toolExecutions`, `streamingMessage`, `lastUsage`, `currentToolGroup`,
   `activityStatus`, `eventCount` for the active agent.
2. **Register it** by appending a `ToolDefinition` entry to
   [`src/lib/toolbox/registry.ts`](./src/lib/toolbox/registry.ts) with a stable
   `id`, human `title`, inline SVG `icon` string, the imported component, and
   an optional `order` (lower = higher on the rail).
3. **(Optional) Backend commands.** Create
   `src-tauri/src/toolbox/your_tool.rs` with typed Tauri commands following
   the placeholder pattern (`#[tauri::command]` wrapper + `ws_*` wrapper
   calling a shared inner fn). Declare the submodule in
   `src-tauri/src/toolbox/mod.rs`, add the commands to the `invoke_handler!`
   in `src-tauri/src/lib.rs`, and add matching match arms to
   `ws::dispatch_command` in `src-tauri/src/ws.rs`. Add a `ToolDescriptor`
   to the list returned by `toolbox::descriptors()`.
4. **Never import `@tauri-apps/api` directly** from a tool. All `invoke`
   calls go through [`src/lib/api.ts`](./src/lib/api.ts) so the Tauri webview
   and the WS browser bridge both work.

**Tool-author constraint (important).** Toolbox tools stay mounted across
agent switches — intentionally, to avoid remount flicker. Any state a tool
keeps locally (expanded sections, scroll position, filter selections) will
appear to leak from one agent to the next. If your tool needs per-agent
memory, key that state by `agentContext.agentId` inside the component; the
framework will not remount on agent switch.

---

## 8. Prompt files

Prompt overrides live on disk, **not** in the DB.

```
~/.config/monarch/prompts/{agent_id}.md     # Linux
~/Library/Application Support/monarch/prompts/{agent_id}.md   # macOS
%APPDATA%\monarch\prompts\{agent_id}.md     # Windows
```

Resolved by [`src-tauri/src/persistence.rs`](./src-tauri/src/persistence.rs) using `dirs::config_dir().join("monarch").join("prompts")`.

Lifecycle:

1. **Spawn:** Rust reads `{agent_id}.md` if present and passes its contents as `customPrompt` in `create_session`. Empty / missing → `null`.
2. **Fallback:** the sidecar's `createSession` uses `buildSystemPrompt(shadow, cwd)` from [`sidecar/src/shadow-oath.ts`](./sidecar/src/shadow-oath.ts) when there's no override. The resulting prompt includes shadow identity (name, title, grade), current date, and cwd.
3. **Runtime edit:** `PromptEditor.svelte` → `save_agent_prompt` (writes the file) + `set_custom_prompt` command (patches the live session). The sidecar updates the mutable `promptRef.current` closure that `DefaultResourceLoader.systemPromptOverride` reads from, so all subsequent Pi prompt rebuilds see the new value.

The per-agent `.md` file is the source of truth for prompt overrides and is safe to edit externally.

---

## 9. Conventions and gotchas

From CLAUDE.md and reinforced by the code:

1. **Rust owns persistence.** The frontend never writes conversation history. All `messages`/`sessions` writes happen inside Rust's sidecar event handler.
2. **Use the sidecar protocol, not the Pi CLI.** There is no `pi --rpc` subprocess; the sidecar hosts Pi SDK sessions in-process.
3. **Session ancestry is canonical.** Continuing a conversation creates a new session row with `parent_session_id`. `get_messages_with_ancestry` is the only correct way to load history.
4. **Prompts are files.** `~/.config/monarch/prompts/{agent_id}.md`. The `custom_prompt` column on `agents` is legacy.
5. **Shadow identity is optional.** If a shadow is set, the default prompt comes from `buildSystemPrompt`. Custom prompts override it.
6. **Sidecar is singleton.** One Node process, many agents, keyed by `agentId`.

### Easy traps

- **Forgot to build the sidecar.** `npm run tauri dev` will fail to find `sidecar/dist/index.js`. Run `npm run build:sidecar` first (or as part of `npm run build`).
- **Stale cached view state.** If you change an agent config and the UI doesn't update, the `agentViewStates` cache in `App.svelte` may need invalidation.
- **Message content format drift.** Messages are stored as JSON strings. User messages may be plain text in older rows; assistant messages are arrays of content blocks; tool results are structured. The sidecar's `normalizeStoredUserContent` / `normalizeStoredAssistantContent` handle both shapes on replay.
- **Race conditions in `session_map`.** It's a `Mutex<HashMap>`. Always acquire with `.lock()?` before mutating from event handler threads.
- **The `pi_session_file` column is a trap.** It's never populated at runtime but still exists. Don't build anything on it.

---

## 10. Build & dev flow

### Prerequisites (from [README.md](./README.md))

- Node.js 18+ and npm.
- Rust toolchain (`rustup`).
- Tauri v2 system deps — see the [official prerequisites guide](https://v2.tauri.app/start/prerequisites/):
  - Linux: WebKitGTK, libappindicator, build-essential.
  - macOS: Xcode Command Line Tools.
  - Windows: MSVC Build Tools + WebView2 runtime.

### Commands

```bash
# first-time setup
npm install
npm install --prefix sidecar

# dev (required once each time sidecar TS changes)
npm run build:sidecar
npm run tauri dev

# production build
npm run build        # build:sidecar + build:web
npm run tauri build  # package desktop binary
```

### Where things land

- Frontend dev server: `http://localhost:1420`
- Sidecar compiled: `sidecar/dist/index.js`
- SQLite DB: `~/.config/monarch/monarch.db`
- Prompt files: `~/.config/monarch/prompts/`
- Production bundle: `src-tauri/target/release/bundle/` (OS-specific: `.app`, `.AppImage`, `.msi`, ...)

---

## 11. What's not implemented yet

A quick map of the delta between [VISION.md](./VISION.md) and reality. Not exhaustive — read the vision doc for the full picture.

| Area | Status | Notes |
|---|---|---|
| Multi-agent delegation & hierarchy | ❌ | Agents are flat; no parent/child or role-based dispatch. |
| Tool-call interception & approval flows | ❌ | Events flow through Rust but there's no gate to pause a tool call. Tracked under the *Agent loop* project in Linear. |
| Memory keeper / layered memory | ❌ | The `memories` table exists but nothing writes to it. Tracked under *Memory & context tools*. |
| Context inspector / manipulation UI | ❌ | No way to see what Pi actually has in context. Tracked under *Memory & context tools*. |
| Time travel / branching UI | ⚠️ Partial | Session ancestry supports branching in the data model, but no UI for rewind/fork. |
| Headless loop / mobile / remote | ❌ | Tauri desktop only. No web server, no tunnel. |
| Git worktree per agent | ❌ | Agents share the spawn-time cwd. |
| Cost budgeting | ⚠️ Partial | Per-message token/cost fields exist but aren't enforced. |
| Voice input | ❌ | Not wired. |
| Command palette | ❌ | A few keybindings (`Ctrl+N`, `Ctrl+B`, `Ctrl+L`, `Ctrl+1-9`) but no fuzzy palette. |
| Auto-compaction | ⚠️ Partial | The `compact` command wires through to Pi but no auto-trigger or UI surface. |

The Linear board has **Agent loop** and **Memory & context tools** projects with milestones covering most of these gaps.

---

## 12. File-path reference

### Rust backend — [`src-tauri/src/`](./src-tauri/src/)

| File | Role |
|---|---|
| `main.rs` | Tauri entry point. |
| `lib.rs` | Command registry, plugin setup, state init. |
| `agent.rs` | Sidecar lifecycle, `spawn_agent`, `send_command`, event handler, crash recovery. |
| `db.rs` | SQLite schema, CRUD, ancestry walk. |
| `models.rs` | Provider discovery, model listing, auth status. |
| `mention.rs` | `list_paths` command — walks cwd for the @-mention file/folder autocomplete (ignore-crate + nucleo-matcher). |
| `persistence.rs` | Prompt file I/O under `~/.config/monarch/prompts/`. |
| `toolbox/mod.rs` | Toolbox `ToolDescriptor` list, `toolbox_list_tools` command. |
| `toolbox/placeholder.rs` | Placeholder tool's `toolbox_placeholder_ping` command. |
| `ws.rs` | WebSocket bridge for browser-hosted UI (mirrors the Tauri command set). |

### Sidecar — [`sidecar/src/`](./sidecar/src/)

| File | Role |
|---|---|
| `index.ts` | Stdin/stdout JSONL loop. |
| `runtime-manager.ts` | Session registry, Pi `AgentSession` management. |
| `protocol.ts` | Command + event type definitions. |
| `shadow-oath.ts` | `buildSystemPrompt(shadow, cwd)` for default prompts. |
| `ui-bridge.ts` | Extension UI request/response routing. |

### Frontend — [`src/`](./src/) and [`src/lib/`](./src/lib/)

| File | Role |
|---|---|
| `App.svelte` | Shell: agent list, active id, restore flow, keybindings. |
| `main.ts` | Svelte mount. |
| `lib/types.ts` | Shared TypeScript types. |
| `lib/Sidebar.svelte` | Agent list + saved agents. |
| `lib/SpawnDialog.svelte` | Modal shell for the new-agent flow. |
| `lib/SpawnForm.svelte` | Form body: shadow identity, cwd, save-as-template, handleSpawn. |
| `lib/TemplateSelector.svelte` | Template chip row, loads via `db_list_agent_templates`. |
| `lib/ModelSelector.svelte` | Provider, model picker, auth status, thinking level, LM Studio context — reusable. |
| `lib/providers.ts` | `PROVIDERS`, `REFRESHABLE_PROVIDERS`, `THINKING_LEVELS` catalogue. |
| `lib/HistoryPanel.svelte` | Session browser. |
| `lib/AgentView.svelte` | Per-agent workspace + event listeners. |
| `lib/AgentHeader.svelte` | Name / model / grade header. |
| `lib/AgentControls.svelte` | Thinking level, token counter, abort. |
| `lib/MessageList.svelte` | Display items renderer. |
| `lib/AssistantMessage.svelte` | Content blocks (text / thinking / tool calls). |
| `lib/ToolGroup.svelte` | Groups of tool calls. |
| `lib/ToolCallCard.svelte` | Single tool call + result. |
| `lib/ChatInput.svelte` | Message composer. |
| `lib/MentionAutocomplete.svelte` | `@`-triggered file/folder suggestion dropdown attached to a textarea (MON-76). |
| `lib/PromptEditor.svelte` | System prompt override dialog. |
| `lib/ExtensionDialog.svelte` | Handles Pi extension UI requests. |
| `lib/api.ts` | Unified `invoke` / `listen` wrapper — Tauri or WebSocket. |
| `lib/toolbox/types.ts` | `ToolDefinition`, `ToolProps`, `AgentContext`, `LiveAgentState`. |
| `lib/toolbox/registry.ts` | The `TOOLS` array — edit to add a tool. |
| `lib/toolbox/liveAgentStore.ts` | Shared per-agent live-state store (`byAgent` Map). |
| `lib/toolbox/persistence.ts` | localStorage helpers for rail width + open ids. |
| `lib/toolbox/ToolRail.svelte` | Right-edge vertical icon strip. |
| `lib/toolbox/ToolPanelStack.svelte` | Stacked panel region left of the rail. |
| `lib/toolbox/tools/PlaceholderTool.svelte` | Sample tool exercising the full store + backend path. |

### Config

| File | Role |
|---|---|
| [`src-tauri/tauri.conf.json`](./src-tauri/tauri.conf.json) | Window + dev URL + dist path. |
| [`src-tauri/Cargo.toml`](./src-tauri/Cargo.toml) | Rust deps. |
| [`package.json`](./package.json) | Frontend scripts + deps. |
| [`sidecar/package.json`](./sidecar/package.json) | Sidecar build script. |
| [`vite.config.ts`](./vite.config.ts) | Vite dev server + build. |
| [`svelte.config.js`](./svelte.config.js) | Svelte preprocessor. |
| [`tsconfig.json`](./tsconfig.json) | Frontend TS. |

---

## Mental model, in one paragraph

Monarch is three processes stapled together with JSON. The **frontend** is a view that renders events. The **Rust backend** owns identity, persistence, and lifecycle — it writes to SQLite, spawns and supervises the sidecar, and routes commands and events between the UI and the sidecar. The **Node sidecar** hosts Pi SDK sessions in memory and is replaceable — on crash it gets respawned and replayed from SQLite. Pi is the execution engine, not the session authority. If you're ever unsure where a piece of state should live: Rust owns it, the frontend displays it, the sidecar operates on it. When in doubt, read [`src-tauri/src/agent.rs`](./src-tauri/src/agent.rs) and [`sidecar/src/runtime-manager.ts`](./sidecar/src/runtime-manager.ts) side by side — they're the contract.
