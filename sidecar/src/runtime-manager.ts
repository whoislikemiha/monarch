/**
 * Manages multiple AgentSession instances in a single Node process.
 * Each Monarch agent gets one in-memory Pi SDK session.
 */

import {
	createAgentSession,
	DefaultResourceLoader,
	SessionManager,
	type AgentSession,
	type AgentSessionEventListener,
} from "@mariozechner/pi-coding-agent";
import type { Api, Model, ThinkingLevel } from "@mariozechner/pi-ai";
import { buildSystemPrompt } from "./shadow-oath.js";
import { createUIBridge, type EmitFn, type UIResolvers } from "./ui-bridge.js";
import type { CreateSessionCommand, LoadSessionCommand } from "./protocol.js";

interface ManagedSession {
	session: AgentSession;
	unsubscribe: () => void;
	uiResolvers: UIResolvers;
	shadow?: CreateSessionCommand["shadow"];
	cwd: string;
	projectInstructions?: string | null;
	/** Live system prompt. Mutated by setCustomPrompt; the loader override closes over this ref. */
	promptRef: { current: string };
}

const OPENROUTER_BASE_URL = "https://openrouter.ai/api/v1";
const LMSTUDIO_DEFAULT_BASE_URL = "http://127.0.0.1:1234/v1";
const LMSTUDIO_DEFAULT_CONTEXT_WINDOW = 32000;
const EMPTY_USAGE = {
	input: 0,
	output: 0,
	cacheRead: 0,
	cacheWrite: 0,
	totalTokens: 0,
	cost: {
		input: 0,
		output: 0,
		cacheRead: 0,
		cacheWrite: 0,
		total: 0,
	},
} as const;

function tryParseStoredContent(content: string): unknown {
	const trimmed = content.trim();
	if (!trimmed) return content;

	const looksSerialized =
		trimmed.startsWith("[") ||
		trimmed.startsWith("{") ||
		trimmed.startsWith("\"");
	if (!looksSerialized) return content;

	try {
		return JSON.parse(content);
	} catch {
		return content;
	}
}

function normalizeStoredUserContent(content: string): string | Array<Record<string, unknown>> {
	const parsed = tryParseStoredContent(content);
	if (typeof parsed === "string" || Array.isArray(parsed)) {
		return parsed as string | Array<Record<string, unknown>>;
	}
	return String(parsed ?? "");
}

function normalizeStoredAssistantContent(content: string): Array<Record<string, unknown>> {
	const parsed = tryParseStoredContent(content);
	if (Array.isArray(parsed)) {
		return parsed as Array<Record<string, unknown>>;
	}
	if (typeof parsed === "string") {
		return [{ type: "text", text: parsed }];
	}
	return [{ type: "text", text: JSON.stringify(parsed) }];
}

