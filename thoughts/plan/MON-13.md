# MON-13 — Adapt context inspector as first toolbox tool

## Summary

MON-11 shipped `src/lib/ContextInspector.svelte` as a modal overlay mounted inside `AgentView.svelte` and toggled by a clickable context meter on `AgentControls`. MON-12 landed a right-side toolbox rail, a pluggable tool registry (`src/lib/toolbox/`), a shared `liveAgentStore`, and a `ToolPanelStack` host that renders any registered tool with a uniform `{ agentContext }` prop. This task retires the modal and registers the inspector as the first real tool in that registry, so the rail becomes the single way to open/close it and agent switches re-point the inspector at the new agent's live state without a remount flicker.

**MON-14 context.** The next refactor (MON-14) makes Rust the canonical owner of `LiveAgentState` and explicitly freezes the `AgentContext.live` contract — "zero diff to tool component files". That constrains the prompt-data routing decision below: `customPrompt` and `projectInstructions` are **not** event-stream state, so they must not live on `LiveAgentState`; otherwise MON-14's Rust emitter has to learn to populate them, which is out of its scope. Instead we add a sibling field on `AgentContext` (e.g. `setup`) fed from the parent mount site. `.live` stays purely event-derived and migrates cleanly in MON-14.

## Relevant files and areas

- `src/lib/ContextInspector.svelte` — the component to migrate. Current shape: a standalone panel with its own header, close button, summary, categories, and hard-coded 320px width (`src/lib/ContextInspector.svelte:321-683`). Props today: `items`, `lastUsage`, `contextWindow`, `sessionStats`, `customPrompt`, `projectInstructions`, `shadow`, `onclose`. It must be rewritten to take the `ToolProps` shape (`{ agentContext }`) and drop `onclose` + its header chrome, since `ToolPanelStack` already supplies those (`src/lib/toolbox/ToolPanelStack.svelte:75-88`).
- `src/lib/toolbox/registry.ts` — the registration point. Currently only `placeholder`. A new entry (`context-inspector` or similar) must be added here with icon/title/order/component, and most likely `hasBackend: false` since the inspector is pure frontend today (`src/lib/toolbox/registry.ts:9-18`).
- `src/lib/toolbox/types.ts` — defines `AgentContext` as `{ agentId, agent, live }`. Extend this with `setup: { customPrompt: string | null; projectInstructions: string | null }`. Keep `live` untouched so MON-14 can swap it wholesale (`src/lib/toolbox/types.ts:15-46`).
- `src/lib/toolbox/liveAgentStore.svelte.ts` — unchanged in shape. `LiveAgentState` stays event-derived only; this file is MON-14's swap target (`src/lib/toolbox/liveAgentStore.svelte.ts:12-31`).
- `src/lib/toolbox/ToolPanelStack.svelte` — host. Already passes `agentContext` through to tool components and owns the panel title + close button (`src/lib/toolbox/ToolPanelStack.svelte:72-89`). No structural change expected; only confirm the inspector looks right inside a `.panel-body` that uses flex + `overflow: auto` + 12px padding.
- `src/lib/AgentView.svelte` — the modal is mounted at `959-970`, state at `53`, reset at `723`, trigger wired to `AgentControls` at `948`. Also owns `customPrompt` loading (`47`, `350`, `754`) and forwards `projectInstructions` from `App.svelte` (`32`, `965-966`). The modal mount block, the `showContextInspector` state, and the `oncontextinspect` prop wiring must all be removed. `customPrompt` stays local to `AgentView` and, along with the forwarded `projectInstructions`, is surfaced to the toolbox via the `AgentContext.setup` field at the mount site where `ToolPanelStack` / rail consumers receive `agentContext`.
- `src/lib/AgentControls.svelte` — exposes `oncontextinspect` prop and a clickable context meter (`src/lib/AgentControls.svelte:14, 25, 160-164`). The callback prop and the `class:clickable` / `onclick` wiring on the context meter must be removed; the meter returns to a static display. The meter itself stays — it's independent of the inspector.
- `src-tauri/src/toolbox/mod.rs` — returns `ToolDescriptor` list to the frontend. If the new tool has no backend commands (likely), a descriptor entry for the inspector should still be added so `toolbox_list_tools` matches the frontend registry, preserving the cross-check MON-12 set up (`src-tauri/src/toolbox/mod.rs:18-28`). No new `src-tauri/src/toolbox/context_inspector.rs` module is needed unless we decide to back something server-side.
- `thoughts/impl/MON-11.md`, `thoughts/plan/MON-12.md`, `thoughts/impl/MON-12.md` — prior-art notes; MON-12's impl explicitly defers this migration to MON-13 and flags the z-index constraint that disappears once the modal is gone.

