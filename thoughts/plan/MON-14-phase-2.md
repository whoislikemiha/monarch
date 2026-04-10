# MON-14 Phase 2 — Frontend cutover + docs + dev indicator

Parent plan: [`MON-14.md`](./MON-14.md). Phase 1: [`MON-14-phase-1.md`](./MON-14-phase-1.md).

**Prerequisite:** Phase 1 is merged. Rust owns `LiveAgentState`, emits on `agent-state-{id}`, and `get_agent_state` is callable. The legacy `agent-event-{id}` channel is still flowing in parallel — Phase 2 switches consumers over.

**Before starting Phase 2:** Read the `HANDOFF` section at the bottom of [`MON-14-phase-1.md`](./MON-14-phase-1.md). It contains the actual shapes, constants, and gotchas as they landed. The plan below describes the *intent*; the handoff describes the *reality*.

## Goal

Make the frontend a passive receiver of Rust-assembled state. Delete all turn-assembly logic in Svelte. Rewire `AgentView.svelte` and `liveAgentStore.svelte.ts` to pull-then-subscribe. Surface a dev-only desync indicator. Update onboarding docs.

After Phase 2, MON-14's parent plan acceptance criteria are fully met.

## Scope

1. **Rewrite `src/lib/toolbox/liveAgentStore.svelte.ts`** as a passive receiver.
   - Drop `emptyLiveState` and any frontend-authored seed shape.
   - Keep `SvelteMap<string, LiveAgentState>` for per-key reactivity.
   - `seedFromSnapshot(agentId, snapshot, version)` — creates or replaces the entry with the incoming snapshot.
   - `applyUpdate(agentId, snapshot, version)` — if `version <= entry.version`, drop (out-of-order / stale); otherwise replace.
   - `removeLiveState(agentId)` — unchanged.
   - No imports from raw event-payload shapes. No assembly logic anywhere in the store.
2. **`src/lib/toolbox/types.ts`** — replace the hand-authored `LiveAgentState` interface with a re-export of the generated type from `src/lib/bindings.ts`. Other boundary types (`AgentRow`, `MessageRow`, etc. currently in `src/lib/types.ts`) become re-exports too, as part of the same pass. Pure-frontend view-state types stay hand-written.
3. **Delete `AgentView.svelte`'s raw-event handler.**
   - Remove `handleEvent`, `streamingMessage` tracking, tool-group assembly, `lastUsage` / `activityStatus` / `eventCount` writes — everything the `unlistenEvent` closure currently reaches.
   - On agent bind:
     - `invoke("get_agent_state", { agentId: target.id })` → `seedFromSnapshot(target.id, snapshot, version)`.
     - `listen("agent-state-{target.id}", payload => applyUpdate(target.id, payload.snapshot, payload.version))` (exact payload shape per Phase 1 handoff).
   - Keep the `agent-exit-{id}` and `agent-stderr-{id}` listeners as-is — they are independent channels.
   - Verify the `AgentContext.live` contract that tool components consume is unchanged. Tool component files must have a literal zero diff — this is the acceptance gate proving the MON-12/MON-13 abstraction held.
4. **Migrate `src/lib/api.ts` (and callers) to the generated typed command wrappers** from `bindings.ts`, where such wrappers exist. The WS-fallback path in `api.ts:44-96` must remain intact — wrap the generated wrappers with the existing shim, or route them through the same `invoke` primitive. Per-agent event channels (`agent-state-{id}`, `agent-event-{id}`, `agent-exit-{id}`) stay on `listen<T>` with an imported type parameter (tauri-specta typed events don't cover interpolated channel names).
5. **Dev-only desync indicator.**
   - Build-time flag: `VITE_MONARCH_DEBUG_DESYNC`. Default **on** in dev/debug build config, unset in prod. (No existing `VITE_` flag convention in the repo as of Phase 1 — this is the first.)
   - When flag is on and `agentContext.live.desynced === true`, render a small visible badge or corner marker inside `AgentView`. Not a blocking overlay.
   - Rationale is documented in the parent plan §14: first time the state is observable; we want to notice desync during dev without UX cost in prod.
6. **`ONBOARDING.md` §5 "Agent lifecycle" and §6 "Sidecar protocol" updates.**
   - `agent-state-{id}` is the canonical assembled-state channel.
   - `agent-event-{id}` is reduced to UI requests and error pings; message/tool event forwarding on that channel is deprecated and will be removed in the follow-up issue.
   - Document the pull-then-subscribe reload pattern and the `state_version` reconciliation rule.
   - Note that `LiveAgentState` is Rust-authored and the TS shape is generated via `specta` into `src/lib/bindings.ts`.
   - Document the phased tokio migration: Phase 1 (async reader, sync write path, `spawn_blocking` for DB) landed in MON-14; Phase 2/3 (async write path + command handlers + `tokio-rusqlite`) is the follow-up (MON-27 or whichever issue id the handoff confirms).

## Out of Phase 2

- Deletion of legacy `agent-event-{id}` message/tool forwarding (follow-up issue).
- Async write path and Tauri command handler conversion (follow-up).
- `tokio-rusqlite` migration (follow-up).
- Any new UI surface or toolbox tool.
- Cross-agent observability views.

## Acceptance criteria for Phase 2 (ends with parent plan satisfied)

- `svelte-check` clean, `cargo check` clean, sidecar `tsc` clean.
- `AgentView.svelte` no longer imports raw event types for assembly purposes.
- The `handleEvent` function and all its call sites are gone from `AgentView.svelte`.
- `liveAgentStore.svelte.ts` has no assembly logic — only `seedFromSnapshot`, `applyUpdate`, `removeLiveState`, and `SvelteMap` reactivity.
- `AgentContext.live` shape is unchanged; toolbox tool component files have zero diff vs. master.
- Agent creation → send-message → streaming → tool execution → completion works end-to-end, rendered entirely from `agent-state-{id}` snapshots.
- Continuing a prior session correctly seeds from `get_agent_state` on mount.
- Sidecar crash recovery produces a single `agent-state-{id}` flush per recovered agent, and the UI reflects rebuilt items without a manual refresh.
- Dev desync indicator appears when forced on in a debug build; is absent in a prod build.
- `ONBOARDING.md` §5 and §6 read coherently with the new model.

## Open micro-questions for Phase 2

- How to force the `desynced` flag in a dev smoke test — possibly a temporary Rust-side debug toggle or a malformed sidecar line fixture. Decide during implementation.
- Whether any tool component's reads from `AgentContext.live` hit a field that the generated type renames vs. the hand-written one. If so, prefer renaming the Rust field to preserve the tool-component zero-diff rule.