function buildDynamicModel(
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

function lmstudioBaseUrl(): string {
	return process.env.LMSTUDIO_BASE_URL || LMSTUDIO_DEFAULT_BASE_URL;
}

/**
 * LM Studio's OpenAI-compatible server ignores the API key, but pi-ai's
 * openai-completions adapter requires one to be non-empty. Register the
 * provider with a dummy key so authentication resolution succeeds.
 */
function ensureLmStudioProviderRegistered(session: AgentSession): void {
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

function resolveModel(
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

export class RuntimeManager {
	private sessions = new Map<string, ManagedSession>();
	private emit: EmitFn;

	constructor(emit: EmitFn) {
		this.emit = emit;
	}

	async createSession(cmd: CreateSessionCommand): Promise<void> {
		if (this.sessions.has(cmd.agentId)) {
			await this.destroySession(cmd.agentId);
		}

		// Monarch owns the system prompt. We feed it through the resource loader's
		// systemPromptOverride so Pi's _baseSystemPrompt IS our prompt from the first
		// byte — survives every tool/extension rebuild and any before_agent_start reset.
		const initialPrompt =
			cmd.customPrompt?.trim() ||
			(cmd.shadow
				? buildSystemPrompt(cmd.shadow, cmd.cwd, cmd.projectInstructions)
				: cmd.projectInstructions?.trim() || "");
		const promptRef = { current: initialPrompt };

		const resourceLoader = new DefaultResourceLoader({
			cwd: cmd.cwd,
			systemPromptOverride: () => promptRef.current,
			noExtensions: true,
			noSkills: true,
			noPromptTemplates: true,
			noThemes: true,
		});
		// Required: DefaultResourceLoader only materializes systemPromptOverride
		// (and extension factories) inside reload(). createAgentSession skips its
		// own reload when a loader is provided.
		await resourceLoader.reload();

		// Create session without model first, then set model via registry
		// This handles arbitrary provider/model combos (including OpenRouter)
		const { session } = await createAgentSession({
			cwd: cmd.cwd,
			thinkingLevel: (cmd.thinkingLevel || "medium") as ThinkingLevel,
			sessionManager: SessionManager.inMemory(cmd.cwd),
			resourceLoader,
		});

		if (cmd.provider === "lmstudio") {
			ensureLmStudioProviderRegistered(session);
		}

		// Resolve model from registry (supports known + custom models)
		const model = resolveModel(session, cmd.provider, cmd.model, cmd.contextWindow);
		if (model) {
			try {
				await session.setModel(model);
			} catch (err) {
				// Model set failed (e.g., no API key) — continue without model, surface error
				this.emit({
					type: "error",
					agentId: cmd.agentId,
					error: `Model setup warning: ${err instanceof Error ? err.message : String(err)}`,
				});
			}
		} else {
			this.emit({
				type: "error",
				agentId: cmd.agentId,
				error: `Model not found in registry: ${cmd.provider}/${cmd.model}`,
			});
		}

		const uiResolvers: UIResolvers = new Map();
		const uiBridge = createUIBridge(cmd.agentId, this.emit, uiResolvers);

		await session.bindExtensions({
			uiContext: uiBridge,
			commandContextActions: {
				waitForIdle: () => session.agent.waitForIdle(),
				newSession: async () => {
					session.sessionManager.newSession();
					return { cancelled: false };
				},
				fork: async (_entryId: string) => {
					// Fork not supported in Monarch (sessions are in-memory)
					return { cancelled: true };
				},
				navigateTree: async () => {
					return { cancelled: true };
				},
				switchSession: async () => {
					return { cancelled: true };
				},
				reload: async () => {},
			},
			onError: (err) => {
				this.emit({
					type: "error",
					agentId: cmd.agentId,
					error: `Extension error [${err.event}]: ${err.error}`,
				});
			},
		});

		const agentId = cmd.agentId;
		const emit = this.emit;

		const listener: AgentSessionEventListener = (event) => {
			emit({
				type: "event",
				agentId,
				event: event as unknown as Record<string, unknown>,
			});
		};

		const unsubscribe = session.subscribe(listener);

		this.sessions.set(cmd.agentId, {
			session,
			unsubscribe,
			uiResolvers,
			shadow: cmd.shadow,
			cwd: cmd.cwd,
			projectInstructions: cmd.projectInstructions,
			promptRef,
		});

		this.emit({
			type: "session_ready",
			agentId: cmd.agentId,
			contextWindow: model?.contextWindow,
		});
	}

	async destroySession(agentId: string): Promise<void> {
		const managed = this.sessions.get(agentId);
		if (!managed) return;

		managed.unsubscribe();
		managed.session.dispose();
		managed.uiResolvers.clear();
		this.sessions.delete(agentId);

		this.emit({ type: "session_destroyed", agentId });
	}

	async prompt(agentId: string, message: string): Promise<void> {
		const managed = this.getSession(agentId);
		if (!managed) return;

		try {
			if (managed.session.isStreaming) {
				await managed.session.followUp(message);
			} else {
				await managed.session.prompt(message);
			}
		} catch (err) {
			this.emit({
				type: "error",
				agentId,
				error: `Prompt error: ${err instanceof Error ? err.message : String(err)}`,
			});
		}
	}

	async abort(agentId: string): Promise<void> {
		const managed = this.getSession(agentId);
		if (!managed) return;
		await managed.session.abort();
	}

	async setModel(
		agentId: string,
		provider: string,
		modelId: string,
		contextWindow?: number | null,
	): Promise<void> {
		const managed = this.getSession(agentId);
		if (!managed) return;

		if (provider === "lmstudio") {
			ensureLmStudioProviderRegistered(managed.session);
		}

		const model = resolveModel(managed.session, provider, modelId, contextWindow);
		if (!model) {
			this.emit({
				type: "error",
				agentId,
				error: `Model not found: ${provider}/${modelId}`,
			});
			return;
		}
		await managed.session.setModel(model);
	}

	setThinkingLevel(agentId: string, level: string): void {
		const managed = this.getSession(agentId);
		if (!managed) return;
		managed.session.setThinkingLevel(level as ThinkingLevel);
	}

	newSession(agentId: string): void {
		const managed = this.getSession(agentId);
		if (!managed) return;
		// Reset both the session manager (file/memory) and the agent's live message state
		managed.session.sessionManager.newSession();
		managed.session.agent.state.messages = [];
	}

	async compact(agentId: string): Promise<void> {
		const managed = this.getSession(agentId);
		if (!managed) return;
		await managed.session.compact();
	}

	/**
	 * Load messages from SQLite into the agent's conversation context.
	 * This replays past messages so the LLM has conversational continuity.
	 */
	loadSession(agentId: string, messages: LoadSessionCommand["messages"]): void {
		const managed = this.getSession(agentId);
		if (!managed) return;

		const agentMessages: Array<Record<string, unknown>> = [];

		for (const msg of messages) {
			if (msg.role === "user") {
				agentMessages.push({
					role: "user",
					content: normalizeStoredUserContent(msg.content),
					timestamp: Date.now(),
				});
			} else if (msg.role === "assistant") {
				agentMessages.push({
					role: "assistant",
					content: normalizeStoredAssistantContent(msg.content),
					model: msg.model,
					usage: { ...EMPTY_USAGE, cost: { ...EMPTY_USAGE.cost } },
					stopReason: "stop",
					timestamp: Date.now(),
				});
			} else if (msg.role === "toolResult") {
				// Reconstruct ToolResultMessage from DB content
				try {
					const parsed = JSON.parse(msg.content);
					agentMessages.push({
						role: "toolResult",
						toolCallId: parsed.toolCallId || "",
						toolName: parsed.toolName || "unknown",
						content: typeof parsed.result === "string"
							? [{ type: "text", text: parsed.result }]
							: [{ type: "text", text: JSON.stringify(parsed.result) }],
						isError: parsed.isError || false,
						timestamp: Date.now(),
					});
				} catch {
					// Skip malformed tool results
				}
			}
		}

		if (agentMessages.length > 0) {
			// Clear existing state first, then inject
			managed.session.agent.state.messages = agentMessages as any;

			// Also persist to the in-memory session manager so compaction works
			for (const msg of agentMessages) {
				managed.session.sessionManager.appendMessage(msg as any);
			}
		}
	}

	handleUIResponse(
		agentId: string,
		requestId: string,
		value: Record<string, unknown>,
	): void {
		const managed = this.sessions.get(agentId);
		if (!managed) return;

		const resolver = managed.uiResolvers.get(requestId);
		if (resolver) {
			resolver(value);
		}
	}

	setCustomPrompt(
		agentId: string,
		prompt?: string | null,
		projectInstructions?: string | null,
	): void {
		const managed = this.sessions.get(agentId);
		if (!managed) return;

		// Update stored project instructions if provided
		if (projectInstructions !== undefined) {
			managed.projectInstructions = projectInstructions;
		}

		const next =
			prompt?.trim() ||
			(managed.shadow
				? buildSystemPrompt(managed.shadow, managed.cwd, managed.projectInstructions)
				: managed.projectInstructions?.trim() || "");
		if (!next) return;

		// Update the ref so any future _rebuildSystemPrompt (tool changes, etc.)
		// pulls the new prompt from the loader override.
		managed.promptRef.current = next;
		// Also patch the live state so the current/next turn uses the new prompt
		// without waiting for a rebuild.
		managed.session.agent.state.systemPrompt = next;
	}

	async disposeAll(): Promise<void> {
		const ids = [...this.sessions.keys()];
		for (const id of ids) {
			await this.destroySession(id);
		}
	}

	private getSession(agentId: string): ManagedSession | undefined {
		const managed = this.sessions.get(agentId);
		if (!managed) {
			this.emit({
				type: "error",
				agentId,
				error: "Session not found",
			});
		}
		return managed;
	}
}
