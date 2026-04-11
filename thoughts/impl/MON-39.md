# MON-39 — Phase 1 cleanup: remove dead code and legacy channels

## What was implemented

A grab-bag sweep that closes out the MON-14 Phase 1 migration. Eight of the
nine ticket items landed as eight small commits on the phase-1 base branch.
Item 7 (`get_str` helper) was already resolved by MON-32's typed
`SidecarEvent` and dropped from scope per the ticket's own escape clause.

Per item:

- **Item 1 — dead `AgentLifecycleState`.** Deleted the enum and the
  `lifecycle` field on `AgentState`. `is_streaming` and `thinking_level` on
  the same struct were also set-once, never-read, so they came out too;
  that let the `#[allow(dead_code)]` come off the struct, which the item 1
  acceptance criterion explicitly required. `thinking_level` stays on the
  `AgentRow`, the sidecar protocol, and the spawn command — it's only the
  in-memory `AgentState` copy that was dead.
- **Item 2 — legacy `agent-event-{id}` dual emission.** Removed only the
  stream-event forwards: the Event-arm emit and the Unknown-event fallback
  in `handle_sidecar_event`. `SessionReady`, `ExtensionUiRequest`, and
  `Error` emits stay. `AgentView.svelte:136-137` documents that three-signal
  shape as the **final** form of the channel, not a deferred cleanup, and
  the frontend still actively listens at `AgentView.svelte:463`. Removing
  them would break the UI today.
- **Item 3 — `uuid_v4_simple`.** Added `uuid = "1"` with the `v4` feature
  as a direct dep (was already transitively present) and replaced the body
  with `Uuid::new_v4().to_string()`. Helper name and visibility kept so
  `project.rs` call sites didn't churn.
- **Item 4 — timestamp format unification (the biggest item).** Chose
  RFC3339 `%Y-%m-%dT%H:%M:%SZ` end-to-end:
  - `chrono_now()` uses `chrono::Utc::now()`; `chrono` added as a direct
    dep with `default-features = false, features = ["clock"]`.
  - `parse_timestamp` in `agent_state.rs` uses `DateTime::parse_from_rfc3339`.
  - Schema `DEFAULT` clauses switched from `(datetime('now'))` to
    `(strftime('%Y-%m-%dT%H:%M:%SZ','now'))` for fresh DBs, and every
    `UPDATE ... SET updated_at = datetime('now')` in `db.rs` switched to
    the `strftime` form so Rust-side writers stop introducing divergent
    rows.
  - The two `events` INSERTs — the only writers that relied on the column
    DEFAULT — now bind `chrono_now()` explicitly.
  - A new `Database::migrate_timestamps_to_rfc3339` runs once at
    `Database::new` inside an `unchecked_transaction`. For each of 12
    affected columns it runs two `UPDATE` passes: the first converts
    Unix-seconds-as-TEXT rows via the `'unixepoch'` modifier (matched by
    `GLOB '[0-9]*' AND NOT GLOB '*-*'`), the second converts SQLite's
    space-separated `datetime('now')` output (matched by
    `GLOB '*-*-* *:*:*' AND NOT GLOB '*T*'`). Idempotent on re-run because
    RFC3339 rows match neither clause. Covers projects, agents, sessions,
    messages, memories, events, and agent_templates.
- **Item 5 — `resolve_sidecar_path`.** Dropped the two `current_dir()`
  probe candidates (undefined in a packaged Tauri build). All probes now
  root at `current_exe()`. Dev mode still works because
  `target/debug/monarch.exe + ../../../sidecar/dist/index.js` reaches the
  project root. `MONARCH_SIDECAR_PATH` stays as the manual override.
- **Item 6 — `persistence.rs` error propagation.** `monarch_dir` and
  `prompts_dir` now return `Result<PathBuf, MonarchError>`, propagating
  `create_dir_all` failures via `?` instead of `.ok()`. `prompts_dir_string`
  and the `get_prompts_dir` Tauri command became fallible, which changed
  the specta binding surface — `bindings.ts` regenerated via
  `cargo run -- --export-bindings`. One-line diff:
  `getPromptsDir: () => string` → `() => typedError<string, ErrorDto>`.
- **Item 7.** Out of scope — resolved by MON-32.
- **Item 8 — `agent-state-{id}` topic allocation on the hot path.** Added
  `AgentStateEntry::new(agent_id: &str)` with a precomputed `topic: String`
  field. The three `or_insert_with(Default)` call sites and the six
  `format!("agent-state-{}", ...)` emit sites all use the cached topic now.
  Per-event allocation count for the topic string is 0.
- **Item 9 — unknown-event `state_version` saturation.** `apply_event`'s
  `InnerEvent::Unknown` arm now returns `ApplyOutcome::NoOp` and no longer
  sets `desynced = true` inline. `mark_desynced` — called from the reader
  task's Unknown early-return in `handle_sidecar_event` — remains the sole
  version-bump surface for desync transitions. The original saturation
  vector only fires if an unknown event bypasses the early-return (e.g.
  empty `agent_id`), but the fix is still correct as defense in depth.

