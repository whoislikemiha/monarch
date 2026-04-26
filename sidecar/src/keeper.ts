/**
 * MON-100 — Slice B: continuous-compaction Keeper worker.
 *
 * Mirrors `classifier.ts`'s structure: one-shot `complete()` against the
 * configured Keeper model, structured-JSON output, never throws. The caller
 * (runtime-manager) emits the result back to Rust as a `keeper_result`
 * event and rewrites Pi's `state.messages` in-place with a synthesized
 * scaffold; this module is purely the LLM round trip.
 *
 * Design notes:
 *
 * - Provider/model/system prompt are shipped per call from Rust (see
 *   `KeeperConfig` in `protocol.ts`). The sidecar stays stateless WRT
 *   Keeper config — `~/.config/monarch/memory.toml` is the source of truth.
 * - Auth resolves through the session's `modelRegistry`, same as the
 *   classifier — without that step, pi-ai's `complete()` falls back to env
 *   vars and ultimately fails for anything other than environment-only auth.
 * - Failure paths return `{ error, ... }` so the caller can still emit a
 *   `keeper_result` event; the Pi turn never blocks on Keeper failure.
 */

import { complete, type Api, type Context, type Model } from "@mariozechner/pi-ai";
import type { AgentSession } from "@mariozechner/pi-coding-agent";
import type { AtomicClaim, KeeperConfig } from "./protocol.js";

export interface KeeperSuccess {
  claims: AtomicClaim[];
  compactionSummary: string;
  model: string;
  tokensIn?: number;
  tokensOut?: number;
  latencyMs: number;
}

export interface KeeperFailure {
  error: string;
  model?: string;
  latencyMs: number;
}

export type KeeperResult = KeeperSuccess | KeeperFailure;

const VALID_KINDS = new Set([
  "fact",
  "decision",
  "constraint",
  "convention",
  "preference",
  "correction",
  "landmark",
]);

function parseKeeperJson(raw: string): {
  claims: AtomicClaim[];
  compactionSummary: string;
} {
  // Strip code fences the model may wrap output in despite instructions.
  const trimmed = raw
    .trim()
    .replace(/^```(?:json)?\s*/i, "")
    .replace(/```$/i, "")
    .trim();
  const parsed = JSON.parse(trimmed) as unknown;
  if (!parsed || typeof parsed !== "object") {
    throw new Error("keeper output was not an object");
  }
  const obj = parsed as Record<string, unknown>;
  const compactionSummary =
    typeof obj.compaction_summary === "string"
      ? obj.compaction_summary
      : typeof obj.compactionSummary === "string"
        ? obj.compactionSummary
        : "";
  const rawClaims = Array.isArray(obj.claims) ? obj.claims : [];
  const claims: AtomicClaim[] = [];
  for (const c of rawClaims) {
    if (!c || typeof c !== "object") continue;
    const claim = c as Record<string, unknown>;
    const title = typeof claim.title === "string" ? claim.title.trim() : "";
    const summary = typeof claim.summary === "string" ? claim.summary.trim() : "";
    const content = typeof claim.content === "string" ? claim.content.trim() : "";
    if (!title || !summary) continue;
    let kind: string | undefined;
    if (typeof claim.kind === "string") {
      const lower = claim.kind.trim().toLowerCase();
      if (VALID_KINDS.has(lower)) kind = lower;
    }
    claims.push({ title, summary, content, kind });
  }
  return { claims, compactionSummary };
}

function resolveProviderModel(
  session: AgentSession,
  provider: string,
  modelId: string,
): Model<Api> | undefined {
  return session.modelRegistry.find(provider, modelId) ?? undefined;
}

/**
 * Run one Keeper distillation pass. Single LLM call against
 * `config.{provider, model}` with `slice` as the user message and
 * `config.systemPrompt` as the system prompt. Returns either a
 * `KeeperSuccess` (claims + summary) or `KeeperFailure`. Never throws.
 */
export async function runKeeper(
  session: AgentSession,
  slice: string,
  config: KeeperConfig,
): Promise<KeeperResult> {
  const startedAll = Date.now();
  if (!slice.trim()) {
    return {
      error: "empty keeper slice",
      latencyMs: Date.now() - startedAll,
    };
  }
  const model = resolveProviderModel(session, config.provider, config.model);
  if (!model) {
    return {
      error: `keeper model ${config.provider}/${config.model} not registered — check credentials`,
      model: `${config.provider}/${config.model}`,
      latencyMs: Date.now() - startedAll,
    };
  }
  // pi-ai's `complete()` does not thread credentials from the session's
  // AuthStorage on its own — resolve via modelRegistry so ~/.pi/agent/auth.json
  // is honoured (same path classifier.ts uses).
  const auth = await session.modelRegistry.getApiKeyAndHeaders(model);
  if (!auth.ok) {
    return {
      error: `auth for ${config.provider}/${config.model}: ${auth.error}`,
      model: `${config.provider}/${config.model}`,
      latencyMs: Date.now() - startedAll,
    };
  }
  const ctx: Context = {
    systemPrompt: config.systemPrompt,
    messages: [
      {
        role: "user",
        content: slice,
        timestamp: Date.now(),
      },
    ],
  };
  try {
    const result = await complete(model, ctx, {
      apiKey: auth.apiKey,
      headers: auth.headers,
    });
    const latencyMs = Date.now() - startedAll;
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
    const parsed = parseKeeperJson(text);
    const usage = result.usage as
      | { input?: number; output?: number }
      | undefined;
    return {
      ...parsed,
      model: `${config.provider}/${config.model}`,
      tokensIn: usage?.input,
      tokensOut: usage?.output,
      latencyMs,
    };
  } catch (err) {
    return {
      error: err instanceof Error ? err.message : String(err),
      model: `${config.provider}/${config.model}`,
      latencyMs: Date.now() - startedAll,
    };
  }
}
