import type { Component } from "svelte";
import type {
  Agent,
  AssistantMessage,
  DisplayItem,
  ToolExecution,
  Usage,
} from "../types";

/**
 * Live, event-stream-derived state for a single agent.
 * Owned by liveAgentStore, written by AgentView's event handler,
 * read by AgentView's rendering and by toolbox tools.
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
}

/** Context handed to every toolbox tool. Null when no agent is active. */
export type AgentContext =
  | { agentId: string; agent: Agent; live: LiveAgentState }
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
