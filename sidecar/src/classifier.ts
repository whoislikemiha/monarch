/**
 * MON-82 — Slice 1: per-turn user-prompt classifier.
 *
 * Runs alongside the Pi turn (fire-and-forget from the caller's perspective)
 * and labels the user's message as `chitchat | simple | decomposable |
 * delegate`. The result is surfaced to the UI via a sidecar `classification`
 * event and persisted by Rust; later slices consume the label to decide
 * whether to invoke the Architect, Steward, etc. Slice 1 itself does not
 * drive any downstream behaviour — this is pure visibility + data.
 *
 * Design notes:
 *
 * - Always awaited with a timeout (default 3s). On timeout / provider crash
 *   the caller receives an "error" result rather than a throw, so the Pi
 *   turn is never blocked by classification.
 * - Provider selection: `primary` first, `fallback` if primary throws.
 *   Failures that trip the fallback are transparent to the UI — the final
 *   result reflects whichever resolved.
 * - Model resolution piggybacks on the AgentSession's `modelRegistry` so
 *   Anthropic credentials (already resolved by pi-ai on session creation)
 *   are reused without plumbing.
 * - Prompt is exported so the settings UI can show it read-only. Editing
 *   is out of scope for Slice 1.
 */

import { complete, type Api, type Context, type Model } from "@mariozechner/pi-ai";
import type { AgentSession } from "@mariozechner/pi-coding-agent";

export type ComplexityLabel =
  | "chitchat"
  | "simple"
  | "decomposable"
  | "delegate";

export interface ClassifierProvider {
  /** `anthropic` | `lmstudio` for Slice 1. */
  provider: string;
  /** Model id as pi-ai expects it (e.g. `claude-haiku-4-5`). */
  model: string;
}

export interface ClassifierConfig {
  enabled: boolean;
  primary: ClassifierProvider;
  fallback?: ClassifierProvider | null;
  timeoutMs: number;
  /** System prompt. Shipped from Rust so the settings UI can display it. */
  systemPrompt: string;
}

export interface ClassificationSuccess {
  complexity: ComplexityLabel;
  confidence: number;
  rationale: string;
  model: string;
  tokensIn?: number;
  tokensOut?: number;
  latencyMs: number;
}

export interface ClassificationFailure {
  error: string;
  model?: string;
  latencyMs: number;
}

export type ClassificationResult = ClassificationSuccess | ClassificationFailure;

export const DEFAULT_CLASSIFIER_SYSTEM_PROMPT = `You are a complexity classifier for incoming user prompts in a multi-agent coding assistant.

Label the user's message with exactly one of:

- chitchat: greetings, social/small-talk, or meta-questions that need no task execution
- simple: a direct request solvable in a single focused turn (e.g. a one-line fix, a factual question, a small rename)
- decomposable: work that benefits from an explicit plan — several files, several decisions, or sequenced steps
- delegate: work that benefits from parallel subtasks or exploration across unrelated areas, where multiple agents should run simultaneously

Bias toward escalation on ambiguity — prefer 'decomposable' over 'simple' when it's a close call. A misclassified simple prompt is cheap; a missed decomposable task is expensive.

Output ONLY a single JSON object, no prose, no code fences:

{"complexity": "<label>", "confidence": <number between 0 and 1>, "rationale": "<one short sentence>"}`;

function extractUserText(
  message: string | Array<{ type: string; text?: string }>,
): string {
  if (typeof message === "string") return message;
  return message
    .filter((p): p is { type: "text"; text: string } => p.type === "text")
    .map((p) => p.text)
    .join("\n");
}

function resolveProviderModel(
  session: AgentSession,
  provider: string,
  modelId: string,
): Model<Api> | undefined {
  return session.modelRegistry.find(provider, modelId) ?? undefined;
}

