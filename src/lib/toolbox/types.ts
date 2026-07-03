import type { Component } from "svelte";
import type {
  Agent,
  AssistantMessage,
  DisplayItem,
  ToolExecution,
  Usage,
} from "../types";

/**
 * Live, Rust-assembled state for a single agent.
 *
 * Wire shape (Rust-authored): see `LiveAgentState` in src/lib/bindings.ts,
 * emitted on `agent-state-{id}` as a JSON object. The `liveAgentStore`
 * adapter converts the wire shape to this frontend shape on seed/update so
 * tool components continue to see `toolExecutions: Map`, `lastUsage?: Usage`,
 * and a derived `currentToolGroup`. This keeps the toolbox contract frozen
 * (MON-14 Phase 2 zero-diff rule for tool components).
 */
export interface LiveAgentState {
  items: DisplayItem[];
  toolExecutions: Map<string, ToolExecution>;
  streamingMessage: AssistantMessage | null;
  lastUsage?: Usage;
  currentToolGroup:
    | { kind: "tool-group"; executions: ToolExecution[]; turnComplete: boolean }
    | null;
  activityStatus: string;
  eventCount: number;
  /** Monotonically increasing per-agent. Used to drop out-of-order snapshots. */
  stateVersion: number;
  /** True when Rust-side reader hit a parse failure or unreconcilable event. */
  desynced: boolean;
  /** True while the agent is actively producing a turn. Drives Abort/ChatInput/status dots. */
  isStreaming: boolean;
}

/**
 * Setup-time configuration surfaced to tools alongside the live state.
 * Sourced from the parent mount site (prompt file, project instructions),
 * not the Pi event stream — kept separate from `live` so MON-14 could swap
 * the `live` producer to Rust without touching tool components.
 */
export interface AgentSetupContext {
  customPrompt: string | null;
  projectInstructions: string | null;
}

/**
 * Context handed to every toolbox tool. Null when no agent is active.
 * `live` is null while the agent has no running/restored session — DB-backed
 * panels (stats, memory, sessions, identity) must render without it; only
 * live-state panels (context inspector) degrade to an explanatory empty state.
 */
export type AgentContext =
  | {
      agentId: string;
      agent: Agent;
      live: LiveAgentState | null;
      setup: AgentSetupContext;
    }
  | null;

/** The one and only prop every toolbox tool component accepts. */
export interface ToolProps {
  agentContext: AgentContext;
}

/** A single entry in the toolbox registry. */
export interface ToolDefinition {
  id: string;
  title: string;
  /** Inline SVG markup rendered as the rail icon. */
  icon: string;
  component: Component<ToolProps>;
  order?: number;
  hasBackend?: boolean;
}
