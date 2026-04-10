# MON-14 cleanup tracker

Post-review execution log for the MON-14 Phase 1 cleanup wave. One parent
issue (MON-14) with 11 sub-issues filed on Linear after the three-way agentic
review. This file is the single source of truth while the work is in flight —
status, open questions, and handoff notes between steps live here, not in
scratchpads or PR descriptions.

## Layout

The tracker is organized into **waves**. Each wave is a set of issues that
can ship together (or in parallel) without blocking each other. Waves run
strictly in order — do not start wave _n+1_ until every issue in wave _n_ is
merged.

Each issue entry has:

- **Checkbox** — flip to `[x]` only after the PR is merged into `master`, not
  at PR-open time. If an issue is split mid-flight, note the split in the
  wave's notes block and add the new checkboxes inline.
- **Linear ID + title** — clickable via the Linear plugin; the branch name on
  each issue follows the repo convention.
- **Priority + labels** — copied from Linear at the time of filing. If
  priorities shift, update here too so this doc stays the authoritative view.
- **One-line hook** — the "what does done look like" for that issue. Full
  problem statements live on Linear.

Between each wave is a **Notes** block. Use it for:

- Surprises encountered while implementing (API quirks, test failures,
  cross-issue interactions).
- Decisions that deviate from the Linear description.
- Things the _next_ wave's executor needs to know but that do not belong in
  any single issue's handoff.
- Links to the merged PRs, so the next person can diff against a known-good
  checkpoint.

Keep notes terse — a few bullets per wave is the target. If a wave's notes
start to look like a design doc, it probably means a new sub-issue is hiding
inside and should be filed rather than inlined here.

At the bottom there is a **Parking lot** section for findings that came out
of implementation but do not fit into any existing sub-issue. Anything added
there should either (a) get filed as a new Linear sub-issue and removed, or
(b) get consciously declined with a one-line reason.

## Execution plan

Order comes from the agentic-debate review recommendation:

> Ship MON-29 now (unblocks merge), then do MON-38 / MON-30 / MON-36 / MON-37
> in any order (or parallel), then the typed-protocol track
> MON-31 → MON-32 → MON-35 → MON-33 → MON-34, then MON-39 as a sweep.

### Wave 0 — Unblock merge

The one-line fix that the review identified as a true merge blocker.

- [ ] **MON-29** — Fix double JSON encoding on `agent-state-{id}` channel  · Urgent · `refactor`, `Bug`
  - Done when: `emit_event` stops wrapping already-serialized JSON in another
    JSON envelope; frontend drops its `JSON.parse` of the inner payload;
    one-frame round trip verified in dev.

**Notes (Wave 0):**

- MON-29 PR: https://github.com/whoislikemiha/monarch/pull/18 — fix landed as a new `emit_state_event` helper alongside
  the existing `emit_event`; the shared helper is untouched, so Wave 1's
  MON-38 is free to move serialization out of the write lock without
  refactoring a shared signature. State-emit call sites now clone
  `LiveAgentState` out of the guard and pass `&state` to the new helper, so
  MON-38 only has to swap the `.clone()` for the deferred-serialize pattern.
  Nothing else surprising; `cargo check`, `cargo clippy` (no new warnings),
  and `svelte-check` all clean.

---

### Wave 1 — Independent correctness and hygiene fixes

These four can run in any order or in parallel. They touch different files
and have no cross-dependencies beyond Wave 0.

- [ ] **MON-38** — Serialize `LiveAgentState` outside the write lock  · High · `performance`
  - Done when: `apply_and_maybe_emit` + `rebuild_state_from_session` + the
    debounce task all `clone` → `drop(guard)` → `to_string`. Reader throughput
    is no longer O(history) per event under load.
- [ ] **MON-30** — Fix debounce cancellation race on agent kill/destroy  · High · `Bug`
  - Done when: a killed or destroyed agent cannot emit a stale snapshot from
    a still-running debounce task. Generation / cancelled flag or `Notify`
    drop — whichever is cheaper.
- [ ] **MON-36** — Sidecar process lifecycle: `Drop` impl and `ExitRequested` hook  · High · `Bug`
  - Done when: Tauri shutdown or panic unwind terminates the Node sidecar.
    Verified by killing the app mid-stream and observing no orphan `node`
    process.
- [ ] **MON-37** — Ordered persistence pipeline via single-consumer mpsc  · High · `refactor`, `Bug`
  - Done when: sidecar → DB writes are serialized through one mpsc consumer,
    `JoinHandle`s are observed, DB errors are logged (not silently dropped),
    and `message_end` cannot land after its own `tool_execution_end`.

**Notes (Wave 1):**

- MON-38 PR: _pending_ — audit confirmed MON-29's refactor already moved
  `serde_json::to_string` out from under the write guard (all emit sites now
  use `emit_state_event(&LiveAgentState)` which serializes on the caller's
  stack, not in the guard). The MON-38 PR codifies the invariant by
  standardizing on the explicit `let snap = guard.state.clone(); drop(guard);
  emit_state_event(.., &snap);` shape at every site and documenting the
  constraint on `emit_state_event`. Sites touched: sidecar recovery emit,
  `rebuild_state_from_session`, `apply_and_maybe_emit` (EmitNow branch),
  `mark_agent_desynced`. Debounce task + `session_destroyed` handler already
  had the explicit form and were left alone. `cargo check`, `cargo clippy`
  (no new warnings), and `svelte-check` all clean.
- Parking-lot items (a) and (b) confirmed during MON-38 setup audit and
  filed as **MON-40** (isStreaming flag no longer toggles) and **MON-41**
  (coarse reactivity in `applyUpdate`). Removed from the parking lot.
