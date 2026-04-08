# Monarch — Cyber Fleet Command

## What

A growing, evolving system of AI agents that work together, remember, and have identity. Not a task runner — a living fleet you command. Agents do work, but they also know who they are, who you are, what they've done, and what's going on around them. You're at the helm.

## Why

Running multiple agents in terminals (even with tmux) gives poor visibility. You can't easily:
- See at a glance what each agent is working on
- Read structured diffs and tool outputs
- Interact with agents via proper text input (not terminal stdin)
- Control agent settings (model, thinking, context) per agent
- Copy/reference/extract things from agent output

## Architecture

**Agent harness:** Pi (pi.dev) — handles the LLM loop, tool execution, sessions, extensions. We don't build agents from scratch.

**Integration:** Pi's RPC mode (`pi --rpc`) — each agent is a Pi subprocess communicating via JSONL over stdin/stdout. Structured events in, commands out. No terminal parsing.

**Desktop shell:** Tauri v2 (Rust backend + Svelte frontend). Lightweight native app.

**Future path:** If RPC becomes limiting, swap to Pi SDK via Node sidecar for full programmatic control. Frontend stays the same.

## UI Concept

```
┌──────────┬──────────────────────────────────────┐
│ Agents   │  Agent: "refactor auth"        [⚙]  │
│          ├──────────────────────────────────────┤
│ ● auth   │  [thinking] Analyzing auth flow...   │
│ ● api    │                                      │
│ ○ tests  │  Found 3 files to modify:            │
│          │  ▸ src/auth.ts                       │
│          │  ▸ src/middleware.ts                  │
│ ──────── │                                      │
│ Status   │  ┌─ Tool: Edit ───────────────────┐  │
│ 2 active │  │ src/auth.ts:42                 │  │
│ 1 idle   │  │ - old code (red)               │  │
│          │  │ + new code (green)             │  │
│          │  └────────────────────────────────┘  │
│          │                                      │
│          ├──────────────────────────────────────┤
│          │  [message input]            [Send]   │
│          │  [☑ thinking] [ctx: 42k] [model ▾]   │
└──────────┴──────────────────────────────────────┘
```

## Key Features (Target)

- **Multi-agent dashboard** — see all agents, their status, what they're working on
- **Rich message display** — markdown, syntax highlighting, collapsible thinking blocks
- **Structured tool output** — diffs as diffs, bash output as terminal blocks, file reads with syntax highlighting
- **Chat input** — proper text box, not terminal stdin
- **Agent controls** — model picker, thinking toggle, context size, abort
- **Copy/extract** — click to copy code blocks, reference file paths, extract diffs
- **Session management** — per-agent session history, branching, forking
- **Inter-agent communication** — Monarch as the router between agents (see below)

## Multi-Agent System

Each Pi RPC process is isolated. Monarch sits in the middle and makes them work together.

### Hierarchy & Delegation

You talk to your right-hand agent — the orchestrator. Give it a task, it evaluates the scope, and if needed spins up specialized agents for specific parts. Agents can lead sub-groups: the orchestrator hands a chunk of work to a lead, the lead splits it further within its team.

```
         You
          │
      Orchestrator
       /    |    \
    Lead A  Lead B  Solo Agent
    / \       |
  A1  A2     B1
```

Not rigid ranks — more like a crew where someone takes point and others follow based on what the work needs.

### Shared Context & Knowledge

All agents can access common knowledge through Monarch — project context, conventions, decisions made so far. Access can be scoped:
- **Global** — everyone sees it (project rules, architecture decisions)
- **Team** — only a sub-group sees it (team-specific context)
- **Need-to-know** — injected by Monarch when relevant

Monarch controls what flows where. Each agent stays focused on its piece, but nobody works blind.

### Audit Trail & Memory

Everything gets logged: which agent did what, when, who told it to, what the result was. Full chain of causation from your initial instruction down to the last file edit.

Could be a dedicated bookkeeper agent running in the background, or agents self-reporting — TBD on implementation. The point: nothing happens in the dark. You can trace any change back through the delegation chain to the original intent.

```
[12:01] You → Orchestrator: "refactor the auth system"
[12:01] Orchestrator → Lead A: "redesign token storage" (reason: auth refactor)
[12:02] Lead A → A1: "migrate session table" (reason: token storage redesign)
[12:03] A1: edited src/db/sessions.ts (tool: edit, lines 42-67)
[12:04] A1 → Lead A: "migration done, needs review"
...
```

