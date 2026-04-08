# Monarch — Features & Roadmap

## Tech Stack

### Desktop Shell
- **Tauri v2** — Rust backend + webview frontend
- **Svelte 5** — frontend UI framework (runes syntax)
- **TypeScript** — frontend language

### Agent Harness
- **Pi** (`@mariozechner/pi-coding-agent`) — LLM orchestration, tool execution, sessions, extensions
- **Integration: Pi RPC mode** (`pi --mode rpc`) — JSONL over stdin/stdout per agent subprocess
- **Future: Pi SDK** — import Pi as a library via Node sidecar for full programmatic control

### Local Pi Repo
- Located at `../pi-mono`
- Packages: `agent`, `ai`, `coding-agent`, `tui`, `web-ui`, `mom`, `pods`

---

## Pi RPC Protocol Reference

Each agent = one `pi --mode rpc` subprocess. Communication is JSONL (one JSON object per line, LF-only framing) over stdin/stdout.

### Commands (Monarch → Pi stdin)

All commands support optional `id` field for request/response correlation.

| Command | Key Fields | Notes |
|---------|-----------|-------|
| **Prompting** | | |
| `prompt` | `message`, `images?`, `streamingBehavior?` | Async — events stream back. `streamingBehavior`: `"steer"` or `"followUp"` |
| `steer` | `message`, `images?` | Inject message mid-run |
| `follow_up` | `message`, `images?` | Queue for after current run |
| `abort` | — | Stop current execution |
| **State** | | |
| `get_state` | — | Returns model, thinking level, session info, streaming status |
| **Model** | | |
| `set_model` | `provider`, `modelId` | Switch model |
| `cycle_model` | — | Cycle to next model |
| `get_available_models` | — | List all models |
| **Thinking** | | |
| `set_thinking_level` | `level` | off/minimal/low/medium/high/xhigh |
| `cycle_thinking_level` | — | Cycle to next level |
| **Queue Modes** | | |
| `set_steering_mode` | `mode` | `"all"` or `"one-at-a-time"` |
| `set_follow_up_mode` | `mode` | `"all"` or `"one-at-a-time"` |
| **Compaction** | | |
| `compact` | `customInstructions?` | Trigger context compaction |
| `set_auto_compaction` | `enabled` | Toggle auto-compaction |
| **Retry** | | |
| `set_auto_retry` | `enabled` | Toggle auto-retry |
| `abort_retry` | — | Cancel pending retry |
| **Bash** | | |
| `bash` | `command` | Execute shell command directly |
| `abort_bash` | — | Cancel running bash |
| **Session** | | |
| `get_session_stats` | — | Token counts, costs |
| `get_messages` | — | Get all messages in session |
| `new_session` | `parentSession?` | Create new session |
| `switch_session` | `sessionPath` | Switch to existing session |
| `fork` | `entryId` | Fork from specific message |
| `get_fork_messages` | — | Get forkable message list |
| `set_session_name` | `name` | Name current session |
| `get_last_assistant_text` | — | Last assistant response text |
| `export_html` | `outputPath?` | Export session as HTML |
| **Meta** | | |
| `get_commands` | — | List available slash commands |

### Events (Pi stdout → Monarch)

| Event | Key Fields | When |
|-------|-----------|------|
| `agent_start` | — | Agent begins processing |
| `agent_end` | `messages` | Agent finished |
| `turn_start` | — | One LLM call begins |
| `turn_end` | `message`, `toolResults` | One LLM call complete |
| `message_start` | `message` | Any message begins |
| `message_update` | `message`, `assistantMessageEvent` | Streaming chunks |
| `message_end` | `message` | Message complete |
| `tool_execution_start` | `toolCallId`, `toolName`, `args` | Tool begins |
| `tool_execution_update` | `toolCallId`, `partialResult` | Tool streaming |
| `tool_execution_end` | `toolCallId`, `result`, `isError` | Tool complete |
| `extension_ui_request` | `id`, `method`, various fields | Extension needs user input |
| `compaction_start` | `reason` | Context compaction starting |
| `compaction_end` | `result` | Context compaction done |
| `auto_retry_start` | `attempt` | Auto-retry beginning |
| `auto_retry_end` | `attempt` | Auto-retry complete |
| `queue_update` | `steering[]`, `followUp[]` | Message queue changed |
| `response` | `command`, `success`, `data?`, `error?` | Command acknowledgment |

