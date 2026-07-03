import { defineTool } from "@mariozechner/pi-coding-agent";
import { Type } from "@mariozechner/pi-ai";
import type { EmitFn } from "./ui-bridge.js";

function cleanOptional(value: string | undefined): string | undefined {
	const trimmed = value?.trim();
	return trimmed ? trimmed : undefined;
}

export function createNarrationTools(agentId: string, emit: EmitFn) {
	return [
		defineTool({
			name: "set_current_action",
			label: "Set Current Action",
			description:
				"Declare the current coherent action being worked on. Use for meaningful chunks of work, not every individual tool call.",
			promptSnippet:
				"set_current_action(intent, previous_outcome?) - declare the current meaningful work chunk; optionally close the prior chunk.",
			promptGuidelines: [
				"Call set_current_action when you begin a meaningful chunk of coding work, investigation, verification, or docs.",
				"Do not call it for every file read, grep, shell command, or tiny step; tool calls inside the chunk are tracked automatically.",
				"When switching from one chunk to another, include previous_outcome to close the prior action in one sentence.",
			],
			parameters: Type.Object({
				intent: Type.String({
					description:
						"One concise sentence describing the current action, such as 'Inspect the auth flow and failing tests'.",
				}),
				previous_outcome: Type.Optional(
					Type.String({
						description:
							"Optional one-sentence result of the previous action when this call is also switching actions.",
					}),
				),
			}),
			async execute(_toolCallId, params) {
				const intent = params.intent.trim();
				const previousOutcome = cleanOptional(params.previous_outcome);
				emit({
					type: "event",
					agentId,
					event: {
						type: "action_transition",
						intent,
						// Rust deserializes inner-event fields as camelCase
						// (rename_all_fields). MON-108 emitted snake_case so
						// previous_outcome was silently dropped — fixed here.
						previousOutcome,
					},
				});
				return {
					content: [
						{
							type: "text",
							text: previousOutcome
								? "Current action updated and previous action closed."
								: "Current action updated.",
						},
					],
					details: { intent, previous_outcome: previousOutcome },
				};
			},
		}),
		defineTool({
			name: "complete_action",
			label: "Complete Action",
			description:
				"Close the current coherent action with a concise outcome when no next action is starting immediately.",
			promptSnippet:
				"complete_action(outcome) - close the active coherent action with its result.",
			promptGuidelines: [
				"Use complete_action when the current chunk is done and you are not immediately starting a new chunk with set_current_action.",
				"Keep the outcome factual and brief; do not include hidden reasoning or a transcript of tool calls.",
			],
			parameters: Type.Object({
				outcome: Type.String({
					description:
						"One concise sentence describing what the completed action accomplished or found.",
				}),
			}),
			async execute(_toolCallId, params) {
				const outcome = params.outcome.trim();
				emit({
					type: "event",
					agentId,
					event: {
						type: "action_complete",
						outcome,
					},
				});
				return {
					content: [{ type: "text", text: "Current action completed." }],
					details: { outcome },
				};
			},
		}),
		defineTool({
			name: "record_decision",
			label: "Record Decision",
			description:
				"Record a sparse, explicit implementation, architecture, scope, or safety decision.",
			promptSnippet:
				"record_decision(decision, rationale?) - record an explicit approach or scope decision.",
			promptGuidelines: [
				"Use record_decision only for decisions that would help the user or a future agent understand why the work went this way.",
				"Do not use it as a scratchpad or to persist raw chain-of-thought; include rationale only when it is explicitly useful.",
			],
			parameters: Type.Object({
				decision: Type.String({
					description:
						"One concise sentence naming the decision, such as 'Use a separate table for working memory'.",
				}),
				rationale: Type.Optional(
					Type.String({
						description:
							"Optional brief rationale, limited to user-visible reasoning or constraints.",
					}),
				),
			}),
			async execute(_toolCallId, params) {
				const decision = params.decision.trim();
				const rationale = cleanOptional(params.rationale);
				emit({
					type: "event",
					agentId,
					event: {
						type: "executor_decision",
						decision,
						rationale,
					},
				});
				return {
					content: [{ type: "text", text: "Decision recorded." }],
					details: { decision, rationale },
				};
			},
		}),
	] as const;
}
