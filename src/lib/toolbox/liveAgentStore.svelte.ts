import { SvelteMap } from "svelte/reactivity";
import type { LiveAgentState } from "./types";

/**
 * Single source of truth for per-agent live state derived from the
 * `agent-event-{id}` stream. AgentView writes here; AgentView's rendering
 * and toolbox tools read from here. Keyed by agentId.
 *
 * Svelte 5 note: `SvelteMap` is required so that `.get(id)` reads become
 * reactive dependencies and `.set(id, entry)` invalidates derivations that
 * previously read that key. A plain `Map` wrapped in `$state` only tracks
 * reassignments of the outer property, not per-key access — so a freshly
 * spawned agent's entry would not propagate to open tools until remount.
 */
export const liveAgentStore: { byAgent: SvelteMap<string, LiveAgentState> } = {
  byAgent: new SvelteMap<string, LiveAgentState>(),
};

export function emptyLiveState(): LiveAgentState {
  // Each entry is its own $state proxy so that per-field writes
  // (items = ..., streamingMessage = ..., activityStatus = ...) are tracked
  // reactively. The outer Map only tracks add/delete; values stored inside
  // are NOT deeply proxied by Svelte 5 unless we wrap them explicitly.
  const entry: LiveAgentState = $state({
    items: [],
    toolExecutions: new Map(),
    streamingMessage: null,
    lastUsage: undefined,
    currentToolGroup: null,
    activityStatus: "",
    eventCount: 0,
  });
  return entry;
}

/** Create the entry for an agent if missing and return it. */
export function ensureLiveState(agentId: string): LiveAgentState {
  let entry = liveAgentStore.byAgent.get(agentId);
  if (!entry) {
    entry = emptyLiveState();
    liveAgentStore.byAgent.set(agentId, entry);
  }
  return entry;
}

/** Reset an agent's live state to empty in place (keeps the same entry). */
export function resetLiveState(agentId: string): LiveAgentState {
  const fresh = emptyLiveState();
  liveAgentStore.byAgent.set(agentId, fresh);
  return fresh;
}

/** Drop an agent's entry entirely (on kill). */
export function removeLiveState(agentId: string): void {
  liveAgentStore.byAgent.delete(agentId);
}
