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
	return `## Relevant Memories\n${lines.join("\n")}`;
}
