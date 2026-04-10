# MON-32 — Typed `SidecarEvent` and `SidecarCommand` enums

Shipped: PR #25, merged into `markocvijanovic1998/mon-14-phase-1-rust-state-ownership` on 2026-04-11 as commit `c3f35ce`. Wave 2 step 2 of 5 (after MON-31, before MON-35).

## What shipped

The Rust ↔ sidecar JSONL protocol is now typed end-to-end. Inbound events
are parsed once at the reader boundary into a `SidecarEvent` enum and
dispatched via pattern-matching; outbound commands are constructed as
typed `SidecarCommand` values and serialized once at the send site. The
per-agent `create_cmd_json: String` replay cache is gone — `AgentState`
carries a typed `SidecarCommand::CreateSession` that the recovery path
serializes on resend. The stringly-typed `get("type").and_then(as_str)
.unwrap_or("")` chains and the ~50 lines of `serde_json::Value` walking
inside `LiveAgentState::apply_event` are deleted.

Unknown event types (outer envelope or inner payload) are represented as
explicit `Unknown { raw }` variants that preserve the original `Value`,
flip `desynced` through `mark_agent_desynced`, and log the raw payload
for forensics. Malformed *known*-tag payloads still propagate as
`serde_json::Error` → `MonarchError::Serde` instead of silently falling
through, so the schema commitment is load-bearing.

The `send_command` / `ws_send_command` frontend passthrough now validates
the caller's JSON payload against the canonical `SidecarCommand` schema
via `from_value` before reserializing. Out of the 11 variants, 6 are
reachable via this path (`prompt`, `abort`, `set_thinking_level`,
`set_model`, `compact`, `set_custom_prompt`) — all five frontend call
sites were audited and match the typed enum exactly.

## Key decisions

- **`InnerEvent::Unknown` handling lives in `handle_sidecar_event`, not
  `apply_event`.** Flipping desync + logging the raw payload happens at
  the reader boundary where `agent_id` is in scope; `build_persist_commands`
  never sees `Unknown` (cleaner match exhaustiveness). The
  `InnerEvent::Unknown { .. }` arm in `apply_event` is a safety-net
  fallback that is unreachable in practice.

- **Custom `Deserialize` impls with `KNOWN_*_TAGS` constants + private
  `KnownXxxEvent` helper enums.** `#[serde(other)]` only supports unit
  variants, so capturing the raw `Value` in `Unknown` needs a manual
  impl. The two-step pattern (peek the tag against a `KNOWN_*_TAGS`
  list, then delegate to the derived helper) gives explicit error
  messages on malformed known-tag payloads and keeps the tag table
  visible in one place. Drift risk is bounded — a missing tag just
  routes the event through `Unknown` (benign) and a stale tag can't
  cause typed deserialization to misfire because the helper does a
  strict decode.

- **Two-pass parse in `handle_sidecar_event`.** Parsing each JSONL line
  once as `serde_json::Value` (for byte-fidelity `LogEvent.data`
  storage of the inner event) and once via `from_value::<SidecarEvent>`
  (for typed dispatch) sidesteps the need to implement `Serialize` on
  `InnerEvent::Unknown { raw }` or lose fields on future Pi SDK
  extensions to known variants. The `Value` clone is O(line-size),
  trivial for JSONL.

- **`ToolExecutionEnd.tool_name: Option<String>`** with a `"unknown"`
  default in `build_persist_commands` preserves the pre-MON-32 persisted
  `toolResult` row shape byte-for-byte. Tightening to required String
  was rejected because I didn't audit whether Pi SDK guarantees the
  field on the end event — the defensive default was cargo-cult, but
  breaking historical row parsing to remove it wasn't in scope.

- **`CompactionStart.reason` / `CompactionEnd.aborted`** stay as
  `Option<T>` with display-time fallbacks (`"unknown"`, `false`) inside
  `apply_event`. The ticket's "no silent defaulting" rule is satisfied
  at the schema level (the Option is explicit); the fallback is only
  for status-item text formatting, not for invariant preservation.

