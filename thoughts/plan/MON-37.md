# MON-37 — Ordered persistence pipeline via single-consumer mpsc

Linear: https://linear.app/monarch-commander/issue/MON-37
Parent: MON-14 Phase 1 cleanup (Wave 1 — last ticket)
Base branch: `markocvijanovic1998/mon-14-phase-1-rust-state-ownership`

## Summary

`handle_sidecar_event` in `src-tauri/src/agent.rs` currently fires one
`tauri::async_runtime::spawn_blocking` per inbound sidecar event and drops the
`JoinHandle` (`let _ = spawn_blocking(...)`, line 754). That has two concrete
failure modes:

1. **Silent failures.** `persist_event` swallows every `db.*_internal(..)` call
   with `let _ =`. Combined with the dropped outer join handle, any SQLite
   error — lock contention, schema drift, disk full — disappears with no log.
2. **Out-of-order writes.** `spawn_blocking`'s default pool has up to 512
   workers. Under a burst, nothing prevents the `message_end` task from
   touching SQLite before an earlier `tool_execution_end` task for the same
   message. SQLite is the canonical store per `CLAUDE.md`, and ancestry replay
   plus `rebuild_agent_state_from_session` assume writes land in arrival
   order.

This ticket replaces the per-event fire-and-forget with a single-consumer
persistence task fed by a bounded `tokio::sync::mpsc` channel: FIFO ordering,
observable errors, and natural back-pressure if the DB stalls. It is also a
clean seam for MON-27 (tokio-rusqlite) later.

## Relevant files and areas

