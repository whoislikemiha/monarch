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
import type { ThinkingLevel } from "@mariozechner/pi-ai";
import { createShadowOathFactory } from "./shadow-oath.js";
import { createUIBridge, type EmitFn, type UIResolvers } from "./ui-bridge.js";
import type { CreateSessionCommand, LoadSessionCommand } from "./protocol.js";

interface ManagedSession {
	session: AgentSession;
	unsubscribe: () => void;
	uiResolvers: UIResolvers;
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

		const resourceLoader = new DefaultResourceLoader({
			cwd: cmd.cwd,
			extensionFactories: [createShadowOathFactory(cmd.shadow, cmd.cwd)],
			noExtensions: true,
		});

		// Create session without model first, then set model via registry
		// This handles arbitrary provider/model combos (including OpenRouter)
		const { session } = await createAgentSession({
			cwd: cmd.cwd,
			thinkingLevel: (cmd.thinkingLevel || "medium") as ThinkingLevel,
			sessionManager: SessionManager.inMemory(cmd.cwd),
			resourceLoader,
		});

		// Resolve model from registry (supports known + custom models)
		const model = session.modelRegistry.find(cmd.provider, cmd.model);
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

		this.sessions.set(cmd.agentId, { session, unsubscribe, uiResolvers });

		this.emit({ type: "session_ready", agentId: cmd.agentId });
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
	): Promise<void> {
		const managed = this.getSession(agentId);
		if (!managed) return;

		const model = managed.session.modelRegistry.find(provider, modelId);
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
					content: msg.content,
				});
			} else if (msg.role === "assistant") {
				let content;
				try {
					content = JSON.parse(msg.content);
				} catch {
					content = [{ type: "text", text: msg.content }];
				}
				agentMessages.push({
					role: "assistant",
					content,
					model: msg.model,
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
