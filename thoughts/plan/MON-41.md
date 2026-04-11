# MON-41 — Coarse reactivity: `liveAgentStore.applyUpdate` swaps object identity per snapshot

Linear: [MON-41](https://linear.app/monarch-commander/issue/MON-41/coarse-reactivity-liveagentstoreapplyupdate-swaps-object-identity-per) (parent: MON-14)
Branch: `markocvijanovic1998/mon-41-coarse-reactivity-liveagentstoreapplyupdate-swaps-object`

## Summary

MON-14 Phase 2 made the frontend a passive receiver of Rust-assembled `LiveAgentState` snapshots arriving on `agent-state-{id}`. In the current store, every incoming snapshot triggers `liveAgentStore.byAgent.set(agentId, adaptSnapshot(snapshot))` — a fresh object identity per emit. `SvelteMap.get(agentId)` reads are reactive on the key, so swapping the value invalidates every consumer of `live`, even ones that read a single scalar field like `live.isStreaming`. At the Rust debounce cap (16ms ≈ 60fps) during streaming, this collapses fine-grained reactivity into a coarse "re-run every derivation once per frame" pattern, and was confirmed during the MON-38 setup audit as the single biggest "snappy and smooth" win available. The fix is to keep a stable, mutably-updated `$state` entry per agent and copy fields onto it in `applyUpdate`, so only derivations reading the actually-changed fields invalidate.

## Relevant files and areas

### The store itself

- `src/lib/toolbox/liveAgentStore.svelte.ts`
  - `liveAgentStore.byAgent: SvelteMap<string, LiveAgentState>` — keyed per-agent map, defined at module scope (line 19-21).
  - `adaptSnapshot(snapshot)` (line 36-63) — converts the wire `LiveAgentState` (`toolExecutions` as object, `lastUsage` null, no `currentToolGroup`) into the view-shape `LiveAgentState` (Map, optional, derived `currentToolGroup`). This mapping is unchanged by this task.
  - `seedFromSnapshot(agentId, snapshot)` (line 69-76) — currently does an unconditional `.set` after adapting.
  - `applyUpdate(agentId, snapshot)` (line 82-92) — drops by `stateVersion` guard; on accept, does a full-object `.set`. **This is the hot path we are rewriting.**
  - `removeLiveState(agentId)` (line 95-97).
  - `detachedLiveState()` (line 100-113) — empty fallback used by `AgentView` before an agent is bound. Not keyed into the store; unaffected.

### View-shape types

- `src/lib/toolbox/types.ts`
  - `LiveAgentState` interface (line 20-36) — the frontend shape with `items`, `toolExecutions`, `streamingMessage`, `lastUsage?`, `currentToolGroup`, `activityStatus`, `eventCount`, `stateVersion`, `desynced`, `isStreaming`.
  - `AgentContext.live: LiveAgentState` (line 50-57) — the toolbox contract that must not change.

### Consumers (invalidation surfaces to verify)

- `src/lib/AgentView.svelte`
  - `live` derivation (line 68-70) reads `liveAgentStore.byAgent.get(boundAgentId)` — the primary heavy reader.
  - Per-field derivations (line 253-258): `items`, `streamingMessage`, `lastUsage`, `activityStatus`, `eventCount`, `isStreaming`. These currently all re-run per emit because their upstream (`live`) re-identifies; after the fix they should only re-run on field change.
  - `live.desynced`, `live.stateVersion` reads in the dev desync badge (line 600-604).
  - `live.items` reads in `countPersistedMessages` calls (line 304, 660) — called from click handlers, not reactive hot paths, but still should stay correct.
- `src/lib/TabBar.svelte` — `liveAgentStore.byAgent.get(agent.id)?.isStreaming` reads at line 48 and 81, one per tab. Today every tab re-renders on every frame of every streaming agent. After the fix they should only re-render when the referenced agent's `isStreaming` actually flips.
- `src/lib/Sidebar.svelte` — same pattern at line 143.
- `src/App.svelte` — `liveAgentStore.byAgent.get(activeTabId) ?? null` at line 547, used for status-dot logic.
- `src/lib/toolbox/tools/PlaceholderTool.svelte:13` — reads `agentContext.live.items`.
- `src/lib/toolbox/tools/ContextInspectorTool.svelte:104-105` — reads `agentContext.live.items` and `agentContext.live.lastUsage`.

Tool component files must remain literally untouched (the MON-14 Phase 2 zero-diff rule for toolbox components).

### Context / history

- `thoughts/impl/MON-14-cleanup.md` — records MON-41 as a confirmed parking-lot item from MON-38 setup, and describes the intended direction ("stable `$state`-wrapped entry, mutate fields"). Declined-emit context for `rebuild_agent_state_from_session` is also here — it's relevant because `seedFromSnapshot` is the "after" hook for rebuilds, and the same rebuild both returns a snapshot (seed) *and* re-emits one (update). Current dedup via `stateVersion` depends on the seeded entry already having the correct version, so the re-seed path must set `stateVersion` correctly for the `applyUpdate` drop rule to fire.
- `thoughts/plan/MON-14-phase-1.md` — sets up the wire contract: snapshots are a flat `LiveAgentState`, `stateVersion: u64`, dual emit (`agent-state-{id}` + legacy) is in place. Nothing here changes.
- `thoughts/plan/MON-14-phase-2.md` — spelled out the `SvelteMap`-based passive receiver design. This task is the perf follow-up to that design, not a revision of it.

## What needs to change

At the module / concept level:

1. **Stable entry identity in `liveAgentStore.byAgent`.** Each agent's value becomes a reactive object whose identity is stable across the agent's whole lifetime in the store. Allocation happens exactly once — on first `seedFromSnapshot` — and is freed by `removeLiveState`. Every `applyUpdate` and every subsequent `seedFromSnapshot` *mutates* the existing entry rather than replacing it.

2. **`applyUpdate` becomes a field-by-field copy.** After adapting the incoming wire snapshot, assign each field onto the existing entry (`existing.items = adapted.items`, `existing.toolExecutions = adapted.toolExecutions`, `existing.streamingMessage = adapted.streamingMessage`, and so on for `lastUsage`, `currentToolGroup`, `activityStatus`, `eventCount`, `stateVersion`, `desynced`, `isStreaming`). The `stateVersion` drop rule runs first, unchanged. The `adaptSnapshot` mapping is unchanged — only the assignment site changes.

3. **`seedFromSnapshot` handles the create-vs-reseat split.** On first seed (no existing entry), create a new reactive entry, populate it, `.set` it into the map. On re-seed (existing entry — session switch, new session, history load, restore), apply the same field-by-field mutation used by `applyUpdate`, **without** the `stateVersion` drop rule, since a seed is an authoritative reset (e.g. after `rebuild_agent_state_from_session` returns a brand new state; its `stateVersion` may be lower than the previous session's final version). This also covers the MON-14-cleanup note that the rebuild path emits *and* returns a snapshot — the seed now mutates in place, the subsequent emit arrives via `applyUpdate` and gets dropped (or reconciled) by version comparison.

4. **Reactive wrapping decision.** The entries need Svelte 5 deep reactivity so that `existing.items = x` invalidates readers of `items` and nothing else. Options to evaluate during implementation:
   - **Module-level `$state` proxy factory.** `.svelte.ts` files support runes, so wrap each new entry via `$state({ ...adapted })` before `.set`. Deep proxying means plain array and object field replacements are tracked at the field level. The `SvelteMap` is then only "signaling" presence/absence of the key; field-level invalidation runs through the proxy.
   - **Manual per-field `$state`.** Avoid the proxy overhead and have each field be its own state source. Heavier to set up and the `LiveAgentState` TS shape would need to stay a plain-looking object for `AgentContext.live` — not preferred.
   - **`SvelteMap` + plain object.** Would not work — the `SvelteMap` only tracks set/get/delete; field mutations on the stored object would not invalidate anything.
   The strong default is option 1: `$state({...})` proxy at entry creation. Confirm during implementation that `Map<string, ToolExecution>` and `DisplayItem[]` behave correctly under the proxy (they should — Svelte 5 proxies arrays, and `Map` objects are wrapped with a reactive flavor when created inside a `$state`; if not, we replace the whole `toolExecutions` field per update anyway, which is still one field vs. the whole entry).

5. **`adaptSnapshot` contract stays.** Keep it pure and keep returning a fresh plain object. The entry creator wraps the result in `$state`. This preserves the current separation between "wire-to-view adapter" and "store write".

6. **No changes to consumers.** `AgentView.svelte`, `TabBar.svelte`, `Sidebar.svelte`, `App.svelte`, and both tool components keep their current read sites. The reactivity improvement comes for free once the store holds a stable proxy. Tool components specifically must have a zero diff.

7. **Preserve the `stateVersion` drop rule.** The guard at the top of `applyUpdate` (incoming version ≤ existing version → return) stays exactly as-is. Required for:
   - Out-of-order snapshots from the Rust reader task.
   - The declined duplicate-emit from `rebuild_agent_state_from_session` (same version as the seed, gets dropped).

8. **Type stability.** `LiveAgentState` in `src/lib/toolbox/types.ts` describes a plain shape; the runtime value is a `$state` proxy but the TS type is the same interface (Svelte's runes are type-transparent). No changes expected to `types.ts` or to `AgentContext.live`.

## Decisions (locked)

1. **Read pattern at every consumer site: derived-local, not inline.** Everywhere that currently calls `liveAgentStore.byAgent.get(id)` inline (`AgentView` `live` derivation, `TabBar` two tab-dot sites, `Sidebar` status-dot site, `App.svelte` active-tab read), pull the entry into a `$derived` local first, then read fields off the local. This gives Svelte's tracker a single stable proxy reference per consumer and lets per-field reads register as per-field dependencies, which is both cleaner and more predictable than relying on whatever `SvelteMap.get(...)?.field` inline does through the tracker. Zero behavior change — only a read-site refactor.
2. **Whole-field replacement for `items` and `toolExecutions`.** `applyUpdate` assigns `existing.items = adapted.items` and `existing.toolExecutions = adapted.toolExecutions`. Rationale:
   - `MessageList` already iterates `items` in a keyed `{#each}`, so Svelte's child reconciliation only re-renders the streaming last item even when the array reference changes.
   - No consumer iterates `toolExecutions` independently — tool components reach executions via `tool-group` entries inside `items`. Per-key map diffing would add code for ~zero observable win.
   - Per-key diffing is explicitly deferred; only revisit if a profile shows a specific hot spot after the main fix lands.
3. **`currentToolGroup`: leave as-is.** `adaptSnapshot` keeps recomputing it per update; it's written as a field on the entry like everything else, so readers invalidate only when the assignment happens. A per-entry `$derived` would be cleaner semantically but requires extra entry-factory plumbing for no meaningful win.

## Remaining open items

1. **Where to measure.** There's no profiling harness in the repo. A qualitative "before/after" check on a streaming turn with multiple open tools is the acceptance gate. During impl, consider a temporary `console.count` on one tool-component render path to confirm the fix, then remove before commit.
2. **`$state` proxy + `Map<string, ToolExecution>` field behavior.** Confirm during impl that a `Map` stored as a field on a `$state`-wrapped object is reassignable cleanly (`existing.toolExecutions = newMap`) and that readers of `live.toolExecutions` invalidate. This is the expected Svelte 5 behavior but the `Map` case is the one worth eyeballing, since the proxy's handling of non-POJO fields is worth a quick smoke test before trusting it.

## Out of scope

- Any Rust-side change: debounce interval, snapshot shape, emit frequency, `emit_state_event` signature, `rebuild_agent_state_from_session` return/emit duality.
- Wire-shape adjustments or `adaptSnapshot` rewrites (only the result's destination changes).
- Tool component refactors — they must keep a literal zero diff.
- New UI surfaces, dev indicators, metrics.
- MON-27 async persistence and tokio-rusqlite migration.
- Revisiting the declined "duplicate emit from rebuild_agent_state_from_session" — it's load-bearing for WS clients.
