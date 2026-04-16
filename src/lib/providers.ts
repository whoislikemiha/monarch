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
// an explicit refresh action (vs. statically known model catalogues).
export const REFRESHABLE_PROVIDERS: ReadonlySet<string> = new Set([
  "openrouter",
  "lmstudio",
]);

