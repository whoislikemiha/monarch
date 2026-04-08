# Monarch

Multi-agent command center built on Tauri v2 + Svelte 5 + Pi coding agent. Shadow army themed (Solo Leveling inspired).

## Project Overview

Monarch is a desktop app for managing a fleet of AI coding agents ("shadows"). Each agent runs as a Pi RPC subprocess (`pi --mode rpc`), communicating via JSONL over stdin/stdout. Agents have persistent identities, custom system prompts (the Shadow Oath), full session history, and real-time SQLite persistence.

See `VISION.md` for the full concept and `FEATURES.md` for phased feature breakdown + Pi RPC protocol reference.

## Tech Stack

- **Rust** (Tauri v2 backend): process management, Pi subprocess spawning, JSONL parsing, IPC, model fetching, SQLite persistence
- **Svelte 5 + TypeScript** (frontend): agent UI, message rendering, controls
- **SQLite** (via rusqlite): agents, sessions, messages, memories, audit trail
- **Pi** (`@mariozechner/pi-coding-agent`): agent harness, LLM orchestration, tool execution
- **Pi source**: `../pi-mono` (local clone, read-only reference for understanding internals)

## Project Structure

```
monarch/
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/default.json
│   └── src/
│       ├── main.rs              # Tauri entry point
│       ├── lib.rs               # Tauri setup, command registration
│       ├── agent.rs             # Agent spawning, lifecycle, IPC
│       ├── db.rs                # SQLite database (schema, CRUD, all tables)
│       ├── models.rs            # Model fetching (OpenRouter API + hardcoded Anthropic/OpenAI)
│       └── persistence.rs       # Legacy JSON persistence + prompt files (being replaced by db.rs)
├── src/
│   ├── main.ts                  # Svelte mount
│   ├── global.css               # CSS variables, custom purple theme
│   ├── App.svelte               # Root layout, agent lifecycle, DB persistence
│   └── lib/
│       ├── types.ts             # Shared TypeScript types
│       ├── Sidebar.svelte       # Agent list (active + saved history)
│       ├── AgentView.svelte     # Main agent chat view + event handling + DB writes
│       ├── AgentHeader.svelte   # Agent name/title bar + overflow menu (prompt, history, compact, new session)
│       ├── AgentControls.svelte # Model tag, thinking dropdown, tokens, abort
│       ├── MessageList.svelte   # Message rendering (plain text streaming, markdown on complete)
│       ├── AssistantMessage.svelte # Markdown rendering + copy button
│       ├── ToolGroup.svelte     # Grouped tool calls per turn (collapsible)
│       ├── ToolCallCard.svelte  # Individual tool execution display (inside ToolGroup)
│       ├── ChatInput.svelte     # Message input textarea
│       ├── SpawnDialog.svelte   # Extract Shadow dialog with model picker + fuzzy search
│       ├── CouncilView.svelte   # Multi-agent parallel response view
│       ├── PromptEditor.svelte  # System prompt viewer/editor per agent
│       ├── HistoryPanel.svelte  # Session history browser (reads from DB, falls back to Pi JSONL)
│       └── ExtensionDialog.svelte # Pi extension UI request handler
├── extensions/
│   └── shadow-oath.ts           # Pi extension: completely replaces system prompt with shadow identity
├── prompts/
│   └── shadow-oath-v1.md        # Archived v1 oath (ceremonial, replaced — too verbose)
├── VISION.md                    # Full project vision and concept
├── FEATURES.md                  # Phased features, Pi RPC protocol, open questions
├── package.json
├── vite.config.ts
└── tsconfig.json
```

## Architecture

```
┌─────────────────────────────────────────┐
│  Svelte Frontend (WebView)              │
│  - Message rendering, chat input        │
│  - Agent sidebar + history              │
│  - Council mode, prompt editor          │
│  - Writes to DB on every event          │
│                                         │
│  Communicates with Rust via Tauri IPC   │
│  (invoke commands, listen to events)    │
└──────────────┬──────────────────────────┘
               │ Tauri IPC
┌──────────────▼──────────────────────────┐
│  Rust Backend (Tauri)                   │
│  - Spawns/manages Pi RPC subprocesses   │
│  - Forwards JSONL events to frontend    │
│  - Model fetching (OpenRouter API)      │
│  - SQLite: agents, sessions, messages,  │
│    memories, events                     │
└──────────────┬──────────────────────────┘
               │ stdin/stdout JSONL
┌──────────────▼──────────────────────────┐
│  Pi RPC Subprocess (per agent)          │
│  - LLM calls, tool execution           │
│  - Session management                   │
│  - Shadow Oath extension loaded         │
└─────────────────────────────────────────┘
```

## Database (SQLite)

Located at `~/.config/monarch/monarch.db`. WAL mode enabled.

| Table | Purpose | Writes |
|-------|---------|--------|
| `agents` | Persistent shadow identities + config | On creation, get_state, agent_end, stats |
| `sessions` | Every conversation linked to an agent | On get_state (captures session file), stats updates |
| `messages` | All user/assistant/tool messages | On message_start (user), message_end (assistant), tool_execution_end |
| `memories` | Layered memory system (core/hot/warm/cold) | Ready — not yet wired |
| `events` | Full audit trail of every Pi event | On every Pi event (full JSON payload) |

