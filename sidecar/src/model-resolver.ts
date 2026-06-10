import type { ThinkingLevel } from "@mariozechner/pi-agent-core";
import { type Api, type Model } from "@mariozechner/pi-ai";
import type { AgentSession } from "@mariozechner/pi-coding-agent";

const OPENROUTER_BASE_URL = "https://openrouter.ai/api/v1";
const LMSTUDIO_DEFAULT_BASE_URL = "http://127.0.0.1:1234/v1";
const LMSTUDIO_DEFAULT_CONTEXT_WINDOW = 32000;

export function buildDynamicModel(
	provider: string,
	modelId: string,
	contextWindowOverride?: number | null,
): Model<Api> | undefined {
	if (provider === "openrouter") {
		return {
			id: modelId,
			name: modelId,
			api: "openai-completions",
			provider,
			baseUrl: OPENROUTER_BASE_URL,
			reasoning: false,
			input: ["text"],
			cost: {
				input: 0,
				output: 0,
				cacheRead: 0,
				cacheWrite: 0,
			},
			contextWindow: 128000,
			maxTokens: 16384,
		};
	}

	if (provider === "lmstudio") {
		const contextWindow =
			contextWindowOverride != null && contextWindowOverride > 0
				? contextWindowOverride
				: LMSTUDIO_DEFAULT_CONTEXT_WINDOW;
		return {
			id: modelId,
			name: modelId,
			api: "openai-completions",
			provider,
			baseUrl: lmstudioBaseUrl(),
			reasoning: false,
			input: ["text"],
			cost: {
				input: 0,
				output: 0,
				cacheRead: 0,
				cacheWrite: 0,
			},
			contextWindow,
			maxTokens: 4096,
		};
	}

	return undefined;
}

export function lmstudioBaseUrl(): string {
	return process.env.LMSTUDIO_BASE_URL || LMSTUDIO_DEFAULT_BASE_URL;
}

const VALID_THINKING_LEVELS: ReadonlySet<string> = new Set([
	"off",
	"minimal",
	"low",
	"medium",
	"high",
	"xhigh",
]);

export function isValidThinkingLevel(level: string): level is ThinkingLevel {
	return VALID_THINKING_LEVELS.has(level);
}

/**
 * LM Studio's OpenAI-compatible server ignores the API key, but pi-ai's
 * openai-completions adapter requires one to be non-empty. Register the
 * provider with a dummy key so authentication resolution succeeds.
 */
export function ensureLmStudioProviderRegistered(session: AgentSession): void {
	try {
		session.modelRegistry.registerProvider("lmstudio", {
			baseUrl: lmstudioBaseUrl(),
			apiKey: "lm-studio",
			api: "openai-completions",
		} as Parameters<typeof session.modelRegistry.registerProvider>[1]);
	} catch {
		// Already registered or validation noop — safe to ignore.
	}
}

export function resolveModel(
	session: AgentSession,
	provider: string,
	modelId: string,
	contextWindowOverride?: number | null,
): Model<Api> | undefined {
	// For lmstudio, always build a dynamic model so a user-supplied context window
	// takes effect even if a registry entry exists.
	if (provider === "lmstudio") {
		return buildDynamicModel(provider, modelId, contextWindowOverride);
	}
	return (
		session.modelRegistry.find(provider, modelId) ??
		buildDynamicModel(provider, modelId, contextWindowOverride)
	);
}
