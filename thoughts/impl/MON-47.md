# MON-47: Extract frontend state from App.svelte into agentStore

## Shipped

Class-based Svelte 5 store at `src/lib/stores/agentStore.svelte.ts` owns the shared frontend state and agent lifecycle. `App.svelte` dropped from ~960 → ~380 lines and is now a thin shell (dialog state, zoom, keybinding routing, derived per-active-agent context). Children import the store directly instead of receiving state via props.

**Net:** +695 / -647 lines. The store file is bigger than the slice it replaced because what was scattered across `App.svelte`'s `<script>` is now documented and organized under one class with explicit sections.

## Files

- `src/lib/stores/agentStore.svelte.ts` — new. `agents`, `projects`, `openTabs`, `activeTabId`, `sidebarCollapsed`, `sidebarShowAll` as `$state`; lifecycle + tab/sidebar methods; `init()` + `setupEffects()` for startup wiring.
- `src/App.svelte` — rewritten as thin shell. Keeps dialog state (`showSpawnDialog`, `showSettings`, `pendingConfirm`, `editingProject`), toolbox state, zoom, `handleKeydown`, and the derived `activeAgent` / `activeProject` / `currentLive` / `agentContext` chain consumed by `ToolPanelStack`.
- `src/lib/Sidebar.svelte` — dropped 5 state props + 4 lifecycle callbacks. Only dialog-trigger callbacks (`oncreate`, `ondismiss`, `ondelete`, `oneditproject`, `onsavetemplate`) remain as props.
- `src/lib/TabBar.svelte` — dropped all props; reads everything from the store.
- `src/lib/SpawnDialog.svelte` — dropped `projects` prop.
- `src/lib/ProjectEditor.svelte` — dropped `agents` prop.
- `src/lib/AgentView.svelte` — dropped `onrestart`, `onspawn`, `onagentchange` props. `onprojectedit` stays (triggers App-owned modal).
- `thoughts/plan/MON-47.md` — plan doc.

## Division of responsibilities (final)

### `agentStore`

**State:** `agents`, `projects`, `openTabs`, `activeTabId`, `sidebarCollapsed`, `sidebarShowAll`.

**Methods:**
- Init: `init()` (loads projects → UI prefs → agents → tab state in that order), `setupEffects()` (registers the `$effect`s that need a component owner).
- Lifecycle: `createAgent`, `restartAgent`, `spawnStoppedAgent`, `killAgent`, `updateAgent`, `newConversation`, `archiveAgent`, `deleteAgent`, `summonAgent`.
- Tabs: `openTab`, `closeTab`, `selectAgent`, `switchToRecentAgent`, `switchToNextAgent`, `switchToTabIndex`.
- Sidebar: `setSidebarShowAll`, `toggleSidebarCollapsed`.
- Queries: `getAgent`.
- Project edit: `replaceProject` (called from App's `ProjectEditor` `onupdate` to avoid re-fetching all projects for a single row change).

### `App.svelte`

- Dialog state and rendering for Spawn / Settings / ProjectEditor / two ConfirmDialogs.
- Confirm flow: Sidebar fires `ondismiss` / `ondelete` → App sets `pendingConfirm` → on confirm, App calls `agentStore.archiveAgent` or `agentStore.deleteAgent`.
- Zoom (`zoomLevel`, `applyZoom`, `handleWheel`) — out of scope per ticket.
- Keybindings (`handleKeydown`) — out of scope per ticket; the handler calls into `agentStore` (`toggleSidebarCollapsed`, `switchToTabIndex`, `switchToRecentAgent`, `switchToNextAgent`) and `invoke("send_command", …)` directly for abort.
- Derived `activeAgent` / `activeProject` / `currentLive` / `agentContext` chain consumed by `ToolPanelStack`. Kept in App because `agentContext` depends on `activeCustomPrompt` (a `$bindable` tied to `AgentView`'s local edit state) and `projects` lookups — none of these need to be accessed from outside.

## Gotchas hit / preserved

- **`$effect` in a class constructor silently no-ops.** Same trap the prior attempt hit. Fix: `setupEffects()` called from `App.svelte` setup script. Documented at the method and in the plan doc so future work keeps the split.
- **Reactive feedback loops.** Three places still hand-enforced:
  1. `uiStateInitialized` gates the persistence `$effect`. Flipped true at the end of `init()`.
  2. `tabHistory` is a plain array, not `$state` — its maintainer effect both reads and writes it.
  3. `setSidebarShowAll` is an imperative method, not driven by an `$effect` on the boolean. `loadSavedAgents` writes to `openTabs` / `activeTabId`, which would cycle if reactive.
- **Theme stayed in App.** `applyTheme` writes to the DOM before first paint — belongs where onMount runs, not in the store. Store intentionally doesn't touch DOM.
- **Confirm dialogs stayed in App** because the ConfirmDialog UI is rendered at that level. The store exposes `archiveAgent` / `deleteAgent` primitives; App owns the "are you sure" coupling.

## Why a fresh branch instead of rebasing PR #42

PR #42 was 49 commits behind master, including MON-57 (avatar), MON-58 (avatar placement in sidebar + header), MON-66 (archive lifecycle + Active/All toggle), and MON-67 (avatar state fix). All four touched `App.svelte` / `Sidebar.svelte` — exactly the files this PR rewrites wholesale. On rebase, both files had content conflicts that weren't mechanical — every new feature needed re-porting into the refactored structure. After conflict 1 of 6, the remaining 5 commits would conflict again on top. A fresh redo against current master was cleaner, and the store captures all current state (including the new archive + Active/All machinery) from the start rather than incrementally absorbing it.

## Verification

- `npx svelte-check` — 0 errors, 1 pre-existing a11y warning in `KeybindingsSettings.svelte`.
- Manual smoke (Tauri dev): pending, to run before merge.
  - Spawn / kill / restart / new-conversation / dismiss / delete / summon
  - Active ↔ All sidebar toggle
  - Tab open / close / switch via Ctrl+1-9 / Ctrl+Tab / Ctrl+PageDown
  - Sidebar collapse
  - Project editor open + save
  - Keybinding `global.focus-chat`, `global.abort-agent`, `global.spawn-agent`, `global.settings`, `global.toggle-sidebar`

## Follow-ups (not in this PR)

- Keyboard + zoom extraction lives in its own ticket per the original MON-47 description. App's `handleKeydown` and zoom are large enough to be their own module.
- `AgentView.svelte`'s internals were intentionally untouched.
- `activeCustomPrompt` (bindable prompt-override string on `AgentView`) is still App-local; if more prompt-related state accumulates this could move into the store or a dedicated per-agent slice.