- Parking-lot item (c) — duplicate emit in `rebuild_agent_state_from_session`
  — audited and declined for folding into MON-38. The Tauri caller dedupes
  via the `state_version` guard in `applyUpdate`; WS clients legitimately
  need the emit; making it conditional would require `emit_state_event`
  signature churn that the briefing explicitly forbade. Left in the
  parking lot with a declined note.

---

### Wave 2 — Typed protocol + error domain track

Strictly sequential: each step depends on the previous one having landed,
because they keep tightening the type system around the same call sites.

- [ ] **MON-31** — Introduce `MonarchError` domain error type  · High · `refactor`
  - Done when: Every Tauri command returns `Result<T, MonarchError>` (or an
    intentional alias); `thiserror` chains replace the `.to_string()` churn;
    poisoned-lock paths surface distinct variants.
  - _Why first in this wave:_ subsequent refactors (MON-32, MON-35, MON-33)
    all touch command signatures — converting the error type once up-front
    avoids three passes over the same lines.
- [ ] **MON-32** — Typed `SidecarEvent` and `SidecarCommand` enums  · High · `refactor`
  - Done when: `apply_event` dispatches on a `#[serde(tag = "type")]` enum
    instead of `get("type").as_str()`; outbound command JSON sites are
    replaced with a typed `SidecarCommand` serialized once. No more
    `unwrap_or("")` on event fields.
  - _Depends on MON-31_: handler returns will thread the new error type.
- [ ] **MON-35** — Re-enable specta coverage for `spawn_agent`  · High · `refactor`
  - Done when: `spawn_agent` collapses its shadow + model fields into
    `SpawnAgentRequest`, fits under the 10-arg specta cap, is re-added to the
    command collection, and the `type Value = unknown; type Vec<T> = T[]`
    post-processing hack in `lib.rs` is deleted.
  - _Depends on MON-32_: the typed command surface is cleaner once the
    sidecar protocol is typed first.
- [ ] **MON-33** — Collapse `ws_*` duplication behind a shared service layer  · High · `refactor`
  - Done when: `#[tauri::command]` handlers and `ws_*` WebSocket counterparts
    both delegate to a single service-layer function per operation. Drift
    risk is eliminated; net ~500 lines of dup removed.
  - _Depends on MON-35_: collapsing the layer is simpler when every command
    already has a clean typed signature.
- [ ] **MON-34** — Unify concurrency primitives across `AgentManager`  · Medium · `refactor`
  - Done when: the `std::sync::Mutex` / `tokio::sync::RwLock` split is
    resolved one way or the other — either `parking_lot::Mutex` everywhere
    sync remains sync, or the whole thing is fully async. No more
    `.lock().map_err(|e| e.to_string())` boilerplate.
  - _Depends on MON-33_: easier to rewrite lock acquisition once the service
    layer is the only caller site.

**Notes (Wave 2):**

<!-- Wave 2 is the longest and most interdependent — leave a checkpoint note
after each issue merges, including any assumptions the next step is making
about the shape left behind. -->

---

### Wave 3 — Sweep

Final tidying pass. Runs after every other wave is merged so it does not
have to re-verify anything.

- [ ] **MON-39** — Phase 1 cleanup: remove dead code and legacy channels  · Medium · `chore`
  - Done when: dead `AgentLifecycleState` is either driven or deleted;
    legacy `agent-event-{id}` message/tool forward is removed now that
    `liveAgentStore` consumes the new channel; `uuid_v4_simple` is replaced
    with the `uuid` crate; `chrono_now` timestamps stop colliding with
    SQLite `datetime('now')` defaults (pick one format, parse both during
    a migration window, then converge).

**Notes (Wave 3):**

<!-- Final sweep notes: what got deleted, what got kept despite the review
flag, and why. -->

---

## Parking lot

Findings from the in-editor review pass that are **not yet filed as Linear
sub-issues**. Each bullet is a candidate — decide during implementation
whether to file, fold into an existing issue, or decline.

- ~~**`agent.isStreaming` no longer flips during normal operation.**~~
  Confirmed during MON-38 setup. Filed as **MON-40** (High, Bug).
  Removed from parking lot.

- ~~**Coarse reactivity in `liveAgentStore.applyUpdate`.**~~ Confirmed
  during MON-38 setup. Filed as **MON-41** (High, performance). Removed
  from parking lot.

- **`rebuild_agent_state_from_session` both returns the snapshot and emits
  on `agent-state-{id}`.** Callers seed via `seedFromSnapshot` _and_ the
  event fires, producing a dropped duplicate via the `state_version` guard.
  Functionally correct, one wasted serialize+emit per rebuild. Minor.
  → **Declined during MON-38 implementation.** The Tauri caller dedupes via
  the `state_version` guard in `applyUpdate` (drops equal version); WS
  clients legitimately need the emit. A conditional-emit path would require
  `emit_state_event` signature churn that the MON-38 briefing explicitly
  forbade ("other Wave 1 issues are not expecting churn there"). Leaving
  the wasted one-emit-per-rebuild as the better tradeoff.

---

## How to update this file

- Check a box only after the PR is merged (`git log master --oneline` has
  the commit).
- Add a bullet to the wave's notes block on every merge. One line minimum:
  PR link + "nothing surprising" or "watch out for X in next step."
- If a finding shows up mid-implementation, add it to the parking lot first,
  then decide whether to file. Never let a finding live only in a PR
  description.
- If an issue is renumbered or split on Linear, update the checkbox line
  here in the same commit that pushes the Linear change.
