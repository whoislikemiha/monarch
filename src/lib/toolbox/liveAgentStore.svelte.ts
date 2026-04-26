import { SvelteMap } from "svelte/reactivity";
import { invoke } from "../api";
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
 * Each map entry is a `$state(...)` proxy whose identity is stable across the
 * agent's lifetime in the store (MON-41). Updates mutate fields in place so
 * that consumers reading a single field like `live.isStreaming` only
 * re-invalidate when that field actually changes, instead of once per
 * snapshot.
 */
export const liveAgentStore: { byAgent: SvelteMap<string, LiveAgentState> } = {
  byAgent: new SvelteMap<string, LiveAgentState>(),
};

/** Field-by-field copy from an adapted snapshot onto an existing reactive entry. */
function assignInto(target: LiveAgentState, source: LiveAgentState): void {
  target.items = source.items;
  target.toolExecutions = source.toolExecutions;
  target.streamingMessage = source.streamingMessage;
  target.lastUsage = source.lastUsage;
  target.currentToolGroup = source.currentToolGroup;
  target.activityStatus = source.activityStatus;
  target.eventCount = source.eventCount;
  target.stateVersion = source.stateVersion;
  target.desynced = source.desynced;
  target.isStreaming = source.isStreaming;
}

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
    isStreaming: snapshot.isStreaming,
  };
}

/**
 * Initial seed for an agent — called after `get_agent_state` returns on bind,
 * and again on session switches / history loads / restores.
 *
 * On first seed, allocates a new `$state(...)` entry and installs it. On
 * re-seed (existing entry), mutates the entry in place without consulting
 * `stateVersion` — a seed is authoritative (e.g. `rebuild_agent_state_from_session`
 * can return a snapshot whose version is lower than the previous session's
 * final version).
 */
export function seedFromSnapshot(
  agentId: string,
  snapshot: WireLiveAgentState,
): LiveAgentState {
  const adapted = adaptSnapshot(snapshot);
  const existing = liveAgentStore.byAgent.get(agentId);
  if (existing) {
    assignInto(existing, adapted);
    return existing;
  }
  const entry = $state(adapted);
  liveAgentStore.byAgent.set(agentId, entry);
  return entry;
}

/**
 * Incremental update from the `agent-state-{id}` channel. Drops the snapshot
 * if its version is not newer than what we already have (out-of-order / stale).
 *
 * On accept, mutates the existing entry field-by-field so that consumers
 * reading a single field only invalidate when that field actually changes
 * (MON-41). If no entry exists yet (update arrived before seed), allocates
 * one — matches the pre-MON-41 behavior of the unconditional `.set`.
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
  const adapted = adaptSnapshot(snapshot);
  if (existing) {
    assignInto(existing, adapted);
    return;
  }
  const entry = $state(adapted);
  liveAgentStore.byAgent.set(agentId, entry);
}

/** Drop an agent's entry entirely (on kill / removal). */
export function removeLiveState(agentId: string): void {
  liveAgentStore.byAgent.delete(agentId);
}

/**
 * Dispatch the sidecar abort command for `agentId` (MON-104). Any component
 * — AgentView, AgentRoster, AgentPortrait — can call this without owning a
 * direct handle to the sidecar wire path. Errors are swallowed because abort
 * is fire-and-forget; the canonical signal that it worked is `isStreaming`
 * flipping back to false.
 */
export async function abortAgent(agentId: string): Promise<void> {
  try {
    await invoke("send_command", {
      id: agentId,
      commandJson: JSON.stringify({ type: "abort" }),
    });
  } catch (err) {
    console.error(`[abortAgent] failed for ${agentId}`, err);
  }
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
    isStreaming: false,
  };
}
