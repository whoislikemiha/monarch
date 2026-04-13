# MON-62: Shadow Stats UI — Profile Card

## What was implemented

A toolbox tool ("Stats") that displays per-agent lifetime stats from the MON-63 backend. Shows shadow identity with experience bar, lifetime token/cost/session numbers, a CSS bar chart for specialization scores, and top tools by call count.

## Key decisions

- **CSS bar chart over radar chart** — simpler, more readable at narrow toolbox panel width, no charting library needed. Can upgrade to radar later.
- **Top 8 tools** — limits the list to avoid scrolling in the panel. Tools sorted by call count descending (backend already sorts this way).
- **Non-zero specialization only** — categories with <0.5% are hidden to avoid visual clutter. Most categories will be zero until MON-64 (file path analysis) enriches the heuristics.
- **$effect for loading** — stats reload automatically when agentContext changes (agent switch), plus manual refresh button.
- **Toolbox order 15** — between Context Inspector (10) and Placeholder (100).

## Files touched

- `src/lib/toolbox/tools/ShadowStatsTool.svelte` — new component
- `src/lib/toolbox/registry.ts` — registered with bar chart icon

## What was left out

- Avatar integration in the card header (MON-58)
- Activity timeline / session history
- Streak indicators
- Radar/spider chart (bar chart for now)
- War Room / sidebar access points
