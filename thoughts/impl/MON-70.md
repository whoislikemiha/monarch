# MON-70 — implementation notes

## What was implemented

One-line behavioral fix in `src-tauri/src/sidecar_protocol.rs`: the `MessageUpdate(assistant)` arm of `apply_event` no longer early-returns `ApplyOutcome::Debounce`. It falls through to the version-bump block at the end of the match so every debounced snapshot during a streaming turn carries a monotonically increasing `state_version`.

That was the whole fix. Everything downstream — the debounce pipeline in `event_handler.rs`, the frontend's `listen` handler, the `applyUpdate` stale-drop check — was already correct. They were just starved of fresh version numbers.

## Root-cause notes

The frontend contract (`applyUpdate` in `liveAgentStore.svelte.ts`) is "drop snapshots whose `stateVersion <= current`." The Rust contract (pre-fix) was "bump `state_version` only on `EmitNow` outcomes." Those two combined meant debounced emits silently tagged with the prior version → frontend discarded all but the first one in every streaming turn.

The old test `state_version_unchanged_on_debounce_early_return` literally pinned the wrong behavior in place. Renamed + rewritten as `state_version_bumps_on_debounce`.

## Key decisions

- Fixed on the Rust side (the wrong side of the contract) rather than loosening the frontend check. The `<=` check is correct — we shouldn't revisit the same version.
- Did not touch the debounce window, the envelope shape, or the frontend reactivity model. Everything else was fine.

## Files touched

- `src-tauri/src/sidecar_protocol.rs` — fall-through in `MessageUpdate`, test rewrite.
- `thoughts/plan/MON-70.md` — research plan.
- `thoughts/impl/MON-70.md` — this file.

## What was left out

- Thinking content still shows a static "thinking..." during the thinking phase. That's the MON-16 bubble work (already a separate ticket/branch). This fix makes the *underlying* thinking deltas arrive incrementally on the wire; MON-16 renders them.
- Debug instrumentation used during investigation (sidecar stderr log, Rust emit log, frontend console log) was fully removed before shipping.
