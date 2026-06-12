/**
 * MON-128 (P3) — chat-shadow tool set.
 *
 * The chat organ reads, directs, and controls — it never mutates the world.
 * Its Pi built-ins are allowlisted to read/grep/find/ls (no bash, no
 * write/edit — see plan decision 5); these custom tools add the substrate
 * reads and the executor control plane.
 *
 * Dispatch discipline (plan decision 1): `hand_to_executor` takes NO payload.
 * Rust injects the verbatim conversation slice since the last handoff — the
 * model decides *when* work crosses over, the mechanism decides *what*.
 */

import { defineTool } from "@mariozechner/pi-coding-agent";
import { Type } from "@mariozechner/pi-ai";
import type { EmitFn } from "./ui-bridge.js";

/** Control surface the RuntimeManager hands to the chat tools. */
export interface ChatToolControls {
	/** Engage the executor pause gate. Returns a human-readable status. */
	pauseExecutor(reason?: string): string;
	resumeExecutor(): string;
	/** Hard-stop the in-flight executor turn (release gate, then abort). */
	stopExecutor(reason?: string): Promise<string>;
	/** Recent executor activity + working memory, fetched from Rust. */
	recallActions(limit?: number): Promise<string>;
	/** Memory search against the L3 substrate, via Rust. */
	memorySearch(query: string): Promise<string>;
}

export function createChatTools(
	agentId: string,
	emit: EmitFn,
	controls: ChatToolControls,
) {
	return [
		defineTool({
			name: "recall_actions",
			label: "Recall Actions",
			description:
				"Read the executor's working memory and recent timeline activity — what it is doing right now and what it did recently. Use this before answering any question about current or recent work.",
			parameters: Type.Object({
				limit: Type.Optional(
					Type.Number({ description: "Max recent events to return (default 20)." }),
				),
			}),
			async execute(_toolCallId, params) {
				const text = await controls.recallActions(params.limit ?? undefined);
				return { content: [{ type: "text" as const, text }], details: {} };
			},
		}),
		defineTool({
			name: "memory_search",
			label: "Memory Search",
			description:
				"Search the shadow's long-term memory (L3) for relevant claims, decisions, and conventions.",
			parameters: Type.Object({
				query: Type.String({ description: "What to search for." }),
			}),
			async execute(_toolCallId, params) {
				const text = await controls.memorySearch(params.query);
				return { content: [{ type: "text" as const, text }], details: {} };
			},
		}),
		defineTool({
			name: "surface_observation",
			label: "Surface Observation",
			description:
				"Record a durable observation from this conversation onto the work timeline — something the captain or a future session should see alongside the work record. Use sparingly.",
			parameters: Type.Object({
				observation: Type.String({ description: "The observation, one or two sentences." }),
			}),
			async execute(_toolCallId, params) {
				emit({
					type: "event",
					agentId,
					sessionRole: "executor",
					event: { type: "chat_observation", observation: params.observation.trim() },
				});
				return {
					content: [{ type: "text" as const, text: "Observation recorded on the timeline." }],
					details: {},
				};
			},
		}),
		defineTool({
			name: "hand_to_executor",
			label: "Hand to Executor",
			description:
				"Dispatch the conversation since the last handoff to the executor as work. Takes no arguments — the captain's words are delivered verbatim; do NOT paraphrase or summarize them yourself. Call this whenever the captain has given work to do (new task, change of direction, follow-up instruction). Never claim work was dispatched without calling this.",
			parameters: Type.Object({}),
			async execute(_toolCallId, _params) {
				emit({ type: "chat_handoff_request", agentId });
				return {
					content: [
						{
							type: "text" as const,
							text: "Handoff requested — the conversation since the last handoff is being delivered to the executor verbatim.",
						},
					],
					details: {},
				};
			},
		}),
		defineTool({
			name: "pause_executor",
			label: "Pause Executor",
			description:
				"Pause the executor at its next tool boundary (the current tool call finishes; the next one waits). Use when the captain wants to interject mid-work.",
			parameters: Type.Object({
				reason: Type.Optional(Type.String({ description: "Why the pause was requested." })),
			}),
			async execute(_toolCallId, params) {
				const text = controls.pauseExecutor(params.reason ?? undefined);
				return { content: [{ type: "text" as const, text }], details: {} };
			},
		}),
		defineTool({
			name: "resume_executor",
			label: "Resume Executor",
			description: "Resume a paused executor.",
			parameters: Type.Object({}),
			async execute(_toolCallId, _params) {
				const text = controls.resumeExecutor();
				return { content: [{ type: "text" as const, text }], details: {} };
			},
		}),
		defineTool({
			name: "stop_executor",
			label: "Stop Executor",
			description:
				"Hard-stop the executor's in-flight turn. Same effect as the captain's stop button. Use only when the captain clearly wants the work halted.",
			parameters: Type.Object({
				reason: Type.Optional(Type.String({ description: "Why the stop was requested." })),
			}),
			async execute(_toolCallId, params) {
				const text = await controls.stopExecutor(params.reason ?? undefined);
				return { content: [{ type: "text" as const, text }], details: {} };
			},
		}),
	];
}
