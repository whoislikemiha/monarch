# MON-50 — Implementation notes

## What was implemented

Surfaced the already-persisted cost data on three existing UI surfaces. No backend or schema changes — cost was already flowing end-to-end into SQLite.

- Per-message cost chip on every assistant turn in the message list.
- Per-session cost on each `HistoryPanel` row and in its preview header.
- Per-agent lifetime cost on each sidebar row, pulled from `agent_stats.total_cost` and refreshed per turn end.
- New shared `formatCost` helper at `src/lib/format.ts`; `ShadowStatsTool` migrated to it.
- Drive-by fix: the cost chip above the input area was showing only the last turn's cost when `sessionStats` wasn't hydrated yet. Reworked it to sum visible assistant items' `usage.cost.total` so it always matches what's on screen.

## Key decisions

- **Zero is hidden.** `formatCost` returns `null` for `cost <= 0`; callers short-circuit rendering so free-provider and local-LLM runs don't show `$0.0000` clutter. Rare "too small to display" amounts surface as `<$0.0001` to distinguish cheap from free.
- **Sidebar source: `AgentStats.totalCost` (Option B).** Authoritative, atomic, matches what `ShadowStatsTool` already shows. One extra `db_get_agent_stats` call per agent on load, consistent with the existing per-agent session fetch.
- **Sidebar refresh cadence: per turn end.** Load once on startup, then re-fetch when `agent.sessionStats?.totalCost` ticks (which only happens when Rust persists a new assistant message). The `sessionStats.totalCost` watcher naturally coalesces the 16ms snapshot bursts into exactly one DB roundtrip per `message_end`. No frontend accumulation — DB stays authoritative.
- **Above-input chip uses visible items, not DB.** The DB-backed `sessionStats` was only refreshed at bind/reset and lagged behind live streaming. Summing visible assistant items' `usage.cost.total` always matches what the user sees and avoids every staleness window.

## Files touched

- `src/lib/format.ts` — new shared formatter.
- `src/lib/types.ts` — `SessionRecord.totalCost`, `Agent.lifetimeCost`.
- `src/lib/MessageList.svelte` — `.cost-tag` next to existing model/token chips.
- `src/lib/HistoryPanel.svelte` — row chip + preview header.
- `src/lib/Sidebar.svelte` — `.agent-cost` chip on each row.
- `src/lib/AgentControls.svelte` — fixed session-cost derivation above input.
- `src/lib/AgentView.svelte` — `sessionStats.totalCost` watcher triggers `refreshLifetimeCost`; carries `totalCost` in `refreshSessionsFromDb`.
- `src/lib/stores/agentStore.svelte.ts` — loads `lifetimeCost` in `loadSavedAgents`; new `refreshLifetimeCost` method.
- `src/lib/toolbox/tools/ShadowStatsTool.svelte` — migrated to shared `formatCost`.

## What was left out

- No dedicated cost dashboard, budgeting, or alerts.
- Cost export (CSV / clipboard) — not an AC.
- No change to the per-agent lifetime refresh for background agents — they only re-fetch when selected (acceptable since turns only happen for agents the user is watching; fine for a lifetime counter).
