# MON-40 — isStreaming flag restored on Rust-owned live state

## What was implemented

After MON-14 Phase 2, the frontend `isStreaming` signal went dead: the old
`AgentView` handler flipped it in `agent_start` / `agent_end`, but those cases
were deleted when Rust took ownership of `LiveAgentState`, and no replacement
field existed on the wire. The Abort button, `ChatInput` disabled state, and
Sidebar/TabBar status dots were all frozen in their idle state throughout a
streaming turn.

Fix: add a canonical `is_streaming: bool` to `LiveAgentState`, flip it inside
`apply_event` at the turn boundaries, and rewire every consumer to read it
from the Rust-owned snapshot via `liveAgentStore` instead of from dead local
state or the stale `Agent.isStreaming` row field.

## Key decisions

- **Boundary policy: on at `MessageStart{assistant}` and `ToolExecutionStart`,
  off at `AgentEnd`.** Tighter boundaries (`TurnEnd`, `MessageEnd`,
  `ToolExecutionEnd`) would flicker mid-turn while tools run or the next LLM
  call spins up. Documented on the field itself so the rationale survives.
  The policy will need revisiting if the sidecar ever fires `MessageEnd`
  mid-agent-turn (parallel tool calls).
- **Ripped `Agent.isStreaming` out entirely** rather than mirroring the
  Rust signal onto it. The row field was never being written after MON-14
  Phase 2, so every consumer of it was lying; a store subscription in
  Sidebar/TabBar is cheaper than maintaining a mirror.
- **Kept `activity_status` independent** from `is_streaming` (label vs.
  control signal) so a future string change can't silently break the button.
- `reset_with_items` clears the flag so session reset and crash-recovery
  rebuilds never leave the Abort button stuck enabled.
- All three transition events are `EmitNow`, so the flag arrives at the
  frontend without being hidden behind the `MessageUpdate` debounce timer.

## Files touched

- `src-tauri/src/agent_state.rs` — new field, `reset_with_items` clears it.
- `src-tauri/src/sidecar_protocol.rs` — `apply_event` flips the flag.
- `src/lib/bindings.ts` — regenerated (one-field diff in `LiveAgentState`).
- `src/lib/toolbox/types.ts` — mirror the field on the hand-authored view
  interface.
- `src/lib/toolbox/liveAgentStore.svelte.ts` — copy through
  `adaptSnapshot`, default in `detachedLiveState()`.
- `src/lib/AgentView.svelte` — `isStreaming` becomes a `$derived`; deleted
  three dead assignments.
- `src/lib/types.ts` — removed `isStreaming` from `Agent`.
- `src/App.svelte` — removed four dead writes.
- `src/lib/Sidebar.svelte`, `src/lib/TabBar.svelte` — read
  `liveAgentStore.byAgent.get(agent.id)?.isStreaming` directly.

## What was left out

- No Rust unit tests — there is no working test harness in this repo
  (plan §Out of scope).
- No live crash-recovery smoke run; covered by reading `reset_with_items`
  + the rebuild emission path rather than by manual kill/restart.
- Unrelated Wave 2 parking-lot items (e.g. `eprintln!` → `tracing` in
  `agent.rs`) explicitly out of scope.
