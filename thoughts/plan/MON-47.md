# MON-47: Extract frontend state from App.svelte into agentStore

## Summary

`App.svelte` is ~830 lines that owns all shared frontend state (`agents[]`, `openTabs[]`, `activeTabId`, `projects[]`) plus agent lifecycle functions, tab management, zoom, keybindings, and layout. Five child components receive this state via props, creating a 4-layer prop-drilling chain. The goal is to extract agent, tab, and project state into a module-level Svelte 5 store (`src/lib/stores/agentStore.svelte.ts`) so components can import what they need directly, turning `App.svelte` into a thin layout + keyboard routing shell.

## Relevant files and areas

| File | Role | Why relevant |
|------|------|-------------|
| `src/App.svelte` | App shell — owns all shared state | **Primary target.** Lines 28–38 declare state, 88–537 define lifecycle/tab/helper functions, 636–659 define derived values. All of this moves to the store. |
| `src/lib/types.ts` | Type definitions | Defines `Agent`, `Project`, `AgentConfig`, `SessionRecord`, `AgentViewState`, `ShadowIdentity`. The store will import these. |
| `src/lib/Sidebar.svelte` | Sidebar navigation | Receives `agents`, `projects`, `activeId`, `collapsed` as props + 5 callbacks. Will import `agents`, `projects`, `activeTabId` from store; callbacks become store function imports. |
| `src/lib/TabBar.svelte` | Tab strip | Receives `agents`, `openTabs`, `activeTabId` + 3 callbacks. All become store imports. |
| `src/lib/AgentView.svelte` | Agent chat/display | Receives `agent` (single, derived), `projectName`, `customPrompt` (bindable) + 4 callbacks. `agent` stays as a prop (it's a derived single value) or becomes a store-derived lookup. |
| `src/lib/SpawnDialog.svelte` | Agent creation dialog | Receives `projects` + 2 callbacks. `projects` becomes a store import. |
| `src/lib/ProjectEditor.svelte` | Project instructions editor | Receives `project`, `agents` + 2 callbacks. `agents` becomes a store import. |
| `src/lib/toolbox/ToolPanelStack.svelte` | Toolbox panels | Receives `openToolIds`, `agentContext`, `width` + 2 callbacks. `agentContext` is derived from store state + `liveAgentStore`. |
| `src/lib/toolbox/liveAgentStore.svelte.ts` | Per-agent live streaming state | **Not being modified**, but the new store's `agentContext` derivation will reference it. |
| `src/lib/toolbox/types.ts` | `AgentContext` type | Defines the shape consumed by toolbox tools — the store will export a derived `agentContext`. |
| `src/lib/keybindings.svelte.ts` | Keyboard binding system | Already module-level. Not modified, but the keyboard handler in App.svelte will call store functions. |
| `src/lib/api.ts` | IPC abstraction | `invoke`/`listen` — the store will use these for DB persistence and sidecar communication. |
| `src/lib/bindings.ts` | Tauri command types | `commands.spawnAgent` — called from `createAgent()` which moves to the store. |

## What needs to change

### 1. Create `src/lib/stores/agentStore.svelte.ts`

A new module-level Svelte 5 store containing:

- **State**: `agents`, `projects`, `openTabs`, `activeTabId`, `tabHistory`, `exitListeners` (internal), `counter` (internal), `uiStateInitialized` flag
- **Derived values**: `activeAgent`, `activeProject`
- **DB row interfaces**: `ProjectDbRow`, `AgentDbRow`, `SessionDbRow` (move from App.svelte)
- **Lifecycle functions**: `createAgent()`, `restartAgent()`, `spawnStoppedAgent()`, `killAgent()`, `updateAgent()`, `newConversation()`
- **Tab functions**: `openTab()`, `closeTab()`, `switchToRecentAgent()`, `switchToNextAgent()`, `selectAgent()`
- **Data loading**: `loadProjects()`, `loadSavedAgents()`, `loadUiState()`, `saveUiState()`
- **Helper**: `formatSpawnError()`, `createViewKey()`

All exported as named exports from the module (not a class or factory — just module-level `$state` and functions).

### 2. Slim down `App.svelte`

What **stays** in App.svelte (local UI concerns):

- `sidebarCollapsed` (UI-only, persisted via `saveUiState` which the store handles)
- `showSpawnDialog`, `showSettings`, `editingProject` (dialog open/close state)
- `zoomLevel`, `applyZoom()`, `handleWheel()` (zoom logic)
- `handleKeydown()` (keyboard routing — calls store functions)
- Toolbox local state: `openToolIds`, `toolboxWidth`, `toggleTool()`, `closeTool()`
- `agentViewRef` (component binding)
- `activeCustomPrompt` (bindable prop for AgentView)
- `agentContext` derivation (depends on store + liveAgentStore + local `activeCustomPrompt`)

What **moves** to the store: everything listed in section 1 above.

The `onMount` in App.svelte becomes a thin init call: `await agentStore.initialize()` (or call `loadProjects`, `loadSavedAgents`, `loadUiState` in sequence).

**Decision point**: `sidebarCollapsed` is persisted in `saveUiState()` alongside `openTabs` and `activeTabId`. Either (a) move `sidebarCollapsed` to the store too so persistence is co-located, or (b) split persistence so App.svelte saves its own UI state. Option (a) is cleaner.

### 3. Update child components

For each component, replace prop-drilled state with direct store imports:

- **Sidebar**: Remove `agents`, `projects`, `activeId` props → import from store. Keep `collapsed` as prop (it's App-local UI state, unless moved to store). Callbacks like `onkill` become `killAgent` imported from store.
- **TabBar**: Remove `agents`, `openTabs`, `activeTabId` props → import from store. Callbacks become store imports.
- **SpawnDialog**: Remove `projects` prop → import from store.
- **ProjectEditor**: Remove `agents` prop → import from store. Keep `project` prop (it's the specific editing target). `onupdate` callback calls a store function to update projects.
- **AgentView**: Keep `agent` as a prop (it's a derived single agent, and re-keyed by `viewKey`). Callbacks like `onrestart`, `onspawn` become store imports. Or keep them as props if the component should stay decoupled.
- **ToolPanelStack**: `agentContext` derivation might stay in App.svelte and be passed as prop, since it depends on local `activeCustomPrompt`.

### 4. Incremental migration strategy

To avoid a big-bang change:
1. Create the store file with state + functions.
2. Have App.svelte import and delegate to the store (verify nothing breaks).
3. Progressively update child components to import from store directly, removing props one component at a time.
4. Final cleanup: remove dead props and intermediary code from App.svelte.

## Open questions

1. **Should `sidebarCollapsed` move to the store?** It's persisted alongside tab state in `saveUiState()`. Moving it keeps persistence logic co-located. Keeping it in App.svelte means splitting the persistence effect. Recommendation: move it.

2. **Should `AgentView` import lifecycle functions directly or keep callback props?** The component is already tightly coupled to the agent model. Direct imports would simplify the interface but reduce reusability (not a concern in practice). Recommendation: direct imports, but would like your take.

3. **Where does `agentContext` get derived?** It depends on `activeAgent` (store), `liveAgentStore` (existing store), and `activeCustomPrompt` (App-local). Options: (a) derive in App.svelte and pass as prop, (b) move `activeCustomPrompt` to the store and derive `agentContext` there. Recommendation: keep `agentContext` derivation in App.svelte for now since `activeCustomPrompt` is a bindable AgentView prop.

4. **Should toolbox state (`openToolIds`, `toolboxWidth`) also move to the store?** It's currently persisted separately via `toolbox/persistence.ts`. Could stay in App.svelte or get its own store. Recommendation: leave it — it's already self-contained with its own persistence module.

## Out of scope

- Extracting keyboard/zoom logic into a separate module (future ticket)
- Refactoring `AgentView.svelte` internals
- Changes to the Rust/sidecar layer
- Modifying `liveAgentStore.svelte.ts` (per-agent runtime state)
- Changing the toolbox persistence system
