# MON-12 — Right-side toolbox rail and pluggable tool registry

## Summary

Introduce a VSCode-style vertical icon rail on the right edge of the Monarch app shell, backed by a pluggable tool registry that spans both the Svelte frontend and the Rust backend. The rail lives in `App.svelte` as global chrome (always visible, outside the `{#key activeAgent.viewKey}` remount boundary), while each registered tool is an agent-scoped component that receives the currently active agent's live state and re-renders reactively on agent switch. Unlike VSCode's activity bar, multiple tools can be open simultaneously and their panels stack vertically inside a shared, horizontally-resizable panel region to the left of the rail. Adding a new tool should require only a registry entry, a Svelte component with a fixed prop contract, and (optionally) a typed Tauri command for backend access. This issue ships the rail, the registry, persistence of rail state, a shared per-agent live-state store that tools consume, and a trivial placeholder tool that exercises both the frontend registration and a typed Rust command. The existing `ContextInspector` modal is left alone; its migration onto the new surface ships as MON-13.

## Relevant files and areas

### Frontend — layout surfaces the rail plugs into

- `src/App.svelte` — root shell. Currently renders `Sidebar` + `.main-panel` as two horizontal flex siblings inside `main.app` (lines 459–552, styles 596–614). The new layout adds two more siblings so the order becomes **`[Sidebar] [main-panel] [ToolPanelStack] [ToolRail]`** — four siblings, rail and panel stack are independent of the main panel. `activeAgent` / `activeId` / `agents` already live here and are the natural source of the active-agent context.
- `src/lib/AgentView.svelte` — per-agent workspace. Lines 8, 41–53, 661, 897, 908–914 currently own the live per-agent state (`items`, `toolExecutions`, `streamingMessage`, `lastUsage`, `currentToolGroup`) *as local `$state`* and mount `ContextInspector` as a modal overlay. Under MON-12, AgentView's **local state becomes a consumer of a shared store** rather than the canonical owner — see "Shared per-agent live-state store" below. The modal overlay wiring stays put this round; MON-13 deletes it.
- `src/lib/AgentControls.svelte` — owns the context inspector trigger button. Untouched in MON-12.
- `src/lib/ContextInspector.svelte` — untouched here.
- `src/lib/api.ts` — the Tauri invoke wrapper added in MON-10. All toolbox Rust calls must go through this, not the raw `@tauri-apps/api` import, to stay consistent with the rest of the codebase.
- `src/lib/types.ts` — shared types. The `Agent` type lives here; `AgentContext`, `LiveAgentState`, and `ToolDefinition` go in `src/lib/toolbox/types.ts` instead to keep the toolbox self-contained.

### Frontend — new module

- `src/lib/toolbox/` — new directory housing:
  - `types.ts` — `ToolDefinition`, `AgentContext`, `ToolProps`.
  - `registry.ts` — the plain-array registry.
  - `ToolRail.svelte` — the icon strip.
  - `ToolPanelStack.svelte` — the stacked panel region.
  - `persistence.ts` — localStorage helpers (width, open ids).
  - `liveAgentStore.ts` — the shared per-agent live-state store (see below).
  - `tools/PlaceholderTool.svelte` — the verification tool.

### Backend — registration surface

- `src-tauri/src/lib.rs` — Tauri command registry and state wiring (lines 1–80+, plus the full `invoke_handler!` macro farther down). The new `toolbox` module is declared here alongside `agent`, `db`, `models`, `persistence`, `ws`, and its Tauri commands are added to `invoke_handler!`.
- `src-tauri/src/toolbox/` — new module. `mod.rs` exposes a `ToolDescriptor` struct and a static list of descriptors; `placeholder.rs` (or equivalent) carries the placeholder tool's typed Tauri command(s).
- `src-tauri/src/agent.rs`, `db.rs`, `models.rs` — reference only, as examples of how other subsystems register typed Tauri commands and share state via `tauri::State`. No changes.

### Docs

- `ONBOARDING.md` — §7 "Frontend layout" (component tree around lines 344–366) and §12 "File-path reference" (around line 505). A new subsection "Adding a toolbox tool" belongs inside or adjacent to §7, with file-path additions in §12.

## What needs to change