### Extension UI Requests

Extensions can request UI interaction via special events. Monarch must handle these for extensions to work.

| Method | Fields | Expected Response |
|--------|--------|-------------------|
| `select` | `title`, `options[]`, `timeout?` | `{ value: string }` |
| `confirm` | `title`, `message`, `timeout?` | `{ confirmed: boolean }` |
| `input` | `title`, `placeholder?`, `timeout?` | `{ value: string }` |
| `editor` | `title`, `prefill?` | `{ value: string }` |
| `notify` | `message`, `notifyType?` | Fire-and-forget (no response needed) |
| `setStatus` | `statusKey`, `statusText` | Fire-and-forget |
| `setWidget` | `widgetKey`, `widgetLines[]`, `widgetPlacement?` | Fire-and-forget |
| `setTitle` | `title` | Fire-and-forget |
| `set_editor_text` | `text` | Fire-and-forget |

Response format: `{ type: "extension_ui_response", id: "<matching-request-id>", ... }`
Cancel format: `{ type: "extension_ui_response", id: "<matching-request-id>", cancelled: true }`

### Key Message Types

```typescript
// Assistant message content blocks
TextContent     { type: "text", text: string }
ThinkingContent { type: "thinking", thinking: string, redacted?: boolean }
ToolCall        { type: "toolCall", id: string, name: string, arguments: Record<string, any> }

// Streaming deltas (in assistantMessageEvent)
text_delta      { type: "text_delta", delta: string, contentIndex: number }
thinking_delta  { type: "thinking_delta", delta: string, contentIndex: number }
toolcall_delta  { type: "toolcall_delta", delta: string, contentIndex: number }

// Usage/cost info on every assistant message
Usage { input, output, cacheRead, cacheWrite, totalTokens, cost: { total, ... } }
```

### Spawning an RPC Agent

```bash
pi --mode rpc \
  --provider anthropic \
  --model claude-sonnet-4-5 \
  --thinking medium \
  --tools read,bash,edit,write,grep,find,ls \
  --extension ./custom-extension.ts \
  --session-dir ./sessions/agent-1
```

No handshake needed. Send `get_state` first to confirm alive and sync actual state.

### Pi Extension System

Extensions are TypeScript modules loaded via `--extension` flag. Key APIs available to extensions:

```typescript
// Registration
api.registerTool(toolDefinition)      // Add LLM-callable tool
api.registerCommand(name, options)     // Add slash command
api.registerShortcut(keyId, options)   // Add keyboard shortcut
api.registerFlag(name, options)        // Add CLI flag

// Events (subscribe to agent lifecycle)
api.on("agent_start", handler)
api.on("agent_end", handler)
api.on("tool_execution_start", handler)
api.on("tool_call", handler)          // Per-tool: bash, read, edit, etc.
api.on("context", handler)            // Inject system prompt context
// ... full lifecycle coverage

// Actions
api.sendMessage(message)              // Send message to agent
api.sendUserMessage(message)          // Send as user message
api.exec(command, args, options)       // Execute commands

// Context available in handlers
ctx.ui          // UI interaction (dialogs, status, widgets)
ctx.cwd         // Working directory
ctx.model       // Current model
ctx.isIdle()    // Agent idle state
ctx.abort()     // Abort current run
ctx.compact()   // Trigger compaction
ctx.getSystemPrompt()  // Read system prompt
```

This is the integration point for Monarch's orchestrator tools (P2.1).

---

## Phase 1 — Single-Agent Polish

The core RPC loop works. Phase 1 is about making a single agent feel solid, reliable, and pleasant to use daily. Everything here gates Phase 2.

### P1.1: State Sync ✦ HIGH PRIORITY
After spawning, Monarch shows what was *configured*, not what Pi *actually loaded*.

- Send `get_state` immediately after spawn, populate UI from response
- Send `get_available_models` and populate model picker with real options
- Handle `response` events for command confirmations
- Sync on model/thinking changes (Pi may reject or modify)

