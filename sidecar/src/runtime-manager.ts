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
import type { ThinkingLevel } from "@mariozechner/pi-agent-core";
import type { Api, ImageContent, Model, TextContent } from "@mariozechner/pi-ai";
import { buildSystemPrompt } from "./shadow-oath.js";
import { createUIBridge, type EmitFn, type UIResolvers } from "./ui-bridge.js";
import type {
	ClassifierInvocation,
	CreateSessionCommand,
	KeeperRunCommand,
	LoadSessionCommand,
	PromptContentPart,
} from "./protocol.js";
import { classify } from "./classifier.js";
import { runKeeper } from "./keeper.js";

interface PendingKeeperRewrite {
	runId: number;
	summary: string;
	tailAnchor: number;
}

interface ManagedSession {
	session: AgentSession;
	unsubscribe: () => void;
	uiResolvers: UIResolvers;
	shadow?: CreateSessionCommand["shadow"];
	cwd: string;
	projectInstructions?: string | null;
	/** MON-98: Stored captain identity payload (L1a). Updated by setCustomPrompt. */
	captainIdentityPayload?: string | null;
	/** MON-98: Stored shadow identity payload (L1b). Updated by setCustomPrompt. */
	shadowIdentityPayload?: string | null;
	/** Live system prompt. Mutated by setCustomPrompt; the loader override closes over this ref. */
	promptRef: { current: string };
	/**
	 * MON-100: Keeper run that completed mid-streaming-turn. Drained on the
	 * next `agent_end` so we never rewrite `state.messages` while Pi is
	 * mutating it. Clears immediately on apply.
	 */
	pendingKeeperRewrite?: PendingKeeperRewrite | null;
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

const VALID_THINKING_LEVELS: ReadonlySet<string> = new Set([
	"off",
	"minimal",
	"low",
	"medium",
	"high",
	"xhigh",
]);

function isValidThinkingLevel(level: string): level is ThinkingLevel {
	return VALID_THINKING_LEVELS.has(level);
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
	/**
	 * MON-82: per-agent FIFO of classification ids awaiting their paired
	 * user `message_end`. Push on `prompt()` when the classifier is
	 * enabled; pop on the first user-role `message_end` for that agent so
	 * the forwarded event carries `classificationId` and Rust can backfill
	 * `classifications.message_id` inline.
	 */
	private pendingClassifications = new Map<string, string[]>();

	constructor(emit: EmitFn) {
		this.emit = emit;
	}

	private pushPendingClassification(agentId: string, id: string): void {
		const existing = this.pendingClassifications.get(agentId);
		if (existing) {
			existing.push(id);
		} else {
			this.pendingClassifications.set(agentId, [id]);
		}
	}

	private popPendingClassification(agentId: string): string | undefined {
		const existing = this.pendingClassifications.get(agentId);
		if (!existing || existing.length === 0) return undefined;
		const id = existing.shift();
		if (existing.length === 0) this.pendingClassifications.delete(agentId);
		return id;
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
				? buildSystemPrompt(cmd.shadow, cmd.cwd, cmd.projectInstructions, cmd.captainIdentityPayload, cmd.shadowIdentityPayload)
				: cmd.projectInstructions?.trim() || "");
		const promptRef = { current: initialPrompt };

		const resourceLoader = new DefaultResourceLoader({
			cwd: cmd.cwd,
			// `agentDir` is where Pi expects agent-local extensions/skills/themes
			// to live. Monarch disables all of those (`noExtensions`/etc.) and
			// owns the system prompt directly, so the value is functionally
			// unused — pass `cmd.cwd` as a safe valid path.
			agentDir: cmd.cwd,
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
		const initialLevel: ThinkingLevel =
			cmd.thinkingLevel && isValidThinkingLevel(cmd.thinkingLevel)
				? cmd.thinkingLevel
				: "off";
		const { session } = await createAgentSession({
			cwd: cmd.cwd,
			thinkingLevel: initialLevel,
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
			// MON-82: if Pi just echoed the user turn, pair it with the
			// in-flight classification id so the persist pipeline on the
			// Rust side can backfill `classifications.message_id` inline
			// once the user row saves.
			let forwarded: Record<string, unknown> = event as unknown as Record<
				string,
				unknown
			>;
			if (
				forwarded.type === "message_end" &&
				typeof forwarded.message === "object" &&
				forwarded.message !== null &&
				(forwarded.message as { role?: string }).role === "user"
			) {
				const cid = this.popPendingClassification(agentId);
				if (cid) {
					forwarded = { ...forwarded, classificationId: cid };
				}
			}
			emit({
				type: "event",
				agentId,
				event: forwarded,
			});

			// MON-51: retry exhaustion is the provider-unreachable path Pi
			// takes when the backend (LM Studio down, network blip, 429
			// storm) refuses long enough to use up maxAttempts. Pi does
			// NOT throw from session.prompt() in that case — it resolves
			// quietly and the only signal is `auto_retry_end` with
			// `success: false`. Mirror it to a top-level `error` so the
			// frontend notification store picks it up alongside other
			// sidecar errors.
			if (
				event.type === "auto_retry_end" &&
				event.success === false
			) {
				emit({
					type: "error",
					agentId,
					error:
						event.finalError ??
						`Request failed after ${event.attempt} retries.`,
				});
			}

			// MON-100: drain a deferred Keeper rewrite once Pi quiesces. We
			// stash one when a Keeper run completes mid-streaming-turn so
			// the rewrite never lands while Pi is still mutating
			// `state.messages`. `agent_end` is the safest natural boundary —
			// no in-flight LLM call, no in-flight tool execution.
			if (event.type === "agent_end") {
				const m = this.sessions.get(agentId);
				if (m?.pendingKeeperRewrite) {
					this.applyKeeperRewrite(m, m.pendingKeeperRewrite);
					m.pendingKeeperRewrite = null;
				}
			}
		};

		const unsubscribe = session.subscribe(listener);

		this.sessions.set(cmd.agentId, {
			session,
			unsubscribe,
			uiResolvers,
			shadow: cmd.shadow,
			cwd: cmd.cwd,
			projectInstructions: cmd.projectInstructions,
			captainIdentityPayload: cmd.captainIdentityPayload,
			shadowIdentityPayload: cmd.shadowIdentityPayload,
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

	async prompt(
		agentId: string,
		message: string | PromptContentPart[],
		classifier?: ClassifierInvocation | null,
	): Promise<void> {
		const managed = this.getSession(agentId);
		if (!managed) return;

		// MON-82: fork the classifier alongside the Pi turn. It resolves
		// independently and emits its own event; the Pi turn is never
		// blocked on classification. Push the id so the next user
		// `message_end` carries the pairing.
		if (classifier?.config.enabled) {
			this.pushPendingClassification(agentId, classifier.id);
			const cid = classifier.id;
			const cfg = classifier.config;
			void (async () => {
				const result = await classify(managed.session, message, {
					enabled: cfg.enabled,
					primary: cfg.primary,
					fallback: cfg.fallback ?? null,
					timeoutMs: cfg.timeoutMs,
					systemPrompt: cfg.systemPrompt,
				});
				if ("error" in result) {
					this.emit({
						type: "classification",
						agentId,
						id: cid,
						model: result.model,
						latencyMs: result.latencyMs,
						error: result.error,
					});
				} else {
					this.emit({
						type: "classification",
						agentId,
						id: cid,
						complexity: result.complexity,
						confidence: result.confidence,
						rationale: result.rationale,
						model: result.model,
						tokensIn: result.tokensIn,
						tokensOut: result.tokensOut,
						latencyMs: result.latencyMs,
					});
				}
			})();
		}

		try {
			if (typeof message === "string") {
				// Plain-text path — preserve existing behaviour exactly.
				if (managed.session.isStreaming) {
					await managed.session.followUp(message);
				} else {
					await managed.session.prompt(message);
				}
			} else {
				// Multimodal path — extract text and image parts separately.
				const text = message
					.filter((p): p is { type: "text"; text: string } => p.type === "text")
					.map((p) => p.text)
					.join("");
				const images: ImageContent[] = message
					.filter(
						(p): p is { type: "image"; data: string; mimeType: string } =>
							p.type === "image",
					)
					.map((p) => ({ type: "image", data: p.data, mimeType: p.mimeType }));

				if (managed.session.isStreaming) {
					await managed.session.followUp(text, images);
				} else {
					await managed.session.prompt(text, { images });
				}
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
		if (!isValidThinkingLevel(level)) {
			this.emit({
				type: "error",
				agentId,
				error: `Unknown thinking level: ${level}`,
			});
			return;
		}
		managed.session.setThinkingLevel(level);
	}

	newSession(agentId: string): void {
		const managed = this.getSession(agentId);
		if (!managed) return;
		// Reset both the session manager (file/memory) and the agent's live message state
		managed.session.sessionManager.newSession();
		managed.session.agent.state.messages = [];
		// MON-100: any deferred Keeper rewrite is keyed against the old
		// message array length and is now nonsensical — drop it.
		managed.pendingKeeperRewrite = null;
	}

	async compact(agentId: string): Promise<void> {
		const managed = this.getSession(agentId);
		if (!managed) return;
		await managed.session.compact();
	}

	/**
	 * MON-100: continuous-compaction Keeper run.
	 *
	 * Captures `tailAnchor = state.messages.length` BEFORE the LLM call so we
	 * know which messages predate the run; the synthesized scaffold replaces
	 * `[0..tailAnchor]` with `[user: "Previous context …", assistant: "Acknowledged"]`
	 * and the tail (anything Pi appended during the round trip) survives
	 * untouched. Captain-facing chat UI reads from the canonical SQLite
	 * `messages` table, so this rewrite only ever affects what the LLM sees.
	 *
	 * On failure: emit `keeper_result` with an `error` field and skip the
	 * rewrite — raw history stays intact and the next threshold crossing
	 * retries.
	 *
	 * On success mid-streaming: stash the rewrite on the managed session and
	 * apply it at the next `agent_end` (the listener drains it). Avoids
	 * mutating `state.messages` while Pi is mid-turn.
	 */
	async keeperRun(cmd: KeeperRunCommand): Promise<void> {
		const managed = this.getSession(cmd.agentId);
		if (!managed) return;

		const tailAnchor = managed.session.agent.state.messages.length;

		const result = await runKeeper(managed.session, cmd.slice, cmd.config);

		if ("error" in result) {
			this.emit({
				type: "keeper_result",
				agentId: cmd.agentId,
				runId: cmd.runId,
				model: result.model,
				latencyMs: result.latencyMs,
				error: result.error,
			});
			return;
		}

		this.emit({
			type: "keeper_result",
			agentId: cmd.agentId,
			runId: cmd.runId,
			claims: result.claims,
			compactionSummary: result.compactionSummary,
			model: result.model,
			tokensIn: result.tokensIn,
			tokensOut: result.tokensOut,
			latencyMs: result.latencyMs,
		});

		const rewrite: PendingKeeperRewrite = {
			runId: cmd.runId,
			summary: result.compactionSummary,
			tailAnchor,
		};
		if (managed.session.isStreaming) {
			managed.pendingKeeperRewrite = rewrite;
		} else {
			this.applyKeeperRewrite(managed, rewrite);
		}
	}

	/**
	 * MON-100: replace the Pi message array's first `tailAnchor` entries with
	 * a two-message synthesized scaffold. Mechanism mirrors `loadSession`'s
	 * direct mutation of `session.agent.state.messages`. Idempotent on
	 * `tailAnchor > current length` (out-of-date anchor — sidecar restart or
	 * a `new_session` happened between dispatch and result; skip).
	 */
	private applyKeeperRewrite(
		managed: ManagedSession,
		rewrite: PendingKeeperRewrite,
	): void {
		const all = managed.session.agent.state.messages;
		if (rewrite.tailAnchor > all.length) {
			return;
		}
		const tail = all.slice(rewrite.tailAnchor);
		const ts = new Date().toISOString();

		// Pull a model id from the most recent assistant entry so the
		// synthesized "Acknowledged" message stays valid for Pi's pipeline.
		// Fall back to a stable placeholder; Pi tolerates unknown ids on
		// replayed/synthesized rows the same way `loadSession` does.
		let modelId = "monarch-keeper";
		for (let i = all.length - 1; i >= 0; i--) {
			const m = all[i] as { role?: string; model?: string };
			if (m?.role === "assistant" && typeof m.model === "string") {
				modelId = m.model;
				break;
			}
		}

		const synthUser = {
			role: "user",
			content: `[Previous context — Keeper compaction @ ${ts}]\n\n## Summary\n\n${rewrite.summary}`,
			timestamp: Date.now(),
		};
		const synthAssistant = {
			role: "assistant",
			content: [
				{
					type: "text",
					text: "Acknowledged. Continuing from this state.",
				},
			],
			model: modelId,
			usage: { ...EMPTY_USAGE, cost: { ...EMPTY_USAGE.cost } },
			stopReason: "stop",
			timestamp: Date.now(),
		};

		managed.session.agent.state.messages = [
			synthUser,
			synthAssistant,
			...tail,
		] as any;
	}

	/**
	 * Load messages from SQLite into the agent's conversation context.
	 * This replays past messages so the LLM has conversational continuity.
	 */
	loadSession(agentId: string, messages: LoadSessionCommand["messages"]): void {
		const managed = this.getSession(agentId);
		if (!managed) return;

		// Rebuild from a clean session state so restored/continued sessions don't
		// accumulate stale messages from a previous in-memory run.
		managed.session.sessionManager.newSession();
		managed.session.agent.state.messages = [];
		// MON-100: same logic as newSession — drop any deferred Keeper rewrite
		// because its tail anchor is no longer meaningful against the rebuilt
		// message array.
		managed.pendingKeeperRewrite = null;

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
			managed.session.agent.state.messages = agentMessages as any;

			// Rebuild the session manager's persisted log from the same source so
			// compaction and future prompts see the exact restored context.
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
		captainIdentityPayload?: string | null,
		shadowIdentityPayload?: string | null,
	): void {
		const managed = this.sessions.get(agentId);
		if (!managed) return;

		if (projectInstructions !== undefined) {
			managed.projectInstructions = projectInstructions;
		}
		// MON-98: update stored identity payloads when provided. `undefined`
		// means "leave unchanged"; an empty string clears the section.
		if (captainIdentityPayload !== undefined) {
			managed.captainIdentityPayload = captainIdentityPayload || null;
		}
		if (shadowIdentityPayload !== undefined) {
			managed.shadowIdentityPayload = shadowIdentityPayload || null;
		}

		const next =
			prompt?.trim() ||
			(managed.shadow
				? buildSystemPrompt(managed.shadow, managed.cwd, managed.projectInstructions, managed.captainIdentityPayload, managed.shadowIdentityPayload)
				: managed.projectInstructions?.trim() || "");
		if (!next) return;

		managed.promptRef.current = next;
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