### 1. Frontend tool registry (`src/lib/toolbox/registry.ts` + `types.ts`)

`types.ts` defines:

- `AgentContext` — the shape passed into every tool: `{ agentId: string; agent: Agent; live: LiveAgentState } | null`. The `live` field is the live state bundle (items, tool executions, streaming message, last usage, current tool group, session stats) read from the shared store.
- `ToolProps` — exactly `{ agentContext: AgentContext }`. All tool components must accept this prop and nothing else.
- `ToolDefinition` — `{ id: string; title: string; icon: string; component: Component<ToolProps>; order?: number; hasBackend?: boolean }`. The `component` field is typed with Svelte 5's `Component<Props>` type so the registry enforces the prop contract at compile time. `icon` is an inline SVG string (decision: inline strings over per-tool icon components; v1 has few tools, one file to scan).

`registry.ts` exports a single `TOOLS: ToolDefinition[]` constant. Adding a tool means editing this one file and creating its component file. No dynamic registration, no runtime mutation.

### 2. Shared per-agent live-state store (`src/lib/toolbox/liveAgentStore.ts`)

**This is the most important new piece.** Today, `AgentView.svelte` owns `items`, `toolExecutions`, `streamingMessage`, `lastUsage`, `currentToolGroup`, etc. as local `$state` populated from the `agent-event-{id}` Tauri stream. Toolbox tools live outside `AgentView` (they're in `App.svelte`) and can't reach that state directly.

The fix is to lift the **data** out of `AgentView` into a shared store keyed by agentId:

```
liveAgentStore = $state(new Map<agentId, LiveAgentState>())
```

Where `LiveAgentState` is a plain object containing everything AgentView currently tracks per agent: items, tool executions, streaming message, last usage, current tool group, session stats, activity status, event count.

**Shape (Svelte 5 note):** Svelte 5 does not make exported primitive `$state` reactive across module boundaries — the reactivity lives on the *outer* proxy. Wrap in an object:

```ts
// src/lib/toolbox/liveAgentStore.ts
export const liveAgentStore = $state({
  byAgent: new Map<string, LiveAgentState>(),
});
```

Consumers import `liveAgentStore` and operate on `liveAgentStore.byAgent`. `Map` mutations are tracked by Svelte 5's reactivity. Module-level `$state` is chosen over the `setContext`/`getContext` API because Monarch has no SSR concerns (Tauri desktop only) and the context API adds boilerplate at every consumer. HMR may occasionally duplicate the store during dev; a restart resolves it.

**Who writes to it:** whoever subscribes to `agent-event-{id}`. In MON-12 that is still `AgentView.svelte`'s existing event handler — but it writes **only** into the store entry for its agent, no local `$state` mirror. Rendering reads from the store as well. Single source of truth.

**Who reads from it:** `AgentView` (for rendering its message list, usage counter, tool groups) and toolbox tools (via the `AgentContext.live` prop their parent passes in). `App.svelte` derives `currentLive = $derived(activeId ? liveAgentStore.byAgent.get(activeId) ?? null : null)` and plugs it into `AgentContext` before handing it to `ToolPanelStack`.

**Lifecycle:** entries are created on agent spawn, updated by AgentView's event handler, and removed on `killAgent`.

**Full extraction, not mirror.** The original plan floated a "mirror" approach where AgentView kept its local `$state` and *also* wrote to the store. That is rejected: keeping two reactive proxies in sync for the same data doubles the reactivity work on every streaming event (and `items` can grow large), adds a divergence risk, and creates ambiguity over which is canonical. Full extraction is the right call — the event handler change is mechanical (`items = …` → `live.items = …`), the rendering sites swap `items` → `live.items`, and the snapshot/restore path either disappears or becomes a thin pass-through.

**Impact on `agentViewStates` cache.** The existing per-agent view state cache in `App.svelte` (`persistCurrentViewState` / `getCachedState`) exists specifically because `{#key activeAgent.viewKey}` remounts `AgentView` on agent switch and local state would otherwise be lost. With live state owned by the store instead of AgentView's locals, the cache is largely redundant — on remount, AgentView can read directly from `liveAgentStore.byAgent.get(boundAgentId)`. The cache deletion belongs in this PR as the final cleanup step, not a follow-up. Any fields the cache holds that are *genuinely* UI-local and not part of the live state (e.g. scroll position, `showStderr`) stay in AgentView's local state and are the exception — the store only owns what today comes from the event stream.

MON-12's acceptance is proven by the placeholder tool reading at least one field from `AgentContext.live` and displaying it — that exercises the full store path end-to-end.

### 3. Rail component (`src/lib/toolbox/ToolRail.svelte`)

A thin vertical strip rendered on the right edge. Reads `TOOLS`, renders one icon button per entry sorted by `order`. Clicking toggles that tool's id in the parent's `openToolIds` state. Active/inactive visual states; tooltip on hover showing `title`; keyboard accessible (focusable, Enter/Space activates).

### 4. Panel stack (`src/lib/toolbox/ToolPanelStack.svelte`)

Renders the stacked panels between `.main-panel` and the rail. Props: `openToolIds: string[]`, `agentContext: AgentContext`, plus a close callback.

For each id in `openToolIds`, looks up the `ToolDefinition`, renders a small panel header (title + close X) and its component with `{ agentContext }` passed in. Panels stack vertically with **equal splits for v1** (no drag-resize between them). Each panel enforces a **minimum height** (e.g. 160px); if the combined minimums exceed the available height, the panel region becomes vertically scrollable. Draggable vertical splits are a follow-up.

### 5. Horizontal splitter + width persistence

A drag handle on the left edge of `ToolPanelStack` resizes it horizontally. Width persists in `localStorage` under `monarch.toolbox.width`. Open tool ids persist under `monarch.toolbox.openIds`. Both restored on mount, defaults if absent.

Width constraints: **min 240px, max 600px**, clamped on both drag and restore. On very narrow windows (< ~900px total), the panel stack should collapse to 0 and require the user to hide the left sidebar (`Ctrl+B`) to regain space — no auto-hide logic in v1.

### 6. Rust-side tool registry (`src-tauri/src/toolbox/`)

A new module mirroring `agent` / `db` / `models` in style. Exposes:

- `ToolDescriptor { id: String, title: String }` — metadata only. A `static` or lazy_static list of descriptors.
- A Tauri command `toolbox_list_tools() -> Vec<ToolDescriptor>` — returns the descriptors so the frontend can cross-check registration.
- **Direct typed Tauri commands per tool** (decided, not facade). The placeholder ships `toolbox_placeholder_ping() -> String` returning something like `"pong @ 2026-04-10T14:42:00Z"`. Future tools that need backend capabilities add their own typed commands under `src-tauri/src/toolbox/<tool_name>.rs` and register them in `lib.rs`'s `invoke_handler!` like everything else.

This is idiomatic Tauri, preserves typing, matches every other subsystem in the repo, and doesn't paint us into a JSON-only facade corner. The issue AC's "stable invoke surface" phrasing will be softened to match: the stability comes from descriptor-backed registration and typed commands, not from a single generic dispatcher.

### 7. Placeholder tool (`src/lib/toolbox/tools/PlaceholderTool.svelte` + `src-tauri/src/toolbox/placeholder.rs`)

Frontend:
- Accepts `{ agentContext }: ToolProps`.
- Displays its title, the active agent id or "no agent", and — **critically for MON-12's end-to-end proof** — one field read from `agentContext.live` (e.g. "messages so far: N") so the store path is exercised.
- Button: "Ping backend" → calls `toolbox_placeholder_ping` via `src/lib/api.ts`'s invoke wrapper, shows the returned string.

Backend:
- A typed `toolbox_placeholder_ping` command returning a static-ish string.

### 8. `App.svelte` integration

Layout becomes four flex siblings:

```
main.app (flex row)
├── Sidebar          (toggleable)
├── .main-panel      (AgentView / CouncilView / empty state)
├── ToolPanelStack   (width = toolboxWidth if any tool open, else 0)
└── ToolRail         (fixed width)
```

`App.svelte` gains:

- `openToolIds: string[] = $state(restoreOpenIds())`
- `toolboxWidth: number = $state(restoreWidth())`
- `currentLive = $derived(activeId ? liveAgentStore.get(activeId) ?? null : null)`
- `agentContext = $derived(activeAgent && currentLive ? { agentId: activeAgent.id, agent: activeAgent, live: currentLive } : null)`
- `$effect` blocks that persist `openToolIds` and `toolboxWidth` to localStorage on change.
- Handlers wiring rail clicks → toggle in `openToolIds`, panel close → remove from `openToolIds`.

The `ToolRail` is always rendered. `ToolPanelStack` is always in the DOM but has zero width (CSS) when `openToolIds.length === 0`. The rail is **not** inside any `{#key}` block — tools never remount on agent switch.

**Council mode:** `CouncilView` replaces `.main-panel` but the rail + panel stack still render. When `councilMode` is true, `agentContext` is forced to `null` and tools show their empty state. Council-aware tools are out of scope.

**Z-index:** `SpawnDialog`, `HistoryPanel`, `ExtensionDialog`, `PromptEditor`, `ProjectEditor`, and AgentView's existing `ContextInspector` overlay are all modal-style and must continue to sit **above** the toolbox panels. The toolbox is inline chrome, not modal.

**No new keybindings in v1.** The rail has no toggle shortcut. Individual tools have no activation shortcuts. `Ctrl+B` still only toggles the left sidebar. Keybindings can be added in a follow-up once real tools exist.

### 9. Tool-author design constraint (docs)

Because toolbox tools **stay mounted across agent switches**, any local state a tool keeps (expanded sections, scroll position, filter selections) will appear to "leak" from one agent into the next. This is an intentional performance choice (no remount flicker) but has consequences for tool authors. The docs subsection must explicitly state:

> Tools must derive their display from `agentContext` reactively. If a tool needs per-agent memory (e.g. remembering which categories were expanded per agent), it must key that state by `agentContext.agentId` itself — the framework will not remount on agent switch.

### 10. Docs — `ONBOARDING.md`

A new short subsection "Adding a toolbox tool" under §7 "Frontend layout":

1. Create `src/lib/toolbox/tools/YourTool.svelte`. Accept exactly `{ agentContext }: ToolProps`. Derive display from `agentContext` reactively; key any per-agent state by `agentContext.agentId` yourself.
2. Add a `ToolDefinition` entry to `src/lib/toolbox/registry.ts`.
3. (Optional) For backend access, add `src-tauri/src/toolbox/your_tool.rs` with typed Tauri commands, declare it in `src-tauri/src/toolbox/mod.rs`, and register commands in `src-tauri/src/lib.rs`'s `invoke_handler!`. Add a matching `ToolDescriptor` to the descriptors list.
4. All frontend `invoke()` calls go through `src/lib/api.ts`, never the raw Tauri import.

Plus file-path reference additions in §12 for `src/lib/toolbox/*` and `src-tauri/src/toolbox/*`.

## Open questions

*(Resolved after review — left here as a record of the decisions made during planning.)*

1. **Store extraction approach.** *Decided: full extraction.* AgentView writes to and reads from `liveAgentStore` exclusively; no local `$state` mirrors for live data. The `agentViewStates` cache in `App.svelte` is deleted in this same PR since the store survives the `{#key}` remount. UI-local fields like scroll position or `showStderr` stay in AgentView's local state — only event-stream-derived data moves.

2. **Placeholder exercises the store path.** *Decided: yes.* The placeholder reads at least one field from `agentContext.live` (e.g. message count) and displays it. Proves the full data path in MON-12 so any bug in the store is caught here, not blamed on the MON-13 inspector migration.

3. **Store shape.** *Decided: module-level `$state` wrapped in an outer object* (`export const liveAgentStore = $state({ byAgent: new Map() })`). No context API. No SSR concerns in Tauri desktop. HMR dev-only quirk is acceptable.

## Out of scope reminders

- No adaptation of the existing context inspector (MON-13 handles that).
- No second real tool — placeholder only.
- No drag-to-reorder rail icons.
- No draggable vertical splits between stacked panels (equal split + min-height + scroll for v1).
- No per-tool settings UI.
- No per-agent persistence of tool open state.
- No new keybindings.
- No council-aware tools.
- No auto-hide on narrow windows.
- No animations/polish beyond "it looks intentional".
- No changes to `AgentView`'s existing modal overlays (`ContextInspector`, `PromptEditor`, `HistoryPanel`) beyond unavoidable CSS reflow from the new right-side siblings.