## Key decisions

- **Deleted `is_streaming` and `thinking_level` from `AgentState` alongside
  `AgentLifecycleState`.** The item 1 acceptance required removing the
  `#[allow(dead_code)]` attribute, which meant any remaining dead field
  would trigger a compiler warning. Both fields were set-once and never
  read, so deletion was consistent with the "cleanup sweep" spirit of the
  ticket.
- **Kept `SessionReady` / `ExtensionUiRequest` / `Error` on `agent-event-{id}`.**
  Verified against `AgentView.svelte` that these are the channel's final
  shape, not a deferred removal. Documented the narrowing in the
  `handle_sidecar_event` doc comment so a future reader does not
  misinterpret the remaining emits as "also scheduled for removal".
- **Option A (RFC3339 everywhere) over Option B (Unix seconds).** Cheaper
  `parse_timestamp` path for Option B, but RFC3339 is the SQLite-idiomatic
  format, human-readable in `sqlite3 .dump`, and the migration complexity
  was the same either direction. The user confirmed A in review.
- **Two-pass migration SQL, not three.** The third pass (skip already-
  RFC3339 rows) is implicit — the two WHERE clauses both fail on rows that
  contain a `T` separator. Less SQL, same idempotency guarantee.
- **Explicit bind in `events` INSERTs instead of trusting the DEFAULT.**
  `CREATE TABLE IF NOT EXISTS` does not re-apply the DEFAULT clause on an
  existing table, so simply rewriting the schema string would leave
  existing DBs producing the old `datetime('now')` format on new
  `events` inserts until their next restart. Binding `chrono_now()`
  explicitly makes Rust the single source of truth for the format.
- **Packaged Tauri build gap out of scope.** `tauri.conf.json` has no
  `externalBin` sidecar bundling; item 5's fix (removing `current_dir`)
  is strictly about eliminating the "undefined in packaged build" bug.
  Wiring the sidecar into a packaged app is a separate ticket.

## Files touched

- `src-tauri/Cargo.toml` — added `uuid` and `chrono` as direct deps.
- `src-tauri/Cargo.lock` — downstream of Cargo.toml.
- `src-tauri/src/agent.rs` — items 1, 2, 3, 4 (`chrono_now`), 5, 8.
  Struct field deletions, dual-emit removal, helper rewrites, path
  resolver, `AgentStateEntry::new`, six emit-site updates.
- `src-tauri/src/agent_state.rs` — item 4 (`parse_timestamp`).
- `src-tauri/src/db.rs` — item 4 schema DEFAULTs, `events` INSERT bind,
  `migrate_timestamps_to_rfc3339` helper + wiring in both `new()` and
  `new_in_memory()`.
- `src-tauri/src/persistence.rs` — item 6 full rewrite.
- `src-tauri/src/sidecar_protocol.rs` — item 9, one arm.
- `src-tauri/src/ws.rs` — item 6 call-site `?` propagation.
- `src/lib/bindings.ts` — regenerated; one line changed for `getPromptsDir`.
- `thoughts/plan/MON-39.md` — research plan (committed alongside impl).
- `thoughts/impl/MON-14-cleanup.md` — Wave 2/3 tracker entry updated.
- `thoughts/impl/MON-39.md` — this file.

## What was left out

- **Item 7** — already resolved by MON-32, as the ticket itself
  anticipated.
- **MON-27 async migration.** `#[tauri::command]` handlers stay sync,
  `db.rs` stays on sync `rusqlite`, `MonarchError::Lock` / `lock_poisoned`
  stay in `error.rs` because `db.rs` and `models.rs` still use them.
- **`SidecarProcess.{child, stdin_tx}` `std::sync::Mutex` sites.**
  Graceful-shutdown protocol; MON-27's call.
- **Packaged Tauri sidecar bundling.** `externalBin` wiring is a
  deployment ticket.
- **Kill-mid-response sidecar session bug** observed during MON-34 manual
  testing. Still needs reproduction before filing.
- **`eprintln!` → `tracing` migration in `agent.rs`** — parking lot item,
  still parked.

## Testing

- `cargo check` clean at every step.
- `cargo clippy --all-targets` clean after the final commit.
- `cargo test` still hits the MON-33-era Windows `STATUS_ENTRYPOINT_NOT_FOUND`
  Tauri DLL quirk locally — CI / Linux runs the `kill_agent_round_trip_funnels
  _through_shared_method` assertion. The test did not need updating because
  no item added a field to `AgentManagerInner`.

## Linear state

Expect the merge to auto-flip MON-39 to Done via the GitHub integration.
Revert to In Review manually until phase-1 reaches master, per the MON-33 /
MON-34 pattern.
