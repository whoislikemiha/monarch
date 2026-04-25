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
  customPrompt?: string | null;
  projectInstructions?: string | null;
  /** User-supplied context window (tokens). Currently only honoured for lmstudio. */
  contextWindow?: number | null;
  /** MON-98: L1a captain identity payload. Absent = no captain section. */
  captainIdentityPayload?: string | null;
  /** MON-98: L1b shadow identity payload. Absent = no shadow identity section. */
  shadowIdentityPayload?: string | null;
}

export interface DestroySessionCommand {
  type: "destroy_session";
  agentId: string;
}

export type PromptContentPart =
  | { type: "text"; text: string }
  | { type: "image"; data: string; mimeType: string };

// MON-82: Rust ships the resolved classifier config on each `prompt`. The
// sidecar is stateless WRT classifier configuration — settings live in
// ~/.config/monarch/classifier.toml and Rust mints the per-turn
// `classificationId` so the user-message row can be linked inline during
// persistence.
export interface ClassifierInvocation {
  id: string;
  config: {
    enabled: boolean;
    primary: { provider: string; model: string };
    fallback?: { provider: string; model: string } | null;
    timeoutMs: number;
    systemPrompt: string;
  };
}

export interface PromptCommand {
  type: "prompt";
  agentId: string;
  message: string | PromptContentPart[];
  classifier?: ClassifierInvocation | null;
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
  /** User-supplied context window (tokens). Currently only honoured for lmstudio. */
  contextWindow?: number | null;
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

export interface SetCustomPromptCommand {
  type: "set_custom_prompt";
  agentId: string;
  prompt?: string | null;
  projectInstructions?: string | null;
  /** MON-98: When present, replaces the stored captain identity payload and rebuilds the prompt. */
  captainIdentityPayload?: string | null;
  /** MON-98: When present, replaces the stored shadow identity payload and rebuilds the prompt. */
  shadowIdentityPayload?: string | null;
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
  | ExtensionUIResponseCommand
  | SetCustomPromptCommand;

// ── Events (Sidecar → Rust via stdout) ──

export interface SessionReadyEvent {
  type: "session_ready";
  agentId: string;
  contextWindow?: number;
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

// MON-82: Classifier output. Emitted once per user turn, independently of
// the Pi turn (the classifier races against the Pi session and never
// blocks it). `id` is a UUID minted sidecar-side so the frontend can pair
// a pending pill with the resolved row and Rust can backfill the
// `classifications.message_id` FK once the user message row lands.
//
// `complexity`, `confidence`, `rationale`, `model`, `tokensIn`, `tokensOut`,
// and `latencyMs` are populated on success; on failure, `error` is set
// (provider crash, timeout, malformed JSON) and the rest may be null.
export type ComplexityLabel =
  | "chitchat"
  | "simple"
  | "decomposable"
  | "delegate";

export interface ClassificationEvent {
  type: "classification";
  agentId: string;
  id: string;
  complexity?: ComplexityLabel;
  confidence?: number;
  rationale?: string;
  model?: string;
  tokensIn?: number;
  tokensOut?: number;
  latencyMs?: number;
  error?: string;
}

export type SidecarEvent =
  | SessionReadyEvent
  | SessionDestroyedEvent
  | AgentEventEnvelope
  | ExtensionUIRequestEvent
  | SidecarErrorEvent
  | ClassificationEvent;