## What needs to change

At the module / concept level:

1. **Inspector becomes a tool component.** Rewrite `ContextInspector.svelte` to accept only `ToolProps` (`{ agentContext }`). Internally, derive everything from `agentContext.live` and `agentContext.agent` (items, lastUsage, contextWindow, sessionStats, shadow). Strip the inspector-owned header row and `onclose` button; rely on `ToolPanelStack`'s panel chrome. Remove the fixed-width panel CSS and let it fill the panel body; keep the summary + category list styling.

2. **Empty state inside the tool.** When `agentContext === null`, render a clear "No agent active" empty state (the current modal never mounted in that case; now the tool is always mounted). Matches the placeholder tool's pattern (`PlaceholderTool.svelte:47-49`).

3. **Route prompt + project-instructions data to the tool via `AgentContext.setup`.** Extend `AgentContext` in `src/lib/toolbox/types.ts` with a sibling field `setup: { customPrompt: string | null; projectInstructions: string | null }`. At the site where the active agent's `AgentContext` is constructed (parent of `ToolRail` / `ToolPanelStack`, ultimately driven from `AgentView`), populate it from `AgentView`'s local `customPrompt` state and `projectInstructions` prop. The inspector reads `agentContext.setup` directly. Rationale: keeps `live` purely event-stream state so MON-14 can swap its producer to Rust with no contract churn, and keeps file-sourced / parent-sourced config out of the Rust-owned path.

4. **Register the tool.** Add an entry to `TOOLS` in `src/lib/toolbox/registry.ts`: id `context-inspector`, title `Context`, `order: 10` (above the placeholder's 100), `hasBackend: false`, and a compact inline SVG layers icon. The panel stack renders tools in `order` ascending, so the inspector sits at the top of the rail.

5. **Mirror on the Rust side.** Append a `ToolDescriptor { id: "context-inspector", title: "Context" }` entry in `src-tauri/src/toolbox/mod.rs::descriptors()` for parity with the frontend registry. No new submodule — the inspector has no backend commands in this issue, and MON-14 will not give it any either (MON-14 adds state emission at the agent level, not per-tool).

6. **Delete the modal wiring.**
   - `AgentView.svelte`: remove the `import ContextInspector`, the `showContextInspector` `$state`, the reset in the view-state snapshot path around line 723, the `oncontextinspect` prop forwarded to `AgentControls`, and the entire `{#if showContextInspector} … {/if}` mount block at 959-970.
   - `AgentControls.svelte`: remove `oncontextinspect` from props and its type, and strip the `class:clickable` / `onclick` from the context meter element; keep the meter visuals.

7. **Verify snapshot/restore of view state still round-trips.** `AgentView`'s `snapshotViewState` / `restoreCachedViewState` currently touch `showContextInspector` (at least via the reset at 723). Confirm nothing else depends on it, and that removing it does not break the view-state cache shape. The acceptance criteria call this out explicitly.

8. **Manual smoke test path.** Spawn two agents, open `Context` from the rail, send messages to each, switch agents, confirm category breakdown and occupancy update live without remount flicker and without a stale snapshot from the previous agent. Close via the panel header × to confirm the rail toggle is the only close path.

## Resolved decisions

1. **Prompt data routing** — extend `AgentContext` with a sibling `setup: { customPrompt, projectInstructions }` field populated at the mount site from `AgentView`'s local state + `projectInstructions` prop. Does not touch `LiveAgentState`, so MON-14 can swap the event-state producer to Rust without touching tool files.
2. **Naming** — id `context-inspector`, title `Context`, order `10`.
3. **Icon** — compact inline SVG stack-of-layers.
4. **Default open state** — closed. User clicks the rail icon to open. MON-12's global persistence remembers their choice.

## Open questions

None blocking implementation. Minor confirmations to make during the work:

- Empty-state copy when no agent is active (default: "No agent active").
- Exact mount site for building `AgentContext` — today `ToolPanelStack` and `ToolRail` are consumed by whichever parent composes the agent shell. Confirm during implementation that a single construction site exists so `setup` is populated once, not twice.

## Out of scope reminders

- No new inspector features — no editing, no compaction triggers, no per-category filters.
- No restyle beyond what's required to sit inside a panel body (drop the fixed width / own header; keep the visuals).
- No second tool added as part of this issue.
- No per-agent persistence of which tools are pinned open — MON-12's global persistence is sufficient.
- No changes to the context meter's appearance on `AgentControls` beyond removing the click handler and clickable styling.
- No backend logic added unless answering open question 1 pulls it in.
