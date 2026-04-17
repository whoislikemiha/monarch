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
- Renders a fixed-position overlay (top-right by default, decision below in open questions). Stack vertically, newest on top.
- Per-notification card: level icon + colour, message, optional agent context line, optional `count` badge when de-duplicated, manual dismiss button.
- Hovering a notification pauses its auto-dismiss timer. Leaving resumes it.
- Keyboard: dismisses the topmost on `Esc` (handled inside the component or via an `App.svelte`-level keymap delegation — see open questions).
- Styling consistent with the existing dialog / panel look (read `SpawnDialog`, `ConfirmDialog`, `SettingsDialog` for the visual language already in use).

### 3. Wire into `App.svelte`

- Import `NotificationStack` and mount it once after `</main>`, before the dialogs block (`src/App.svelte:341`).
- Call `notificationsStore.setupEffects()` alongside the existing `agentStore.setupEffects()` (`src/App.svelte:94`).

### 4. Wire into existing error sources

- `src/lib/stores/agentStore.svelte.ts:485-489` — in the `createAgent` catch, after `formatSpawnError(...)`, push an `error`-level notification with the formatted message and the agent's name.
- `src/lib/AgentView.svelte:232-235` — replace the `console.error` in the `sidecar_error` case with a notification push that includes the agent name + the inner `error` text.
- `src/lib/AgentView.svelte` — in the `agent-exit-{id}` handler, when the exit code is non-zero, push an `error`-level notification.
- Keep `agent.stderrLines[]` writes as-is in all sites — the panel and compact-error view are still the drill-down.

### 5. Conventions and docs

- Add `notificationsStore.svelte.ts` and `NotificationStack.svelte` to the "Start Here" table in `CLAUDE.md`.
- Add a short paragraph in `ONBOARDING.md` (frontend section) on the notification flow: who pushes, what's shown, what's dropped (de-dup window).
- No changes to the Rust event channel taxonomy — both the existing channel doc lines in `CLAUDE.md` (line 120) and `ONBOARDING.md` (line 330) stay correct.

## Open questions

1. **Position.** Top-right vs bottom-right. Top-right keeps notifications away from the chat composer and aligns with the modals' focus. Bottom-right is more conventional for app toasts. Default: top-right unless the user prefers bottom-right.
2. **Stack capacity.** Cap the visible stack at e.g. 5 — older overflow gets collapsed into a "+N more" pill, or simply dropped from view (kept in store until expiry). Need to pick.
3. **Click-through behaviour.** Clicking a notification could (a) jump to the originating agent (`agentStore.setSelectedId`), (b) copy the underlying text, (c) do nothing. Default proposal: clicking the agent-context line jumps to the agent; clicking the message body copies. Confirm.
4. **De-dup key strictness.** Should the de-dup key be exact `(level + message + agentId)`, or normalised (e.g. trim trailing identifiers / hashes)? Strict is simpler and probably fine for v1.
5. **Esc keymap.** `App.svelte` already wires `onkeydown={handleKeydown}` (`src/App.svelte:278`). The cleanest spot to hook `Esc` is inside that handler, but that risks competing with dialog-close behaviour. Likely safer: handle inside `NotificationStack` itself with a window listener that bails if a dialog is open. Need to confirm there's no existing global `Esc` consumer that would break.
6. **Sound / visual ping.** Out of scope per the issue, but worth flagging — operators may want an audible cue for `error`-level notifications. Defer unless asked.
7. **Test strategy.** No frontend test harness exists in the repo today (per ONBOARDING). Acceptance is "verified manually in dev with WEBKIT_DISABLE_DMABUF_RENDERER=1 GDK_BACKEND=x11 npm run tauri dev" by reproducing each error path. Confirm this is acceptable.

## Out of scope reminders

- No Rust changes. The `sidecar` process keeping stderr on `eprintln!` instead of emitting on `agent-stderr-{id}` is a known gap but separate work.
- No replacement of the per-agent stderr panel (`AgentView.svelte:743-760`) or the compact error view (`AgentView.svelte:647-680`).
- No notification history persistence (no SQLite table, no inbox).
- No OS-level notifications via Tauri's notification API.
- No new generic Rust → frontend event bus for non-error info; this issue covers errors / warnings only.
- Console.error calls that are pure dev diagnostics (e.g. websocket transport reconnection in `src/lib/api.ts:71`) stay on console.
