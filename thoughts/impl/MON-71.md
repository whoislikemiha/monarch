# MON-71 — Measure and display task durations

## What shipped

Wall-clock duration measurement and inline display for every turn, tool call, and thinking block. The three headers now show duration chips; the end-of-agent status line reads "Agent finished in 2 min 14 sec". Durations persist in SQLite and survive restart.

Concrete UX:

- **Turn header** — live ticker (`Assistant ● 3 sec …`) while streaming; static `15 sec` chip once the turn finalizes, next to the existing model / token / cost chips.
- **Tool-call header** — live `running... 3 sec` while the tool runs; static `15 sec` chip once it completes. Sub-1-second tool calls (trivial reads, greps) show no chip.
- **Thinking toggle** — `Thought for 12 sec` when the block has a finalized duration; plain `Thinking` otherwise (live pre-finalize, sub-1-sec blocks, and pre-MON-71 historical rows).
- **Agent finished status** — `Agent finished in 2 min 14 sec`.

## Key decisions

- **Stamping lives in Rust, not the sidecar.** The plan suggested raw timestamps on the wire. Implementation stamps `chrono::Utc::now().timestamp_millis()` at the Rust event-handler boundary — same semantic (raw wall-clock, not pre-computed delta) with zero sidecar protocol changes. The ~ms-scale latency between sidecar emission and Rust arrival is irrelevant at second-granularity display.
- **Thinking-block boundaries via content-block diffs.** Pi SDK has no explicit thinking-end events at the envelope layer. Implementation records the earliest `now_ms` the first time a thinking block at each content index is observed during `MessageUpdate`. End time is `message_end_ms` — tight enough given the debounce window is 16 ms.
- **Persistence split between column and embedded JSON.** Turn duration gets a new nullable `messages.duration_ms` column; tool durations embed `durationMs` inside the stored toolResult JSON blob; thinking durations are injected as `_monarch.durationMs` into each thinking block's JSON before the assistant message is persisted. This keeps the schema minimal while still surviving restart.
- **Pre-apply duration peek.** `event_handler.compute_event_durations` reads the live state under a read lock *before* `apply_event` runs, so `build_persist_commands` sees the pre-mutation start times. Necessary because `apply_event` clears `turn_started_at_ms` and overwrites `started_at_ms` on tool executions.
- **Single per-view 1 Hz ticker.** One `$effect` on `AgentView` threads `nowMs` down through `MessageList` → `ToolGroup` → `ToolCallCard`. Active only while `isStreaming` so idle views don't trigger re-renders. Beats per-block timers.
- **Sub-1-sec chip policy: omit entirely.** `formatDuration(ms)` returns `null` below 1 second; the render sites hide the chip. Makes an absent chip unambiguously mean "too fast or unknown," which is the same visual treatment historical (pre-MON-71) rows get.
- **Rust-side `format_duration_ms` helper mirrors the TS `formatDuration`.** Needed so the "Agent finished in ..." status text and the frontend chip labels stay byte-identical. No shared formatter between Rust and TS — duplicated deliberately at ~15 lines each.

## Files touched

- `src-tauri/src/agent_state.rs` — new fields on `ToolExecution`, `StreamingMessage`, `DisplayItem::Assistant`, `LiveAgentState`; `format_duration_ms` helper; recovery path reads durations back.
- `src-tauri/src/sidecar_protocol.rs` — `apply_event` stamps + computes durations; `record_new_thinking_blocks` + `finalize_thinking_durations` helpers; `now_ms` wrapper.
- `src-tauri/src/db.rs` — `messages.duration_ms` migration; `MessageRow` field + map/insert updates.
- `src-tauri/src/agent/persist.rs` — `EventDurations` struct; save assistant-turn duration on row; embed tool `durationMs` in toolResult JSON.
- `src-tauri/src/agent/event_handler.rs` — `compute_event_durations` pre-apply peek.
- `src/lib/format.ts` — `formatDuration(ms)`.
- `src/lib/AgentView.svelte` — 1 Hz ticker `$effect`, `nowMs` plumbed down.
- `src/lib/MessageList.svelte`, `ToolGroup.svelte`, `ToolCallCard.svelte`, `AssistantMessage.svelte` — duration chips in the three render sites; `Thought for X` label.
- `src/lib/types.ts`, `bindings.ts` — new fields on `DisplayItem`, `ToolExecution`, `AssistantMessage`, `ThinkingContent`.

## What was left out

- No backfill for pre-MON-71 messages — historical rows render without a chip, by design.
- No live counter on thinking blocks during streaming. The `Thought for X sec` label is the finalized-only version per the Claude.ai / ChatGPT convention. A ticking thinking counter would require surfacing `thinking_block_starts` on the wire; deferred as low value.
- No agent-level lifetime wall-clock stat in the sidebar. Explicitly out of scope per the issue — separate feature if wanted.
- No sub-second precision display. Whole seconds only; the internal ms precision is only so durations round correctly.
- No aggregate analytics (median tool time, slowest tool, etc.) — out of scope.