function parseClassifierJson(raw: string): {
  complexity: ComplexityLabel;
  confidence: number;
  rationale: string;
} {
  // Models sometimes wrap JSON in code fences despite instructions; strip
  // obvious wrappers before parsing.
  const trimmed = raw
    .trim()
    .replace(/^```(?:json)?\s*/i, "")
    .replace(/```$/i, "")
    .trim();
  const parsed = JSON.parse(trimmed) as unknown;
  if (!parsed || typeof parsed !== "object") {
    throw new Error("classifier output was not an object");
  }
  const obj = parsed as Record<string, unknown>;
  const complexity = obj.complexity;
  if (
    complexity !== "chitchat" &&
    complexity !== "simple" &&
    complexity !== "decomposable" &&
    complexity !== "delegate"
  ) {
    throw new Error(`classifier output complexity invalid: ${String(complexity)}`);
  }
  const confidence = typeof obj.confidence === "number" ? obj.confidence : 0;
  const rationale = typeof obj.rationale === "string" ? obj.rationale : "";
  return { complexity, confidence, rationale };
}

async function runProvider(
  session: AgentSession,
  p: ClassifierProvider,
  systemPrompt: string,
  userText: string,
  signal: AbortSignal,
): Promise<ClassificationSuccess> {
  const model = resolveProviderModel(session, p.provider, p.model);
  if (!model) {
    throw new Error(
      `classifier model ${p.provider}/${p.model} not registered — check credentials`,
    );
  }
  // pi-ai's `complete()` does NOT thread credentials from the session's
  // AuthStorage on its own — it falls back to env vars and ultimately to
  // an empty key. Resolve explicitly via the session's modelRegistry so
  // ~/.pi/agent/auth.json is honoured, matching how Pi's own
  // session.prompt() reaches the provider.
  const auth = await session.modelRegistry.getApiKeyAndHeaders(model);
  if (!auth.ok) {
    throw new Error(`auth for ${p.provider}/${p.model}: ${auth.error}`);
  }
  const started = Date.now();
  const ctx: Context = {
    systemPrompt,
    messages: [
      {
        role: "user",
        content: userText,
        timestamp: Date.now(),
      },
    ],
  };
  const result = await complete(model, ctx, {
    signal,
    apiKey: auth.apiKey,
    headers: auth.headers,
  });
  const latencyMs = Date.now() - started;
  // `complete` returns an AssistantMessage; shape:
  // { role: "assistant", content: ContentPart[] | string, usage?, ... }
  const text =
    typeof result.content === "string"
      ? result.content
      : result.content
          .filter(
            (c: unknown): c is { type: "text"; text: string } =>
              typeof c === "object" &&
              c !== null &&
              (c as { type?: unknown }).type === "text" &&
              typeof (c as { text?: unknown }).text === "string",
          )
          .map((c) => c.text)
          .join("");
  const parsed = parseClassifierJson(text);
  const usage = result.usage as
    | { input?: number; output?: number }
    | undefined;
  return {
    ...parsed,
    model: `${p.provider}/${p.model}`,
    tokensIn: usage?.input,
    tokensOut: usage?.output,
    latencyMs,
  };
}

/**
 * Classify a user message with timeout + single-provider fallback. Never
 * throws; failure paths return a `ClassificationFailure` so the caller can
 * still emit an event and let the UI render a "failed" pill.
 */
export async function classify(
  session: AgentSession,
  message: string | Array<{ type: string; text?: string }>,
  config: ClassifierConfig,
): Promise<ClassificationResult> {
  const startedAll = Date.now();
  const userText = extractUserText(message);
  if (!userText.trim()) {
    return {
      error: "empty user message",
      latencyMs: Date.now() - startedAll,
    };
  }
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), config.timeoutMs);
  try {
    try {
      return await runProvider(
        session,
        config.primary,
        config.systemPrompt,
        userText,
        controller.signal,
      );
    } catch (primaryErr) {
      if (!config.fallback) {
        return {
          error: `primary ${config.primary.provider}/${config.primary.model}: ${
            primaryErr instanceof Error ? primaryErr.message : String(primaryErr)
          }`,
          model: `${config.primary.provider}/${config.primary.model}`,
          latencyMs: Date.now() - startedAll,
        };
      }
      try {
        return await runProvider(
          session,
          config.fallback,
          config.systemPrompt,
          userText,
          controller.signal,
        );
      } catch (fallbackErr) {
        return {
          error: `both providers failed — primary: ${
            primaryErr instanceof Error ? primaryErr.message : String(primaryErr)
          }; fallback: ${
            fallbackErr instanceof Error
              ? fallbackErr.message
              : String(fallbackErr)
          }`,
          latencyMs: Date.now() - startedAll,
        };
      }
    }
  } finally {
    clearTimeout(timeout);
  }
}
