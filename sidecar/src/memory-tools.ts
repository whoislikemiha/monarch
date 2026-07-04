import { defineTool } from "@mariozechner/pi-coding-agent";
import { Type } from "@mariozechner/pi-ai";
import type { EmitFn } from "./ui-bridge.js";
import type { MemorySearchResult } from "./protocol.js";
import { oneLine } from "./stored-content.js";

export function createSuggestMemoryTool(agentId: string, emit: EmitFn) {
	return defineTool({
		name: "suggest_memory",
		label: "Suggest Memory",
		description:
			"Suggest a noteworthy fact, decision, preference, or convention for the curator to consider later.",
		promptSnippet:
			"suggest_memory(title, summary, content) - flag a durable fact, decision, preference, or convention for later curator review.",
		promptGuidelines: [
			"Use suggest_memory only for durable information that should likely survive this objective.",
			"The tool records a suggestion only; the curator decides whether it becomes memory.",
		],
		parameters: Type.Object({
			title: Type.String({
				description: "Short title for the suggested memory.",
			}),
			summary: Type.String({
				description: "One-sentence summary of what should be remembered.",
			}),
			content: Type.String({
				description: "Supporting detail, evidence, or context for the curator.",
			}),
		}),
		async execute(_toolCallId, params) {
			const title = params.title.trim();
			const summary = params.summary.trim();
			const content = params.content.trim();
			emit({
				type: "event",
				agentId,
				event: {
					type: "memory_suggestion",
					title,
					summary,
					content,
				},
			});
			return {
				content: [
					{
						type: "text",
						text: "Memory suggestion queued for curator review if an active objective is available.",
					},
				],
				details: { title, summary, content },
			};
		},
	});
}

/** MON-130: explicit delimiters around injected recall so the boundary is
 * unambiguous — the model sees system-provided context (not user words), and
 * the sidecar strips the block from the forwarded user `message_end` before
 * it reaches persistence (`stripInjectedContext`). Same philosophy as MON-75
 * attachments: the stored user message is what the user typed; injections
 * are re-created at prompt time. */
export const INJECTED_MEMORIES_OPEN = "<relevant-memories>";
export const INJECTED_MEMORIES_CLOSE = "</relevant-memories>";

const INJECTED_MEMORIES_RE = /<relevant-memories>[\s\S]*?<\/relevant-memories>\s*/g;

export function formatRelevantMemories(results: MemorySearchResult[]): string {
	const lines = results
		.slice(0, 8)
		.map((result) => {
			const memory = result.memory;
			const title = oneLine(memory.title || `Memory #${memory.id}`, 90);
			const summary = oneLine(memory.summary, 260);
			const content = memory.content
				? ` ${oneLine(memory.content, 220)}`
				: "";
			return `- ${title}: ${summary}${content}`;
		})
		.filter(Boolean);
	if (lines.length === 0) return "";
	return `${INJECTED_MEMORIES_OPEN}\nRelevant memories recalled for this turn (system-provided context, not part of the user's message):\n${lines.join("\n")}\n${INJECTED_MEMORIES_CLOSE}`;
}

/** Remove injected recall blocks from a user message's text so persistence
 * (and everything downstream: chat rendering, objective-title heuristics,
 * the Curator's activity slice) only ever sees the user's own words. */
export function stripInjectedContext(text: string): string {
	return text.replace(INJECTED_MEMORIES_RE, "").replace(/^\s+/, "");
}
