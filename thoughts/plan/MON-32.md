# MON-32 — Typed `SidecarEvent` / `SidecarCommand` enums

- **Linear:** https://linear.app/monarch-commander/issue/MON-32
- **Branch:** `mon-32-typed-sidecarevent-and-sidecarcommand-enums`
- **Base:** `mon-14-phase-1-rust-state-ownership` @ `9dacfca` (MON-31 merge)
- **Wave:** 2, step 2 of 5 (MON-31 merged → MON-32 → MON-35 → MON-33 → MON-34)

## Summary

Replace the stringly-typed Rust ↔ sidecar JSONL protocol with a pair of
serde-tagged enums that mirror `sidecar/src/protocol.ts` one-to-one. Inbound
events are parsed once at the reader boundary into `SidecarEvent`;
`LiveAgentState::apply_event` dispatches on the typed variant so the
`get("x").and_then(as_str).unwrap_or("")` chains and the silent field
defaulting go away. Outbound commands are constructed as typed
`SidecarCommand` values and serialized once at the send site; the
per-agent `create_cmd_json: String` replay cache is replaced with the typed
variant so recovery round-trips through `serde_json::to_string` instead of
storing a pre-serialized blob. Parse failures flow through
`MonarchError::Serde` / `MonarchError::sidecar_parse` (already wired by
MON-31) and surface via `mark_agent_desynced` the way malformed envelopes
already do.

This is pure type-safety churn. No behavior changes for any successful
event; the observable difference is that previously-silent malformed fields
now flip the dev-only desync indicator.

## Relevant files and areas

### Source of truth (do not move, just cross-check)

- **`sidecar/src/protocol.ts`** (lines 1–145) — canonical wire contract.
  Eleven command types (`create_session`, `destroy_session`, `prompt`,
  `abort`, `set_model`, `set_thinking_level`, `new_session`, `compact`,
  `load_session`, `extension_ui_response`, `set_custom_prompt`) and five
  event types (`session_ready`, `session_destroyed`, `event`,
  `extension_ui_request`, `error`). The Rust enums must mirror these.
  Note the `ShadowConfig` shape at lines 8–13 — reused by
  `create_session`.

### Inbound — event parsing and application

- **`src-tauri/src/agent_state.rs`** — `LiveAgentState::apply_event`
  (lines 178–408). The central `match event_type` dispatch: 15 known
  variants plus a catch-all. Every branch uses the `get("field").and_then`
  chain with `unwrap_or` defaulting. This is the primary refactor target.
  Helpers `streaming_from_json` (475), `parse_usage` (495), and
  `extract_user_text` (455) also walk `serde_json::Value`; these should
  either be replaced with typed destructuring on the new enum variants or
  repointed to operate on already-parsed sub-structs.

- **`src-tauri/src/agent.rs::handle_sidecar_event`** (lines 696–833) — the
  reader-side dispatcher. Parses each stdout line, matches on the outer
  envelope type (`session_ready` / `session_destroyed` / `event` /
  `extension_ui_request` / `error`), and for `event` extracts the inner
  event and calls `build_persist_commands` + `apply_and_maybe_emit`. This
  is where the one-shot parse of the line into `SidecarEvent` lives.

- **`src-tauri/src/agent.rs::build_persist_commands`** (lines 1044–1156)
  — currently takes `event_type: &str` + `event: &serde_json::Value` and
  walks the payload for `message_end` / `tool_execution_end` fields to
  build `PersistCommand` variants. Should take a reference to the typed
  inner event so `.get("toolCallId").and_then(as_str)` collapses to
  direct field access. Internal to the persistence consumer — fair game
  per the Wave 2 handoff.

- **`src-tauri/src/agent.rs::apply_and_maybe_emit`** (lines 869–950ish) —
  currently takes `&serde_json::Value` and forwards into
  `apply_event`. Signature threads through the new type.

- **`src-tauri/src/agent.rs::mark_agent_desynced`** — already exists; the
  landing point for reader-side parse failures. The current malformed-
  envelope branch at line 767 is the template.

### Outbound — command construction sites

All live in `src-tauri/src/agent.rs`. Current `serde_json::json!` sites
that are actual sidecar commands (per the grep pass):

- **`spawn_agent`** (line 1424 shadow + 1434 `create_session`) and its
  twin **`ws_spawn_agent`** (lines 1819 + 1829). Build the
  `create_session` command with nested `ShadowConfig`. These feed the
  `create_cmd_json: String` replay cache at 1462 / 1852.
