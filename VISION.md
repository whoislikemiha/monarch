# Monarch — Vision

> This is the north star, not the current state. For what's built today, see [CLAUDE.md](./CLAUDE.md) and [ONBOARDING.md](./ONBOARDING.md).

## What

A growing, evolving system of AI agents that work together, remember, and have identity. Not a task runner — a living fleet you command. Agents do work, but they also know who they are, who you are, what they've done, and what's going on around them. You're at the helm.

## Why

Running multiple agents in terminals gives poor visibility and no coordination. You can't see what each agent is working on at a glance, interact with structured output, control per-agent settings, or make agents aware of each other. Monarch solves this by sitting in the middle — the command center that connects agents, manages their state, and gives you full control.

## Multi-Agent Orchestration

Agents can work in hierarchies — one agent delegates to others, leads coordinate sub-groups, workers execute. But the hierarchy is flexible and configurable, not a rigid tree. You might run a classic orchestrator-lead-worker setup, or a flat pool of specialists, or something in between. Monarch provides the primitives (delegation, scoped visibility, reporting chains) and you wire them however the work demands.

```
         You
          |
      Orchestrator           ... or flat:    You
       /    |    \                          / | | \
    Lead   Lead   Solo                   A  B  C  D
    / \      |
  W1  W2    W3
```

### Scoped Visibility

Agents see what they need, not everything:

- **Global context** — project rules, architecture decisions, shared knowledge. Everyone sees it.
- **Team context** — scoped to a sub-group. Leads and their workers share it.
- **Task context** — the agent's own assignment and immediate surroundings.

Monarch controls what flows where. Agents stay focused on their piece without being overwhelmed by the full picture.

### Delegation & Reporting

Agents can spin up other agents, assign work, and receive results. The delegation chain is tracked end-to-end — from your initial instruction through every sub-task down to the last file edit. Any change can be traced back to the original intent.

### Permissions & Approval Flows

All agent actions flow through Monarch, making it the natural place for permission control:

- **Auto-approve** — runs silently (reads, searches, safe operations)
- **Notify** — runs but flagged in the activity feed (file edits, non-critical changes)
- **Require approval** — pauses until you approve or reject (destructive commands, deploys)
- **Escalate** — agent can't self-approve, bumps up the chain

Permissions are scoped by role. Trust by default, restrict by exception.

## Memory

Each agent has persistent memory across sessions — who they are, what they've done, what they've learned, and their relationships within the fleet. Agents evolve over time. An agent that's done 50 auth-related tasks becomes your auth expert — not because you labeled it, but because it remembers all that work.

### Layered Recall

Memory has to be efficient — enough to be useful, not so much it drowns the context window.

- **Core identity** — always loaded. Who am I, who's the captain, my role. Tiny footprint.
- **Hot memory** — recent and important. Current task, last interactions, active project context. Loaded by default.
- **Warm memory** — relevant to current work. Past tasks in the same area, related learnings. Surfaced automatically when the task overlaps.
- **Cold memory** — everything else. Full history, old conversations, archived learnings. Retrieved on explicit search or when Monarch detects relevance.

### Memory Keeper

A dedicated background agent (or system process — implementation TBD) that maintains the fleet's memory:

- Distills completed tasks into clean memory entries
- Updates functional memories so agents don't work with stale knowledge
- Merges duplicates, prunes contradictions
- Promotes/demotes memories between layers based on relevance and recency
- The fleet's librarian — observes the audit trail and keeps the books

## Context Control

The agent's context window is the single most important factor in agent quality. A bloated or irrelevant context makes agents dumb, a clean focused one makes them sharp.

**Inspect:** Open any agent's full context and see exactly what the LLM sees — system prompt, message history, tool results, injected knowledge. Visualize token usage, see what's taking up space.

**Manipulate (manual):**

- Remove messages or tool results that are no longer relevant
- Edit messages in the context to fix bad instructions or clarify
- Pin important messages so they survive compaction
- Inject new context (docs, specs, code)
- Reorder to optimize attention (important stuff at top/bottom)

**Manipulate (automatic):**

- Smart compaction that understands what matters, not just truncation
- Auto-prune stale tool results (old file reads when the file has changed)
- Auto-inject relevant context when the task shifts
- Context budget management — keep under a target size, prioritize what stays
- Relevance scoring — flag context entries that are probably dead weight

Most agent failures aren't model failures — they're context failures. Controlling context is controlling agent intelligence.

## Parallel Conversations

You can talk to an agent while it's working. Not by interrupting its task context — by having a parallel conversation stream that shares the same memory and identity but doesn't pollute the implementation context.

Think of it as two views into the same agent: one where work happens, one where you chat. You can give guidance, ask questions, or just check in without derailing what's in progress. The agent knows both conversations are happening and can draw on either.

## Time Travel

Every agent action is atomic and documented — tool calls with full diffs, who triggered what, when. This makes the entire system rewindable:

- **Timeline view** — scrub through history, see the state of everything at any point
- **Rollback** — revert to a previous state. Undo the last N actions, or go back to before a task started
- **Branch from past** — go back to a checkpoint and try a different approach

The data model already supports session ancestry and branching. The vision is a full DAG of agent activity where any node is a rewindable, branchable state.

## Git Worktree Support

Native worktree integration so agents don't step on each other or on your working tree:

- Each agent (or team) works in its own git worktree — isolated branch, isolated files
- Monarch manages worktree lifecycle: create on spawn, cleanup on completion
- No merge conflicts mid-work — agents work in parallel on separate branches
- Review diffs and merge from within Monarch
- Visual worktree map: which agent is on which branch, what's diverged, what's ready to merge

## Remote & Mobile Access

Monarch exposes a web interface so you can interact with the fleet from your phone or any device:

- See all agents, their status, what they're working on
- Send messages, kick off tasks, approve actions
- Get notified when something needs attention
- Review output, diffs, and tool calls

The desktop app is the brain. The mobile view is a remote control.

## Appearance & Identity

Agents have visual identity — avatars, animations, customizable appearance. The fleet should feel alive, not like a list of process IDs. Shadow grades, titles, and personalities are reflected visually. Appearance is configurable per agent.

## Voice Input

Talk to your agents. Push-to-talk keybind — hold to record, release to send. Works for any agent or group chat. Natural for quick commands, especially useful from mobile.

## Cost Tracking & Budgeting

Monarch sees every LLM call across all agents:

- Cost per agent, per task, per team
- Which model is being used where
- Budget limits per agent/team — pause or downgrade model when threshold hit
- Helps decide when to use cheap models for grunt work vs expensive for orchestration

## Keyboard-First

The whole app is navigable by keyboard. The feel should be closer to vim/tmux than a web app — fast, muscle-memory friendly, no reaching for the mouse. Everything has a keybinding, all customizable.

## Code Access

You shouldn't have to leave Monarch to see what agents are building:

- Click any file path in agent output to preview with syntax highlighting
- Diffs rendered inline (unified or side-by-side)
- "Open in editor" button — opens at the exact line in your external editor