This is the observability layer — not just seeing what's happening now, but being able to rewind and understand how you got here.

### Time Travel

Every agent action is atomic and documented — tool calls with full diffs, who triggered what, when. This makes the entire system rewindable:

- **Timeline view** — scrub through history, see the state of everything at any point in time
- **Rollback** — revert to a previous state. Undo the last 5 agent actions, or go back to "before the auth refactor started"
- **Branch from past** — go back to a checkpoint and try a different approach, like git but for the whole multi-agent workflow

Building blocks:
- Pi sessions are append-only with tree branching — can fork from any point
- Git provides file-level snapshots (agents can auto-commit at checkpoints)
- Monarch's audit trail links every file change to the agent action and original intent

Combined: a full DAG of agent activity where any node is a rewindable, branchable state.

### Agent-Facing API

Monarch isn't just a UI for humans — it's also a system that agents interact with. Agents can query and act on the setup itself, scoped by their role.

**Orchestrator sees everything:**
- All agents, their status, skills, current tasks
- Team structures and who reports to whom
- System-wide audit trail
- Can spawn/kill agents, reassign work, restructure teams
- Can read shared context and modify global knowledge

**Team leads see their team:**
- Their direct agents, status, what they're working on
- Can reassign within their group
- Can read team-level shared context
- Can escalate to orchestrator
- No visibility into other teams

**Workers see their task:**
- Their own assignment and scoped context
- Can report status/completion back up the chain
- Can request help or flag blockers
- No system-level visibility

This is implemented as scoped tools — Monarch exposes different Pi tools depending on the agent's role. The orchestrator gets `monarch_system_status`, `monarch_spawn_agent`, `monarch_assign_task`. A worker gets `monarch_report_status`, `monarch_request_help`. Same system, different interface per agent.

The principle: agents interact with the setup, but only the parts they need. Nobody gets distracted by information outside their scope.

### Permissions & Approval Flows

All agent actions flow through Monarch, so it's the natural place for permission control. Pi's `tool_call` event can be intercepted and blocked before execution.

**Permission levels:**
- **Auto-approve** — runs silently (read, grep, ls, safe stuff)
- **Notify** — runs but flagged in the activity feed (file edits, non-critical changes)
- **Require approval** — pauses until you approve or reject (destructive commands, deploys, `rm`, force push)
- **Escalate** — agent can't self-approve, bumps up the chain (worker → lead → orchestrator → you)

**Scoped by role:**
- Orchestrator: can spawn agents, reassign work — auto-approved. But needs your OK for system-level changes.
- Team leads: can direct their team — auto-approved. Destructive file ops need approval.
- Workers: read/grep/ls auto-approved. Edits notify. Deletes and bash commands require approval.

**Approval from anywhere:** desktop app or phone. Agent blocks until it gets a response. Timeout rules configurable — if no response in X minutes, auto-reject or escalate.

**Default: everything runs.** You only add gates where they matter. Trust by default, restrict by exception.

### Mobile / Remote Access

Monarch exposes a web interface and API so you can interact with the system from your phone or any device.

**Approach:** Tauri backend serves a lightweight web UI alongside the desktop app. Connect from your phone's browser on the same network, or expose it remotely via tunnel (Tailscale, Cloudflare Tunnel, etc.).

**What you can do from your phone:**
- See all agents, their status, what they're working on
- Send messages to any agent or the orchestrator
- Kick off new tasks ("hey, refactor the auth system")
- Get notified when something needs your attention (approval, error, completion)
- Review agent output, diffs, and tool calls
- Approve or reject actions from agents that need confirmation

The desktop app is the brain. The mobile view is a remote control — lightweight, focused on status and interaction, not on running agents locally.

### Dual Mode: Functional + Conversational

Agents operate in two modes simultaneously:

**Functional (task execution):**
The default loop. CONTEXT + TASK → RESULTS. Agents dispatch other agents, execute work, report back. This is the backbone — tight, focused, no fluff.

