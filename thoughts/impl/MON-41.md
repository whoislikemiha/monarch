# MON-41 — Coarse reactivity: stable `$state` entry per agent in `liveAgentStore`

PR: https://github.com/whoislikemiha/monarch/pull/32

## What was implemented

Before: every `agent-state-{id}` snapshot caused `liveAgentStore.byAgent.set(agentId, adaptSnapshot(snapshot))`, swapping the whole entry object. At the Rust debounce cap (~60fps during streaming) this collapsed all fine-grained reactivity into a "re-run every derivation once per frame" pattern.

After: each agent has a single `$state(...)` proxy installed on first seed. `applyUpdate` and re-seeds copy fields onto the existing proxy one-by-one, so consumers reading a single field like `live.isStreaming` only invalidate when that field changes. Identity is stable across the agent's lifetime in the store; allocation happens once per agent, cleanup via `removeLiveState`.

## Key decisions

- **Update-before-seed fallback in `applyUpdate`.** If no entry exists yet when an update arrives, allocate a new `$state` entry and `.set` it. Preserves pre-MON-41 behavior (unconditional install) rather than silently dropping the snapshot.
- **`stateVersion` drop rule unchanged in `applyUpdate`, skipped in `seedFromSnapshot`.** Seeds are authoritative (e.g. `rebuild_agent_state_from_session` can return a lower version than the previous session's final); updates still drop stale / out-of-order snapshots by version.
- **Subcomponent for `{#each}` read sites.** `TabBar` and `Sidebar` read `byAgent.get(agent.id)?.isStreaming` inline inside `{#each}` loops, and Svelte 5 `$derived` can't be declared per-iteration. Extracted `AgentStatusDot.svelte` so each iteration gets its own stable `$derived` for the store lookup. `AgentView` and `App.svelte` already pulled the entry into a `$derived` local and were unchanged.
- **Dot CSS moved into the subcomponent.** Scoped CSS doesn't cross component boundaries, so `.tab-dot.*` / `.status-dot.*` rules and `@keyframes pulse` migrated from `TabBar` and `Sidebar` into `AgentStatusDot`, keyed by a `baseClass` prop so both size variants coexist.
- **`adaptSnapshot` untouched.** It still returns a fresh plain object; the store just wraps that in `$state(...)` on first install and copies its fields onto the existing proxy on subsequent calls.
- **Tool components literal zero-diff.** Per the MON-14 Phase 2 rule.

## Files touched

- `src/lib/toolbox/liveAgentStore.svelte.ts` — added `assignInto` helper; rewrote `seedFromSnapshot` and `applyUpdate` to mutate in place over a stable `$state` proxy.
- `src/lib/AgentStatusDot.svelte` — new subcomponent owning the per-row `$derived` lookup and the dot styles for both `tab-dot` and `status-dot` variants.
- `src/lib/TabBar.svelte` — replaced two inline `liveAgentStore.byAgent.get(...)?.isStreaming` reads with `<AgentStatusDot>`; removed now-dead `.tab-dot.*` rules and `@keyframes pulse`.
- `src/lib/Sidebar.svelte` — same treatment for the one sidebar row.
- `thoughts/plan/MON-41.md` — research plan committed alongside.

## What was left out

- **No runtime profiling harness.** Plan flagged a qualitative before/after as the acceptance gate; the `console.count` smoke test referenced in the plan was not run during impl and is left to whoever verifies the PR.
- **No per-key diffing of `toolExecutions` or per-field `$derived` for `currentToolGroup`.** Plan decisions #2 and #3 explicitly defer these — whole-field replacement is good enough because `MessageList` already keys its `{#each}` and no consumer iterates `toolExecutions` independently.
- **No Rust-side changes.** Debounce interval, snapshot shape, emit frequency, and `rebuild_agent_state_from_session` duality are all out of scope.
