# MON-51 — Error Notification System

## Summary

Monarch has several error sources but no consistent surface for showing them to the operator. Spawn failures land in `agent.stderrLines` and only render inside the per-agent stderr panel. `sidecar_error` events on `agent-event-{id}` are caught and logged via `console.error` only. Sidecar process exits and the various caught exceptions across the frontend are similarly silent. The plan is to add a small Svelte 5 store + an `App.svelte`-mounted overlay component that pulls these existing error signals into a single, app-wide notification surface — without changing the Rust side and without disturbing the existing per-agent stderr panel, which stays as the deeper drill-down for an agent in `error` status.

## Relevant files and areas

### Existing error sources to wire in

- `src/lib/stores/agentStore.svelte.ts:98-117` — `formatSpawnError(err)`. Parses the `MonarchError` DTO returned from `spawn_agent` into a human-readable string. The notification at this site is the headline acceptance criterion.
- `src/lib/stores/agentStore.svelte.ts:485-489` — call site of `formatSpawnError` inside `createAgent()`'s catch handler. Currently appends the formatted message to `agent.stderrLines[]` and flips `agent.status = "error"`. We add a notification push here.
- `src/lib/AgentView.svelte:195-242` — `handleNarrowEvent`, the dispatcher for `agent-event-{id}` envelope types after the MON-14 narrowing.
- `src/lib/AgentView.svelte:232-235` — the `sidecar_error` case currently does `console.error(...)` only. This becomes a notification push.
- `src/lib/AgentView.svelte:578-584` — the `agent-event-${target.id}` `listen()` registration that drives `handleNarrowEvent`.
- `src/lib/AgentView.svelte:600-606` — the `agent-stderr-${target.id}` listener. Stays as-is for now — Rust does not currently emit on this channel (`src-tauri/src/agent/sidecar.rs:268-276` only `eprintln!`s sidecar stderr), so wiring it would be no-op work and is explicitly out of scope.
- `src/lib/AgentView.svelte` — the agent-exit listener (search for `agent-exit-${target.id}`). Non-zero exit code is the second canonical "the operator must know" signal beyond sidecar_error.

### Frontend layout / mount point

- `src/App.svelte:280-341` — `<main class="app">` shell. The `NotificationStack` mounts after `</main>` and before the modal dialogs (`SpawnDialog`, `ProjectEditor`, `EditAgentDialog`, `SettingsDialog`, `ConfirmDialog`) so it overlays everything but does not affect roster/main-panel sizing.
- `src/App.svelte:94` — pattern for invoking `setupEffects()` on a store from component context.

### Store conventions to mirror

- `src/lib/stores/agentStore.svelte.ts` — class-based singleton store using `$state()` for reactive fields, exported instance, `setupEffects()` called from `App.svelte`.
- `src/lib/toolbox/liveAgentStore.svelte.ts` — secondary reference for the runes pattern; uses `SvelteMap` for per-agent keyed state.

### Type / shape sources

- `src-tauri/src/error.rs:1-144` — `MonarchError` enum and serialised `kind` strings (`sidecar*`, `db`, `persistence`, `http`, `invalidInput`, `notFound`, `lock`, `io`, `serde`). The notification message can use these kinds for icon / styling decisions.
- `src-tauri/src/agent/event_handler.rs:79-226` — Rust side of `agent-event-{id}` emission, including the `sidecar_error` (line 220-225) and `Unknown` (228-238) envelope cases. Useful only as reference — no Rust changes in this issue.
- `src/lib/bindings.ts` — auto-generated Tauri command/error types. Notification payload types should reuse these where applicable rather than redefining them.

## What needs to change

### 1. New: notifications store

A new `src/lib/stores/notificationsStore.svelte.ts` module:

- A small `Notification` type — at minimum: stable `id`, `level` (`error` | `warning` | `info`), `message`, optional `agentId` + `agentName` for context, optional `kind` (mapping `MonarchError.kind` for styling), `createdAt`, `count` for de-dup, optional `durationMs`.
- A class-based `NotificationsStore` exposing a `$state` array, plus `add(input)`, `dismiss(id)`, `dismissAll()`, and a `setupEffects()` method that schedules auto-expiry timers.
- De-duplication: when `add` is called with the same `(level, message, agentId)` within a short window (~5s), instead of pushing a new entry, increment the `count` on the existing one, refresh `createdAt`, and reset its timer.
- Auto-dismiss policy: `error` persists until manually dismissed; `warning` defaults to ~6s; `info` defaults to ~4s. The caller can override per-notification.
- Singleton export (`notificationsStore`).

### 2. New: `<NotificationStack>` component

A new `src/lib/NotificationStack.svelte` component:

