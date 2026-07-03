# MON-39 — Phase 1 cleanup: remove dead code and legacy channels

**Linear:** https://linear.app/monarch-commander/issue/MON-39
**Base:** `mon-14-phase-1-rust-state-ownership` @ `f81c017` (post-MON-34, docs commit)
**PR target:** the same Wave 2 base branch, not `master`
**Parent:** MON-14 Phase 1

## Summary

A sweep of nine grab-bag cruft sites in `src-tauri/` that the MON-14 Phase 1 migration left behind and that the Wave 2 refactors (MON-31/32/33/34/35) didn't touch on their way through. All items are line-level or small-module changes — this is an explicit cleanup ticket, not a design pass. Item 7 is already resolved by MON-32's typed `SidecarEvent` (drop from scope). The remaining eight items fall into three buckets: (a) delete or wire up dead state and legacy channels that Phase 2 is about to move past, (b) replace three unsafe helpers (`uuid_v4_simple`, `chrono_now`, `resolve_sidecar_path`) with correct equivalents and migrate persisted rows where formats diverged, (c) fix two runtime hygiene bugs (swallowed `create_dir_all` errors in `persistence.rs`, unbounded `state_version` bumps on unknown events).

## Relevant files and areas

### `src-tauri/src/agent.rs`
The bulk of the work lives here. Key sites:

- **Lines 43–63** — `AgentLifecycleState` enum + `AgentState` struct. `#[allow(dead_code)]` at line 50. `lifecycle` is set to `Idle` once in `spawn` (line 761) and in the tests' `seeded_agent_state` helper (line 1634). **Never transitions.** Item 1.
- **Lines 1037–1183** — `handle_sidecar_event`. Emits `agent-event-{id}` as the "legacy raw channel" in five places: SessionReady (1081), Unknown early-return (1139), ExtensionUiRequest (1174), Error (1180), and the main Event arm (1164). The doc comment at 1030–1036 explicitly labels this a Phase-1 dual emission. Item 2 removes the Event-arm emit (1163–1166) and the Unknown-event forward (1139–1141); the other three (SessionReady / ExtensionUiRequest / Error) are **not** frontend-assembled message events — the ticket text says "the emit sites in `handle_sidecar_event`" but the scope is the message/tool-event forwards, not session lifecycle. **Open question below.**
- **Lines 1582–1597** — `chrono_now()` and `uuid_v4_simple()` helpers, `pub(crate)` (bumped from `pub` by MON-33 so `project.rs` can reuse them). Item 3 and Item 4. Must keep visibility or update `project.rs` at the same time.
- **Lines 965–980** — `resolve_sidecar_path()`. Already has `std::env::current_exe()` fallbacks at the bottom; the two `std::env::current_dir()` probes at 968–969 are the broken ones in a packaged build. Item 5.
- **Hot-path `format!("agent-state-{}", ...)`** — six sites: 559, 646, 1115, 1285, 1297, 1322. Item 8 caches the string on `AgentStateEntry` (defined at `agent.rs:151–155`).

### `src-tauri/src/agent_state.rs`
- **Line 441** — `parse_timestamp` only parses `i64`. Sessions whose `timestamp` column is populated by SQLite's `datetime('now')` default return `None` here. Item 4's migration needs to pick one canonical format so `parse_timestamp` round-trips both the Rust-written and the SQLite-defaulted rows. The `LiveAgentState`'s `timestamp: Option<i64>` shape is i64-seconds, so picking "store Unix seconds as `INTEGER`" is the native fit, but existing `TEXT NOT NULL` columns would need an SQL migration.
- **Lines 163, 187, 207–210** — `ApplyOutcome::EmitNow` / `NoOp` enum and `mark_desynced` helper. Not changed by item 9 directly, but item 9 relies on `mark_desynced` being the single path that bumps `state_version` for desync transitions.

### `src-tauri/src/sidecar_protocol.rs`
- **Lines 622–625** — `apply_event`'s `InnerEvent::Unknown { .. }` arm returns `EmitNow` and sets `desynced = true`. Combined with the wrapper bump at 628–629, this bumps `state_version` per unknown event. Item 9 flips the arm to return `NoOp` (and removes the in-arm `desynced = true`, since `mark_desynced` is the canonical setter called from `handle_sidecar_event:1135`).
- **Note:** the reader-side path in `handle_sidecar_event:1129` already short-circuits `InnerEvent::Unknown` via `mark_agent_desynced` before it reaches `apply_event`. So the saturation vector the ticket describes only fires if the fast-path early-return is bypassed — e.g. unknowns that arrive with an empty `agent_id`. Item 9 is still the right fix (defense in depth) and belongs in the same sweep.

### `src-tauri/src/persistence.rs`
- **Lines 5–17** — `monarch_dir()` (line 9) and `prompts_dir()` (line 15) both `.ok()` on `std::fs::create_dir_all`. Item 6. `read_agent_prompt_file` / `write_agent_prompt_file` / `prompts_dir_string` / the Tauri commands below need to be able to surface the real error; `prompts_dir_string` currently returns `String`, not `Result`, so its shape has to change or it has to lazily initialize.

