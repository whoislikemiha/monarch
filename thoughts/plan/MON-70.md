# MON-70 — Pi SDK streaming: text & thinking deltas only surface at message_end

## Summary

Streaming content arrives from Pi SDK as expected, is applied to Rust's live state, and is debounce-emitted to the frontend — yet the webview renders nothing until the turn completes, then paints everything at once. Root-cause this and make incremental deltas visible.

## Investigation

Instrumented three layers with timestamps + payload size:

1. **Sidecar** (`runtime-manager.ts`) — logs every Pi SDK event. Result: `message_update` fires once per token (hundreds over ~3s). Pi emits cleanly.
2. **Rust** (`emit_state_event`) — logs every snapshot emit with serialized streaming content length. Result: snapshots emit every ~18ms with steadily-growing content (1107 → 2697 bytes). Rust is working correctly.
3. **Frontend** (`AgentView.svelte listen`) — logs every snapshot arrival with `stateVersion`. Result: snapshots arrive rapidly with growing block counts but the **same `stateVersion` across the entire streaming burst**.

## Root cause

`src-tauri/src/sidecar_protocol.rs:503` in `apply_event`'s `MessageUpdate` arm:

```rust
return ApplyOutcome::Debounce;   // <-- early return
```

The early return bypasses the version-bump block at the bottom of the match. Every debounced snapshot during a streaming turn therefore carries the same `stateVersion` as the `MessageStart` that opened the turn.

The frontend's `applyUpdate` in `src/lib/toolbox/liveAgentStore.svelte.ts` drops snapshots whose version is not strictly greater:

```ts
if (existing && incomingVersion <= existing.stateVersion) return;
```

→ the first debounced snapshot applies, every subsequent one is discarded as stale. Only the `message_end` event (which does bump the version via `EmitNow`) lands, producing the "everything at once" visual.

There's even a test — `state_version_unchanged_on_debounce_early_return` — that asserted the buggy behavior. Written at a time when either the frontend check didn't exist or the `MessageUpdate` path wasn't wired yet.

## What needs to change

1. **`src-tauri/src/sidecar_protocol.rs`** — replace the `return ApplyOutcome::Debounce;` with a fall-through so `MessageUpdate(assistant)` bumps `state_version` like every other non-NoOp outcome. Rewrite the corresponding test to assert the version *does* bump.

That's the only line of behavior that needs to change. Back out all debug instrumentation added during investigation (sidecar event log, Rust emit log, frontend listener log).

## Verification

- `cargo check` / `svelte-check` / `npm run build:sidecar` all pass.
- Unit test `state_version_bumps_on_debounce` (renamed from the old one) covers regression.
- Manual: send a prompt to a thinking-enabled model; thinking block and text both grow token-by-token in the UI instead of dumping at the end.

## Out of scope

- UX polish of the thinking/streaming affordance — owned by MON-16.
- Changing the debounce window or the `<=` stale-drop check shape.
- Any sidecar or Pi SDK changes.
