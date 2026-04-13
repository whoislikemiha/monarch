# MON-47: Extract frontend state from App.svelte into agentStore

## Goal

`App.svelte` has grown into a ~950-line "brain" owning all shared frontend state: `agents[]`, `openTabs`, `activeTabId`, `projects[]`, sidebar filter, plus every lifecycle function (spawn/kill/restart/archive/delete/summon/etc.). Children get everything via prop-drilling. Extract the shared state + lifecycle into a class-based Svelte 5 store under `src/lib/stores/agentStore.svelte.ts`, leaving `App.svelte` as a thin shell (layout, dialogs, keybindings, zoom).

## History

A first attempt (PR #42, opened 2026-04-11) was closed on 2026-04-13 after accumulating 49 commits of drift on master. Both target files (`App.svelte`, `Sidebar.svelte`) absorbed significant new features since (MON-57 avatar, MON-58 avatar placement, MON-66 archive lifecycle + Active/All sidebar toggle, MON-67 avatar state fix). Rebasing would not have been mechanical — every new feature needed re-porting into the refactored structure. This doc captures the redo plan against current master.

## Division of responsibilities

### Moves to `agentStore`

**State:**
- `agents: Agent[]`
- `projects: Project[]`
- `openTabs: string[]`
- `activeTabId: string | null`
- `sidebarCollapsed: boolean`
- `sidebarShowAll: boolean` (MON-66)

**Lifecycle:**
- `createAgent`, `restartAgent`, `spawnStoppedAgent`, `killAgent`, `updateAgent`, `newConversation`
- `openTab`, `closeTab`, `selectAgent`
- `switchToRecentAgent`, `switchToNextAgent`
- `setSidebarShowAll`, `setSidebarCollapsed`
- `archiveAgent`, `deleteAgent`, `summonAgent` (MON-66 primitives — confirms stay in App)

**Initialization / persistence:**
- `init()` — loads projects, UI prefs, agents, tab state
- `setupEffects()` — registers `$effect` for persisting UI state (must be called from a component context; `$effect` in a class constructor at module scope fails silently, same gotcha the first attempt hit)

**Internal:**
- `counter`, `exitListeners`, `tabHistory`, `uiStateInitialized`

### Stays in `App.svelte`

- Dialog state: `showSpawnDialog`, `showSettings`, `pendingConfirm`, `editingProject`
- `agentViewRef` + `activeCustomPrompt` (component-level bindings)
- Derived: `activeAgent`, `activeProject`, `currentLive`, `agentContext`
- Toolbox state (`openToolIds`, `toolboxWidth`)
- Zoom (`zoomLevel`, `applyZoom`, `handleWheel`) — out of scope per ticket
- Keybinding handler (`handleKeydown`) — out of scope per ticket; it calls store methods

## Why class-based

Svelte 5 forbids exporting bare `$state` variables from modules if they're reassigned (reassignment breaks the reactive proxy reference). The class-instance pattern sidesteps this: state lives as class fields, all mutations are property writes on a stable instance. This was the fix from the prior attempt and carries over.

## Confirm-flow coupling

`dismiss` and `delete` both need a confirm dialog. The dialog UI lives in `App.svelte` (renders `<ConfirmDialog>`). Flow:
1. Sidebar calls its local `ondismiss`/`ondelete` callbacks.
2. App sets `pendingConfirm = { kind, agent }`.
3. On confirm, App calls `agentStore.archiveAgent(id)` or `agentStore.deleteAgent(id)`.

`summon` is trivially reversible → no confirm → Sidebar calls `agentStore.summonAgent()` directly.

## Feedback-loop traps (inherited from prior attempt)

1. `$effect` persistence of `openTabs`/`activeTabId`/`sidebarCollapsed`/`sidebarShowAll` must be gated on `uiStateInitialized` so it doesn't fire during init and clobber saved state with defaults.
2. `tabHistory` stays a plain array (not `$state`) — its updater both reads and writes it; `$state` would cause an infinite loop. Updated from an `$effect` that watches `activeTabId`/`openTabs`.
3. `setSidebarShowAll` is imperative (not driven by an `$effect`) because `loadSavedAgents` writes to `activeTabId`/`openTabs` and would create a reactive loop.

## Consumers to rewire

| File | Current props dropped | New |
|------|----------------------|-----|
| `Sidebar.svelte` | `agents`, `projects`, `collapsed`, `activeId`, `showAll`, `onselect`, `onsummon`, `ontoggleshowall`, `oneditproject`, `onsavetemplate` (savetemplate stays — it's a local handler) | Imports `agentStore`. Keeps `ondismiss`/`ondelete`/`oneditproject`/`onsavetemplate` as props (dialog triggers owned by App). |
| `TabBar.svelte` | `agents`, `openTabs`, `activeTabId`, `onselect`, `onclose`, `onnewconversation` | Imports `agentStore` for state + `selectAgent`, `closeTab`, `newConversation`. |
| `SpawnDialog.svelte` | `projects` | Imports `agentStore` for `projects`. |
| `ProjectEditor.svelte` | `agents` (read-only) | Imports `agentStore` for `agents`. |
| `AgentView.svelte` | `onrestart`, `onspawn`, `onagentchange` | Imports `agentStore.restartAgent`, `spawnStoppedAgent`, `updateAgent`. Keeps `agent`, `projectName`, `onprojectedit`, `customPrompt` as props (per-instance). |

## Non-goals

- Extracting keyboard/zoom (separate ticket per MON-47 description).
- Refactoring `AgentView.svelte` internals.
- Touching Rust/sidecar or `liveAgentStore.svelte.ts`.

## Acceptance

- `src/lib/stores/agentStore.svelte.ts` exists, owns the state listed above.
- `App.svelte` shrinks substantially, retains only dialog/zoom/keybinding/derived-context concerns.
- Children import state directly rather than receiving it via props.
- `svelte-check` clean.
- Manual Tauri smoke: spawn, kill, restart, dismiss, delete, summon, Active/All toggle, tab switch, sidebar collapse, project editor, keybindings still work.
