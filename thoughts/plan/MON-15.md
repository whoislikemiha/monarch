# MON-15: Tabbed Agent Layout with Collapsible Sidebar Drawer

## Sub-issues (build order)

1. **MON-17** — Lazy restore: agents persist as stopped, spawn on interaction
2. **MON-18** — Tabbed agent layout with tab bar and new conversation dropdown
3. **MON-19** — Collapsible sidebar drawer with icon rail
4. **MON-21** — Persist tab and pane state to SQLite across restarts
5. **MON-20** — Split view: side-by-side agent panes

## Summary

Currently, agents are displayed as a vertical list in a fixed 220px left sidebar (`Sidebar.svelte`). Only one agent can be viewed at a time — switching agents via `selectAgent()` in `App.svelte` sets `activeId`, which triggers a destructive re-render of `AgentView` via `{#key activeAgent.viewKey}`. The sidebar also holds the "+" create button, project grouping, council mode toggle, and context menus.

This change refactors the layout to: (1) move agent switching to a horizontal tab bar above the main panel, (2) allow multiple tabs to be open simultaneously, (3) convert the sidebar into a collapsible drawer, and (4) add a "+" button on the tab bar that opens a dropdown to start a new conversation with an *existing* running agent (distinct from the sidebar's "+" which spawns a brand new agent).

## Relevant files and areas

- **`src/App.svelte`** — Main layout container. Owns `agents[]`, `activeId`, `showSidebar`, `agentViewStates` Map, keyboard shortcuts (Ctrl+1-9, Ctrl+B, Ctrl+N). Renders `Sidebar` conditionally via `{#if showSidebar}` and `AgentView` via `{#key activeAgent.viewKey}`. This is the primary file that needs restructuring.

- **`src/lib/Sidebar.svelte`** — 220px fixed sidebar. Renders agent list grouped by project, "+" button, council mode toggle, saved agent items, context menus. Currently the sole mechanism for agent selection and creation. Needs to become a collapsible drawer and lose its agent-switching role (that moves to tabs).

- **`src/lib/AgentView.svelte`** — Per-agent view with messages, input, tools, side panels. Currently only one instance exists at a time (destroyed/recreated on switch). The `snapshotViewState()`/restore pattern already caches per-agent state — this pattern continues to work with tabs.

- **`src/lib/types.ts`** — `Agent` interface (has `viewKey` for destructive switching), `AgentViewState` interface. No schema changes needed, but the concept of "open tabs" is new state that doesn't exist in the type system yet.

- **`src/lib/AgentHeader.svelte`** — Header inside AgentView showing agent name/title/project. Some of this info may now be redundant with tab labels.

## What needs to change

### 1. New component: TabBar

A new `TabBar.svelte` component rendered between the sidebar and the main panel area (horizontally above `AgentView`). It manages:
- A list of "open tab" IDs (subset of all agents) — which agents have tabs open
- The currently active tab (replaces `activeId` for switching)
- Tab rendering: each tab shows agent name, status dot, close (x) button
- A "+" button at the end that opens a dropdown of all running agents not yet in a tab, clicking one opens a new conversation with that agent
- Keyboard shortcut targets: Ctrl+1-9 should map to tab order, not agent creation order

### 2. New state concept: open tabs vs. all agents

`App.svelte` currently has `agents[]` (all agents) and `activeId` (which one is shown). The refactor introduces:
- `openTabs: string[]` — ordered list of agent IDs that have open tabs
- `activeTabId: string | null` — which tab is focused (replaces `activeId`)
- When an agent is created or restored, it automatically gets a tab
- Closing a tab removes it from `openTabs` but does NOT kill the agent
- The "+" on the tab bar opens a dropdown of running agents to add to tabs (starts a new conversation session)

### 3. Sidebar becomes a collapsible drawer

`Sidebar.svelte` transforms from a fixed 220px panel to a drawer that can slide in/out:
- Collapsed state: either fully hidden or a narrow icon strip (needs design decision)
- Expanded state: same content as today — agent overview, "Create Agent" button, project groups, council mode
- `Ctrl+B` toggles drawer open/closed (already wired, just needs animation/transition)
- The sidebar retains the "Create Agent" (spawn) button — this is distinct from the tab bar's "+" which selects from existing agents
- Consider: sidebar could show ALL agents (including ones without open tabs) as an overview/management panel

### 4. Layout restructuring in App.svelte

Current layout: `[Sidebar | MainPanel]` (horizontal flex)

New layout:
```
[Sidebar(drawer)] [TabBar + MainPanel (vertical flex)]
                   [TabBar                            ]
                   [AgentView                         ]
```

The `.main-panel` div wraps both `TabBar` and `AgentView` in a column flex. The sidebar overlays or pushes content depending on drawer behavior.

### 5. Multi-tab AgentView rendering

Currently uses `{#key activeAgent.viewKey}` which destroys and recreates the component. For fast tab switching, the existing `agentViewStates` caching pattern already handles this well — switching tabs snapshots the current view and restores the target. No need to keep multiple AgentView instances alive simultaneously (that would multiply event listeners and memory).

### 6. Tab persistence and lazy restore

**Tab persistence**: Open tabs (ordered list of agent IDs + active tab) are saved to SQLite. A simple `ui_state` key-value table in `db.rs` stores `{ openTabs: string[], activeTabId: string }` as JSON. Updated whenever tabs change.

**Lazy restore (replaces the current restore bar)**: On app start, all previously-saved agents load into their tabs immediately but in a `stopped` / disabled state — no sidecar process is spawned. The UI shows the agent's previous messages (from SQLite/cache) but the agent is inert. When the user sends a message or otherwise interacts, *that's* when `spawn_agent` fires, the sidecar starts, and the session resumes (or a new session is created with `parentSessionId`).

This eliminates the current "Restore All / Dismiss" bar (`showRestoreBar`, `restoreAllAgents()`, `dismissRestore()` in App.svelte) and replaces it with a seamless experience where the app feels persistent. Agents are always there; they just wake up on demand.

Key changes:
- Remove `showRestoreBar`, `restoreAllAgents()`, `dismissRestore()`, and the restore bar UI from `App.svelte`
- `loadSavedAgents()` populates `agents[]` directly in stopped state instead of going into a separate `savedAgents[]` array
- `AgentView` / `ChatInput` detects when a stopped agent receives user input and triggers spawn before sending the message
- The `savedAgents` concept merges with `agents` — all agents are just agents, some are running and some are stopped

### 7. Side-by-side agent panels (split view)

The main panel can be split into two (or more) panes, each showing a different agent's `AgentView`. This is feasible because the refactor already introduces multi-tab state and per-agent view caching.

**Layout model**: The main panel area becomes a flex container that can hold 1-N panes. Each pane has its own active tab from the tab bar. One pane is the "focused" pane (receives keyboard shortcuts, new tabs open here).

**Split interactions**:
- Right-click a tab → "Open in split" to create a new pane with that agent
- A split button/icon in the tab bar or agent header to split the current view
- Closing the last tab in a pane collapses that pane
- Consider: `Ctrl+\` to toggle split, mirroring VS Code convention

**Multiple live AgentViews**: Unlike tab switching (which destroys/recreates), split view keeps multiple `AgentView` instances mounted simultaneously. This means:
- Multiple event listeners active (one per agent) — already supported since each listens on `agent-event-{agentId}`
- Each pane has its own scroll container and input area
- The `agentViewStates` cache is less relevant for split agents since they stay alive, but still needed for agents that are tabbed-but-not-visible within a pane
- Focus management: only one pane's `ChatInput` is focused at a time; clicking a pane or using a shortcut switches focus

**State**: `App.svelte` needs a pane model, e.g.:
- `panes: { id: string, activeTabId: string }[]` — each pane tracks which tab is active in it
- `focusedPaneId: string` — which pane receives keyboard input
- Panes are ephemeral UI state but could be persisted alongside tab state in SQLite for restart continuity

### 8. Keyboard shortcut updates

- `Ctrl+1-9` — Switch between open tabs (not all agents)
- `Ctrl+B` — Toggle sidebar drawer
- `Ctrl+N` — Still opens SpawnDialog (from sidebar)
- Consider: `Ctrl+W` to close current tab (without killing agent)
- Consider: `Ctrl+T` to open the tab "+" dropdown

## Resolved decisions

1. **Collapsed sidebar**: Narrow icon rail (~40px), not fully hidden. Will hold buttons/actions even when collapsed.

2. **Sidebar-to-tab interaction**: Clicking an agent in the sidebar switches to its existing tab if one is open; if not, opens a new tab for that agent's current session.

3. **Tab "+" behavior**: Creates a new session (new conversation) for the selected agent — new SQLite session row with `parentSessionId` pointing to the previous session.

4. **Tab close vs. agent kill**: Closing a tab keeps the agent running. Killing an agent (from sidebar) closes its tab AND terminates the process.

5. **Council mode**: Replaces the entire main panel (not a tab). Council mode toggle becomes a button visible in the collapsed sidebar icon rail.

## Out of scope

- Drag-to-reorder tabs
- ~~Tab persistence across app restarts~~ — **moved in scope**: open tabs will be persisted and restored on restart
- ~~Split-view / side-by-side agent panels~~ — **moved in scope**: split panes with multiple live AgentViews
- Changes to the agent runtime, sidecar protocol, or SQLite schema
- Redesigning the SpawnDialog
- Mobile/responsive layout considerations
