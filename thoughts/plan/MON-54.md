# MON-54: Add tests for `apply_event` state machine

## Summary

`apply_event` in `sidecar_protocol.rs:435` is the pure function that assembles `LiveAgentState` from a stream of typed `InnerEvent` values. It is the single most critical path in the system — every UI state transition (streaming indicators, tool groups, display items, version bumps) flows through it. It currently has zero tests. The goal is to add a comprehensive `#[cfg(test)] mod tests` covering every event variant, the state transitions they trigger, and the edge cases called out in the Linear issue.

## Relevant files and areas

| File | Why |
|------|-----|
| `src-tauri/src/sidecar_protocol.rs` | Contains `apply_event` (L435-638), `streaming_from` (L644-657), `extract_user_text` (L665-683), and all `InnerEvent` / `Message` types. Tests will live here in a `#[cfg(test)]` module. |
| `src-tauri/src/agent_state.rs` | Defines `LiveAgentState` (L130-164), `ApplyOutcome` (L170-181), `DisplayItem` (L88-113), `ToolExecution` (L61-70), `StreamingMessage` (L74-81), `Usage` / `Cost` (L32-49). All test assertions target these types. `commit_streaming_message` (L204-213) is exercised by the `AgentEnd`-without-`MessageEnd` scenario. |
| `src-tauri/Cargo.toml` | May need a `[dev-dependencies]` section if we want `serde_json` test helpers or assertion crates, but `serde_json` is already a regular dep so likely no additions needed. |

## What needs to change

### 1. Test module in `sidecar_protocol.rs`

Add a `#[cfg(test)] mod tests` at the bottom of `sidecar_protocol.rs`. Each test constructs a `LiveAgentState::default()`, feeds it a sequence of `InnerEvent` values via `apply_event`, and asserts the resulting state fields.

### 2. Test cases (from the Linear issue + code analysis)

**Happy-path message lifecycle:**
- `MessageStart(assistant)` -> N x `MessageUpdate(assistant)` -> `MessageEnd(assistant)` — assert `streaming_message` is set then cleared, `is_streaming` flips on then stays on (only cleared by `AgentEnd`), final `DisplayItem::Assistant` is pushed, `state_version` increments per non-NoOp event.

**User message:**
- `MessageStart(user)` — assert `DisplayItem::User` pushed with correct content extraction (string content, array-of-blocks content). Assert `desynced` is reset to `false`.

**Tool execution lifecycle:**
- `ToolExecutionStart` -> `ToolExecutionEnd` — assert tool group created in `items`, `tool_executions` map populated, status transitions (`Running` -> `Done` / `Error`), `activity_status` set/cleared.

**Tool grouping:**
- Two consecutive `ToolExecutionStart` events (same turn) — assert they land in the same `ToolGroup`, not two separate ones. `current_tool_group_idx` points to the single group.

**Interleaved tool + message events:**
- `MessageStart(assistant)` -> `MessageEnd(assistant)` -> `ToolExecutionStart` -> `ToolExecutionEnd` -> `MessageStart(assistant)` -> `MessageEnd(assistant)` — assert correct ordering and tool group indexing in `items`.

**AgentEnd without prior MessageEnd:**
- `MessageStart(assistant)` -> `MessageUpdate(assistant)` -> `AgentEnd` — assert `commit_streaming_message` fires: `streaming_message` is `None` after, and a `DisplayItem::Assistant` was pushed from the streaming content. `is_streaming` is `false`.

**AgentStart / AgentEnd bookends:**
- Assert `activity_status` set/cleared, `is_streaming` cleared on end, `DisplayItem::Status` items pushed.

**TurnStart / TurnEnd:**
- Assert `current_tool_group_idx` is reset to `None` on both. `TurnEnd` marks the current tool group's `turn_complete = true`.

**Unknown event:**
- Assert `ApplyOutcome::NoOp`, no `state_version` bump.

**NoOp events (AutoRetryEnd, QueueUpdate, ToolExecutionUpdate):**
- Assert `ApplyOutcome::NoOp`, no version bump.

**MessageUpdate for non-assistant role:**
- Assert `ApplyOutcome::NoOp`.

**MessageStart with unknown role:**
- Assert `ApplyOutcome::NoOp`.

**Compaction events:**
- `CompactionStart` / `CompactionEnd` — assert status items pushed, `activity_status` set/cleared.

**state_version invariant:**
- After any non-NoOp event, version increments by exactly 1. After NoOp events, version stays the same.

**event_count invariant:**
- Always increments by 1 regardless of outcome.

**Debounce outcome:**
- `MessageUpdate(assistant)` returns `ApplyOutcome::Debounce` (not `EmitNow`), but still bumps `state_version`.

### 3. Helper constructors

Add small test-only factory functions to reduce boilerplate:
- `fn make_message(role, content, model)` -> `Message`
- `fn make_assistant_message_event(event_type)` -> `InnerEvent` for start/update/end
- `fn fresh_state()` -> `LiveAgentState::default()` (trivial but reads well)

### 4. No new dependencies expected

`serde_json::json!` macro (already available) is sufficient for building test `Value` payloads. Standard `assert_eq!` / `assert!` / `matches!` macros cover all assertions.

## Open questions

1. **Test file location:** The natural home is a `#[cfg(test)] mod tests` inside `sidecar_protocol.rs` since `apply_event` lives there. Alternatively, a separate `tests/` integration test file could work but would require making more types `pub`. Leaning toward inline module — any preference?

2. **Property-based testing:** The Linear issue mentions "property-style tests." Should we add `proptest` or `quickcheck` as a dev-dependency for actual property-based generation of event sequences, or is the term used loosely to mean "test properties/invariants of the state machine" with hand-written sequences?

## Out of scope

- Testing the sidecar event *deserialization* (`InnerEvent`/`SidecarEvent` `Deserialize` impls) — that's a separate concern from the state machine logic.
- Testing the Tauri event emission / debounce timer in `agent.rs` — `apply_event` is pure; the emission plumbing is integration-level.
- Testing `display_items_from_messages` recovery path in `agent_state.rs` — related but separate function.
- Testing `LiveAgentState::reset_with_items` or `mark_desynced` — simple enough to not warrant dedicated tests in this ticket.
