# Monarch

Multi-agent command center built on Tauri v2 + Svelte 5 + Pi coding agent. Shadow army themed (Solo Leveling inspired).

## Project Overview

Monarch is a desktop app for managing a fleet of AI coding agents ("shadows"). Each agent runs as a Pi RPC subprocess (`pi --mode rpc`), communicating via JSONL over stdin/stdout. Agents have persistent identities, custom system prompts (the Shadow Oath), and session history.

See `VISION.md` for the full concept and `FEATURES.md` for phased feature breakdown + Pi RPC protocol reference.

## Tech Stack

- **Rust** (Tauri v2 backend): process management, Pi subprocess spawning, JSONL parsing, IPC, model fetching, persistence
- **Svelte 5 + TypeScript** (frontend): agent UI, message rendering, controls
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
│       ├── models.rs            # Model fetching (OpenRouter API + hardcoded Anthropic/OpenAI)
│       └── persistence.rs       # Agent registry, session history, prompt storage
├── src/
│   ├── main.ts                  # Svelte mount
│   ├── global.css               # CSS variables, Oxocarbon theme
│   ├── App.svelte               # Root layout, agent lifecycle, persistence
│   └── lib/
│       ├── types.ts             # Shared TypeScript types
│       ├── Sidebar.svelte       # Agent list (active + saved history)
│       ├── AgentView.svelte     # Main agent chat view + event handling
│       ├── AgentHeader.svelte   # Agent name/title bar + overflow menu
│       ├── AgentControls.svelte # Model tag, thinking picker, tokens, abort
│       ├── MessageList.svelte   # Message rendering
│       ├── AssistantMessage.svelte # Markdown rendering + copy button
│       ├── ToolCallCard.svelte  # Tool execution display
│       ├── ChatInput.svelte     # Message input textarea
│       ├── SpawnDialog.svelte   # Extract Shadow dialog with model picker
│       ├── CouncilView.svelte   # Multi-agent parallel response view
│       ├── PromptEditor.svelte  # System prompt viewer/editor per agent
│       ├── HistoryPanel.svelte  # Session history browser
│       └── ExtensionDialog.svelte # Pi extension UI request handler
├── extensions/
│   └── shadow-oath.ts           # Pi extension: replaces system prompt with shadow identity
├── prompts/
│   └── shadow-oath-v1.md        # Archived v1 oath (ceremonial, too verbose)
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
│  - Agent persistence + session history  │
│  - Prompt file storage                  │
└──────────────┬──────────────────────────┘
               │ stdin/stdout JSONL
┌──────────────▼──────────────────────────┐
│  Pi RPC Subprocess (per agent)          │
│  - LLM calls, tool execution           │
│  - Session management                   │
│  - Shadow Oath extension loaded         │
└─────────────────────────────────────────┘
```

## Current State (Checkpoint 1)

### Working
- Agent spawning with shadow identity (name, title, grade)
- Shadow Oath extension — fully replaces Pi's system prompt
- Custom system prompts per agent (editable from UI, saved to ~/.config/monarch/prompts/)
- Multi-provider model picker (OpenRouter API fetch + Anthropic/OpenAI hardcoded)
- Fuzzy search model filtering
- Message streaming and display (markdown, thinking blocks, tool calls)
- Council mode — broadcast prompt to multiple agents, side-by-side responses
- Session persistence — agents survive app restarts
- Session history — past conversations tracked per agent
- Agent header with overflow menu (prompt, history, compact, new session)
- Activity status bar (shows what agent is doing: "Calling LLM...", "Running tool: bash", etc.)
- Error surfacing — failed commands show as red notifications, not swallowed
- Compact error view for failed extractions
- Copy button on messages, selectable text throughout
- Keyboard shortcuts: Ctrl+N extract, Ctrl+B sidebar, Ctrl+L council, Ctrl+1-9 switch, Ctrl+C copy/abort
- HMR enabled for frontend development

### Data Storage
- `~/.config/monarch/agents.json` — agent registry with session history
- `~/.config/monarch/prompts/{agent-id}.md` — custom prompt overrides
- Pi session files at `~/.pi/agent/sessions/` — message history (JSONL)

### Known Issues / TODO
- Session restore sometimes misses messages (timing between spawn and get_messages)
- OpenRouter models that Pi doesn't recognize get a warning
- Some models hang (OpenRouter free tier reliability)
- No Anthropic/OpenAI API keys configured (only OpenRouter works via env var)

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

### Building
```bash
npm run build
cargo tauri build
```

## Conventions

### Rust (src-tauri/)
- Tauri commands are the API surface between frontend and backend
- Each major feature gets its own module (agent.rs, models.rs, persistence.rs)
- Use `tauri::Emitter` to push events to frontend, `#[tauri::command]` for frontend-callable functions
- JSONL parsing: one JSON object per line from Pi stdout, split on `\n`
- Wrap non-Send/Sync types in Mutex for Tauri state
- Use `serde(rename_all = "camelCase")` on structs that cross the IPC boundary

### Svelte (src/)
- Svelte 5 runes syntax (`$state`, `$props`, `$effect`, `$derived`)
- Components in `src/lib/`
- Types in `src/lib/types.ts`
- Tauri IPC: `invoke()` for commands, `listen()` for events from Rust
- Oxocarbon-inspired dark theme (custom purple palette)
- JetBrainsMono Nerd Font

### Shadow Oath Extension (extensions/)
- Loaded via `--extension` flag on Pi spawn
- Reads identity from env vars (SHADOW_NAME, SHADOW_TITLE, SHADOW_GRADE, SHADOW_ID)
- Completely replaces Pi's system prompt via `before_agent_start` event
- Checks for custom prompt file at ~/.config/monarch/prompts/{id}.md
- Falls back to generated oath if no custom prompt exists

### Pi Integration
- Each agent = one `pi --mode rpc` subprocess
- Send commands as JSONL to stdin: `{"type": "prompt", "message": "..."}\n`
- Receive events as JSONL from stdout: `{"type": "message_update", ...}\n`
- All Pi RPC types documented in `FEATURES.md`
- Pi source at `../pi-mono` for reference — don't modify it
- Session restore via `--session <path>` flag on spawn

### General
- Dark theme throughout — custom purple palette
- Keyboard-first — every action should have a keybinding
- Don't add features beyond what's being implemented — the vision is big, build incrementally
- Shadows live their identity — they don't explain it. Keep system prompts lean.
