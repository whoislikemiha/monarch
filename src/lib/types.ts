// Shadow grades — from Solo Leveling, highest to lowest
export type ShadowGrade =
  | "Grand Marshal"
  | "Marshal"
  | "General"
  | "Elite Knight"
  | "Knight"
  | "Elite"
  | "Normal";

export const SHADOW_GRADES: ShadowGrade[] = [
  "Grand Marshal",
  "Marshal",
  "General",
  "Elite Knight",
  "Knight",
  "Elite",
  "Normal",
];

// Shadow identity — who this agent is in the army
export interface ShadowIdentity {
  shadowName: string;
  shadowTitle: string;
  shadowGrade: ShadowGrade;
}

// Project — a codebase root that groups agents
export interface Project {
  id: string;
  name: string;
  rootPath: string;
  instructions?: string | null;
  createdAt: string;
  updatedAt: string;
}

// Agent state
export type AgentStatus = "running" | "idle" | "stopped" | "error";

export interface Agent {
  id: string;
  viewKey: string;
  name: string;
  status: AgentStatus;
  projectId?: string;
  provider?: string;
  model?: string;
  thinkingLevel?: string;
  cwd?: string;
  sessionStats?: SessionStats;
  stderrLines: string[];
  exitCode?: number | null;
  shadow?: ShadowIdentity;
  contextWindow?: number;
  sessionId?: string;
  sessions: SessionRecord[];
  sourceSessionId?: string; // Session ancestry to replay when restoring/continuing
  /** MON-66: ISO timestamp when the shadow was archived, or undefined if active. */
  archivedAt?: string;
  /**
   * MON-50: cached lifetime cost from `agent_stats.total_cost`. Loaded
   * alongside the agent on startup and refreshed per turn end so the sidebar
   * counter stays in sync.
   */
  lifetimeCost?: number;
}

// A session record — one conversation
export interface SessionRecord {
  sessionId: string;
  model?: string;
  provider?: string;
  startedAt: string;
  messageCount?: number;
  totalCost?: number;
}

// Reusable spawn preset — captures the fields SpawnDialog exposes so
// users can one-click-prefill the dialog instead of refilling each time.
export interface AgentTemplate {
  id: string;
  name: string;
  provider?: string | null;
  model?: string | null;
  thinkingLevel?: string | null;
  cwd?: string | null;
  shadowName?: string | null;
  shadowTitle?: string | null;
  shadowGrade?: string | null;
  createdAt: string;
  updatedAt: string;
}

// Agent spawn config
export interface AgentConfig {
  provider?: string;
  model?: string;
  thinkingLevel?: string;
  cwd?: string;
  shadow?: ShadowIdentity;
  /** User-supplied context window in tokens. Currently only surfaced for lmstudio. */
  contextWindow?: number;
}

// Session stats from get_session_stats
export interface SessionStats {
  totalTokens: number;
  totalCost: number;
  messageCount: number;
  turnCount: number;
}

// Extension UI request types
export interface ExtensionUIRequest {
  requestId: string;
  method: "select" | "confirm" | "input" | "editor" | "notify" | "setStatus" | "setWidget" | "setTitle" | "set_editor_text";
  title?: string;
  message?: string;
  options?: string[];
  placeholder?: string;
  prefill?: string;
  timeout?: number;
  notifyType?: "info" | "warning" | "error";
  statusKey?: string;
  statusText?: string;
  widgetKey?: string;
  widgetLines?: string[];
  widgetPlacement?: "aboveEditor" | "belowEditor";
}

// Pi message content blocks
export interface TextContent {
  type: "text";
  text: string;
}

export interface ThinkingContent {
  type: "thinking";
  thinking: string;
  redacted?: boolean;
}

export interface ToolCallContent {
  type: "toolCall";
  id: string;
  name: string;
  arguments: Record<string, any>;
}

export interface ImageContent {
  type: "image";
  data: string;
  mimeType: string;
}

