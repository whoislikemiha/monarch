# MON-51 — Implementation notes

## What was implemented
A single top-right notification surface that surfaces errors and warnings across the Monarch UI. Before this change, errors either landed in per-agent `stderrLines` (only visible inside one agent's view), got logged via `console.error` (invisible without devtools), or — in the case of Pi's auto-retry exhaustion — weren't observable at all from the frontend. The feature covers spawn failures, `sidecar_error` events, non-zero agent exits, and Pi retry exhaustion, with dedup, auto-expiry for non-error levels, hover-pause, stack cap + overflow pill, and agent-name headers that jump to the originating agent's chat.

## Key decisions

- **Listener registration lives in `agentStore`, not `AgentView`.** The plan originally put the `sidecar_error` hook inside `AgentView.handleNarrowEvent`, but that listener only attaches for the currently-viewed agent. The whole motivating case is "agent #4 silently rate-limits while I'm focused on agent #2," which required per-agent listeners that outlive view switches. A new `registerAgentListeners` helper registers both the `agent-exit-{id}` and `agent-event-{id}` listeners for every spawned agent and folds their teardown into the existing `exitListeners` map so `killAgent` stays one line.

- **Pi retry exhaustion is surfaced via the sidecar, not a new channel.** Testing revealed that LM Studio-off is Pi's *auto-retry* path, not its *throw* path — `session.prompt()` resolves quietly after maxAttempts with only an `auto_retry_end { success: false, finalError }` event. Rust drops that field. Rather than thread a new field through the protocol, the sidecar's `AgentSessionEventListener` mirrors retry exhaustion to a top-level `type: "error"`, which Rust already forwards as `sidecar_error`. Smallest cross-layer change.

- **No `setupEffects()` on the notifications store.** The plan mentioned it for parity with `agentStore`, but the store uses plain `setTimeout` for expiry — no `$effect` registrations needed, and the component tree doesn't need to own any of its lifecycle.

- **Dedup key is strict.** Exact `(level, message, agentId)` within a 5 s window collapses into a `×N` count badge. Simpler than normalising the message, avoids false positives, and has been enough to prevent the loop-stacks the issue called out.

- **Vitest introduced for the store, not the component.** First frontend test harness in the repo. Scoped tightly to store logic — the code where bugs hide (timers, dedup math, error-level persistence). Component-level and integration tests were skipped as low-value given the setup cost at v1.

- **`AgentView`'s `console.error` for `sidecar_error` kept as a dev diagnostic.** The user-facing toast fires from the store's listener; the `console.error` remains useful when debugging the active agent and was cheap to leave.

## Files touched
- `src/lib/stores/notificationsStore.svelte.ts` (new) — the runes-based store.
- `src/lib/stores/notificationsStore.test.ts` (new) — 10 unit tests.
- `src/lib/NotificationStack.svelte` (new) — fixed-position overlay.
- `src/App.svelte` — mount the stack above the main flex layout.
- `src/lib/stores/agentStore.svelte.ts` — wire spawn-failure toasts in both spawn paths, add `registerAgentListeners`, switch `spawnStoppedAgent` to use `formatSpawnError` for consistency.
- `src/lib/AgentView.svelte` — update the `sidecar_error` comment to reflect that toasts now come from the store.
- `sidecar/src/runtime-manager.ts` — mirror `auto_retry_end { success: false }` to a top-level `error` event.
- `package.json` — `vitest` devDep, `npm test` + `npm run test:watch` scripts.
- `CLAUDE.md`, `ONBOARDING.md` — doc updates (test command, file table, Notifications section explaining the flow).

## What was left out
- **`agent-stderr-{id}` wiring.** Rust's sidecar reader currently only `eprintln!`s; the channel is defined but never emitted on. Per-agent sidecar stderr routing requires the Node sidecar to tag its stderr lines before printing — separate work.
- **Keyboard `Esc` dismiss.** Intentionally out per user decision — `Esc` stays reserved for dialogs.
- **Component-level tests for `NotificationStack.svelte`.** Store logic is tested; rendering is not. A future ticket can add jsdom + Svelte testing-library if the cost becomes worth it.
- **Sound / visual ping for `error`.** Flagged in the plan, deferred.
- **Persisted notification history / inbox.** Never considered in scope.

## Follow-ups filed during this work
- **MON-79** — Extract Shadow button should be gated on provider + model selection. Discovered while testing LM Studio: the spawn succeeded silently with no model set.
- **MON-80** — Subscription-backed agents (Anthropic Pro/Max, OpenAI Codex) should show usage-against-quota instead of per-token cost. Research-first ticket.
