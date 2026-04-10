# MON-37 — Ordered persistence pipeline via single-consumer mpsc

Linear: https://linear.app/monarch-commander/issue/MON-37
Parent: MON-14 Phase 1 cleanup (Wave 1)

## What was implemented

`handle_sidecar_event` used to fire one `spawn_blocking(persist_event)` per
inbound event and drop the `JoinHandle`. That gave two failure modes:

1. **Silent errors.** Every `db.*_internal(..)` call inside `persist_event`
   was wrapped in `let _ = ..`, and the outer join handle was also dropped,
   so lock contention / schema drift / disk-full all disappeared.
2. **Out-of-order writes.** `spawn_blocking`'s pool has up to 512 workers.
   Under a burst there was nothing preventing `message_end` from landing
   in SQLite before the `tool_execution_end` for the same turn.

This ticket replaces that with a single-consumer persistence pipeline:

- A bounded `tokio::sync::mpsc::channel::<PersistCommand>(256)` on
  `AgentManager`.
- One consumer task, spawned once in `AgentManager::new()`, that drains the
  channel in a `while let Some(cmd) = rx.recv().await { ... }` loop and
  awaits each `spawn_blocking(cmd.apply(..))` before pulling the next.
  Ordering is restored because there is exactly one consumer. Blocking
  work still happens (rusqlite is synchronous — MON-27 removes that), but
  it is sequential.
- `PersistCommand` enum with three variants:
  - `LogEvent` — always emitted, one per inbound sidecar event.
  - `SaveAssistantMessage` — emitted on `message_end`; applying it runs
    `save_message_internal` **and** `increment_session_message_count` in
    that order, so the stats update cannot race the insert.
  - `SaveToolResult` — emitted on `tool_execution_end`.
- Session id is resolved on the producer side (in the sidecar reader
  task) and baked into the `PersistCommand`. This is load-bearing: if
  the consumer re-resolved from the session map, ordering guarantees
  would be meaningless on mutation.
- Back-pressure: the reader task does `persist_tx.send(cmd).await`, not
  `try_send`. If the consumer falls behind, the reader stalls — that is
  the whole point of a bounded channel. Losing events silently under
  load would be worse than a visible stall.
- On persist failure the consumer logs via `eprintln!` (matching the
  rest of `agent.rs`) **and** calls `mark_agent_desynced` so the dev
  indicator surfaces DB problems the same way it surfaces parser
  failures. It does not panic or break the loop.

## Key decisions

- **Manager-lifetime consumer, not sidecar-lifetime.** Spawned in
  `AgentManager::new()`, captures an `Arc<Database>` clone, exits only
  when all `persist_tx` senders drop (i.e. process exit). Consequence:
  enqueued commands survive a sidecar crash+respawn, which is what we
  want — we do not want to lose writes because Node died.
- **Do not store `Arc<Database>` on `AgentManager` as a struct field.**
  `AgentManager::new(db)` takes the handle, passes a clone into the
  consumer task, and forgets it. Other manager methods that need a
  database handle already receive it as a parameter from the Tauri
  command layer; keeping that plumbing avoids a sprawling
  "move everything onto `self`" refactor for no real benefit.
- **`ensure_sidecar` loses its `db` param.** After the reader task stops
  capturing `db_clone`, nothing inside `ensure_sidecar` uses the
  database. Callers (`spawn_agent`, `ws_spawn_agent`, `recover_sidecar`)
  updated to match. `recover_sidecar` still takes `db` for its own
  session replay logic.
- **`app_handle` field is now `Arc<Mutex<Option<AppHandle>>>`.** Needed
  so the consumer task can read it (for `mark_agent_desynced`) without
  a back-reference to `AgentManager`. `set_app_handle` /
  `get_app_handle` bodies are unchanged because `Arc<Mutex<_>>` derefs
  to `Mutex<_>`.
- **`tracing` not added.** The acceptance criteria said "logs errors
  via `tracing`" but the rest of `agent.rs` is 100% `eprintln!`.
  Introducing one new logger for one site creates two conventions in
  the same file. Used `eprintln!("[monarch] persist failed: {}", e)` —
  the spirit of the bullet (errors are visible) is satisfied. Filed
  "migrate agent.rs logging from eprintln! to tracing" as a Wave 2
  parking-lot item.
