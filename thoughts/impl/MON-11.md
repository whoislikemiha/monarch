# MON-11: Context Inspector Sidebar

## What was implemented

A persistent right-side panel that shows a structured breakdown of the LLM's context window contents. Triggered by clicking the context meter bar in AgentControls. Categories group context by type (user messages, assistant messages, thinking, tool calls, tool outputs) with estimated token counts. Each category is collapsible/expandable for deeper inspection.

## Key decisions

- **Token estimation via ~4 chars/token heuristic** — the API gives total input/output tokens but no per-category breakdown, so we estimate from text content. The summary bar uses real API totals.
- **Wrapper div approach** — added `agent-view-wrapper` as a horizontal flex container around `agent-view` + inspector, rather than restructuring the existing agent-view div, to minimize layout disruption.
- **Derived categories** — all categories are computed reactively from `DisplayItem[]` using `$derived.by()`, so they update live as the conversation progresses.

## Files touched

- `src/lib/ContextInspector.svelte` — new component (the sidebar panel)
- `src/lib/AgentControls.svelte` — made context meter clickable, added `oncontextinspect` prop
- `src/lib/AgentView.svelte` — wrapper layout, state management, wiring

## What was left out

- System prompt category — would need fetching the prompt file content; deferred for now.
- Per-file grouping for Read tool outputs — tool results are grouped generically, not by file path.
- Resizable panel width.
