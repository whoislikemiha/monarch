# MON-50 — Wire cost tracking into the UI

## Summary

Cost data already flows end-to-end: the sidecar emits `usage.cost.total` on every `message_end`, Rust persists it to `messages.cost` and `sessions.total_cost`, and the lifetime per-agent total is aggregated in `agent_stats.total_cost`. The typed surface is already present (`Usage.cost`, `SessionRow.totalCost`, `AgentStats.totalCost`). What's missing is purely presentational — three places in the UI need to start reading and showing it:

1. **Per-message cost** on each assistant turn in the message list.
2. **Per-session total** in the history panel (both the list row and the preview header).
3. **Per-agent lifetime total** in the sidebar shadow list.

Pure frontend wiring, no backend or schema changes.

## Relevant files and areas

- `src/lib/types.ts`
  - `Usage.cost.{input,output,total}` (lines 180–191) — already typed, already on every assistant `DisplayItem`.
  - `SessionRecord` (lines 64–70) — currently omits `totalCost`; needs the field added so HistoryPanel can read it.
  - `DisplayItem` assistant variant (lines 289–300) already carries `usage?: Usage`.
- `src/lib/MessageList.svelte:22–36` — renders the assistant message label with `model` + `totalTokens`. This is where per-message cost belongs (right next to the token tag).
- `src/lib/HistoryPanel.svelte:124–144, 147–160` — session list rows and preview header. Needs a `totalCost` column on each row and in the preview header.
- `src/lib/HistoryPanel.svelte:34–41` — the refresh mapping currently strips `totalCost` off the DB row; fix by carrying it through.
- `src/lib/AgentView.svelte:88–122` — `refreshSessionsFromDb` also strips `totalCost`; same fix.
- `src/lib/stores/agentStore.svelte.ts:206–233` — `loadSavedAgents` also strips it. Same fix.
- `src/lib/Sidebar.svelte:180–220` — shadow row layout; per-agent lifetime cost line belongs in `.agent-info` under the name/grade.
- `src/lib/bindings.ts:127, 158–170` — `AgentStats` with `totalCost` is already exported; `db_get_agent_stats` command already registered. Sidebar can pull from here.
- `src/lib/toolbox/tools/ShadowStatsTool.svelte:76, 132` — existing `formatCost` helper. Lift it to a shared util so all three sites use the same formatting.

## What needs to change

1. **Shared cost formatter.** Extract the existing `formatCost(n)` from `ShadowStatsTool.svelte` into a small shared helper (e.g. `src/lib/format.ts`) so message-list, history panel, sidebar, and the toolbox all render cost the same way (e.g. `$0.0042`, `<$0.001` for very small amounts). Pick the precision rules at that point and apply consistently.

2. **Per-message cost in `MessageList.svelte`.** Alongside the existing `model-tag` and `token-tag`, add a `cost-tag` bound to `item.usage?.cost?.total`. Guard the render on presence so older persisted rows without cost still display.

3. **`SessionRecord` gains `totalCost`.** Add an optional `totalCost?: number` field to the type. Propagate it through every site that constructs a `SessionRecord` from a DB row (`AgentView.refreshSessionsFromDb`, `HistoryPanel.refreshSessions`, `agentStore.loadSavedAgents`).

4. **Per-session total in `HistoryPanel.svelte`.** Render `session.totalCost` next to the existing `msgs` chip on each row, and in the preview header next to the model name. Same formatter.

5. **Per-agent lifetime total in `Sidebar.svelte`.** Two viable sources:
   - **Option A:** Aggregate over `agent.sessions[].totalCost` — zero new IPC, but only as complete as the already-loaded session list.
   - **Option B:** Pull `AgentStats.totalCost` via `db_get_agent_stats` once per agent at load time (parallel to the existing session fetch in `loadSavedAgents`), stash on `Agent` as a new `lifetimeCost?: number` field.

   Recommendation: **Option B**. `agent_stats` is the authoritative aggregate (incremented atomically in `db.rs`), matches what `ShadowStatsTool` already displays, and avoids the "lifetime total only counts what's loaded" footgun. Adds one DB roundtrip per agent on app start — the sidebar already does that for sessions, so it's a known pattern.

6. **Styling.** Pick compact typography so costs don't dominate any of the three surfaces — likely a muted monospace chip matching the existing `token-tag` / `session-msgs` vibe. One pass in each component's `<style>`.

## Open questions

- **Formatter precision.** ShadowStatsTool's existing `formatCost` is probably fine but we should confirm one behavior now so all three surfaces agree: what to show for `< $0.0001` turns (e.g. a LM Studio local run where cost is 0). Options: `$0.0000`, `<$0.001`, or hide the chip entirely if zero. My lean: hide when `cost === 0` so free-provider runs don't show a meaningless `$0.00` everywhere.
- **Sidebar source: A vs B.** I recommend B (see above). Confirm before implementing so I don't do the wrong one.
- **Agent-level cost refresh cadence.** `AgentStats.totalCost` is a snapshot at load time. Should the sidebar refresh after each `message_end` (live ticking), or only on next app reload? My lean: keep it simple — load once, maybe re-fetch when the active agent's session count changes. Live-ticking the sidebar for every token would be overkill for a lifetime counter.
- **Placement on AssistantMessage label row.** Token count and cost can be on the same line; if label gets cramped at narrow widths, consider moving cost to an on-hover tooltip. Flag for during implementation.

## Out of scope

- Any backend or schema changes. All data is already persisted.
- Changing cost calculation or provider pricing tables.
- A dedicated "cost dashboard" view — tickets for that can come later; this is just surfacing on existing UI.
- Cost on the user messages (there is no input-side cost row in our data model separate from the assistant's `usage.cost.input`).
- Cost breakdown charts, cost budgets / alerts, or per-model cost comparison.
- Exporting cost data (CSV, clipboard, etc.).
- Moving `formatCost` usages in `ShadowStatsTool` to the new shared util beyond what's needed to avoid duplicating the function — the tool already works; just drop the local copy and import.