Nothing gets lost. Every meaningful state change triggers an immediate DB write. No timers or polling.

## Current State (Checkpoint 2)

### Working
- Agent spawning with shadow identity (name, title, grade)
- Shadow Oath v2 — fully replaces Pi's system prompt, lean ("live it, don't explain it")
- Custom system prompts per agent (editable from UI, saved to ~/.config/monarch/prompts/)
- Multi-provider model picker (OpenRouter API fetch + Anthropic/OpenAI hardcoded, 1hr cache)
- Fuzzy search model filtering (space-separated terms, each must match)
- Smooth token streaming (plain text during stream, markdown on complete)
- Tool calls grouped per turn into collapsible ToolGroup cards
- Council mode — broadcast prompt to multiple agents, side-by-side responses
- SQLite persistence — agents, sessions, messages, events all saved in real-time
- Session history — browsable from sidebar (saved agents) or agent header menu
- Agent header with overflow menu (prompt, history, compact, new session, + new chat button)
- Thinking level dropdown picker (not cycle)
- Activity status bar (shows what agent is doing)
- Error surfacing — failed commands show as red notifications
- Compact error view for failed extractions (no empty chat UI)
- Copy button on messages, selectable text throughout, Ctrl+C copies when text selected
- Keyboard shortcuts: Ctrl+N extract, Ctrl+B sidebar, Ctrl+L council, Ctrl+1-9 switch, Ctrl+C copy/abort
- HMR enabled for frontend development
- Sidebar split: Active agents + History (saved agents with session counts)

### Data Storage
- `~/.config/monarch/monarch.db` — SQLite: agents, sessions, messages, memories, events
- `~/.config/monarch/prompts/{agent-id}.md` — custom prompt overrides (read by shadow-oath extension)
- `~/.config/monarch/agents.json` — legacy JSON persistence (kept for migration, being phased out)
- Pi session files at `~/.pi/agent/sessions/` — Pi's own persistence (we read but don't depend on)

### Known Issues / TODO
- OpenRouter models that Pi doesn't recognize get a warning (Pi issue, harmless)
- Some models hang (OpenRouter free tier reliability)
- Session restore message loading could be more reliable
- Need to load messages from DB on restore instead of get_messages command

## Development

### Prerequisites
- Rust (stable)
- Node.js 20.6+
- Pi coding agent installed (`npm i -g @mariozechner/pi-coding-agent`)
- WebKitGTK dev libs (Arch: `webkit2gtk-4.1`)
- `OPENROUTER_API_KEY` environment variable set

### Running
```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 GDK_BACKEND=x11 cargo tauri dev
```
The env vars are needed for NVIDIA + Wayland (Hyprland).

Frontend changes hot reload via Vite HMR. Rust changes require app restart (cargo recompiles automatically on `cargo tauri dev`).

### Building
```bash
npm run build
cargo tauri build
```

## Conventions

### Rust (src-tauri/)
- Tauri commands are the API surface between frontend and backend
- Each major feature gets its own module (agent.rs, db.rs, models.rs)
- Use `tauri::Emitter` to push events to frontend, `#[tauri::command]` for frontend-callable functions
- JSONL parsing: one JSON object per line from Pi stdout, split on `\n`
- Wrap non-Send/Sync types in Mutex for Tauri state
- Use `serde(rename_all = "camelCase")` on structs that cross the IPC boundary
- SQLite accessed via `Database` struct with `Mutex<Connection>`

### Svelte (src/)
- Svelte 5 runes syntax (`$state`, `$props`, `$effect`, `$derived`)
- Components in `src/lib/`
- Types in `src/lib/types.ts`
- Tauri IPC: `invoke()` for commands, `listen()` for events from Rust
- Custom purple dark theme
- JetBrainsMono Nerd Font
- DB writes happen in AgentView on events, and in App.svelte on agent creation

### Shadow Oath Extension (extensions/)
- Loaded via `--extension` flag on Pi spawn (absolute path resolved from cwd/exe)
- Reads identity from env vars (SHADOW_NAME, SHADOW_TITLE, SHADOW_GRADE, SHADOW_ID)
- Completely replaces Pi's system prompt via `before_agent_start` event
- Checks for custom prompt file at ~/.config/monarch/prompts/{id}.md first
- Falls back to generated lean oath if no custom prompt exists
- Key directive: "You live your identity — you don't explain it"

### Pi Integration
- Each agent = one `pi --mode rpc` subprocess
- Send commands as JSONL to stdin: `{"type": "prompt", "message": "..."}\n`
- Receive events as JSONL from stdout: `{"type": "message_update", ...}\n`
- All Pi RPC types documented in `FEATURES.md`
- Pi source at `../pi-mono` for reference — don't modify it
- Session restore via `--session <path>` flag on spawn
- Pi's default system prompt is fully replaced by the Shadow Oath extension

### General
- Dark theme throughout — custom purple palette
- Keyboard-first — every action should have a keybinding
- Don't add features beyond what's being implemented — the vision is big, build incrementally
- Shadows live their identity — they don't explain it. Keep system prompts lean.
- Save everything to DB immediately — no timers, no data loss