### P1.2: Extension UI Handling ✦ HIGH PRIORITY
Pi extensions emit `extension_ui_request` events for user interaction. Without handling these, **any extension that needs a dialog silently fails**. This directly blocks the orchestrator extension (P2.1).

- Handle `select` → dropdown/menu in UI
- Handle `confirm` → yes/no dialog
- Handle `input` → text input dialog
- Handle `editor` → multiline text editor dialog
- Handle `notify` → toast notification
- Handle `setStatus` → status indicator per agent
- Handle `setWidget` → custom widget display
- Handle `setTitle` → update agent tab title
- Send `extension_ui_response` with matching `id` back to Pi
- Send `cancelled: true` if user dismisses

### P1.3: Error Handling & Agent Lifecycle ✦ HIGH PRIORITY
If Pi crashes, the agent goes silent with no feedback. Basic reliability.

- Detect Pi process exit (already have `agent-exit` event) → show clear error state in UI
- Display stderr output for diagnostics (already captured, needs UI)
- Offer restart button on crash (re-spawn with same config)
- Graceful shutdown: send `abort` before `kill_agent`
- Handle hung agents: timeout detection, force kill option
- Handle spawn failures (Pi not installed, bad config) with clear error messages

### P1.4: Session Persistence & Restore
Close the app, lose everything. Pi persists sessions to disk — Monarch just needs to remember what was running.

- Save agent registry to disk: id, name, config, Pi session path, status
- On app start, offer to restore previous agents
- Reconnect to Pi sessions (spawn new process, point to existing session dir)
- Track session paths from Pi's `get_state` response
- Persist across app restarts without losing agent context/history

### P1.5: Keyboard Navigation
The vision says "closer to vim/tmux." Currently mouse-only. Cheap to add, transforms daily use.

- `Ctrl+1-9` — switch between agents
- `Ctrl+N` — spawn new agent
- `Ctrl+C` — abort current agent (when not in input)
- `/` or `i` — focus message input
- `Escape` — unfocus input, return to navigation
- `Ctrl+B` — toggle sidebar
- `j/k` — navigate messages
- `Space` or `Enter` — expand/collapse tool calls
- `y` — copy focused code block
- `Ctrl+K` — command palette (later)

### P1.6: Message Display Polish
The rendering works but needs refinement for daily use.

- Syntax highlighting in code blocks (Shiki or Prism)
- Clickable file paths in agent output → open in external editor
- Copy button on code blocks
- Collapsible long tool results (file reads, large outputs)
- Better diff rendering (side-by-side option, syntax highlighting in diffs)
- Image display for image content blocks
- Error message styling (distinct from normal output)

### P1.7: Agent Controls Enhancement
Fill out the control surface using commands Pi already supports.

- Model picker populated from `get_available_models` (tied to P1.1)
- Session info display (name, token count, cost) from `get_session_stats`
- Compact button (send `compact` command)
- Auto-compaction toggle (`set_auto_compaction`)
- Auto-retry toggle (`set_auto_retry`)
- New session button (`new_session`)
- Session name editing (`set_session_name`)

---

## Phase 2 — Multi-Agent

### P2.1: Orchestrator Extension ✦ KEY UNLOCK
The feature that makes Monarch more than a Pi wrapper. A Pi extension that gives the orchestrator agent system-level tools.

**Architecture:**
```
Orchestrator Pi process
  ↓ calls monarch_list_agents tool
  ↓ extension executes tool
  ↓ HTTP request to Monarch local API
Monarch Rust backend
  ↓ queries AgentManager state
  ↓ returns JSON response
  ↓ back through extension to Pi to LLM
```

**Monarch local API (Rust):**
- Lightweight HTTP server (localhost only) started by Tauri backend
- Endpoints: `/agents`, `/agents/:id/send`, `/agents/spawn`, `/agents/:id/kill`, `/agents/:id/output`
- Auth: shared secret token passed to extension via env var

**Pi extension (`monarch-tools.ts`):**
- `monarch_list_agents` — see all agents, status, current task
- `monarch_spawn_agent` — create new agent with config
- `monarch_send_message` — send message to another agent
- `monarch_get_agent_output` — read another agent's recent output
- `monarch_kill_agent` — stop an agent
- Uses `api.registerTool()` for each, `fetch()` to hit Monarch API in execute

