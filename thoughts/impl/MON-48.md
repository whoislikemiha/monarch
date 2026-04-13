# MON-48 — Implementation notes

## What was implemented

Removed the duplicated `extract_user_text` function. Promoted the `agent_state.rs` copy from private to `pub(crate)` and updated the one call site in `sidecar_protocol.rs` (inside `apply_event`) to reference it. Dropped the duplicate and its doc comment.

## Files touched

- `src-tauri/src/agent_state.rs` — visibility bump
- `src-tauri/src/sidecar_protocol.rs` — call site updated, duplicate removed

Shipped as part of the bundled bug-sweep PR (#57) alongside MON-49 and MON-55.
