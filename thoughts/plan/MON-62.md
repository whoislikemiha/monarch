# MON-62: Shadow Stats UI — Profile Card

## Summary

Build a toolbox tool that displays a shadow's lifetime stats, specialization profile, and tool usage breakdown. The data comes from `db_get_agent_stats` (MON-63). The component follows the existing toolbox tool pattern (ToolProps, agentContext, invoke via api.ts) and registers in the toolbox registry alongside ContextInspector and Placeholder. It renders stats using the dark theme CSS variables and existing layout patterns (summary rows, progress bars, sections).

## Relevant files and areas

| File | Why it matters |
|------|---------------|
| `src/lib/toolbox/registry.ts` | Where the new tool gets registered. Pattern: id, title, icon, component, order, hasBackend. ContextInspector has order=10. |
| `src/lib/toolbox/tools/ContextInspectorTool.svelte` | The closest pattern reference — complex stats display with sections, progress bars, formatted numbers, derived computations. |
| `src/lib/toolbox/tools/PlaceholderTool.svelte` | Simpler example — shows the canonical ToolProps destructuring, null-agent guard, and backend invoke pattern. |
| `src/lib/toolbox/types.ts` | `ToolProps` and `AgentContext` interfaces. Tool receives `agentContext` which contains `agentId`, `agent` (with shadow identity), `live`, `setup`. |
| `src/lib/api.ts` | `invoke<T>()` for calling Tauri commands. Used as `invoke<AgentStats>("db_get_agent_stats", { agentId })`. |
| `src/lib/bindings.ts` (line 125, 150-163, 341-354, 379-383) | `dbGetAgentStats` signature plus `AgentStats`, `SpecializationScores`, `ToolUsageEntry` types. |
| `src/global.css` | Dark theme variables: `--bg-panel`, `--text-primary`, `--text-muted`, `--accent`, `--success`, `--error`, etc. |
| `src/lib/AgentHeader.svelte` | How agent identity (shadow name, title, grade) is displayed — font, size, color conventions. |

## What needs to change

### 1. New tool component: `src/lib/toolbox/tools/ShadowStatsTool.svelte`

A Svelte 5 component accepting `ToolProps`. On mount (or when `agentContext` changes), calls `invoke<AgentStats>("db_get_agent_stats", { agentId })` to load stats. Displays:

**Header section** — Shadow name, title, grade, and an experience bar (0-100 from `stats.experience`) styled like the ContextInspector health track.

**Numbers section** — Summary rows showing total tokens (input/output), total cost, sessions, messages, turns. Uses the existing `.row` / `.label` / `.value` layout pattern from PlaceholderTool.

**Specialization section** — The 12 specialization scores visualized. Options: CSS-only horizontal bar chart (each category as a labeled row with a filled bar proportional to its score), or a radar/spider chart via SVG. A simple bar chart is more readable at toolbox panel width and doesn't need a charting library. Only show categories with non-zero scores. Label the highest-scoring category as the primary specialization (e.g. "Research Specialist").

**Tool usage section** — Top 5-10 tools by call count, each as a row with tool name, call count, and error count. Error count colored with `--error` when > 0.

### 2. Register in toolbox registry

Add the new tool to `src/lib/toolbox/registry.ts` with a stats/chart SVG icon. Order should be between ContextInspector (10) and Placeholder (100) — suggest order 15. `hasBackend: true` since it calls `db_get_agent_stats`.

### 3. No backend changes needed

MON-63 already provides `db_get_agent_stats`. The command is registered in specta and generate_handler. TypeScript types are in bindings.ts.

## Open questions

1. **Specialization chart style** — CSS bar chart (simple, fits panel width, no deps) vs SVG radar chart (cooler looking, more complex)? I'm leaning bar chart for the first pass — it's easier to read in a narrow panel and doesn't need a library. We can always upgrade to radar later.

2. **Refresh behavior** — Load on mount only, or also provide a refresh button? Stats don't change in real-time, so load-on-mount should be sufficient. But a refresh button is cheap and nice to have.

## Out of scope

- Avatar integration (MON-58)
- Activity timeline / session history
- Streak indicators
- War Room / sidebar access points
- Real-time stats streaming
