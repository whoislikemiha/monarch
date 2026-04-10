# MON-12 — Right-side toolbox rail and pluggable tool registry

## What was implemented

A VSCode-style right-edge tool rail with a stacked, horizontally resizable
panel region for pluggable per-agent tools. Adding a tool now requires one
entry in `src/lib/toolbox/registry.ts`, one Svelte component with a fixed
`{ agentContext }: ToolProps` contract, and — optionally — a typed Tauri
command under `src-tauri/src/toolbox/`. A placeholder tool ships as the
verification vehicle.

Under the hood the PR also lifts per-agent live conversation state
(items, tool executions, streaming message, usage, current tool group,
activity status, event count) out of `AgentView.svelte`'s local `$state`
and into a shared `liveAgentStore` keyed by agent id. AgentView becomes a
consumer of that store; toolbox tools consume the same store via
`AgentContext.live`. The old `agentViewStates` snapshot/restore cache in
`App.svelte` is deleted — the store survives the `{#key activeAgent.viewKey}`
remount by construction.

A matching `toolbox` module on the Rust side registers descriptors and
typed per-tool commands (placeholder ships `toolbox_placeholder_ping`),
wired into both `tauri::generate_handler!` and `ws::dispatch_command` so
the Tauri webview and WS browser bridge both work.

## Key decisions

- **Full extraction, not mirror.** AgentView writes live state only through
  the store. Keeping a local `$state` mirror in parallel would have doubled
  reactivity work on every streaming event (items can grow large) and
  introduced divergence risk. The diff is mechanical — `items = …` →
  `l.items = …` where `l = writeLive(targetAgentId)`.
- **Each store entry is its own `$state(...)` proxy.** This was caught
  during review. Svelte 5's reactive Map only tracks add/delete on the
  outer Map — values inserted via `.set()` are not deeply wrapped. Without
  explicit `$state()` on each entry, per-field writes like `l.items = [...]`
  mutate the object in place but don't invalidate any derived depending on
  them. Wrapping each entry fixes it.
- **`.svelte.ts` extension is mandatory for the store.** Runes in plain `.ts`
  files throw at module evaluation. Discovered the hard way after the first
  dev run showed a blank window.
- **Direct typed Tauri commands per tool, not a generic dispatcher.**
  Matches the rest of the repo, preserves typing, keeps new tools adding
  match arms in one place on each side.
- **DB reconciliation is first-bind-only.** Since the store survives
  `{#key}` remount, reloading from SQLite on every agent switch is
  redundant. Load happens on first bind, `sourceSessionId` restore, or
  explicit session switch. Events missed while an agent is in the
  background are accepted as pre-existing behavior; a follow-up task will
  rework the snapshot/restore path.
- **No drag-to-resize between stacked panels in v1.** Equal flex split +
  160px min-height + `overflow-y: auto` on the container. Good enough
  until there are enough tools to warrant it.
- **Width clamp at 240–600px** on both drag and localStorage restore.
  Narrow-window auto-hide is out of scope — users hide the left sidebar
  with `Ctrl+B` instead.
- **Tool-author cross-agent state constraint documented.** Tools stay
  mounted across agent switches on purpose (no remount flicker). Any
  per-agent memory must be keyed by `agentContext.agentId` inside the
  component; the framework won't remount.

## Files touched

### Frontend — new
- `src/lib/toolbox/types.ts` — `ToolDefinition`, `ToolProps`, `AgentContext`, `LiveAgentState`.
- `src/lib/toolbox/registry.ts` — the `TOOLS` array + `getTool` / `sortedTools` helpers.
- `src/lib/toolbox/liveAgentStore.svelte.ts` — module-level `$state({ byAgent: Map })` + `ensureLiveState` / `resetLiveState` / `removeLiveState`. Entries are `$state`-wrapped.
- `src/lib/toolbox/persistence.ts` — localStorage helpers for width and open ids with clamp.
- `src/lib/toolbox/ToolRail.svelte` — right-edge icon strip.
- `src/lib/toolbox/ToolPanelStack.svelte` — stacked panel region with horizontal drag handle.
- `src/lib/toolbox/tools/PlaceholderTool.svelte` — verification tool exercising both store and backend.

### Frontend — modified
- `src/App.svelte` — 4-sibling layout (Sidebar / main-panel / ToolPanelStack / ToolRail); `openToolIds` + `toolboxWidth` state with `$effect` persist; `currentLive` + `agentContext` derived; cache plumbing (`agentViewStates`, `getAgentViewState`, `updateAgentViewState`) deleted; `killAgent` calls `removeLiveState`.
- `src/lib/AgentView.svelte` — major: all event-stream-derived local state replaced by reads/writes on the store; `snapshotViewState` / `persistCurrentViewState` / `restoreCachedViewState` deleted; `bindAgent` skips DB reload when store entry is non-empty; UI-local state (`isStreaming`, `showStderr`, modal flags, listener handles) stays local.

### Backend — new
- `src-tauri/src/toolbox/mod.rs` — `ToolDescriptor`, descriptors list, `toolbox_list_tools` + `ws_toolbox_list_tools`.
- `src-tauri/src/toolbox/placeholder.rs` — `toolbox_placeholder_ping` / `ws_toolbox_placeholder_ping` returning `pong @ unix:<secs>` (std::time, no new dependency).

### Backend — modified
- `src-tauri/src/lib.rs` — declares `mod toolbox`; registers toolbox commands in `invoke_handler!`.
- `src-tauri/src/ws.rs` — adds `toolbox_list_tools` and `toolbox_placeholder_ping` dispatch arms.

### Docs
- `ONBOARDING.md` — §7 state flow rewritten around `liveAgentStore`; new "Adding a toolbox tool" subsection (4-step recipe); §12 file-path reference extended with all new toolbox files on both sides of the IPC boundary.

### Housekeeping (not MON-12 per se)
- `src-tauri/icons/*` — Tauri-generated icon assets that were sitting untracked.
- `thoughts/plan/MON-12.md` — the implementation plan itself.
- `.agents/` + `skills-lock.json` — Claude Code harness state accidentally swept in by a `git add -A`; left in place and noted in the commit message rather than rewriting history.

## What was left out

- **MON-13: inspector migration.** The existing `ContextInspector` modal is untouched — its migration onto the new toolbox surface is tracked separately.
- **Drag-resizable vertical splits** between stacked panels.
- **Keybindings** for the rail or individual tools.
- **Per-agent persistence** of tool open state.
- **Council-aware tools.** In council mode `agentContext` is forced to null and tools show their empty state.
- **Deeper rework of the snapshot/restore path.** The store only sees events while AgentView is mounted, so events that fire while an agent is in the background are still missed (same as pre-PR behavior). A follow-up task will lift the event listener out of AgentView so the store is always live.
- **A second real tool.** Placeholder is the only shipped tool — MON-13 adds the first real one.