export type ContentBlock = TextContent | ThinkingContent | ToolCallContent | ImageContent;

// Pi messages
export interface UserMessage {
  role: "user";
  content: string | ContentBlock[];
  timestamp?: number;
}

export interface AssistantMessage {
  role: "assistant";
  content: ContentBlock[];
  model?: string;
  usage?: Usage;
  stopReason?: string;
  errorMessage?: string;
  timestamp?: number;
}

export interface ToolResultMessage {
  role: "toolResult";
  toolCallId: string;
  toolName: string;
  content: (TextContent | ImageContent)[];
  isError: boolean;
  timestamp?: number;
}

export type PiMessage = UserMessage | AssistantMessage | ToolResultMessage;

export interface Usage {
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
  totalTokens: number;
  cost: {
    input: number;
    output: number;
    total: number;
  };
}

// Streaming delta events
export interface AssistantMessageEvent {
  type:
    | "start"
    | "text_start"
    | "text_delta"
    | "text_end"
    | "thinking_start"
    | "thinking_delta"
    | "thinking_end"
    | "toolcall_start"
    | "toolcall_delta"
    | "toolcall_end"
    | "done"
    | "error";
  contentIndex?: number;
  delta?: string;
  content?: string;
  partial?: AssistantMessage;
}

// Tool execution tracking
export interface ToolExecution {
  toolCallId: string;
  toolName: string;
  args: any;
  result?: any;
  isError?: boolean;
  status: "running" | "done" | "error";
}

// Pi SDK events (via sidecar)
export type PiEvent =
  | { type: "session_ready"; agentId: string; contextWindow?: number }
  | { type: "agent_start" }
  | { type: "agent_end"; messages: PiMessage[] }
  | { type: "turn_start" }
  | { type: "turn_end"; message: PiMessage; toolResults: ToolResultMessage[] }
  | { type: "message_start"; message: PiMessage }
  | {
      type: "message_update";
      message: PiMessage;
      assistantMessageEvent: AssistantMessageEvent;
    }
  | { type: "message_end"; message: PiMessage }
  | {
      type: "tool_execution_start";
      toolCallId: string;
      toolName: string;
      args: any;
    }
  | {
      type: "tool_execution_update";
      toolCallId: string;
      toolName: string;
      args: any;
      partialResult: any;
    }
  | {
      type: "tool_execution_end";
      toolCallId: string;
      toolName: string;
      result: any;
      isError: boolean;
    }
  | { type: "queue_update"; steering: string[]; followUp: string[] }
  | { type: "compaction_start"; reason: string }
  | { type: "compaction_end"; reason: string; aborted: boolean }
  | { type: "auto_retry_start"; attempt: number }
  | { type: "auto_retry_end"; attempt: number }
  | {
      type: "extension_ui_request";
      agentId?: string;
      requestId: string;
      method: string;
      [key: string]: any;
    }
  | { type: "sidecar_error"; error: string };

export interface AgentViewState {
  sessionId?: string;
  /** Count of user+assistant items at snapshot time — compared against DB to detect background updates. */
  messageCount: number;
  items: DisplayItem[];
  toolExecutions: ToolExecution[];
  streamingMessage: AssistantMessage | null;
  /** True if the agent was mid-stream when this snapshot was captured. Used to invalidate the cache. */
  wasStreaming: boolean;
  lastUsage?: Usage;
  showStderr: boolean;
  activityStatus: string;
  eventCount: number;
  currentToolGroup: { kind: "tool-group"; executions: ToolExecution[]; turnComplete: boolean } | null;
}

// Display item — what we render in the message list
export type DisplayItem =
  | { kind: "user"; content: string; timestamp?: number }
  | {
      kind: "assistant";
      content: ContentBlock[];
      usage?: Usage;
      model?: string;
      timestamp?: number;
    }
  | { kind: "tool-group"; executions: ToolExecution[]; turnComplete: boolean }
  | { kind: "status"; text: string }
  | { kind: "notification"; text: string; level: "info" | "warning" | "error" };