- No props; reads directly from `notificationsStore.notifications`.
- Renders a fixed-position overlay in the **top-right**. Z-index must layer above the agent portrait, which is sticky-positioned at the top — pick a z value above the highest existing sticky/dialog layer (audit existing `z-index` usage before settling on a value).
- Stack vertically, newest on top.
- Per-notification card has an optional **header line**: when the notification carries an `agentId`, render the agent name as a clickable link that calls `agentStore.setSelectedId(agentId)` (jumps to that agent's chat, where the stderr panel and compact-error view show the underlying detail). When there is no agent context, omit the header.
- Body: level icon + colour, message text, optional `count` badge when de-duplicated, manual dismiss button.
- Hovering a notification pauses its auto-dismiss timer. Leaving resumes it.
- Stack capacity: cap visible at **5**; older entries collapse into a "+N more" pill at the bottom of the stack. Clicking the pill expands them. Entries still expire / dismiss normally while collapsed.
- No `Esc` keybinding — `Esc` stays reserved for dialogs and other interactions.
- No sound for v1.
- Styling consistent with the existing dialog / panel look (read `SpawnDialog`, `ConfirmDialog`, `SettingsDialog` for the visual language already in use).

### 3. Wire into `App.svelte`

- Import `NotificationStack` and mount it once after `</main>`, before the dialogs block (`src/App.svelte:341`).
- Call `notificationsStore.setupEffects()` alongside the existing `agentStore.setupEffects()` (`src/App.svelte:94`).

### 4. Wire into existing error sources

- `src/lib/stores/agentStore.svelte.ts:485-489` — in the `createAgent` catch, after `formatSpawnError(...)`, push an `error`-level notification with the formatted message and the agent's name.
- `src/lib/AgentView.svelte:232-235` — replace the `console.error` in the `sidecar_error` case with a notification push that includes the agent name + the inner `error` text.
- `src/lib/AgentView.svelte` — in the `agent-exit-{id}` handler, when the exit code is non-zero, push an `error`-level notification.
- Keep `agent.stderrLines[]` writes as-is in all sites — the panel and compact-error view are still the drill-down.

### 5. Tests — introduce Vitest for the store

No frontend test harness exists in the repo today. This issue introduces one — narrowly scoped to the new store:

- Add `vitest` + `@vitest/ui` as devDependencies. Vite already drives the frontend build, so Vitest is the natural fit — no separate config needed beyond a minimal `vitest.config.ts` (or reuse `vite.config.ts`).
- Add a `"test": "vitest run"` script to `package.json`.
- New test file: `src/lib/stores/notificationsStore.test.ts`. Cover:
  - `add` pushes an entry with the expected shape and unique `id`.
  - `dismiss(id)` removes the entry and clears its pending auto-expiry timer (assert no orphan timers using Vitest fake timers).
  - `error` level never auto-expires; `warning` and `info` auto-expire at the configured defaults (verified via `vi.useFakeTimers()` + `vi.advanceTimersByTime`).
  - Dedup: calling `add` with the same `(level, message, agentId)` within the 5 s window increments `count` on the existing entry, refreshes `createdAt`, and resets the timer rather than pushing a new one. Outside the window, a fresh entry is pushed.
  - `dismissAll` empties the array and cancels all timers.
- No component tests for `NotificationStack.svelte` (not worth the jsdom + Svelte testing-library setup at v1).
- No wiring / integration tests against `invoke` (too heavy vs the payoff).

### 6. Manual verification

Run `WEBKIT_DISABLE_DMABUF_RENDERER=1 GDK_BACKEND=x11 npm run tauri dev` and reproduce each wired error path:

- **Spawn failure** — clear the Anthropic API key in Settings, Extract Shadow against an Anthropic model, confirm a toast appears with the formatted message and an agent-name header that jumps to the failed agent's chat.
- **`sidecar_error` mid-session** — temporarily add a one-shot `throw new Error("test sidecar_error")` inside `sidecar/src/runtime-manager.ts` on the stream path, rebuild the sidecar, send a message, confirm the toast appears, then revert the hack.
- **Agent exit** — `pkill -f sidecar/dist/index.js` from another terminal during an active session, confirm the toast appears.

### 7. Conventions and docs

- Add `notificationsStore.svelte.ts` and `NotificationStack.svelte` to the "Start Here" table in `CLAUDE.md`.
- Add a short paragraph in `ONBOARDING.md` (frontend section) on the notification flow: who pushes, what's shown, what's dropped (de-dup window).
- Document the new `npm test` script + Vitest setup in `CLAUDE.md` under "Build & Dev" / type-checking (this is the first frontend test command in the repo).
- No changes to the Rust event channel taxonomy — both the existing channel doc lines in `CLAUDE.md` (line 120) and `ONBOARDING.md` (line 330) stay correct.

## Decisions (resolved)

1. **Position** — top-right, z-index above the sticky agent portrait.
2. **Stack capacity** — cap at 5 visible; overflow collapses into a "+N more" pill.
3. **Click behaviour** — header shows the agent name when agent-specific; clicking the name jumps to that agent's chat. No click action on the body itself; explicit dismiss button handles dismissal.
4. **De-dup window** — strict `(level + message + agentId)` key, 5 s window.
5. **Esc** — untouched; `Esc` stays reserved for dialogs.
6. **Sound** — none for v1.
7. **Tests** — Vitest for `notificationsStore` only; manual smoke for the wiring sites (see sections 5 and 6).

## Out of scope reminders

- No Rust changes. The `sidecar` process keeping stderr on `eprintln!` instead of emitting on `agent-stderr-{id}` is a known gap but separate work.
- No replacement of the per-agent stderr panel (`AgentView.svelte:743-760`) or the compact error view (`AgentView.svelte:647-680`).
- No notification history persistence (no SQLite table, no inbox).
- No OS-level notifications via Tauri's notification API.
- No new generic Rust → frontend event bus for non-error info; this issue covers errors / warnings only.
- Console.error calls that are pure dev diagnostics (e.g. websocket transport reconnection in `src/lib/api.ts:71`) stay on console.
