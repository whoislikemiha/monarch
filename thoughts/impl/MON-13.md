# MON-13 — implementation notes

## What was implemented

The standalone `ContextInspector` modal that MON-11 mounted inside
`AgentView.svelte` has been retired and re-registered as the first real
toolbox tool (`context-inspector`, title `Context`, order `10`). The rail
is now the single way to open/close it, and agent switches re-point the
inspector at the new agent's live state without a remount flicker.

`AgentContext` grew a sibling `setup: { customPrompt, projectInstructions }`
field, populated at the `App.svelte` mount site, so prompt/project data
stays out of the event-stream-derived `LiveAgentState`. MON-14 can swap the
`live` producer to Rust without touching any tool component files.

## Key decisions

- **Setup field, not live state.** Custom prompt and project instructions
  are parent-sourced, not event-derived, so adding them to `LiveAgentState`
  would have forced MON-14's Rust emitter to learn about them too. The
  `setup` sibling on `AgentContext` keeps the MON-14 contract frozen.
- **`customPrompt` surfaced via `$bindable`.** `AgentView` still owns the
  `get_agent_prompt` load. `App.svelte` binds the value up so it can plug
  it into `agentContext.setup` without duplicating the fetch.
- **`SvelteMap` fix.** MON-12's `liveAgentStore.byAgent` was a plain `Map`
  wrapped in `$state`, which only tracks outer-property reassignment, not
  per-key `.get()` / `.set()`. Opening the tool, then spawning a new agent,
  left the panel stuck until remount because the `$derived(byAgent.get(id))`
  in `App.svelte` never re-ran. Swapped to `SvelteMap` from
  `svelte/reactivity`. Also the reason the MON-12 placeholder tool
  "worked" — its usual flow (open tool after agent already bound) avoided
  the race.
- **Stale `customPrompt = null` reset removed.** Pre-existing bug in
  `loadSessionMessages` that clobbered the prompt on every session swap;
  would have propagated up through the new `$bindable` as a flicker.
- **Margin hack for panel padding.** `ContextInspectorTool` uses
  `margin: -12px` to bleed into `ToolPanelStack`'s built-in 12px
  `.panel-body` padding so the summary/category/footer dividers stay
  edge-to-edge. The plan explicitly said not to restructure `ToolPanelStack`.

## Files touched

- `src/lib/toolbox/types.ts` — `AgentSetupContext` + `setup` on `AgentContext`.
- `src/lib/toolbox/tools/ContextInspectorTool.svelte` — new tool (derives
  everything from `agentContext.live/.agent/.setup`, empty state when null).
- `src/lib/toolbox/registry.ts` — registered `context-inspector` (order 10).
- `src/lib/toolbox/liveAgentStore.svelte.ts` — `SvelteMap` reactivity fix.
- `src-tauri/src/toolbox/mod.rs` — mirrored descriptor.
- `src/App.svelte` — `activeCustomPrompt` bound from `AgentView`;
  `agentContext.setup` populated from it + `activeProject?.instructions`.
- `src/lib/AgentView.svelte` — removed `ContextInspector` import,
  `showContextInspector` state, reset line, `oncontextinspect` wiring, and
  modal mount; `customPrompt` is `$bindable`; dropped `projectInstructions`
  prop; removed stray `customPrompt = null` reset.
- `src/lib/AgentControls.svelte` — removed `oncontextinspect` prop,
  clickable styling, onclick, a11y-ignore comments, `.clickable` CSS.
- `src/lib/ContextInspector.svelte` — deleted.

## What was left out

- No new inspector features (no editing, no compaction triggers, no
  per-category filters) — explicit out-of-scope in the plan.
- No restyle beyond sitting inside a panel body.
- No second tool.
- No per-agent persistence of which tools are pinned open — MON-12's
  global persistence was deemed sufficient.
- PromptEditor → `customPrompt` reload-on-save is still pre-existing
  behavior (modal didn't reload before either); not a regression.
