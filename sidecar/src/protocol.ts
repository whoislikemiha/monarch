/**
 * IPC protocol between Rust backend and Node sidecar.
 * All messages are JSONL (one JSON object per line) over stdin/stdout.
 */

// ── Commands (Rust → Sidecar via stdin) ──

export interface ShadowConfig {
  name: string;
  title: string;
  grade: string;
  id: string;
}

export interface CreateSessionCommand {
  type: "create_session";
  agentId: string;
  cwd: string;
  provider: string;
  model: string;
  thinkingLevel: string;
  shadow?: ShadowConfig;
}

export interface DestroySessionCommand {
  type: "destroy_session";
  agentId: string;
}

export interface PromptCommand {
  type: "prompt";
  agentId: string;
  message: string;
}

export interface AbortCommand {
  type: "abort";
  agentId: string;
}

export interface SetModelCommand {
  type: "set_model";
  agentId: string;
  provider: string;
  modelId: string;
}

export interface SetThinkingLevelCommand {
  type: "set_thinking_level";
  agentId: string;
  level: string;
}

export interface NewSessionCommand {
  type: "new_session";
  agentId: string;
}

export interface CompactCommand {
  type: "compact";
  agentId: string;
}

export interface LoadSessionCommand {
  type: "load_session";
  agentId: string;
  messages: Array<{
    role: "user" | "assistant" | "toolResult";
    content: string;
    model?: string;
  }>;
}

export interface ExtensionUIResponseCommand {
  type: "extension_ui_response";
  agentId: string;
  requestId: string;
  value: Record<string, unknown>;
}

export type SidecarCommand =
  | CreateSessionCommand
  | DestroySessionCommand
  | PromptCommand
  | AbortCommand
  | SetModelCommand
  | SetThinkingLevelCommand
  | NewSessionCommand
  | CompactCommand
  | LoadSessionCommand
  | ExtensionUIResponseCommand;

// ── Events (Sidecar → Rust via stdout) ──

export interface SessionReadyEvent {
  type: "session_ready";
  agentId: string;
}

export interface SessionDestroyedEvent {
  type: "session_destroyed";
  agentId: string;
}

export interface AgentEventEnvelope {
  type: "event";
  agentId: string;
  event: Record<string, unknown>;
}

export interface ExtensionUIRequestEvent {
  type: "extension_ui_request";
  agentId: string;
  requestId: string;
  method: string;
  [key: string]: unknown;
}

export interface SidecarErrorEvent {
  type: "error";
  agentId: string;
  error: string;
}

export type SidecarEvent =
  | SessionReadyEvent
  | SessionDestroyedEvent
  | AgentEventEnvelope
  | ExtensionUIRequestEvent
  | SidecarErrorEvent;