**Conversational (relationship layer):**
A separate thread, not tied to any task lifecycle. You can talk to any agent at any time. The conversation is personal — the agent knows:
- Who it is (identity, role, personality)
- Who you are (preferences, how you communicate)
- What it's currently working on
- What it worked on before (memory across sessions)
- Its place in the fleet

"Good job, keep going" → agent remembers that. It's motivation, not a task instruction. Goes into its personal memory, not the task context.

These two modes coexist. An agent can be mid-task and you drop in to chat. The conversation doesn't pollute the task context — they're separate streams that share the same memory.

### Group Chat

Talk to all your agents at once. Or a team. Or any subset. Not just for fun (though it is) — useful for:
- Briefing the whole fleet on a new project direction
- Asking "who has context on the payment system?" and seeing who responds
- Collaborative problem-solving across agents with different specialties
- Just vibing with your crew

### Agent Identity & Memory

Each agent has persistent memory across sessions:
- **Who they are** — name, role, personality, specialties
- **What they've done** — task history, wins, failures, learnings
- **Relationships** — who they work with, who dispatched them, your interactions with them
- **Recent memories** — ranked by importance, "good job" sits at the top
- **Project knowledge** — what they know about the codebase, conventions, decisions

Agents evolve over time. An agent that's done 50 auth-related tasks becomes your auth expert — not because you labeled it that, but because it remembers all that work. The fleet grows and specializes organically.

### Memory Architecture

Memory has to be efficient — enough to be useful, not so much it drowns the context window.

**Layered recall:**
- **Core identity** — always loaded. Who am I, who's the captain, my role, my team. Tiny, ~200 tokens.
- **Hot memory** — recent and important. Current task, last few interactions with you, active project context. Loaded by default. ~500-1000 tokens.
- **Warm memory** — relevant to current work. Past tasks in the same area, related learnings, known patterns. Pulled in when the task overlaps. On-demand.
- **Cold memory** — everything else. Full task history, old conversations, archived learnings. Only retrieved when the agent explicitly searches for it or Monarch detects relevance.

Agents don't dig into deeper memory unless they need it. If a task touches auth and the agent worked on auth 3 weeks ago, warm memory surfaces that automatically. If not relevant, it stays cold.

**Memory Keeper (background agent):**

A dedicated agent that maintains the fleet's memory system:
- After each task completes, distills the result into a clean memory entry (what was done, what was learned, what changed)
- Updates functional memories — "auth now uses JWT instead of sessions" so agents don't work with stale knowledge
- Merges duplicate memories, prunes contradictions
- Promotes/demotes memories between layers based on relevance and recency
- Maintains the shared knowledge base that all agents draw from
- Essentially the fleet's librarian — it doesn't do tasks, it keeps the books

The memory keeper watches the audit trail and processes it in the background. Agents don't have to self-report — the keeper observes and records.

### Code Access

You shouldn't have to leave Monarch to see what agents are building.

**Built-in code viewer:**
- Click any file path in agent output → see the code with syntax highlighting
- Agent diffs rendered inline — old/new side by side or unified
- Line numbers, search, jump to definition (basic)
- Read-only by default, keeps it simple

**Quick edit (later):**
- Embed Monaco or CodeMirror for in-app editing when you need to tweak something
- Not a full IDE — just enough to fix a line or adjust what the agent wrote

**External editor integration:**
- "Open in Zed/VS Code/Neovim" button on any file
- Opens at the exact line the agent was working on
- Monarch handles agent management, your real editor handles serious editing

Monarch is the command center, not an IDE. But you should never feel like you need to context-switch to another app just to see a file.

### Editable Agent Output

Agent output isn't read-only. Anything an agent writes — messages, code, docs — you can edit in place.

- **Edit messages** — click any agent response, change a sentence, fix wording. The edited version becomes canonical in the agent's context. Next time the agent looks at its history, it sees your version.
- **Edit file output** — agent wrote code you don't like? Edit it right in the output view, save writes back to disk.
- **Inline editing** — no modal, no separate view. Click/keybind on the block, it becomes editable, change what you want, done.

This means you're not just reviewing agent work — you're collaborating with it. Fix the 10% the agent got wrong without regenerating the 90% it got right.

### File References Everywhere

Every file path in agent output is a clickable link. Every code block shows its source. Two clicks to open anything in your external editor.

