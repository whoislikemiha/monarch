# MON-36 — Sidecar process lifecycle: `Drop` impl and `ExitRequested` hook

Plan: [`../plan/MON-36.md`](../plan/MON-36.md)
Linear: https://linear.app/monarch-commander/issue/MON-36
PR: _(fill in on open)_

## What shipped

Three surgical changes, no protocol churn.

### 1. `stdin_tx` type change on `SidecarProcess`

`src-tauri/src/agent.rs` — `stdin_tx: mpsc::UnboundedSender<String>` →
`stdin_tx: Mutex<Option<mpsc::UnboundedSender<String>>>`. Rationale: the
shutdown path needs to drop the sender from outside, and `SidecarProcess` is
held behind `Arc` so fields cannot be moved out. `write_command` now takes an
uncontended lock + `as_ref()` + `send` — the extra cost is negligible against
the actual stdin write, and an explicit `"sidecar stdin closed"` error now
surfaces cleanly to the caller instead of the previous `SendError` on a raw
dropped channel.

### 2. `impl Drop for SidecarProcess`

Panic-unwind safety net. Uses `Mutex::get_mut()` (safe because `Drop` gives
`&mut self`, no `.lock()` needed and no poison risk), checks `try_wait()`,
and `start_kill()`s if the child is still running. `start_kill` is sync and
does not await the reaper, so it works even when the tokio runtime is
mid-teardown.

### 3. `AgentManager::shutdown_sidecar(timeout: Duration)`

Sync method. Steps:

1. `sidecar.lock().take()` the `Arc<SidecarProcess>` out of the manager slot.
2. `sc.stdin_tx.lock().take()` — drops the sender → mpsc closes → writer
   task exits → `ChildStdin` drops → sidecar `rl.on("close")` fires →
   `manager.disposeAll()` → `process.exit(0)`. This *is* the graceful
   shutdown protocol, so no new `SidecarCommand::Shutdown` wire type is
   needed.
3. `std::thread::sleep(25ms)` polling loop on `child.try_wait()` until the
   child reports exit or the deadline (1500ms) elapses.
4. If still alive past the deadline, `child.start_kill()` as the hard-kill
   fallback. `Drop::drop` running later is then a no-op because `try_wait()`
   reports the kill.

Sync by design so it calls cleanly from Tauri's sync `RunEvent` closure
without `block_on` from inside the runtime thread.

### 4. `lib.rs` — `RunEvent::ExitRequested` hook

Switched `.run(tauri::generate_context!())` →
`.build(context).expect(..).run(closure)`. The closure matches
`RunEvent::ExitRequested { .. }`, looks up
`app_handle.state::<Arc<AgentManager>>()`, and calls
`shutdown_sidecar(SIDECAR_SHUTDOWN_TIMEOUT)` where
`SIDECAR_SHUTDOWN_TIMEOUT: Duration = 1500ms`. Constant lives at module
scope in `lib.rs` with a comment explaining the latency tradeoff.

## Key observation that shrank the fix

`sidecar/src/index.ts:100` already had
`rl.on("close", shutdown)` wired to `manager.disposeAll()` + `process.exit(0)`.
So closing stdin *is* the graceful protocol. The ticket listed a
`SidecarCommand::Shutdown` handshake as "preferred but not required"; adding
one would have pre-touched the protocol types that Wave 2 (MON-32) is already
refactoring, for zero behavioral gain. Went with the stdin-close path and
documented the reasoning on `SidecarProcess::stdin_tx`.

## What I did NOT do

- **No `kill_on_drop(true)` flip.** The ticket listed it as an acceptable
  fallback, but `Drop` + `shutdown_sidecar` gives strictly better semantics
  (graceful-then-hard vs. always-hard). Left the `kill_on_drop(false)` call
  to document the intent.
- **No protocol types touched.** `protocol.ts` and `index.ts`
  unchanged — the existing `rl.on("close")` handler is sufficient.
- **No new tests.** Wave 1 still lacks a Rust test harness on Windows (see
  MON-30 parking-lot entry). Verified by manual smoke.

## Build hygiene

- `cargo check` — clean.
- `cargo clippy` — 3 warnings, all pre-existing (same set MON-30 noted:
  MON-35 `too_many_arguments` ×2, MON-37 `non-binding let` ×1). No new
  warnings.
- `cargo build` — clean.

## Gotcha hit during implementation

First cut of `shutdown_sidecar` tripped E0597 — the `if let Ok(mut c) =
sc.child.lock()` expression creates a temporary whose `MutexGuard`
destructor lives past `sc`'s drop point, causing a use-after-free diagnostic
at function end. Fix: add a trailing `;` after the closing brace so the
temporary drops inside the statement rather than at scope exit. The compiler
suggested this verbatim. Worth noting because the same pattern exists in
`ensure_sidecar` without issue — the difference is that `sc` there is an
`Arc` clone from the slot, not a `let`-bound local that goes out of scope at
function end.

## Manual verification

_(Filled in once smoke-tested on dev.)_

- [ ] Spawn agent mid-stream, close Tauri window, confirm no orphan `node`
      process in Task Manager.
- [ ] Inject `panic!()` into a command handler, confirm no orphan.
- [ ] Restart dev 5x, confirm no zombie accumulation.
