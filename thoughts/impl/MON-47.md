# MON-47: Extract frontend state from App.svelte into agentStore

## What was implemented

Extracted all shared frontend state (agents, projects, tabs, sidebar) and lifecycle functions from `App.svelte` into a dedicated class-based Svelte 5 store at `src/lib/stores/agentStore.svelte.ts`. Child components now import state directly from the store instead of receiving it via prop-drilling through App.svelte.

## Key decisions

- **Class-based store, not module-level exports.** Svelte 5 forbids exporting `$state` variables that are reassigned from `.svelte.ts` modules. A class with a singleton instance (`export const agentStore = new AgentStore()`) is the standard workaround. Consumer API is `agentStore.agents` instead of bare `agents`.

- **`setupEffects()` method.** `$effect` requires a component owner — calling it in a class constructor at module scope fails silently. Effects (tab history tracking, UI state persistence) are set up via `agentStore.setupEffects()` called during App.svelte's component initialization.

- **`agentContext` stays in App.svelte.** It depends on `activeCustomPrompt` which is a bindable prop on AgentView, making it inherently local to the component tree. Kept the derivation in App.svelte rather than forcing it into the store.

- **Toolbox state stays in App.svelte.** `openToolIds` and `toolboxWidth` already have their own persistence module (`toolbox/persistence.ts`), so moving them would add complexity without benefit.

## Files touched

- **Created:** `src/lib/stores/agentStore.svelte.ts` — the store
- **Rewritten:** `src/App.svelte` (~830 → ~385 lines)
- **Modified:** `src/lib/Sidebar.svelte`, `src/lib/TabBar.svelte`, `src/lib/SpawnDialog.svelte`, `src/lib/ProjectEditor.svelte`, `src/lib/AgentView.svelte` — updated to import from store

## What was left out

- Keyboard/zoom logic extraction (separate ticket per plan)
- Refactoring AgentView internals
- Changes to Rust/sidecar layer
- Modifying liveAgentStore.svelte.ts