### `src-tauri/src/project.rs`
- **Line 9** — `use crate::agent::{chrono_now, uuid_v4_simple};`
- **Lines 69–70** — only call sites outside `agent.rs`. Items 3 and 4 have to update these at the same time as the helpers themselves.

### `src-tauri/src/db.rs`
- **Schema at lines 59–158** — every `*_at` / `timestamp` column is `TEXT NOT NULL DEFAULT (datetime('now'))`. Also the `UPDATE ... SET updated_at=datetime('now')` clauses at 288, 378. Item 4's migration has to touch either the DEFAULTs or the Rust writers — both populate the same columns with incompatible formats today.
- Also hosts the `Database::new` entry point where an additive migration step would land for item 4.

### `src-tauri/Cargo.toml`
- `uuid` is transitively present (5 hits in `Cargo.lock`) but not a direct dep. Item 3 adds it as a direct dep with the `v4` feature. Small Cargo.lock churn expected.

### `src-tauri/src/agent.rs` tests (lines 1614+)
- `kill_agent_round_trip_funnels_through_shared_method` seeds both maps via `mgr.inner.lock()` and asserts kill clears both. If any item adds a new `AgentManagerInner` field (none of the scoped items should), update the seed path. If item 8's `AgentStateEntry` gains a cached topic, `live_entry`'s `or_insert_with(|| Arc::new(AgentStateEntry::default()))` (line 436, 1251, 1314) needs a constructor that takes the agent_id.

## What needs to change

At the module / concept level, grouped by item:

1. **Dead `AgentLifecycleState`.** Delete. The struct field, the enum, the `Idle` assignments at `spawn:761` and in `seeded_agent_state:1634`, the `#[allow(dead_code)]` attribute, the `Serialize`/`Deserialize` derives, and any downstream imports. Don't rewire from sidecar events — the ticket offers that as an alternative but "delete" is the scoped choice given Phase 2 will own the real lifecycle model.

2. **Legacy `agent-event-{id}` forward for message/tool events.** Remove the Event-arm emit at 1163–1166 and the Unknown-event forward at 1139–1141 in `handle_sidecar_event`. Keep the SessionReady, ExtensionUiRequest, and Error emits — those are session-lifecycle signals, not stream-assembled events, and the frontend reads them off `agent-event-{id}` independently of `liveAgentStore`. **Verify** this split against `liveAgentStore.svelte.ts` and any `listen('agent-event-...')` sites in the frontend before deleting.

3. **`uuid_v4_simple` → `uuid` crate.** Add `uuid = { version = "1", features = ["v4"] }` as a direct dep in `src-tauri/Cargo.toml`. Replace the helper body with `Uuid::new_v4().to_string()`, keep the function name and `pub(crate)` visibility so `project.rs` stays untouched, or inline at the two call sites (`project.rs:69` and the spawn path in `agent.rs`) and delete the helper. Prefer keeping the helper since there's an existing call site that would otherwise be duplicated.

4. **Timestamp format unification.** Two reasonable shapes:
   - **Option A — ISO8601 everywhere.** Rewrite `chrono_now()` to return an ISO8601 string (add `chrono` if not already direct — it is transitively available). Rewrite `parse_timestamp` in `agent_state.rs` to parse ISO8601 → Unix seconds via `chrono::DateTime::parse_from_rfc3339`. Data migration: existing Unix-seconds rows must be rewritten to ISO8601. SQL migration step at `Database::new` scans the affected columns and converts rows whose content matches `^\d+$`.
   - **Option B — Unix seconds everywhere.** Change the schema DEFAULTs from `datetime('now')` to `strftime('%s','now')`, keep `chrono_now()`, keep `parse_timestamp` as-is. Migration rewrites existing ISO8601 rows to Unix seconds. Cheaper `parse_timestamp` path, but diverges from the SQL ecosystem convention.

   **Recommended: Option A.** ISO8601 is the SQLite-idiomatic format, diffs/logs are readable, the `parse_timestamp` change is one line. Cost is the one-time migration + pulling `chrono` in as a direct dep. The migration runs once at DB open, wraps in a single transaction, and is idempotent (re-running finds no `^\d+$` rows).

5. **`resolve_sidecar_path` packaging fix.** Drop the two `std::env::current_dir()` candidates at lines 968–969. Keep the `MONARCH_SIDECAR_PATH` env override (dev/test escape hatch) and the two `current_exe()`-relative candidates. If dev mode with `cargo tauri dev` relied on `current_dir`, add a `tauri::path::BaseDirectory::Resource`-based candidate for the packaged case, and document the dev workflow in the function doc comment. **Verify** that `cargo tauri dev` still finds `sidecar/dist/index.js` after the change — if it doesn't, the env override becomes mandatory for dev and the README/ONBOARDING may need a note.

