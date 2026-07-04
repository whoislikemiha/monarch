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
import { randomUUID } from "node:crypto";
import type { ThinkingLevel } from "@mariozechner/pi-agent-core";
import { type ImageContent } from "@mariozechner/pi-ai";
import { buildSystemPrompt } from "./agent-persona.js";
import { createUIBridge, type EmitFn, type UIResolvers } from "./ui-bridge.js";
import { createNarrationTools } from "./narration-tools.js";
import { installNarrationNudge } from "./narration-nudge.js";
import { createPlanTools } from "./plan-tools.js";
import { createReportTools } from "./report-tools.js";
import type {
	ClassifierInvocation,
	CreateSessionCommand,
	KeeperRunCommand,
	LoadSessionCommand,
	MemorySearchResult,
	PromptContentPart,
} from "./protocol.js";
import { classify } from "./classifier.js";
import { runKeeper } from "./keeper.js";
import { ensureLmStudioProviderRegistered, isValidThinkingLevel, resolveModel } from "./model-resolver.js";
import { extractPromptText, normalizeStoredAssistantContent, normalizeStoredUserContent } from "./stored-content.js";
import {
	createSuggestMemoryTool,
	formatRelevantMemories,
	stripInjectedContext,
} from "./memory-tools.js";

interface MemorySearchResolver {
	agentId: string;
	resolve: (results: MemorySearchResult[]) => void;
	timeout: NodeJS.Timeout;
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
}

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
	private memorySearchResolvers = new Map<string, MemorySearchResolver>();

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
			await this.destroySession(cmd.agentId, { silent: true });
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
			customTools: [
				createSuggestMemoryTool(cmd.agentId, this.emit),
				...createNarrationTools(cmd.agentId, this.emit),
				...createPlanTools(cmd.agentId, this.emit),
				...createReportTools(cmd.agentId, this.emit),
			],
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

		// MON-130: enforce narration cadence — append a system-reminder to
		// tool results after a run of un-narrated tool calls. Composes onto
		// the afterToolCall hook Pi installed in the AgentSession constructor.
		installNarrationNudge(session);

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
				(forwarded.type === "message_start" || forwarded.type === "message_end") &&
				typeof forwarded.message === "object" &&
				forwarded.message !== null &&
				(forwarded.message as { role?: string }).role === "user"
			) {
				if (forwarded.type === "message_end") {
					const cid = this.popPendingClassification(agentId);
					if (cid) {
						forwarded = { ...forwarded, classificationId: cid };
					}
				}
				// MON-130: what the user typed is what Monarch shows and
				// stores — strip injected recall blocks from the FORWARDED
				// COPY only (Pi's live context keeps them; deep-copy so we
				// never mutate the session's own message object). Both
				// boundaries matter: the live chat bubble is built from
				// message_start, persistence from message_end.
				forwarded = {
					...forwarded,
					message: stripInjectedFromUserMessage(
						forwarded.message as Record<string, unknown>,
					),
				};
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

			// Guard against silent empty/errored assistant turns. The
			// openai-codex (Responses API) backend can complete a turn with
			// zero output items, or surface a provider stream error that Pi
			// resolves into an assistant message with empty content /
			// stopReason "error" — WITHOUT throwing from prompt(). Pi emits a
			// normal `message_end` in that case, so the empty turn otherwise
			// persists as a blank assistant bubble with no feedback (looks
			// like a hang). Detect it and mirror to a top-level `error` so the
			// notification store toasts it. A user-initiated abort produces an
			// empty/partial turn too, so exclude `stopReason: "aborted"`.
			if (event.type === "message_end") {
				const msg = forwarded.message as
					| {
							role?: string;
							content?: unknown[];
							stopReason?: string;
							errorMessage?: string;
					  }
					| undefined;
				if (msg && msg.role === "assistant" && msg.stopReason !== "aborted") {
					const isEmpty =
						!Array.isArray(msg.content) || msg.content.length === 0;
					const hasError =
						msg.stopReason === "error" ||
						(typeof msg.errorMessage === "string" &&
							msg.errorMessage.length > 0);
					if (isEmpty || hasError) {
						emit({
							type: "error",
							agentId,
							error: msg.errorMessage
								? `The model returned an error: ${msg.errorMessage}`
								: "The model returned an empty response (no output). " +
									"This usually clears on retry; if it persists, start a new session.",
						});
					}
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

	/**
	 * Tear down an agent's in-memory session. `silent` suppresses the
	 * `session_destroyed` event for the replace-before-recreate path inside
	 * `createSession` (MON-127): that destroy is an implementation detail, and
	 * announcing it makes Rust/frontend treat a healthy respawning agent as
	 * exited (false "stopped" status + error toast).
	 */
	async destroySession(agentId: string, opts?: { silent?: boolean }): Promise<void> {
		const managed = this.sessions.get(agentId);
		if (!managed) return;

		managed.unsubscribe();
		managed.session.dispose();
		managed.uiResolvers.clear();
		for (const [requestId, resolver] of this.memorySearchResolvers) {
			if (resolver.agentId === agentId) {
				clearTimeout(resolver.timeout);
				resolver.resolve([]);
				this.memorySearchResolvers.delete(requestId);
			}
		}
		this.sessions.delete(agentId);

		if (!opts?.silent) this.emit({ type: "session_destroyed", agentId });
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
			const outboundMessage = managed.session.isStreaming
				? message
				: await this.withRelevantMemories(agentId, message);

			if (typeof outboundMessage === "string") {
				// Plain-text path — preserve existing behaviour exactly.
				if (managed.session.isStreaming) {
					await managed.session.followUp(outboundMessage);
				} else {
					await managed.session.prompt(outboundMessage);
				}
			} else {
				// Multimodal path — extract text and image parts separately.
				const text = outboundMessage
					.filter((p): p is { type: "text"; text: string } => p.type === "text")
					.map((p) => p.text)
					.join("");
				const images: ImageContent[] = outboundMessage
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

	private async withRelevantMemories(
		agentId: string,
		message: string | PromptContentPart[],
	): Promise<string | PromptContentPart[]> {
		const query = extractPromptText(message).trim();
		if (!query) return message;

		const started = Date.now();
		const results = await this.requestMemorySearch(agentId, query);
		const elapsed = Date.now() - started;
		if (elapsed > 200) {
			process.stderr.write(
				`[sidecar] memory search for ${agentId} took ${elapsed}ms\n`,
			);
		}
		if (results.length === 0) return message;

		const prefix = formatRelevantMemories(results);
		if (!prefix) return message;

		if (typeof message === "string") {
			return `${prefix}\n\n${message}`;
		}

		let injected = false;
		const parts = message.map((part) => {
			if (!injected && part.type === "text") {
				injected = true;
				return { ...part, text: `${prefix}\n\n${part.text}` };
			}
			return part;
		});
		if (!injected) {
			return [{ type: "text", text: prefix }, ...parts];
		}
		return parts;
	}

	private requestMemorySearch(
		agentId: string,
		query: string,
	): Promise<MemorySearchResult[]> {
		return new Promise((resolve) => {
			const requestId = randomUUID();
			const timeout = setTimeout(() => {
				this.memorySearchResolvers.delete(requestId);
				resolve([]);
			}, 200);
			this.memorySearchResolvers.set(requestId, {
				agentId,
				resolve,
				timeout,
			});
			this.emit({
				type: "memory_search_request",
				agentId,
				requestId,
				query,
			});
		});
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
	 * untouched. Supervisor-facing chat UI reads from the canonical SQLite
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
					// Legacy duplicate rows (raw content arrays double-persisted
					// from `message_end` between 2026-04 and 2026-06) carry no
					// toolCallId. Replaying one sends an empty `call_id`, which
					// the Codex Responses API rejects with a 400 — skip them;
					// the canonical blob row for the same call follows anyway.
					if (
						typeof parsed !== "object" ||
						parsed === null ||
						Array.isArray(parsed) ||
						typeof parsed.toolCallId !== "string" ||
						parsed.toolCallId.length === 0
					) {
						continue;
					}
					agentMessages.push({
						role: "toolResult",
						toolCallId: parsed.toolCallId,
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

	handleMemorySearchResponse(
		agentId: string,
		requestId: string,
		results: MemorySearchResult[],
		error?: string | null,
	): void {
		const resolver = this.memorySearchResolvers.get(requestId);
		if (!resolver || resolver.agentId !== agentId) return;

		clearTimeout(resolver.timeout);
		this.memorySearchResolvers.delete(requestId);
		if (error) {
			process.stderr.write(
				`[sidecar] memory search response ${requestId} for ${agentId}: ${error}\n`,
			);
			resolver.resolve([]);
			return;
		}
		resolver.resolve(Array.isArray(results) ? results : []);
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

/** MON-130: deep-copy a user message with injected recall blocks removed
 * from its text content. Handles both content shapes (plain string, block
 * array). Pi's in-memory message is never touched — this only cleans the
 * copy forwarded to Rust for persistence. */
function stripInjectedFromUserMessage(
	message: Record<string, unknown>,
): Record<string, unknown> {
	const content = message.content;
	if (typeof content === "string") {
		return { ...message, content: stripInjectedContext(content) };
	}
	if (Array.isArray(content)) {
		return {
			...message,
			content: content.map((block) =>
				block &&
				typeof block === "object" &&
				(block as { type?: string }).type === "text" &&
				typeof (block as { text?: unknown }).text === "string"
					? { ...block, text: stripInjectedContext((block as { text: string }).text) }
					: block,
			),
		};
	}
	return message;
}
