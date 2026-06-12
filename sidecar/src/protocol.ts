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

/**
 * MON-128 (P3): which organ a session-addressed command targets / a
 * session-tagged event came from. Absent on the wire means "executor" —
 * the only role that existed pre-P3.
 */
export type SessionRole = "executor" | "chat";

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
  sessionRole?: SessionRole;
}

export interface DestroySessionCommand {
  type: "destroy_session";
  agentId: string;
  /** Absent on destroy = tear down every organ of the agent (kill path). */
  sessionRole?: SessionRole;
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
  sessionRole?: SessionRole;
}

export interface AbortCommand {
  type: "abort";
  agentId: string;
  sessionRole?: SessionRole;
}

export interface SetModelCommand {
  type: "set_model";
  agentId: string;
  provider: string;
  modelId: string;
  /** User-supplied context window (tokens). Currently only honoured for lmstudio. */
  contextWindow?: number | null;
  sessionRole?: SessionRole;
}

export interface SetThinkingLevelCommand {
  type: "set_thinking_level";
  agentId: string;
  level: string;
  sessionRole?: SessionRole;
}

export interface NewSessionCommand {
  type: "new_session";
  agentId: string;
  sessionRole?: SessionRole;
}

export interface CompactCommand {
  type: "compact";
  agentId: string;
  sessionRole?: SessionRole;
}

export interface LoadSessionCommand {
  type: "load_session";
  agentId: string;
  messages: Array<{
    role: "user" | "assistant" | "toolResult";
    content: string;
    model?: string;
  }>;
  sessionRole?: SessionRole;
}

export interface ExtensionUIResponseCommand {
  type: "extension_ui_response";
  agentId: string;
  requestId: string;
  value: Record<string, unknown>;
  sessionRole?: SessionRole;
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
  sessionRole?: SessionRole;
}

// MON-100: Keeper run config + command. Rust resolves provider/model from
// `~/.config/monarch/memory.toml` and ships the system prompt per call so the
// sidecar stays stateless WRT Keeper config (same shape as ClassifierInvocation).
export interface KeeperConfig {
  provider: string;
  model: string;
  systemPrompt: string;
}

export interface KeeperRunCommand {
  type: "keeper_run";
  agentId: string;
  /** Provenance row id (`memory_keeper_runs.id`); echoed in the result. */
  runId: number;
  /** `continuous` or `objective_close`; reserved for future model/prompt branching. */
  trigger: string;
  /** Textual rendering of recent messages + last summary + related memories. */
  slice: string;
  config: KeeperConfig;
}

export interface MemoryRow {
  id: number;
  agentId?: string | null;
  scope: string;
  projectId?: string | null;
  parentId?: number | null;
  layer: string;
  kind?: string | null;
  title: string;
  summary: string;
  content?: string | null;
  manualOverride: boolean;
  sourceObjectiveId?: string | null;
  sourceSessionId?: string | null;
  sourceEvents?: string | null;
  fileRefs?: string | null;
  embeddingModelId?: string | null;
  supersedesId?: number | null;
  archivedAt?: string | null;
  createdAt: string;
  lastAccessedAt?: string | null;
  accessCount: number;
}

export interface MemorySearchResult {
  memory: MemoryRow;
  source: "fts" | "vector" | "hybrid" | string;
  ftsRank?: number | null;
  vectorRank?: number | null;
}

export interface MemorySearchResponseCommand {
  type: "memory_search_response";
  agentId: string;
  requestId: string;
  results: MemorySearchResult[];
  error?: string | null;
}

/** MON-128 (P3): captain-side executor control — same machinery the chat
 * organ's pause/resume/stop tools use, addressable from Rust/UI. */
export interface ExecutorControlCommand {
  type: "executor_control";
  agentId: string;
  action: "pause" | "resume" | "stop";
  reason?: string | null;
}

/** MON-128: Rust's answer to a recall_actions_request (chat tool bridge). */
export interface RecallActionsResponseCommand {
  type: "recall_actions_response";
  agentId: string;
  requestId: string;
  /** Pre-formatted text block (working memory + recent events). */
  payload?: string | null;
  error?: string | null;
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
  | SetCustomPromptCommand
  | KeeperRunCommand
  | MemorySearchResponseCommand
  | ExecutorControlCommand
  | RecallActionsResponseCommand;

// ── Events (Sidecar → Rust via stdout) ──

export interface SessionReadyEvent {
  type: "session_ready";
  agentId: string;
  contextWindow?: number;
  sessionRole?: SessionRole;
}

export interface SessionDestroyedEvent {
  type: "session_destroyed";
  agentId: string;
  sessionRole?: SessionRole;
}

export interface AgentEventEnvelope {
  type: "event";
  agentId: string;
  event: Record<string, unknown>;
  sessionRole?: SessionRole;
}

export interface ExtensionUIRequestEvent {
  type: "extension_ui_request";
  agentId: string;
  requestId: string;
  method: string;
  sessionRole?: SessionRole;

  [key: string]: unknown;
}

export interface SidecarErrorEvent {
  type: "error";
  agentId: string;
  error: string;
  sessionRole?: SessionRole;
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

// MON-100: atomic claim shape produced by the Keeper. `kind` is open-string
// on the wire (Rust persists it verbatim) but the Keeper prompt restricts it
// to: fact | decision | constraint | convention | preference | correction |
// landmark.
export interface AtomicClaim {
  title: string;
  summary: string;
  content: string;
  kind?: string;
}

// MON-100: Keeper result. Emitted once per `keeper_run`. On success, `claims`
// + `compactionSummary` are populated. On failure (timeout, provider crash,
// JSON parse error) `error` is set and the rest may be null. The sidecar
// rewrites Pi's `state.messages` with a synthesized scaffold inline on
// success — Rust only handles persistence and live-state reset.
export interface KeeperResultEvent {
  type: "keeper_result";
  agentId: string;
  runId: number;
  claims?: AtomicClaim[];
  compactionSummary?: string;
  model?: string;
  tokensIn?: number;
  tokensOut?: number;
  latencyMs?: number;
  error?: string;
}

/** MON-128 (P3): the chat organ requested a handoff — Rust builds the
 * verbatim conversation slice since the last watermark and injects it into
 * the executor (prompt when idle, followUp when streaming). */
export interface ChatHandoffRequestEvent {
  type: "chat_handoff_request";
  agentId: string;
}

/** MON-128: chat tool bridge — ask Rust for working memory + recent
 * timeline events. Rust answers with recall_actions_response. */
export interface RecallActionsRequestEvent {
  type: "recall_actions_request";
  agentId: string;
  requestId: string;
  limit?: number | null;
}

export interface MemorySearchRequestEvent {
  type: "memory_search_request";
  agentId: string;
  requestId: string;
  query: string;
  topK?: number | null;
}

export interface MemorySuggestionInnerEvent {
  type: "memory_suggestion";
  title: string;
  summary: string;
  content: string;
}

export type SidecarEvent =
  | SessionReadyEvent
  | SessionDestroyedEvent
  | AgentEventEnvelope
  | ExtensionUIRequestEvent
  | SidecarErrorEvent
  | ClassificationEvent
  | KeeperResultEvent
  | MemorySearchRequestEvent
  | ChatHandoffRequestEvent
  | RecallActionsRequestEvent;
