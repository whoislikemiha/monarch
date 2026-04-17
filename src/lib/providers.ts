// Provider catalogue — source of truth for the four model backends Monarch
// can drive. Shared between `SpawnDialog` (the spawn flow) and any future
// runtime model switcher in `AgentView`.

export interface ProviderOption {
  label: string;
  value: string;
}

export const PROVIDERS: readonly ProviderOption[] = [
  { label: "Anthropic", value: "anthropic" },
  { label: "OpenAI Codex", value: "openai-codex" },
  { label: "OpenRouter", value: "openrouter" },
  { label: "LM Studio", value: "lmstudio" },
] as const;

// Providers whose model lists are fetched over the network and benefit from
// an explicit refresh action. All four providers now discover their model
// lists dynamically (Anthropic + Codex via `/v1/models` using the Pi-stored
// OAuth credential, OpenRouter and LM Studio via their own list endpoints).
export const REFRESHABLE_PROVIDERS: ReadonlySet<string> = new Set([
  "anthropic",
  "openai-codex",
  "openrouter",
  "lmstudio",
]);

