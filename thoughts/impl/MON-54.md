# MON-54: Implementation notes

## What was implemented

Added a `#[cfg(test)] mod tests` block to `sidecar_protocol.rs` with 30 unit tests exercising the `apply_event` state machine. Tests cover every `InnerEvent` variant, state machine invariants, tool grouping logic, and edge cases.

## Key decisions

- **Inline test module** over separate integration test file — `apply_event` and all event types are local to `sidecar_protocol.rs`, no need to widen visibility.
- **Hand-written sequences** over property-based testing (`proptest`/`quickcheck`) — the event space is finite and the sequences that matter are specific, well-defined scenarios from real agent turns.
- **No new dependencies** — `serde_json::json!` and standard assert macros are sufficient.
- **Discovered early-return edge case** — `MessageUpdate(assistant)` does `return ApplyOutcome::Debounce` before the `state_version` bump block at the end of `apply_event`. Added a dedicated test to pin this behavior.

## Files touched

- `src-tauri/src/sidecar_protocol.rs` — added `#[cfg(test)] mod tests` (~740 lines)

## What was left out

- Deserialization tests for `InnerEvent`/`SidecarEvent` (`Deserialize` impls) — separate concern.
- Integration-level tests for Tauri event emission / debounce timer in `agent.rs`.
- Tests for `display_items_from_messages` recovery path in `agent_state.rs`.
- Tests for `LiveAgentState::reset_with_items` and `mark_desynced` — trivial methods.
