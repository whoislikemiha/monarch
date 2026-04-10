import { SvelteMap } from "svelte/reactivity";
import type { LiveAgentState as WireLiveAgentState } from "../bindings";
import type { AssistantMessage, DisplayItem, ToolExecution, Usage } from "../types";
import type { LiveAgentState } from "./types";

/**
 * Passive receiver for Rust-assembled agent state (MON-14 Phase 2).
 *
 * The entire turn-assembly state machine moved to Rust (`src-tauri/src/agent_state.rs`)
 * and snapshots arrive on `agent-state-{id}`. This store no longer builds
 * state from raw events — callers pull once with `get_agent_state` and
 * subscribe for incremental snapshots, then hand each snapshot to
 * `seedFromSnapshot` (initial) or `applyUpdate` (subsequent).
 *
 * Svelte 5 note: `SvelteMap` is required so that `.get(id)` reads become
 * reactive dependencies and `.set(id, entry)` invalidates derivations that
 * previously read that key.
 */
export const liveAgentStore: { byAgent: SvelteMap<string, LiveAgentState> } = {
  byAgent: new SvelteMap<string, LiveAgentState>(),
};

/**
 * Convert the Rust-emitted `LiveAgentState` wire shape into the frontend view
 * shape consumed by `AgentView.svelte` and toolbox tools.
 *
 * Gotchas captured from the Phase 1 handoff:
 *   - `toolExecutions` arrives as a plain object; tool components expect a Map.
 *   - `lastUsage` arrives as `null`; the view shape uses `undefined` to match
 *     the MON-12/13 toolbox contract (tools read `live.lastUsage?.cost` etc.).
 *   - `currentToolGroup` is not on the wire (it's `#[serde(skip)]`); derive it
 *     by scanning for the last open tool-group in `items`.
 *   - `streamingMessage` shape is structurally compatible with
 *     `AssistantMessage` (same camelCase fields); cast rather than rebuild.
 */
function adaptSnapshot(snapshot: WireLiveAgentState): LiveAgentState {
  const items = snapshot.items as unknown as DisplayItem[];
  const toolExecutions = new Map<string, ToolExecution>(
    Object.entries(snapshot.toolExecutions) as [string, ToolExecution][],
  );

  let currentToolGroup: LiveAgentState["currentToolGroup"] = null;
  for (let i = items.length - 1; i >= 0; i--) {
    const item = items[i];
    if (item.kind === "tool-group") {
      if (!item.turnComplete) currentToolGroup = item;
      break;
    }
  }

  return {
    items,
    toolExecutions,
    streamingMessage: (snapshot.streamingMessage as AssistantMessage | null) ?? null,
    lastUsage: (snapshot.lastUsage as Usage | null) ?? undefined,
    currentToolGroup,
    activityStatus: snapshot.activityStatus,
    eventCount: Number(snapshot.eventCount),
    stateVersion: Number(snapshot.stateVersion),
    desynced: snapshot.desynced,
  };
}

/**
 * Initial seed for an agent — called after `get_agent_state` returns on bind.
 * Replaces any existing entry unconditionally.
 */
export function seedFromSnapshot(
  agentId: string,
  snapshot: WireLiveAgentState,
): LiveAgentState {
  const view = adaptSnapshot(snapshot);
  liveAgentStore.byAgent.set(agentId, view);
  return view;
}

/**
 * Incremental update from the `agent-state-{id}` channel. Drops the snapshot
 * if its version is not newer than what we already have (out-of-order / stale).
 */
export function applyUpdate(
  agentId: string,
  snapshot: WireLiveAgentState,
): void {
  const existing = liveAgentStore.byAgent.get(agentId);
  const incomingVersion = Number(snapshot.stateVersion);
  if (existing && incomingVersion <= existing.stateVersion) {
    return;
  }
  liveAgentStore.byAgent.set(agentId, adaptSnapshot(snapshot));
}

/** Drop an agent's entry entirely (on kill / removal). */
export function removeLiveState(agentId: string): void {
  liveAgentStore.byAgent.delete(agentId);
}

/** Empty detached state used as a fallback before an agent is bound. */
export function detachedLiveState(): LiveAgentState {
  return {
    items: [],
    toolExecutions: new Map(),
    streamingMessage: null,
    lastUsage: undefined,
    currentToolGroup: null,
    activityStatus: "",
    eventCount: 0,
    stateVersion: 0,
    desynced: false,
  };
}