- **Channel capacity: 256.** Documented in the code comment as tunable.
  The sidecar emits at human-scale rates; 256 is comfortably above a
  normal burst, and hitting the cap surfaces DB stalls via the
  back-pressure stall rather than by growing unbounded memory. Not
  load-bearing.
- **No automated test.** The repo has no working Rust test harness on
  Windows (see MON-30 parking-lot entry), and there is no Linux CI.
  Mitigation per plan: the consumer is a pure async function
  (`run_persist_consumer`) with no Tauri dependency at the type level,
  so a future harness can drive it directly with a stub receiver. The
  `AppHandle` is reached via the `Arc<Mutex<Option<_>>>` slot and is
  `None` until Tauri setup wires it, which cleanly no-ops the desync
  path in a test context.

## Files touched

- `src-tauri/src/agent.rs`
  - New: `PersistCommand` enum + `apply`, `build_persist_commands`,
    `run_persist_consumer`.
  - `AgentManager` struct: `app_handle` is now `Arc<Mutex<Option<_>>>`;
    added `persist_tx: mpsc::Sender<PersistCommand>`. No `db` field.
  - `AgentManager::new(db: Arc<Database>)`: creates the channel, spawns
    the consumer task.
  - `ensure_sidecar`: loses `db` param. Reader task captures a
    `persist_tx` clone instead of `db_clone`.
  - `handle_sidecar_event`: loses `db` param, gains `persist_tx`
    param. The `"event"` arm replaces the dropped-`spawn_blocking`
    block with `build_persist_commands` + `persist_tx.send(..).await`.
  - Free function `persist_event` deleted.
  - Callers of `ensure_sidecar` (`spawn_agent`, `ws_spawn_agent`,
    `recover_sidecar`) updated.
- `src-tauri/src/lib.rs`: `AgentManager::new` is called with
  `database.clone()`.
- `thoughts/impl/MON-14-cleanup.md`: Wave 1 notes entry for MON-37,
  plus parking-lot item for the `tracing` sweep.
- `thoughts/plan/MON-37.md`: the implementation plan, committed on the
  same branch (per the durable "always commit thoughts/plan and
  thoughts/impl" rule).

## Manual smoke-test plan

The Rust test harness situation (MON-30 parking-lot entry) means there is
no automated ordering test shipping with this PR. Before merge, smoke:

1. **Happy-path chat.** Start Monarch, spawn an agent, send a short
   prompt that replies without tool calls. Confirm `messages` in
   SQLite has the new `assistant` row after the turn, and
   `sessions.message_count` / `total_tokens` / `total_cost` incremented.
2. **Tool-heavy chat.** Spawn an agent on a real repo, send "run
   `ls`" or similar. Confirm that in SQLite, the `toolResult` row for
   each tool call precedes the next `assistant` row — this is the
   ordering invariant the ticket exists to enforce.
3. **Sidecar crash recovery.** Kill the Node sidecar process
   externally mid-stream. Confirm: no persist errors, no desync
   indicator, and the recovery path (MON-14 `recover_sidecar`) reloads
   history cleanly. The persist consumer should survive because it is
   manager-lifetime.
4. **DB lock simulation (optional).** Open the `monarch.db` in a
   sqlite3 CLI with `BEGIN IMMEDIATE;` and hold it while sending a
   chat. Expected: the `eprintln!("[monarch] persist failed: ...")`
   line fires on the first failed write and the dev indicator flips
   (`VITE_MONARCH_DEBUG_DESYNC=1`). Releasing the lock and sending
   another message should clear the indicator on the next
   `message_start`.

## What was left out

- **Migrating `db.rs` to `tokio-rusqlite`** — that is MON-27.
  `spawn_blocking` is still present inside the consumer; this ticket
  only makes the blocking work sequential, not non-blocking.
- **Converting `agent.rs` logging to `tracing`** — parking-lot item
  (see decision above).
- **Automated ordering test** — blocked on the Rust test harness
  situation. The consumer is shaped so a future harness can drive it.
- **`ensure_sidecar` db-param removal cascading further.** I removed
  `db` from `ensure_sidecar` only. `send_with_recovery`, `recover_sidecar`,
  and the Tauri command handlers still take `db` because they use it for
  session replay / project resolution / message writes outside the
  persist pipeline. Not worth a wider refactor in a Wave 1 ticket.
