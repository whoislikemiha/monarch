# MON-71 — Measure and display task durations in chat headers

## Summary

Monarch renders the *what* of an agent turn — tool calls, thinking blocks, assistant messages — but never the *how long*. This feature measures wall-clock duration for three units of work (the turn itself, each tool call, each thinking block) and displays a human-readable duration inline in each unit's header. The thinking-block case complements the MON-16 work just landed: a collapsed bubble should read "Thought for 15 sec" instead of just "Thinking". Scope is deliberately capped at these three units — the agent-lifetime wall-clock stat in the sidebar is a separate issue.

The change spans all three layers: the sidecar must emit start/end timestamps for the relevant content-block boundaries, Rust must assemble those into `LiveAgentState` and persist them, and the frontend must render them with a new `formatDuration` helper. Historical messages must render correctly after app restart, which means durations live in SQLite, not just in memory.

## Relevant files and areas

### Sidecar (timestamp source of truth)

- **`sidecar/src/runtime-manager.ts`** — The Pi SDK session host. Where raw SDK events are translated into our wire protocol. Any `performance.now()` / `Date.now()` capture must happen here at content-block boundaries. This is the only place we can reliably mark *when* a tool call or thinking block actually started/ended.
- **`sidecar/src/protocol.ts`** — Sidecar→Rust event type definitions. Needs new optional timing fields on the events corresponding to block starts/ends (tool execution start/end, thinking block start/end, turn start/end).

### Rust backend (assembly + persistence)

- **`src-tauri/src/sidecar_protocol.rs`** — Wire protocol types on the Rust side. Mirror the new timing fields added in `protocol.ts`. Currently `Message` carries `timestamp: Option<i64>` at Unix-second granularity (line 145); `TurnStart`/`TurnEnd` (lines 208–209) and `ToolExecutionStart`/`ToolExecutionEnd` (lines 219–231) carry none. No thinking-block-specific events exist yet at all.
- **`src-tauri/src/agent_state.rs`** — Assembles `LiveAgentState` from the sidecar event stream. `ToolExecution` (lines 69–70) and `StreamingMessage` (line 79) are the in-memory shapes that need duration fields. This is where "start time captured, end time captured → compute duration" lives.
- **`src-tauri/src/db.rs`** — SQLite schema. A migration adds duration columns to the `messages` table (or to a new columns covering turn/tool/thinking durations, depending on how we model it — see open question). The existing `timestamp` column on `messages` (line 174) gives us turn-end; we'd need something equivalent for start + per-sub-block durations.
- **`src-tauri/src/bindings.ts`** — auto-generated; regenerate after any type change via `cargo run -- --export-bindings`.

### Frontend (render surface)

- **`src/lib/format.ts`** — Central display-formatter module. Currently holds only `formatCost()`. Needs a new `formatDuration(seconds: number): string` returning `< 1 sec`, `15 sec`, `2 min 14 sec`, `1 hr 30 min` etc.
- **`src/lib/MessageList.svelte`** (lines 19–39) — Renders per-message headers. The assistant-message label row (lines 25–37) currently shows model, token count, cost chips; turn duration joins them as another chip. Also contains the streaming-thinking live preview (lines 67–88) which needs a running-elapsed indicator.
- **`src/lib/ToolCallCard.svelte`** (lines 101–116) — Tool-call header button. Duration chip slots in near the status dot / tool name, either replacing or sitting alongside the "running" tag while in progress and swapping to final duration on completion.
- **`src/lib/AssistantMessage.svelte`** (lines 129–140) — Thinking toggle button. The current "Thinking" label becomes "Thought for 15 sec" once the block finalizes; while streaming, shows a live counter or just "Thinking…".
- **`src/lib/liveAgentStore.svelte.ts`** — Reactive per-agent state. Will need to surface the new duration fields so the three render sites above can read them.
- **`src/lib/types.ts`** — Frontend-facing type shims around bindings. Likely touch-free if we regenerate bindings correctly, but worth verifying.

### Existing conventions to mirror

- **`src/lib/Sidebar.svelte:204–206`** + **`src/lib/types.ts:65–66`** — the `lifetimeCost` chip is the closest existing pattern: a value that's computed server-side, cached in the DB (`agent_stats.total_cost` per MON-50), and rendered as a formatted chip. Duration chips should follow the same shape: computed+persisted by Rust, exposed via bindings, formatted on the frontend.

## What needs to change

### 1. Sidecar: emit timestamps at block boundaries

Instrument `runtime-manager.ts` to capture `Date.now()` (ms since epoch) at four moments per turn:

