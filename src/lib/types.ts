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
  /** MON-73: "rive" | "image" | undefined (undefined = default rive preset). */
  avatarType?: "rive" | "image";
  /**
   * MON-73: For "rive" = path to .riv file (undefined = default).
   * For "image" = built-in web path ("/avatars/foo.svg") or absolute
   * filesystem path (loaded via convertFileSrc).
   */
  avatarPath?: string;
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

// Result of `detect_project` — the auto-generated binding emits an unusable
// serde_json::Value type, so the shape lives here instead.
export interface DetectedProject {
  rootPath: string;
  name: string;
  projectId?: string | null;
  hasInstructions: boolean;
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
  /** MON-71: Rust-injected metadata, currently carrying `durationMs` for finalized thinking blocks. */
  _monarch?: { durationMs?: number };
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
  /** MON-71: wall-clock anchor for the active turn; set on the streaming message so the frontend ticker has an anchor across debounced snapshots. */
  turnStartedAtMs?: number | null;
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
  /** MON-71: wall-clock start in ms since epoch; drives the live "N sec" ticker while running. */
  startedAtMs?: number | null;
  /** MON-71: final duration in ms; set at tool_execution_end and preserved across restart. */
  durationMs?: number | null;
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
  | {
      kind: "user";
      content: string;
      timestamp?: number;
      /** MON-75: persisted image attachments sent with this user message.
       * Only present on snapshots rebuilt from SQLite; empty for items
       * assembled live from sidecar events (the frontend bridges the
       * in-flight window with its ephemeral `sentImages` map). */
      attachments?: MessageAttachment[];
    }
  | {
      kind: "assistant";
      content: ContentBlock[];
      usage?: Usage;
      model?: string;
      timestamp?: number;
      /** MON-71: final turn duration in ms; set at MessageEnd and restored from SQLite on reload. */
      durationMs?: number | null;
    }
  | { kind: "tool-group"; executions: ToolExecution[]; turnComplete: boolean }
  | { kind: "status"; text: string }
  | { kind: "notification"; text: string; level: "info" | "warning" | "error" };

/** MON-75: frontend-side attachment descriptor. Mirrors the
 * `MessageAttachmentRow` specta export but lives here so `types.ts` stays
 * the one place the display layer imports from. */
export interface MessageAttachment {
  path: string;
  mimeType: string;
  position: number;
}
