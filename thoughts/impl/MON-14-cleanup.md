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

<!-- Note anything learned while shipping MON-29 that affects the rest of
the plan. If the fix forced a signature change on emit_event, flag it here
so Wave 1 knows. -->

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

<!-- Each wave-1 issue should leave a one-liner here on merge: PR link,
anything non-obvious, any new findings that went into the parking lot. -->

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

- **`agent.isStreaming` no longer flips during normal operation.** Pre-MON-14
  the `agent_start` / `agent_end` cases in `AgentView.svelte`'s
  `handleEvent` called `updateAgent()` to toggle `isStreaming`, which drives
  `AgentControls.svelte:187` (stop-vs-send button, input disable). Phase 2
  deleted those cases and `LiveAgentState` does not carry an equivalent, so
  the stop button never engages during streaming. Smallest fix: derive in
  `AgentView.svelte` from `live.activityStatus !== ""`. Larger fix: add an
  explicit `isStreaming: bool` to `LiveAgentState` flipped in `apply_event`.
  → **Action:** file as a new sub-issue (MON-40?) before Wave 1 if confirmed
  during MON-29 smoke-testing.

- **Coarse reactivity in `liveAgentStore.applyUpdate`.** Each incoming
  snapshot calls `SvelteMap.set(id, newEntry)` with a fresh object identity,
  so every `$derived` reading any field of `live` re-runs at ~60fps during
  streaming. Pre-refactor did field-level writes into a `$state` entry,
  giving fine-grained reactivity. Fix: keep a stable `$state` entry per
  agent and mutate field-by-field in `applyUpdate`. High-impact for
  perceived smoothness with multiple open tools.
  → **Action:** file as a new sub-issue before Wave 1; this is the single
  biggest "snappy and smooth" win available and the review thread did not
  cover it.

- **`rebuild_agent_state_from_session` both returns the snapshot and emits
  on `agent-state-{id}`.** Callers seed via `seedFromSnapshot` _and_ the
  event fires, producing a dropped duplicate via the `state_version` guard.
  Functionally correct, one wasted serialize+emit per rebuild. Minor.
  → **Action:** probably fold into MON-38 (it is already touching the
  emit path); if the diff grows too large there, file separately.

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