- `src-tauri/src/agent.rs`
  - `SidecarProcess` struct (~l.73): unchanged. MON-36's `stdin_tx:
    Mutex<Option<..>>` shape is adjacent but unrelated.
  - `pub struct AgentManager` (l.152) and `AgentManager::new` (l.169): the
    likely owner of the persistence `Sender`. Will gain one new field.
  - `AgentManager::ensure_sidecar` (l.253): spawns reader/writer tasks. This
    is also where the persistence consumer task should be spawned (once, when
    the manager is initialized, **not** per sidecar respawn — the consumer's
    lifetime is the manager's, not the sidecar's).
  - `AgentManager::shutdown_sidecar` (l.213): MON-36's graceful-then-hard
    teardown. Dropping the manager (or explicitly dropping the persist
    sender here) lets the consumer drain and exit. Needs a small addition
    at most.
  - Reader task spawn block (l.328–359): captures `db_clone` and
    `session_map_clone`. Will instead capture a `persist_tx` clone. `db` no
    longer needs to be plumbed through `handle_sidecar_event` once persist is
    offloaded — audit whether any other branch still uses it.
  - `handle_sidecar_event` (l.664) and in particular the `"event"` arm at
    l.728–781: the `spawn_blocking` block at l.749–762 is the only caller
    site. Replaces with a `persist_tx.try_send(cmd)` (or `send(cmd).await`)
    after resolving `session_id` on the producer side.
  - `persist_event` (l.932): current body covers three responsibilities
    — log to `events` table, persist `message_end`, persist
    `tool_execution_end`. Audit: only those two event types currently write
    to `messages`, everything else only logs. The refactor must preserve the
    "log every event, write messages only for these arms" behaviour exactly.
  - `get_session_id` helper (l.613): session_id resolution. Should move to
    the producer side so the command embeds its own `Option<String>`;
    otherwise ordering guarantees don't help when the session_map is mutated
    between enqueue and apply.
- `src-tauri/src/db.rs`
  - `save_message_internal` (l.373), `log_event_internal` (l.415),
    `increment_session_message_count` (l.432): the three call targets that
    `persist_event` currently invokes. Signatures stay as-is; the new
    `PersistCommand::apply` just calls them.
- `src-tauri/Cargo.toml`
  - No `tracing` dependency today. Decision below.
- `thoughts/impl/MON-14-cleanup.md`
  - Wave 1 tracker. Gets a one-line Wave 1 bullet on PR open (per the
    tracker-etiquette rules from the handoff).
- `thoughts/impl/MON-37.md`
  - New impl notes file, per the durable "always commit thoughts/plan and
    thoughts/impl" rule.

## What needs to change

### Shape of the refactor

Introduce a `PersistCommand` enum inside `agent.rs` (or a sibling module —
decide when implementing). The variants correspond to the current effects
of `persist_event`:

- A log-event variant, carrying `agent_id`, `Option<session_id>`,
  `event_type`, serialized event data. Always emitted for every `event`
  arrival, matching current behaviour.
- A save-message variant for the assistant `message_end` case, carrying the
  fully materialized `MessageRow` fields plus the `tokens`/`cost` needed for
  the session stats update. Applying this variant performs *both* the
  `save_message_internal` and the `increment_session_message_count` call, in
  that order — so the stats update cannot race the insert.
- A save-message variant for the `tool_execution_end` case, carrying the
  synthesized `toolResult` `MessageRow`.

The producer (the `"event"` arm in `handle_sidecar_event`) builds zero, one,
or two commands per inbound event:

- Always a log-event command.
- Plus, conditionally, one save-message command if `event_type` matches and
  a session id exists.

Both commands get enqueued in order, on the same channel, by the same
producer task. FIFO is preserved because there is exactly one consumer.
Session-id resolution happens **on the producer side**, before enqueueing,
so the command carries its own `Option<String>`.

### Channel and consumer ownership

- Channel: `tokio::sync::mpsc::channel::<PersistCommand>(256)`. The
  sidecar is a single process emitting human-scale event rates;
  back-pressure should kick in well before 1024, and surfacing stalls
  earlier is preferable. The constant carries a short comment explaining
  the reasoning. Not load-bearing; can be revisited.
- Sender: a new `persist_tx: mpsc::Sender<PersistCommand>` field on
  `AgentManager`. `Sender` is `Clone`, cheap, no lock needed. Initialized in
  `AgentManager::new()`.
- Consumer: a single `tauri::async_runtime::spawn`'d task, also started in
  `AgentManager::new()`. Captures an `Arc<Database>` clone. Pattern:
  `while let Some(cmd) = rx.recv().await { ... }`. Inside the loop, call
  `spawn_blocking(move || cmd.apply(&db))` and **await** the
  `JoinHandle` — this is the single place where blocking work is still
  necessary (rusqlite) but it is now sequential, so ordering and
  observability are both restored.
- `cmd.apply(&db)` returns `Result<(), String>`. On `Err`, log via
  `eprintln!("[monarch] persist failed: {}", e)` to match the existing
  logging style in `agent.rs` (all `eprintln!`). On `JoinError`, also log.
  Do not panic the consumer on error — keep draining.
- On persist failure, **also** call `mark_agent_desynced` so the dev-only
  indicator surfaces DB problems the same way it surfaces parser failures.
  `mark_agent_desynced` takes `&AppHandle + &ws_tx + &live_states +
  agent_id`, so the consumer task captures clones of all three at
  construction time. The `agent_id` is carried on every `PersistCommand`
  variant (producer-side, same as `session_id`) so the consumer can flip
  the right entry without an extra lookup.

### Where the consumer lives

- **Manager-level, not sidecar-level.** Database is `Arc<Database>` and
  lives for the process; the persist task should too. Spawning in
  `AgentManager::new()` means the task starts on manager construction and
  dies when the manager drops (sender drops → channel closes → loop
  exits). This also means the consumer survives sidecar respawns, which is
  what we want — we do not want to lose enqueued commands on a sidecar
  crash.
- The database handle today is created outside the manager and passed
  *into* `ensure_sidecar`. Refactor `AgentManager::new` to take
  `Arc<Database>` (and the clones needed for `mark_agent_desynced`: the
  `live_states` map is already a manager field, so only `AppHandle` and
  `ws_broadcast` need threading — `ws_broadcast` is already a field, and
  `AppHandle` is stored lazily via `set_app_handle`). Consumer task is
  spawned from `new()`; it uses the manager's own `live_states` + `ws_tx`
  clones and reads `app_handle` via the existing `Mutex<Option<_>>` on
  each desync call (no-ops cleanly if the handle isn't set yet).
  Initialization touch points: `lib.rs` (or wherever `AgentManager::new`
  is called) now passes the already-constructed `Arc<Database>`.
- `AgentManager` has no `Drop` today and doesn't need one for MON-37: when
  the `Arc<AgentManager>` held by Tauri state is dropped at process exit,
  all its fields drop, `persist_tx` drops, the consumer loop exits
  naturally. `shutdown_sidecar` does not need to reach into the persist
  pipeline.

### Reader task plumbing

- `ensure_sidecar` currently clones `db_clone` into the reader task so it
  can pass it into `handle_sidecar_event`. After this change, the reader
  task needs a `persist_tx.clone()` instead. `handle_sidecar_event`'s
  signature loses `db` (confirm no other arm uses it) and gains a
  `&mpsc::Sender<PersistCommand>` or captures it via closure — whichever
  reads cleaner.
- The reader task running on `tauri::async_runtime` is fine; `try_send` vs
  `send().await` is the back-pressure decision. Use `send().await` so the
  reader task actually stalls when the DB is lagging — that is the whole
  point of a bounded channel. Logging on full/close would be nice but not
  required; if `send` returns `Err`, the receiver has been dropped which
  only happens at shutdown, so log once with `eprintln!` and return.

### What `persist_event` becomes

The free function `persist_event` is replaced by:

- A `PersistCommand::apply(&Database) -> Result<(), String>` method whose
  per-variant body is the same SQL today, but now returning the inner
  `Result` instead of swallowing it.
- A producer-side `build_persist_commands(...)` helper (or inlined match)
  that takes the inner event and produces the zero-to-two commands to
  enqueue.

Delete `persist_event` once the call site is migrated. Avoid keeping it as
a shim — the diff is small enough.

### Logging decision

Do **not** pull `tracing` into `Cargo.toml` in this PR. Rationale: the rest
of `agent.rs` is 100% `eprintln!`; introducing `tracing` just for this one
site creates two logging conventions in the same file and is larger in
scope than a Wave 1 cleanup. Use `eprintln!("[monarch] persist failed:
{}", e)` and file a parking-lot item "migrate agent.rs logging from
eprintln! to tracing" as Wave 2 territory. The ticket says "logs errors
via tracing" as an acceptance bullet, but the spirit of the bullet is
"errors are visible," which `eprintln!` satisfies in the current
codebase.

### Tests

- The project does not currently have a working Rust test harness on
  Windows (MON-30 parking-lot entry: `#[tokio::test]` hits
  `STATUS_ENTRYPOINT_NOT_FOUND 0xc0000139` because of Tauri DLL linkage,
  and there is no Linux CI). So an automated ordering test as written in
  the ticket is not runnable locally.
- Mitigation: structure the consumer loop as a pure async function that
  takes a `tokio::sync::mpsc::Receiver<PersistCommand>` and an
  `Arc<Database>`. No `AppHandle`, no Tauri dependency. Leave a
  `#[cfg(test)] mod tests` that a future Linux CI job can drive. Do not
  attempt to run the tests locally.
- Write a short manual smoke-test plan in `thoughts/impl/MON-37.md`:
  (a) run a short chat, verify messages table ordering; (b) run a
  tool-heavy chat and check `tool_execution_end` rows precede their
  containing `message_end`.

### Tracker + docs

- On PR open: add a one-line Wave 1 bullet to
  `thoughts/impl/MON-14-cleanup.md` with the PR link, mirroring the
  MON-36/MON-30/MON-38/MON-29 pattern. Do **not** check the tracker
  checkbox (master-only rule).
- `thoughts/impl/MON-37.md`: implementation notes, decisions taken,
  parking-lot additions, manual smoke-test plan. Commit on the same
  branch as the code (durable rule).

## Resolved decisions

1. **Desync on persist failure:** flip `mark_agent_desynced` from the
   consumer in addition to the `eprintln!` log, so the dev indicator
   surfaces DB problems the same way it surfaces parser failures.
2. **Channel capacity:** `256`.
3. **`AgentManager::new` signature:** takes `Arc<Database>` at
   construction time.
4. **Tracing:** not added in this PR; `eprintln!` is consistent with the
   rest of `agent.rs`. File "migrate agent.rs logging to tracing" as a
   Wave 2 parking-lot item.

## Out of scope

- Migrating `db.rs` to `tokio-rusqlite` — that is MON-27.
- Converting `agent.rs` logging from `eprintln!` to `tracing` — parking
  lot.
- Standing up a Rust test harness on Windows — MON-30 parking-lot entry,
  pre-Wave-2 decision.
- Any frontend change. This is purely a Rust-side correctness fix; the
  legacy `agent-event-{id}` and the new `agent-state-{id}` emits are both
  preserved exactly.
- Touching the `SidecarCommand` wire protocol — Wave 2 (MON-31).
- Removing `persist_event`'s legacy behaviour for unhandled event types
  (the `_ => {}` arm) — preserve as-is.
