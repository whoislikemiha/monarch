/**
 * Bundled provider-logo avatars (static/avatars/providers/*.svg).
 * An agent with no explicit avatar (`avatar_type` NULL) automatically renders
 * its provider's logo; the monogram remains only as a last-resort fallback
 * for unknown providers.
 */
const PROVIDER_LOGOS: ReadonlySet<string> = new Set([
  "anthropic",
  "openai-codex",
  "openrouter",
  "lmstudio",
]);

export function providerLogoPath(provider?: string | null): string | undefined {
  if (!provider || !PROVIDER_LOGOS.has(provider)) return undefined;
  return `/avatars/providers/${provider}.svg`;
}

export const PROVIDER_LOGO_PRESETS: ReadonlyArray<{ label: string; path: string }> = [
  { label: "Claude", path: "/avatars/providers/anthropic.svg" },
  { label: "OpenAI", path: "/avatars/providers/openai-codex.svg" },
  { label: "OpenRouter", path: "/avatars/providers/openrouter.svg" },
  { label: "LM Studio", path: "/avatars/providers/lmstudio.svg" },
];
