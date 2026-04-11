# MON-40 — isStreaming flag no longer toggles during streaming

## Summary

After MON-14 Phase 2 moved event assembly from `AgentView.svelte` into Rust, the `isStreaming` flag that drives the Abort button, the `ChatInput` `disabled` state, and the Sidebar/TabBar status dots is no longer ever set to `true`. The old frontend handler flipped it in `agent_start` / `agent_end`, but those event cases were deleted when Rust took ownership of `LiveAgentState`. The replacement wire shape in `src-tauri/src/agent_state.rs` has no `is_streaming` field, so the frontend has no live signal to derive from — the button and input are visually frozen in their idle state throughout a streaming turn.

The fix is to add a canonical `is_streaming: bool` to `LiveAgentState`, flip it inside `apply_event` at the right turn boundaries, publish it through the existing `agent-state-{id}` snapshot channel, and rewire the frontend to read the flag from the store instead of its dead local `$state`. This matches the Phase 2 principle that Rust owns assembled state; the frontend renders it.

## Relevant files and areas

### Rust — where the signal is produced

- `src-tauri/src/agent_state.rs` — defines `LiveAgentState` (the wire snapshot, lines 130–153). New `is_streaming` field lives here. Also hosts `reset_with_items` (lines 180–188) which currently resets `activity_status`/`desynced` and must now also reset `is_streaming`. `commit_streaming_message` (lines 192–201) and `mark_desynced` (lines 207–210) do not need changes but are adjacent touch points.
- `src-tauri/src/sidecar_protocol.rs` — hosts `apply_event` (lines 435–635). This is the one place that advances `LiveAgentState` on live events. The boundaries that matter:
  - `AgentStart` (439) / `AgentEnd` (446) — outer envelope. `AgentEnd` must clear the flag.
  - `TurnStart` (454) / `TurnEnd` (459) — per-LLM-call. Candidate "done with this call" boundary, but does **not** always mean streaming is over (next turn may follow).
  - `MessageStart { message }` with `role == "assistant"` (486–490) — the canonical start of a visible streaming turn.
  - `MessageEnd { message }` with `role == "assistant"` (505–519) — canonical end.
  - `ToolExecutionStart` (521) / `ToolExecutionEnd` (557) — tools run between assistant messages; streaming semantics here are a design call (see open questions).
  - `CompactionStart` (592) / `CompactionEnd` (600) — background work, not a user-visible stream.
  - The `Unknown` arm (628) currently returns `NoOp`; no change needed.
- `src-tauri/src/agent.rs` — hosts `rebuild_agent_state_from_session` and the snapshot emission path (the reader task driving `apply_event`, see around lines 1227–1251 and the `DEBOUNCE_MILLIS` coalescing). No logic change expected, but verify the snapshot emitted post-`reset_with_items` correctly serializes `is_streaming: false` on session reset and rebuild.

### Frontend — where the signal is consumed

- `src/lib/AgentView.svelte`
  - Line 46: `let isStreaming = $state(false)` — becomes a `$derived` off `live.isStreaming`.
  - Line 341 (`resetUiLocalState`), line 433 (`bindAgent` — `isStreaming = target.isStreaming`), line 472 (`agent-exit` listener) — all dead assignments once the flag is derived from the store; remove.
  - Line 612 (`<AgentControls {isStreaming} ... />`) and line 624 (`<ChatInput disabled={isStreaming} />`) — these keep working because the identifier is now a derived value.
- `src/lib/toolbox/types.ts` (lines 20–34) — hand-authored toolbox-facing `LiveAgentState` interface with field renames vs. the wire shape (`toolExecutions: Map`, derived `currentToolGroup`, etc.). Phase 2 originally planned to collapse this into a re-export of `bindings.ts`, but that didn't land — it's still an adapter boundary. Add `isStreaming: boolean` here.
- `src/lib/toolbox/liveAgentStore.svelte.ts` — the adapter that translates the wire `LiveAgentState` from `bindings.ts` into the toolbox shape on every `seedFromSnapshot` / `applyUpdate`. Must pass `isStreaming` through the translation; ensure `detachedLiveState()` defaults it to `false`. (The store is also the gating seam for reactivity per MON-41; MON-40 only adds a field, does not change the update strategy.)
- `src/lib/bindings.ts` — specta-generated; regenerate after the Rust field is added. Should be a one-line diff.
- `src/lib/types.ts` (line 51: `isStreaming: boolean` on `Agent`) — still used by `Sidebar.svelte:142` and `TabBar.svelte:47,80` for the status dot. Two options:
  1. Leave the Agent-row field and have `App.svelte` mirror the Rust signal onto it via the existing `onagentchange` path whenever the store updates.
  2. Rip out `agent.isStreaming` entirely and have Sidebar/TabBar read from `liveAgentStore.byAgent.get(id).isStreaming` directly.