- All file paths are interactive — click to preview in Monarch, double-click or keybind to open in Zed/VS Code/Neovim
- Tool results always show the full file path + line numbers
- Diffs link to both the before and after state
- "Open all files" button on any agent task — opens every file the agent touched in your editor
- Breadcrumb trail: task → agent → tool call → file → line

### Git Worktree Support

Native worktree integration so agents don't step on each other or on your working tree.

- Each agent (or team) can work in its own git worktree — isolated branch, isolated files
- Monarch manages worktree lifecycle: create on agent spawn, cleanup on completion
- No merge conflicts mid-work — agents work in parallel on separate branches
- When done, review the diff and merge from within Monarch
- Your main working tree stays clean — agents never touch it directly unless you say so
- Visual worktree map: see which agent is on which branch, what's diverged, what's ready to merge
- Ties into time travel — each worktree is a rewindable, branchable state

### Keyboard-First Navigation

The whole app is navigable by keyboard. Click if you want, but everything has a keybinding.

- Switch between agents (`ctrl+1-9`, `ctrl+tab`)
- Focus message input (`/` or `i`)
- Send message (`enter` or `ctrl+enter`)
- Open command palette (`ctrl+k`) — search agents, run commands, switch views
- Abort current agent (`ctrl+c`)
- Spawn new agent (`ctrl+n`)
- Toggle sidebar (`ctrl+b`)
- Navigate message history (`j/k` or arrows)
- Expand/collapse tool calls (`space` or `enter`)
- Copy code block (`y` on focused block)
- All keybindings customizable

The feel should be closer to vim/tmux than a web app. Fast, muscle-memory friendly, no reaching for the mouse.

### Voice Input

Talk to your agents. Hold a key, speak, it transcribes and sends.

- Push-to-talk keybind — hold to record, release to send
- Works for any agent or group chat
- Whisper/local STT for low latency and privacy
- Natural for quick commands: "hey, abort that and try a different approach"
- Especially useful from mobile — voice is faster than phone keyboard

### Cost Tracking

Monarch sees every LLM call across all agents. Track spend in real time:
- Cost per agent, per task, per team
- Which model is being used where
- Helps decide when to use cheap models for grunt work vs expensive for orchestration
- Set budget limits per agent/team — pause or downgrade model when threshold hit

### Context Control

The agent's context window is just a long text that grows with every interaction. It's the single most important factor in agent quality — a bloated or irrelevant context makes agents dumb, a clean focused one makes them sharp.

**Inspect:** At any point, open an agent's full context and read it. See exactly what the LLM sees — system prompt, message history, tool results, injected knowledge. Visualize token usage, see what's taking up space.

**Manipulate (manual):**
- Remove specific messages or tool results that are no longer relevant
- Edit messages in the context (fix a bad instruction, clarify something)
- Pin important messages so they survive compaction
- Inject new context (paste in a doc, a spec, relevant code)
- Reorder to put important stuff where the model pays most attention (top/bottom)

**Manipulate (automatic — future):**
- Smart compaction that understands what matters, not just truncation
- Auto-prune stale tool results (old file reads when the file has since changed)
- Auto-inject relevant context when the task shifts (agent starts working on auth → inject auth docs)
- Context budget management — keep it under a target size, prioritize what stays
- Cross-agent context dedup — if two agents read the same file, the system knows and manages it
- Relevance scoring — flag context entries that are probably dead weight

**Why this matters:** Most agent failures aren't model failures, they're context failures. The model had the wrong info, stale info, or too much noise. Controlling context is controlling agent intelligence.

### Agent Templates

Reusable agent configs. Define once, spawn many:
- System prompt, model, thinking level
- Tool access and permission rules
- Role (orchestrator, lead, worker)
- Scoped context access
- Save as templates, spawn from them with one click
- "Frontend Agent", "Test Writer", "Code Reviewer" etc. as ready-to-go presets

## Current State

- Tauri v2 + Svelte prototype with embedded PTY terminals (raw terminal, works but limited)
- Next step: replace PTY with Pi RPC subprocess, build structured UI for events

## Tech Stack

- **Rust** (Tauri backend): process management, Pi subprocess spawning, JSONL IPC
- **Svelte 5** (frontend): agent UI, message rendering, controls
- **Pi** (agent harness): LLM orchestration, tool execution, sessions
- **xterm.js**: may keep for raw terminal fallback view