6. **`persistence.rs` error propagation.** Change `monarch_dir` / `prompts_dir` from returning `PathBuf` to returning `Result<PathBuf, MonarchError>`. Update the Tauri command surface accordingly — `prompts_dir_string` becomes fallible. Call sites in `read_agent_prompt_file` and `write_agent_prompt_file` propagate via `?`. The `.ok()` suppressions go away.

7. **Out of scope** — already fixed by MON-32.

8. **Cache `agent-state-{id}` topic.** Add a `topic: Arc<str>` (or `String`) field to `AgentStateEntry`. Replace `AgentStateEntry::default()` call sites with a constructor `AgentStateEntry::new(agent_id: &str)`. Replace the six `format!("agent-state-{}", ...)` sites with `entry.topic.as_ref()`. Entries are created in `live_entry` (line 433), the `or_insert_with` closures at 1251 and 1314, and nowhere else — three creation sites total.

9. **Unknown-event state_version bump.** In `sidecar_protocol.rs:622–625`, change the `InnerEvent::Unknown` arm to return `ApplyOutcome::NoOp` and remove the `desynced = true` assignment (the canonical setter is `mark_desynced` from `agent_state.rs:207–210`, called via `mark_agent_desynced` in `handle_sidecar_event`). The wrapper at 628–629 already skips the bump on `NoOp`. Net effect: an unknown event coming through `apply_event` no longer bumps `state_version`; the version only moves when `mark_desynced` fires once per desync transition.

### Cross-cutting deliverables

- `thoughts/plan/MON-39.md` (this file) and `thoughts/impl/MON-39.md` committed on the PR branch, per the handoff-note convention.
- Wave 2 tracker entry in `thoughts/impl/MON-14-cleanup.md` updated in the same PR.
- `cargo check` + `cargo clippy --all-targets` clean locally. The MON-33 `kill_agent_round_trip_funnels_through_shared_method` test still hits the Windows `STATUS_ENTRYPOINT_NOT_FOUND` Tauri DLL quirk on `cargo test` — same CI-only gate MON-33/34 already documented.
- After merge: Linear auto-flip to Done expected; revert to In Review manually until phase-1 reaches master, per the Wave 2 pattern.

## Open questions

1. **Item 2 scope — which `agent-event-{id}` emits come out?** The ticket text says "Remove the emit sites in `handle_sidecar_event`" but there are five. My read is that only the Event-arm forward (1163–1166) and the Unknown-event forward (1139–1141) are in scope, because the other three (SessionReady, ExtensionUiRequest, Error) carry session-lifecycle payloads the frontend consumes through a different path than `liveAgentStore`. Confirm before deleting — a grep of `src/` for `agent-event-` and `listen(` should settle it, and I'll do that pass during implementation unless you want to call it now.

2. **Item 4 — ISO8601 vs Unix seconds.** Option A (ISO8601) is my recommendation; Option B is cheaper. Either works. If you have a preference based on how you want timestamps to look in `sqlite3` dumps and log files, say so now; otherwise I'll go with A.

3. **Item 4 migration shape — one-shot conversion or dual-parse window?** Ticket says "do not silently re-parse both formats forever". A one-shot migration at `Database::new` that runs inside a transaction is the right shape, but it means rolling back to a pre-MON-39 binary after merge would see rows in the new format and might misread them. Given this is still a pre-ship phase-1 branch and there's no production data, one-shot is fine — confirming before I commit to it.

4. **Item 5 — does `cargo tauri dev` still find the sidecar after dropping `current_dir` candidates?** I can verify during implementation; if it breaks, fallback is `MONARCH_SIDECAR_PATH` as a documented dev requirement or adding a `current_dir()`-gated-by-`cfg!(debug_assertions)` candidate. Flagging in case you already know.

5. **Item 8 — `Arc<str>` vs owned `String` for the cached topic?** The topic is only read for `emit` calls; `Arc<str>` is cheaper to clone but `String` is simpler and the allocation is one-per-agent. Recommend `String`.

## Out of scope reminders

- **Item 7** — already resolved by MON-32's typed `SidecarEvent`. Do not reintroduce the `get_str` helper concept.
- **MON-27 async migration.** Do **not** flip `#[tauri::command]` bodies to `async fn`. Do **not** move `db.rs` to `tokio-rusqlite`. Do **not** delete `MonarchError::Lock` or the `lock_poisoned` helper (`db.rs` and `models.rs` still use them — ~44 call sites).
- **`SidecarProcess.{child, stdin_tx}` `std::sync::Mutex` sites.** Graceful-shutdown protocol. MON-34 scoped them out; MON-39 does too.
- **Lock hierarchy on `AgentManagerInner`.** MON-34 made this uniform; don't touch it. If item 8 adds a field to `AgentStateEntry`, that's the inner `tokio::RwLock` — separate lock, no hierarchy impact.
- **Kill-mid-response sidecar session bug** observed during MON-34 manual testing — file a new ticket if reproduced, do not expand MON-39 to cover it.
- **Full `apply_event` catch-all redesign.** Item 9 is a two-line behavior fix, not a redesign of `ApplyOutcome` or the desync window semantics.
