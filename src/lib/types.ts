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

// Agent state
export type AgentStatus = "running" | "idle" | "stopped" | "error";

export interface Agent {
  id: string;
  name: string;
  status: AgentStatus;
  provider?: string;
  model?: string;
  thinkingLevel?: string;
  cwd?: string;
  isStreaming: boolean;
  sessionStats?: SessionStats;
  stderrLines: string[];
  exitCode?: number | null;
  shadow?: ShadowIdentity;
  sessionFile?: string;
  sessionId?: string;
  sessions: SessionRecord[];
}

// A session record — one conversation
export interface SessionRecord {
  sessionFile: string;
  sessionId?: string;
  model?: string;
  provider?: string;
  startedAt: string;
  messageCount?: number;
}

// Serializable agent config for persistence
export interface SavedAgent {
  id: string;
  name: string;
  provider?: string;
  model?: string;
  thinkingLevel?: string;
  cwd?: string;
  shadow?: ShadowIdentity;
  activeSession?: SessionRecord;
  sessions: SessionRecord[];
}

// Agent spawn config
export interface AgentConfig {
  provider?: string;
  model?: string;
  thinkingLevel?: string;
  cwd?: string;
  extensions?: string[];
  shadow?: ShadowIdentity;
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
  id: string;
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

// Pi RPC events
export type PiEvent =
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
  | {
      type: "response";
      id?: string;
      command: string;
      success: boolean;
      data?: any;
      error?: string;
    }
  | { type: "queue_update"; steering: string[]; followUp: string[] }
  | { type: "compaction_start"; reason: string }
  | { type: "compaction_end"; reason: string; aborted: boolean }
  | { type: "auto_retry_start"; attempt: number }
  | { type: "auto_retry_end"; attempt: number }
  | {
      type: "extension_ui_request";
      id: string;
      method: string;
      [key: string]: any;
    };

// Council — parallel prompt to multiple shadows
export interface CouncilSession {
  id: string;
  prompt: string;
  agentIds: string[];
  responses: Map<string, CouncilResponse>;
  selectedAgentId?: string;
  status: "streaming" | "voting" | "decided";
  timestamp: number;
}

export interface CouncilResponse {
  agentId: string;
  shadowName: string;
  shadowGrade?: ShadowGrade;
  content: ContentBlock[];
  isStreaming: boolean;
  usage?: Usage;
  model?: string;
  votes: number;
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