- **`send_command`** (1478) and **`ws_send_command`** (1865) — parse
  frontend-provided JSON, inject `agentId`, reserialize. See the open
  question below about how to handle this narrow passthrough.
- **`kill_agent`** (1496) and **`ws_kill_agent`** (1874) — `destroy_session`.
- **`load_session_context`** (1588 for message rows + 1596 outer cmd)
  and **`ws_load_session_context`** (1898 + 1900) — `load_session` with
  an inline message array.
- **`new_agent_session`** (1681) and **`ws_new_agent_session`** (1944) —
  `new_session`.
- **`switch_agent_session`** (1728) and **`ws_switch_agent_session`**
  (1978) — also emit `new_session`.
- **`respond_extension_ui`** (1747) and **`ws_respond_extension_ui`**
  (1991) — `extension_ui_response`.

### Out of scope — `serde_json::json!` sites that are NOT sidecar commands

Do not touch these; they emit frontend-facing events via `emit_event` or
return Tauri response bodies:

- `handle_sidecar_event` line 722 (`ready_event` → `agent-event-{id}`).
- `handle_sidecar_event` line 736 (`null` payload to `agent-exit-{id}`).
- `handle_sidecar_event` line 822 (`error_event` → `agent-event-{id}`).
- `build_persist_commands` line 1130 — the `toolResult` JSON that is
  **stored in SQLite**, not sent to the sidecar. Leave as-is; it is a
  storage-format concern, not a protocol concern.
- `agent.rs` line 1314 and 2011 — Tauri command return bodies for
  `detect_project` / related.

### Replay cache

- **`src-tauri/src/agent.rs` `AgentState::create_cmd_json: String`**
  (declared around line 57, populated at 1462 / 1852, replayed at 468
  inside `send_with_recovery`). This is the replay cache the ticket calls
  out. It currently stores the pre-serialized JSON string; should store
  a `SidecarCommand::CreateSession(...)` value and serialize on
  recovery resend. `send_to_sidecar` / `send_with_recovery` still take
  `&str`, so the serialize happens at the call site.

## What needs to change

### 1. New module: `src-tauri/src/sidecar_protocol.rs`

Define two enums mirroring `sidecar/src/protocol.ts`.

- **`SidecarCommand`** — `#[derive(Serialize)]`,
  `#[serde(tag = "type", rename_all = "snake_case")]`. One variant per
  TS interface: `CreateSession`, `DestroySession`, `Prompt`, `Abort`,
  `SetModel`, `SetThinkingLevel`, `NewSession`, `Compact`,
  `LoadSession`, `ExtensionUiResponse`, `SetCustomPrompt`. Field names
  use `#[serde(rename_all = "camelCase")]` to match the wire. A nested
  `ShadowConfig` struct (name/title/grade/id) is reused by
  `CreateSession`. `LoadSession::messages` uses a
  `LoadSessionMessage { role, content, model }` sub-struct so the
  consumers don't hand-build `serde_json::Value`.
- **`SidecarEvent`** — `#[derive(Deserialize)]`, same serde tagging. The
  five outer envelope types plus an `Unknown { raw: serde_json::Value }`
  fallback (via custom deserializer or an untagged wrapper) for
  forward-compat with sidecar versions that ship new event types the
  Rust side doesn't know yet.
- **`InnerEvent`** (inside the `event` envelope) — separate enum for the
  payload the sidecar wraps in `AgentEventEnvelope.event`. Variants match
  what `apply_event` handles today: `AgentStart`, `AgentEnd`,
  `TurnStart`, `TurnEnd`, `MessageStart { message }`,
  `MessageUpdate { message }`, `MessageEnd { message }`,
  `ToolExecutionStart { tool_call_id, tool_name, args }`,
  `ToolExecutionEnd { tool_call_id, result, is_error }`,
  `CompactionStart { reason }`, `CompactionEnd { aborted }`,
  `AutoRetryStart { attempt }`, `AutoRetryEnd`, `QueueUpdate`,
  `ToolExecutionUpdate`, and `Unknown { raw }`.
- **`Message`** — typed struct for the inner `message` field used by
  `MessageStart` / `MessageUpdate` / `MessageEnd`. Role is an enum
  (`User` | `Assistant` | other); content is kept as
  `Vec<serde_json::Value>` (the per-block content schema is owned by Pi
  SDK and intentionally left opaque — same reasoning that already
  applies in `agent_state.rs` for `ContentBlocks`). `usage` and
  `model` optional; `usage` maps to the existing `Usage` struct in
  `agent_state.rs` (via `#[serde(rename_all = "camelCase")]` on the
  fields).