- `src/lib/AgentControls.svelte` — consumer of the prop; no changes needed (already takes `isStreaming` as a boolean prop, line 15).
- `src/App.svelte` — sets `isStreaming: false` at agent creation (lines 160, 249) and on stop (lines 345, 454). If option 1 above is chosen, this is where the mirroring lives; if option 2, these assignments go away.

### Docs / tracker

- `thoughts/impl/MON-14-cleanup.md` parking lot entry (around line 887) — mark MON-40 done once this ships.

## What needs to change

At the module / concept level:

1. **Extend `LiveAgentState`** in `src-tauri/src/agent_state.rs` with a new `is_streaming: bool` field (serde camelCase, specta `Type`, default `false`). Update `reset_with_items` so session reset clears it.
2. **Teach `apply_event`** in `src-tauri/src/sidecar_protocol.rs` to flip the flag at the chosen turn boundaries. Current leaning: `true` on `MessageStart { assistant }` and `ToolExecutionStart`, `false` on `AgentEnd`. Other candidate boundaries (`TurnEnd`, `MessageEnd`, `ToolExecutionEnd`) are open questions — see below. Whatever policy is chosen must be expressed as a short doc comment on the field so later readers know why.
3. **Regenerate specta bindings** so `src/lib/bindings.ts`'s `LiveAgentState` gains the new field.
4. **Mirror the field across the toolbox adapter boundary**: add `isStreaming: boolean` to the hand-authored `src/lib/toolbox/types.ts` interface, default it in `detachedLiveState()`, and extend the wire→toolbox translation in `liveAgentStore.svelte.ts` so `seedFromSnapshot` / `applyUpdate` copy the field through. Additive only — the MON-14 Phase 2 zero-diff rule for tool components must hold (no tool component files should change).
5. **Delete the dead local `isStreaming` `$state`** in `AgentView.svelte` and replace it with a derived value off `live.isStreaming`. Remove the three dead assignments (`resetUiLocalState`, `bindAgent`, `agent-exit` listener).
6. **Decide Agent-row mirror fate** (option 1 vs option 2 above). Default recommendation: option 2 — remove the Agent-row field and have Sidebar/TabBar read from the store — because the Agent row is currently a lie everywhere it's used. Fall back to option 1 if that risks rendering loops or if the store's coarse reactivity makes per-row subscriptions awkward.
7. **Smoke-test crash recovery**: kill the sidecar mid-turn and confirm that when recovery rebuilds state the Abort button does not get stuck enabled (i.e. `rebuild_agent_state_from_session` publishes `is_streaming: false`).

## Locked decisions

1. **Off-switch boundary: `AgentEnd`.** `true` on `MessageStart { assistant }` and `ToolExecutionStart`; `false` on `AgentEnd`. Tighter boundaries (`TurnEnd`, `MessageEnd`, `ToolExecutionEnd`) rejected to avoid mid-turn flicker while tools run or the next LLM call spins up.
2. **`is_streaming` stays `true` during tool execution between assistant messages.** A turn is not "done" until the agent stops talking and tools stop running.
3. **Rip out `Agent.isStreaming`.** Remove the field from `src/lib/types.ts`, remove the four `App.svelte` assignments (lines 160, 249, 345, 454), remove the `isStreaming = target.isStreaming` line in `AgentView.svelte` (line 433), and have `Sidebar.svelte` and `TabBar.svelte` read `liveAgentStore.byAgent.get(agent.id)?.isStreaming` directly.
4. **Parallel tool calls (future).** Current sidecar emits `MessageEnd (assistant)` only when the full agent turn is done talking, so MON-40 can safely ignore that boundary. If Pi SDK later adds a mode where `MessageEnd` fires mid-agent-turn with more tool calls to follow, the off-switch policy will need to be revisited — call this out in the doc comment on the `is_streaming` field.

## Remaining open questions

1. **Activity status relationship.** `activity_status` already roughly tracks streaming via its strings ("Receiving response...", "Running tool: ..."). Keep them as independent signals (`activity_status` = label, `is_streaming` = control signal) or couple them? Leaning: keep independent so a future string change cannot silently break the button.
2. **Coalescing / debounce sanity check.** `MessageUpdate` is `ApplyOutcome::Debounce`, but `MessageStart` (our on-switch) and `AgentEnd` (our off-switch) are both `EmitNow`, so the flag transitions should arrive at the frontend without being hidden behind the coalescing timer. Worth a manual smoke test during implementation.

## Out of scope

- Any changes to `activity_status` semantics or the existing status strings.
- MON-41 (coarse reactivity in `applyUpdate`) — sibling issue, will be handled separately.
- Migrating `eprintln!` logging in `agent.rs` to `tracing` (Wave 2 parking-lot item).
- Rust test harness setup — there is no working test harness in this repo; verification is manual (`cargo check`, `cargo clippy`, `svelte-check`, and a live sidecar smoke test).
- Touching how `agent-exit-{id}` clears state beyond the dead `isStreaming` assignment.
