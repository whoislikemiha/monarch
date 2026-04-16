// Provider-aware thinking-level capability table + display mapping.
//
// Pi SDK owns the real clamping (see pi-ai/providers/*.js: `mapThinkingLevelToEffort`,
// `clampReasoning`, `adjustMaxTokensForThinking`). This table mirrors the same
// shape so the UI only shows levels Pi will actually honor — and we can render
// each provider's native label (e.g. "max" on Opus 4.6, "HIGH" on Gemini).
//
// The sidecar validates incoming levels against Pi's typed enum
// (`@mariozechner/pi-agent-core` ThinkingLevel) and Pi clamps silently if a
// model cannot honor the requested level. If this table drifts from Pi,
// symptoms are cosmetic (we show a level Pi silently lowers) — never unsafe.

export const ALL_THINKING_LEVELS = [
  "off",
  "minimal",
  "low",
  "medium",
  "high",
  "xhigh",
] as const;

export type ThinkingLevel = (typeof ALL_THINKING_LEVELS)[number];

export function isThinkingLevel(value: string): value is ThinkingLevel {
  return (ALL_THINKING_LEVELS as readonly string[]).includes(value);
}

interface ModelThinkingProfile {
  supportsThinking: boolean;
  levels: readonly ThinkingLevel[];
  /** Override label per Pi-canonical level. Missing keys fall back to the Pi string. */
  labels?: Partial<Record<ThinkingLevel, string>>;
}

const NO_THINKING: ModelThinkingProfile = {
  supportsThinking: false,
  levels: ["off"],
};

// Anthropic adaptive models (Claude 4.6 family) use effort-level thinking:
// Opus 4.6: low | medium | high | max (xhigh → max).
// Sonnet 4.6: low | medium | high (xhigh clamped to high in Pi).
const ANTHROPIC_OPUS_46: ModelThinkingProfile = {
  supportsThinking: true,
  levels: ["off", "low", "medium", "high", "xhigh"],
  labels: { xhigh: "max" },
};
const ANTHROPIC_SONNET_46: ModelThinkingProfile = {
  supportsThinking: true,
  levels: ["off", "low", "medium", "high"],
};
// Older Anthropic reasoning models use token-budget thinking. Pi maps
// minimal/low/medium/high to budgets; xhigh is not used for non-adaptive.
const ANTHROPIC_NON_ADAPTIVE: ModelThinkingProfile = {
  supportsThinking: true,
  levels: ["off", "minimal", "low", "medium", "high"],
};

const GOOGLE_GEMINI: ModelThinkingProfile = {
  supportsThinking: true,
  levels: ["off", "minimal", "low", "medium", "high"],
  labels: {
    minimal: "MINIMAL",
    low: "LOW",
    medium: "MEDIUM",
    high: "HIGH",
  },
};

// OpenAI gpt-5.1-codex-max, gpt-5.2, gpt-5.2-codex, gpt-5.3, gpt-5.3-codex
// — per pi-agent-core ThinkingLevel doc comment.
const OPENAI_XHIGH_MODELS: readonly string[] = [
  "gpt-5.1-codex-max",
  "gpt-5.2",
  "gpt-5.2-codex",
  "gpt-5.3",
  "gpt-5.3-codex",
];
const OPENAI_XHIGH: ModelThinkingProfile = {
  supportsThinking: true,
  levels: ["off", "minimal", "low", "medium", "high", "xhigh"],
};
const OPENAI_STANDARD: ModelThinkingProfile = {
  supportsThinking: true,
  levels: ["off", "minimal", "low", "medium", "high"],
};

function anthropicProfile(modelId: string): ModelThinkingProfile {
  const id = modelId.toLowerCase();
  if (id.includes("opus-4-6") || id.includes("opus-4.6")) return ANTHROPIC_OPUS_46;
  if (id.includes("sonnet-4-6") || id.includes("sonnet-4.6")) return ANTHROPIC_SONNET_46;
  if (id.includes("sonnet") || id.includes("opus") || id.includes("claude-3-7")) {
    return ANTHROPIC_NON_ADAPTIVE;
  }
  return NO_THINKING;
}

function openaiProfile(modelId: string): ModelThinkingProfile {
  const id = modelId.toLowerCase();
  if (OPENAI_XHIGH_MODELS.some((m) => id.includes(m))) return OPENAI_XHIGH;
  if (id.startsWith("gpt-5") || id.startsWith("o1") || id.startsWith("o3") || id.startsWith("o4")) {
    return OPENAI_STANDARD;
  }
  return NO_THINKING;
}

// OpenRouter proxies many upstreams. If the route prefix names an upstream we
// recognize (e.g. `anthropic/claude-opus-4-6`), use that upstream's profile so
// the UI matches the real provider ladder.
function openRouterProfile(modelId: string): ModelThinkingProfile {
  const id = modelId.toLowerCase();
  const slash = id.indexOf("/");
  if (slash > 0) {
    const upstream = id.slice(0, slash);
    const rest = id.slice(slash + 1);
    if (upstream === "anthropic") return anthropicProfile(rest);
    if (upstream === "google") return GOOGLE_GEMINI;
    if (upstream === "openai") return openaiProfile(rest);
  }
  // Unknown OpenRouter route — fall back to a conservative reasoning ladder.
  return OPENAI_STANDARD;
}

export function thinkingProfile(provider: string, modelId: string): ModelThinkingProfile {
  if (!modelId) return NO_THINKING;
  switch (provider) {
    case "anthropic":
      return anthropicProfile(modelId);
    case "openai-codex":
      return openaiProfile(modelId);
    case "openrouter":
      return openRouterProfile(modelId);
    case "lmstudio":
      return NO_THINKING;
    default:
      return NO_THINKING;
  }
}

export function availableLevels(provider: string, modelId: string): readonly ThinkingLevel[] {
  return thinkingProfile(provider, modelId).levels;
}

export function supportsThinking(provider: string, modelId: string): boolean {
  return thinkingProfile(provider, modelId).supportsThinking;
}

export function displayLabel(provider: string, modelId: string, level: ThinkingLevel): string {
  const profile = thinkingProfile(provider, modelId);
  return profile.labels?.[level] ?? level;
}

/** Clamp a stored/selected level to the nearest supported value for the model. */
export function clampLevel(
  provider: string,
  modelId: string,
  level: string | null | undefined,
): ThinkingLevel {
  const levels = availableLevels(provider, modelId);
  if (level && isThinkingLevel(level) && levels.includes(level)) return level;
  return levels[0] ?? "off";
}