Module placement: `src-tauri/src/sidecar_protocol.rs`, declared in
`lib.rs`. `mod sidecar_protocol;` next to `mod agent_state;`.

### 2. `LiveAgentState::apply_event` signature flip

Change to `apply_event(&mut self, event: &InnerEvent) -> ApplyOutcome`.
Each match arm becomes a pattern match on the variant with field
destructuring, so `unwrap_or("")` / `unwrap_or(0)` go away entirely. The
`Unknown { raw: _ }` arm sets `desynced = true` the same way the current
catch-all does; it is the one place where "unknown event type" stays a
legitimate runtime condition (sidecar version skew).

`streaming_from_json` and `parse_usage` collapse to direct struct
construction from the typed `Message` / `Usage` structs — or are removed
if the `Message` struct can be cloned directly into
`StreamingMessage`. `extract_user_text` still runs against
`serde_json::Value` (the `content` field is intentionally opaque) but
the caller passes the already-parsed `Message.content` slice instead
of walking `event.get("message").get("content")`.

### 3. `handle_sidecar_event` one-shot parse

Replace `let parsed: serde_json::Value = serde_json::from_str(line)?`
with `let event: SidecarEvent = serde_json::from_str(line)?` and match
on the variants directly. Parse failures flow through `MonarchError` via
the existing `From<serde_json::Error>` impl; the function currently
doesn't return `Result`, so the parse error path becomes an explicit
`if let Err = ...` that logs + calls `mark_agent_desynced(app, ws_tx,
live_states, agent_id)` — matching the existing malformed-envelope
branch. The `agent_id` is inside the variant for each known event type,
so it's available after the match; for the `Err` case we don't have
`agent_id` and the existing behavior is "log and return", which we
preserve.

The `"event"` branch destructures `AgentEventEnvelope { agent_id, event }`
and forwards `&event` (the inner `InnerEvent`) into
`build_persist_commands` and `apply_and_maybe_emit`.

### 4. `build_persist_commands` typed signature

Change to `fn build_persist_commands(agent_id: &str, session_id:
Option<String>, event: &InnerEvent) -> Vec<PersistCommand>`. The
`message_end` and `tool_execution_end` arms become pattern matches on
`InnerEvent::MessageEnd { message }` / `InnerEvent::ToolExecutionEnd
{ tool_call_id, result, is_error }` with direct field access. The
`LogEvent` branch still stores the event as serialized JSON; re-
serialize the typed event back to a string (`serde_json::to_string`)
since the DB schema expects a string blob.

The internal `serde_json::json!` at line 1130 (the stored `toolResult`
content) stays — that is a storage format, not a sidecar command.

### 5. Outbound command sites

Every `let cmd = serde_json::json!({...})` for a sidecar command
becomes `let cmd = SidecarCommand::Xxx { ... }` and the send path
becomes `state.send_to_sidecar(&serde_json::to_string(&cmd)?)` (or
`send_with_recovery`). The `?` hits the existing `From<serde_json::Error>
for MonarchError` impl added in MON-31.

Shadow construction in `spawn_agent` / `ws_spawn_agent` uses the
`Option<ShadowConfig>` field on `CreateSession` directly — the
conditional-build pattern (`if shadow_name.is_some() || ...`) stays,
but emits an `Option<ShadowConfig>` instead of an
`Option<serde_json::Value>`.

