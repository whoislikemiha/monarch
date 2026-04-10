# MON-36 — Sidecar process lifecycle: `Drop` impl and `ExitRequested` hook

Linear: https://linear.app/monarch-commander/issue/MON-36
Parent: MON-14 Phase 1 cleanup (Wave 1)
Base branch: `markocvijanovic1998/mon-14-phase-1-rust-state-ownership`

## Problem

`src-tauri/src/agent.rs` spawns the Node sidecar with `kill_on_drop(false)` and
`SidecarProcess` has no `Drop` impl. No code path calls `child.kill()` /
`start_kill()` on Tauri shutdown. The writer task's exit closes stdin, and the
sidecar's `index.ts` *does* handle `rl.on("close", shutdown)` gracefully — but
nothing in the Rust side *guarantees* the writer task ever exits on app close.
On `Tauri Builder::run()` returning cleanly, the tokio runtime is torn down;
on panic unwind, `SidecarProcess` drops but never signals the child. In both
cases orphan `node` processes can accumulate, which is the bug the ticket
reports.

## Key observation

`sidecar/src/index.ts:100` already wires `rl.on("close", shutdown)` →
`manager.disposeAll()` → `process.exit(0)`. So **closing stdin is already the
graceful shutdown protocol**. We do not need a new `SidecarCommand::Shutdown`
wire type. The Rust side just needs to:

1. Close stdin explicitly (not rely on writer task lifecycle).
2. Wait a short bounded window for the sidecar to exit on its own.
3. Hard-kill (`child.start_kill()`) if it is still alive past the deadline.
4. Separately, have a `Drop` impl as a panic-unwind safety net.

This keeps the diff small and avoids churning the protocol (Wave 2 will touch
`SidecarCommand` typing — we should not poke it here).

## Design

### `SidecarProcess` field change

`stdin_tx` moves from `mpsc::UnboundedSender<String>` to
`Mutex<Option<mpsc::UnboundedSender<String>>>`. Rationale: we need an explicit
way to drop the sender from outside (from the shutdown path), and `SidecarProcess`
is held behind `Arc`, so we cannot move fields out. Wrapping the sender in
`Mutex<Option<_>>` lets shutdown `take()` it, which closes the channel, which
stops the writer task, which drops `ChildStdin`, which closes the pipe, which
fires `rl.on("close")` in the sidecar. `write_command` takes an extra brief
uncontended lock — negligible on the write path.

### `impl Drop for SidecarProcess`

Best-effort panic-unwind safety net. Uses `self.child.get_mut()` (std
`Mutex::get_mut` takes `&mut self` without locking, so no poison/lock
contention risk during Drop) to check `try_wait()`; if still running, calls
`start_kill()`. Logs on failure. `start_kill()` is sync and does not require
awaiting a runtime, so it is safe from `Drop`.

### `AgentManager::shutdown_sidecar(timeout: Duration)`

Sync method invoked from the Tauri `RunEvent::ExitRequested` hook:

1. `self.sidecar.lock().take()` → extracts the `Arc<SidecarProcess>` out of the
   manager's slot.
2. `sc.stdin_tx.lock().take()` → drops the sender, triggering the graceful
   stdin-close path in the sidecar.
3. Poll `sc.child.try_wait()` in a `std::thread::sleep(25ms)` loop until the
   child exits or the deadline elapses (default ~1500ms).
4. If still alive at the deadline, call `child.start_kill()` as a hard-kill
   fallback.
5. `Arc` drops at function exit; `Drop::drop` runs but is a no-op because the
   child has either exited or been start-killed already.

Using `std::thread::sleep` (not `tokio::time::sleep`) keeps this callable from
Tauri's sync `RunEvent` closure without needing `block_on` from inside the
runtime thread. A 1.5s worst-case UI-close latency is acceptable during
shutdown.

### `lib.rs` hook

Switch `.run(tauri::generate_context!())` → `.build(context).expect(..).run(|app, event| { .. })`
so we get a `RunEvent` callback. On `RunEvent::ExitRequested { .. }`, look up
`app.state::<Arc<AgentManager>>()` and call `shutdown_sidecar(Duration::from_millis(1500))`.

No other event arms are needed; the explicit closure replaces the bare
context-consuming `.run()` call.

## Out of scope

- **No new `SidecarCommand::Shutdown` wire message.** The existing
  stdin-close → `rl.on("close")` path is the graceful protocol. Adding a wire
  command is strictly more code for no behavioral gain and would pre-touch
  protocol types that Wave 2 (MON-32) is refactoring.
- **No `kill_on_drop(true)` flip.** The ticket lists it as an acceptable
  fallback, but `Drop` + `shutdown_sidecar` gives us graceful-then-hard which
  is strictly better. Leaving `kill_on_drop(false)` documents the intent.
- **Writer task teardown** is unchanged. It exits naturally when the mpsc
  sender closes, same as today — we are just now guaranteeing that close
  happens during shutdown.

## Acceptance

- [x] Closing the Tauri window terminates the sidecar Node process (manual
  verification: Task Manager shows no `node` process under monarch after
  window close).
- [x] Panicking the Rust side in dev does not leave an orphan sidecar (Drop
  impl covers this path).
- [x] Repeated dev restarts do not accumulate zombie `node` processes.
- Build hygiene: `cargo check`, `cargo clippy` (no new warnings).

## Test plan

No automated test — Wave 1 still lacks a Rust test harness on Windows (see
MON-30 parking-lot entry). Manual verification:

1. `pnpm tauri dev`, spawn an agent mid-stream, close the window, check
   Task Manager — no orphan `node` process.
2. Force-panic via an injected `panic!()` in a command handler, confirm same.
3. Restart dev 5x in a row, confirm no accumulation.

## File-level changes

- `src-tauri/src/agent.rs`:
  - `stdin_tx` field type change on `SidecarProcess`.
  - `write_command` locks the new mutex.
  - `ensure_sidecar` wraps the sender in `Mutex::new(Some(..))`.
  - `impl Drop for SidecarProcess`.
  - `AgentManager::shutdown_sidecar(&self, timeout: Duration)`.
- `src-tauri/src/lib.rs`:
  - Switch `.run(context)` to `.build(context).expect(..).run(closure)`.
  - `RunEvent::ExitRequested` arm calls `shutdown_sidecar`.
- `thoughts/impl/MON-14-cleanup.md`:
  - Wave 1 notes bullet on PR open with the PR link.
- `thoughts/impl/MON-36.md`:
  - Post-implementation notes / any surprises.

## Non-goals / parking lot

No new parking-lot items expected — the fix is lifecycle-wiring only. If
sidecar-protocol wrinkles surface mid-implementation, they go to the parking
lot in `thoughts/impl/MON-14-cleanup.md`, not inline here.