**Orchestrator system prompt:**
- Explains its role: you're the lead agent in a fleet
- Describes available tools and when to use them
- Delegation patterns: when to spawn vs do it yourself

### P2.2: Audit Trail
Log everything for observability. Required before giving agents more autonomy.

- Every Pi event from every agent stored persistently
- Structured log: timestamp, agent id, event type, details
- Storage: SQLite (single file, queryable, handles concurrent writes)
- UI: timeline view, filterable by agent/event type
- Trace any file change → tool call → agent → task → who requested it

### P2.3: Shared Context
Common knowledge accessible to all agents.

- Global context injected into agent system prompts via `--system-prompt` or extension `context` event
- Editable in UI: project rules, architecture decisions, conventions
- Scoped context per team/role (for when hierarchy exists)
- Updated manually or by orchestrator

### P2.4: Git Worktree Integration
Agents working in isolation so they don't step on each other.

- On agent spawn, optionally create worktree from current branch
- Agent's cwd = worktree path
- Visual worktree map in UI (which agent → which branch)
- Diff view between worktrees
- Merge flow from within Monarch
- Cleanup on agent completion

---

## Phase 3 — Identity & Memory

### P3.1: Agent Identity
Persistent agent definitions that survive across sessions.

- Templates: name, role, system prompt, model, tools, permissions
- Personality/style (optional)
- Skill specializations (learned from history)
- Spawn from template with one click

### P3.2: Layered Memory System
- Core identity (~200 tokens, always loaded)
- Hot memory (~500-1000 tokens, recent + important)
- Warm memory (on-demand, relevant to current task)
- Cold memory (searchable archive)

### P3.3: Memory Keeper
Background agent maintaining the fleet's memory.

- Distills completed tasks into clean memory entries
- Updates functional knowledge ("auth now uses JWT")
- Promotes/demotes between memory layers
- Watches audit trail, processes in background

### P3.4: Conversational Mode
Separate thread from task execution.

- Chat with any agent outside of task context
- Agent remembers personal interactions
- Group chat across agents

---

## Phase 4 — Polish & Access

### P4.1: Code Viewer
- Click file paths → syntax-highlighted preview in-app
- Inline diffs with full highlighting
- "Open in Zed/VS Code/Neovim" button at exact line
- "Open all files" for everything an agent touched

### P4.2: Editable Output
- Click agent messages to edit in place
- Edited version becomes canonical in agent context
- File edits write back to disk

### P4.3: Permissions & Approval
- Default: everything runs
- Gate specific actions behind approval (destructive ops, deploys)
- Approve from desktop or phone
- Scoped by agent role

### P4.4: Mobile Web UI
- Tauri backend serves lightweight web interface
- Remote access via tunnel (Tailscale/Cloudflare)
- Status, messaging, task dispatch from phone
- Approve/reject actions remotely

### P4.5: Voice Input
- Push-to-talk, local STT (Whisper), send as text

---

## Phase 5 — Intelligence

### P5.1: Automatic Context Management
- Smart compaction beyond truncation
- Auto-prune stale tool results
- Auto-inject relevant context on task shift
- Relevance scoring

### P5.2: Cost Tracking
- Real-time spend per agent/task/team
- Budget limits with auto-downgrade

### P5.3: Conflict & Loop Detection
- Detect agents editing same files
- Detect retry loops
- Auto-intervene or escalate

### P5.4: Time Travel
- Timeline of all agent activity (built on audit trail)
- Rollback to any checkpoint
- Branch from past states (built on Pi session forking)

---

## Open Questions

1. **Monarch local API transport:** HTTP on localhost? Unix socket? HTTP is simpler, socket is faster. Leaning HTTP for debuggability.
2. **Storage:** SQLite for audit trail + agent registry. Pi handles session persistence itself.
3. **Auth:** Pi manages API keys. Monarch stores agent configs (provider, model) but not keys.
4. **Multi-project:** One Monarch instance per project for now. Multi-project later if needed.
5. **Syntax highlighting:** Shiki (accurate, heavy) vs Prism (lighter). Try Shiki first, fall back if too slow.
6. **Pi installation:** Require system install for now. Bundle later if distribution matters.