### 6. Replay cache: `create_cmd_json: String` → `create_cmd:
SidecarCommand`

Rename to `create_cmd: SidecarCommand`. `send_with_recovery`'s resend
branch (around line 468) does `serde_json::to_string(&state.create_cmd)?`
instead of cloning the pre-serialized string. Populate at 1462 / 1852
with the already-built typed command. Note that we are still holding
the lock when we call `to_string` on recovery resend; this is the same
cost as the MON-38 constraint for `emit_state_event`, and the resend
path is rare (crash recovery only), so the serialization cost under the
`agents` Mutex is acceptable. If it turns out to be noticeable, a
`.clone()` + `drop(guard)` + `to_string` form is a trivial follow-up.

### 7. TypeScript cross-check

Walk `sidecar/src/protocol.ts` field-by-field against the new Rust
enums. Any mismatch (missing optional, camelCase vs snake_case, type
drift) gets reconciled in the Rust module and documented in the PR
description. No changes to TS unless a true drift is found — the
ticket explicitly says the TS side is the canonical contract.

## Resolved decisions

1. **`send_command` / `ws_send_command` passthrough — option (a),
   narrow typed passthrough.** Parse the frontend `command_json` into
   `SidecarCommand`, inject `agentId` via a match on
   `&mut SidecarCommand` (11 arms, one per variant), reserialize via
   `to_string`. First implementation step: grep the frontend for
   every `invoke("send_command", ...)` call site and confirm the
   payload shapes line up with the typed enum. If any call site sends
   a shape that doesn't fit, resolve it by updating the frontend to
   match the canonical `protocol.ts` contract (not by widening the
   Rust type) — the whole point of this ticket is to make the wire
   schema load-bearing.

2. **`Unknown` variant implementation — custom `Deserialize` impl.**
   Self-contained, single-pass parse, clean error messages. Applied
   to both `SidecarEvent` (outer envelope) and `InnerEvent` (inner
   event) — see decision 4. Approx. 20 lines of manual `Deserialize`
   per enum: deserialize the tagged object generically, read the
   `type` field, dispatch to the matching variant's deserializer, fall
   through to `Unknown { raw }` on unknown tags.

3. **`apply_event` location — free function in `sidecar_protocol.rs`.**
   Signature: `fn apply_event(state: &mut LiveAgentState, event:
   &InnerEvent) -> ApplyOutcome`. `LiveAgentState` loses the inherent
   method; `commit_streaming_message` and any other helpers that were
   private to the method become `pub(crate)` methods on
   `LiveAgentState` so the free function can call them. This keeps
   the protocol knowledge (enum + dispatch) co-located in one module
   and lets `agent_state.rs` stay focused on the snapshot shape.

4. **`Unknown` placement — both enums.** Outer `SidecarEvent::Unknown`
   catches new top-level envelope types; `InnerEvent::Unknown` catches
   new event kinds under the `event` envelope. Both flip desync via
   `mark_agent_desynced` the same way.

5. **`TurnStart` / `TurnEnd` — unit variants.** `apply_event` reads no
   fields today. If the TS cross-check surfaces fields the sidecar
   actually ships, promote to struct variants at that point; otherwise
   leave as unit variants.

## Out of scope reminders

- **No `tracing` migration** in `agent.rs`. The file uses `eprintln!`
  throughout; parking-lot item, file-wide sweep later.
- **Do not touch `PersistCommand::apply` / `run_persist_consumer`.**
  MON-31 migrated them to `MonarchError`; stringification at the
  log/desync boundary stays.
- **Do not collapse `spawn_agent` / `ws_spawn_agent` arguments.**
  MON-35 owns that via `SpawnAgentRequest`. The two pre-existing
  `too_many_arguments` clippy warnings stay pre-existing.
- **Do not refactor `ws_*` duplication.** MON-33 owns the shared
  service layer.
- **Do not touch lock topology.** MON-34 owns the `std::sync::Mutex`
  vs `tokio::sync::RwLock` split.
- **Do not touch frontend-facing event emission.** `agent-event-{id}`,
  `agent-exit-{id}`, `agent-state-{id}` payloads are unchanged. The
  legacy raw forwarding at line 796 stays. This ticket is strictly the
  Rust ↔ sidecar wire.
- **Do not touch SQLite storage format.** The `toolResult` content
  JSON built at line 1130 is a DB blob, not a sidecar command.
- **No Rust tests.** Repo has no test harness (see MON-30 parking-lot
  note). Verification is `cargo check` + `cargo clippy` + manual smoke:
  spawn → prompt → tool call → kill → respawn.

## Verification plan

1. `cargo check` — zero new warnings.
2. `cargo clippy -- -D warnings` (minus the two pre-existing
   `too_many_arguments` on `spawn_agent` / `ws_spawn_agent`).
3. `svelte-check` — unchanged; no frontend files touched.
4. Manual smoke from a fresh DB:
   - Spawn an agent → verify `create_session` on the sidecar stdin
     log (dev-mode) matches the previous shape byte-for-byte.
   - Send a prompt that triggers a tool call → verify
     `tool_execution_start` / `tool_execution_end` fold into the
     `LiveAgentState` snapshot the same as before; desync flag stays
     false.
   - Kill the agent → `destroy_session` command shape unchanged.
   - Respawn + trigger recovery path (close sidecar process via
     task manager mid-stream) → verify `send_with_recovery` resends
     the typed `create_session` and the session is rehydrated.
   - Open a second chat (`new_session`) and a switched session
     (`switch_agent_session`) → both emit `new_session` correctly.
5. Diff `sidecar/src/protocol.ts` against the new Rust enums; document
   any reconciled drift in the PR description.