- **Frontend `send_command` passthrough uses the `Value`-inject pattern,
  not `#[serde(default)] agent_id` on every variant.** The frontend
  posts payloads without `agentId`; the Tauri command parses the
  payload as a raw `Value`, injects the id, then re-deserializes into
  `SidecarCommand`. One shared deserialization path validates the
  schema with no per-variant pollution. The alternative (carrying a
  `set_agent_id` helper + `#[serde(default)]` on 11 variants) was
  prototyped and rejected for the noise.

- **`Usage` and `Cost` pick up struct-level `#[serde(default)]`.** The
  pre-MON-32 `parse_usage` helper defaulted every field to `0` /
  `0.0` — making the typed `Usage` deserialize require all fields
  would have broken against Pi SDK events that omit any of them (the
  defaulting was there for a reason). Struct-level `default` reads
  as "fill in what's missing from `Default::default()`" which is the
  minimum behavior-preserving change.

## Files touched

- **new** `src-tauri/src/sidecar_protocol.rs` (~680 lines) — the whole
  typed protocol module + the free `apply_event` function.
- `src-tauri/src/lib.rs` — module declaration.
- `src-tauri/src/agent.rs` — every outbound sidecar-command site
  migrated to typed `SidecarCommand`; `AgentState.create_cmd_json` →
  `create_cmd`; `handle_sidecar_event` / `apply_and_maybe_emit` /
  `build_persist_commands` take typed events; `send_command` /
  `ws_send_command` validation round-trip.
- `src-tauri/src/agent_state.rs` — `LiveAgentState::apply_event`
  inherent method deleted; `commit_streaming_message` promoted to
  `pub(crate)`; `streaming_from_json` / `parse_usage` helpers removed
  (replaced by derived Deserialize + `streaming_from` in
  `sidecar_protocol.rs`); `Usage` / `Cost` get `#[serde(default)]`.

Commits on the PR branch: `c8a7994` (outbound) → `d8af6be` (inbound +
`apply_event` move) → `52d901d` (typed `send_command` passthrough).

## What was left out / out-of-scope landmines next agent should know

- **`eprintln!` → `tracing` migration** is still a parking-lot item.
  `handle_sidecar_event` logs `[monarch] Unknown sidecar inner event for
  {id}: {raw}` via `eprintln!` for `Unknown` variants — consistent with
  the rest of `agent.rs`, not a `tracing` regression.

- **The two `too_many_arguments` clippy warnings on `spawn_agent` /
  `ws_spawn_agent` still stand.** MON-35 owns them via
  `SpawnAgentRequest`. MON-32 intentionally did not touch those
  signatures even though the typed `SidecarCommand::CreateSession` now
  makes the collapse trivial.

- **`PersistCommand::apply` / `run_persist_consumer` untouched.** Their
  `MonarchError` migration shipped in MON-31; the consumer still
  stringifies at its log/desync boundary and MON-32 left that alone.

- **Frontend-facing `serde_json::json!` sites in `agent.rs` were
  intentionally preserved.** The remaining uses at lines 743 / 757 /
  842 / 1150 / 1357 / 2037 are `agent-event-{id}` envelope payloads,
  the SQLite `toolResult` storage blob, and `detect_project` Tauri
  return bodies — none of them are Rust ↔ sidecar protocol. The
  acceptance bullet "Zero `serde_json::json!` macro uses in `agent.rs`
  for sidecar commands" is met with that scope carveout.

- **Two `.and_then(|a| a.as_str())` calls remain in `agent.rs`** at
  the typed-parse-failure branch (line 728) and the envelope-level
  `Unknown` branch (line 854). Both pull `agentId` out of the raw
  `Value` in paths where we don't have a typed event to destructure —
  they're fallback-only, not main dispatch.

- **The `Value = unknown; Vec<T> = T[]` post-processing hack in
  `lib.rs::export_bindings`** is untouched. It workarounds a specta rc.24
  bug in TS emission of `serde_json::Value`, and MON-32 actually adds
  *more* Value references (`SidecarCommand::ExtensionUiResponse.value`,
  `ToolExecutionStart.args`, `ToolExecutionEnd.result`,
  `InnerEvent::Unknown.raw`) — the hack still carries them through.
  MON-35's acceptance bullet about deleting the hack will need to
  address this separately; it is not a pure spawn_agent problem.