- Turn start (when we begin processing the user input)
- Turn end (when the assistant's final content block closes)
- Tool-call start / end (per tool invocation)
- Thinking-block start / end (per thinking content block — this is brand new; nothing tracks thinking block lifecycle today)

Decision to make (open question): emit raw `start_ms` / `end_ms` timestamps and let Rust compute deltas, *or* compute `duration_ms` in the sidecar and emit only the delta. The former is more flexible (Rust can timestamp-align events for other features later) and cleaner to reason about; the latter is fewer fields to pass around. Recommendation: raw timestamps on both sides, compute deltas at the display boundary.

### 2. Wire protocol: add optional timing fields

Mirror the sidecar fields in `sidecar_protocol.rs`. Keep them `Option<i64>` for forward-compat — old sidecar builds or historical replayed events shouldn't break. A new event variant for thinking-block start/end will likely be needed, since nothing equivalent exists today.

### 3. State assembly: track start/end in `LiveAgentState`

In `agent_state.rs`, thread the timestamps from incoming events into the matching in-memory structures:

- `ToolExecution` gets `started_at_ms` / `ended_at_ms` (or just `duration_ms` if we settle on the sidecar-side-compute answer)
- `StreamingMessage` gets equivalent fields for the turn itself
- A new thinking-block shape — or fields on whatever already represents thinking in `LiveAgentState` — gets the same treatment

### 4. Persistence: extend SQLite schema

Add duration columns so historical messages can render durations after restart. Shape TBD — two reasonable models:

- **Per-message columns** — widen the `messages` table with `duration_ms` (turn duration), and store per-tool / per-thinking durations inside the structured JSON content blocks that already live there.
- **Denormalized tool_executions table** — only if we want per-tool durations queryable / aggregable; probably overkill for a pure-display feature.

Recommend the first: turn duration on the `messages` row, sub-block durations embedded in the content-block JSON. Migration is additive (nullable column + JSON field), no backfill.

### 5. Frontend: add `formatDuration` and wire into three headers

Add `formatDuration(seconds: number)` to `src/lib/format.ts`. Breakpoints:

- `< 1` → `< 1 sec`
- `< 60` → `N sec`
- `< 3600` → `M min N sec` (drop "0 sec" if exactly at minute boundary → `M min`)
- `>= 3600` → `H hr M min`

Wire into the three render sites identified above. For the live (in-progress) case, use a single `$state` "now" ticker (one timer per agent view, not per block) to drive live counters.

### 6. Regenerate bindings and type-check

Run `cargo run -- --export-bindings` from `src-tauri/`, confirm the new fields appear in `src/lib/bindings.ts`, and run `npx svelte-check` + `cargo check` to close the loop.

## Open questions

1. **Where to compute the delta** — raw timestamps on the wire and delta at render, or pre-computed `duration_ms` in the sidecar? Recommendation above is raw timestamps; confirm before implementation.
2. **Live-counter granularity** — do we want a ticking "14 sec… 15 sec… 16 sec…" counter while a tool call is running, or just a static "running" indicator that swaps to a duration on completion? The ticking version is nicer UX but means every live block triggers re-renders once a second. A single shared timer per agent view is cheap; still worth asking.
3. **Minimum display threshold** — should sub-second tool calls show `< 1 sec` or just no duration chip at all? Suggesting `< 1 sec` so the absence of a chip unambiguously means "duration unknown" (e.g. historical message pre-dating this feature).
4. **Format for the thinking header specifically** — `Thought for 15 sec` (Claude.ai/ChatGPT convention) vs just `15 sec` as a chip next to "Thinking". The former reads better but requires a different render path than tool/turn chips; happy to go either way.
5. **Historical session behavior on restart** — when `get_messages_with_ancestry` loads a mix of pre-MON-71 and post-MON-71 messages, pre-MON-71 messages will have null durations. Confirm that rendering nothing (no chip) in that case is acceptable — no backfill, no "unknown" placeholder.
6. **Does the agent-level lifetime stat already use wall-clock anywhere?** — Research suggests no (the existing `lifetimeCost` is cost, not time). If the user meant to include agent-level elapsed time in this ticket, scope should expand; otherwise, leaving it out per the issue's Out-of-Scope section.

## Out of scope reminders

- No agent-level sidebar wall-clock stat (distinct feature).
- No backfill of historical rows — null durations render as no chip.
- No sub-second display precision; whole seconds only.
- No aggregate analytics (median tool time, slowest tool, etc.).
- No performance profiling UI.
