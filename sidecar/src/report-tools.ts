import { defineTool } from "@mariozechner/pi-coding-agent";
import { Type } from "@mariozechner/pi-ai";
import type { EmitFn } from "./ui-bridge.js";

// Gentle length caps. Hard truncation rather than rejection — a
// complete_quest call should never fail; the report should land even if a
// field is overlong. Mirrors the pattern in plan-tools.ts.
const SUMMARY_MAX = 1500;
const REFLECTION_MAX = 1500;
const SENTENCE_MAX = 500;
const ROLE_MAX = 80;
const GRADE_MAX = 40;
const LIST_MAX = 64;

function trimTo(value: string, max: number): string {
	const trimmed = value.trim();
	return trimmed.length <= max ? trimmed : `${trimmed.slice(0, max - 1)}…`;
}

function cleanOptional(value: string | undefined, max: number): string | undefined {
	if (value === undefined) return undefined;
	const trimmed = trimTo(value, max);
	return trimmed.length > 0 ? trimmed : undefined;
}

function cleanStringList(value: unknown, max: number): string[] {
	if (!Array.isArray(value)) return [];
	const out: string[] = [];
	for (const item of value.slice(0, LIST_MAX)) {
		if (typeof item !== "string") continue;
		const trimmed = trimTo(item, max);
		if (trimmed) out.push(trimmed);
	}
	return out;
}

export function createReportTools(agentId: string, emit: EmitFn) {
	return [
		defineTool({
			name: "complete_quest",
			label: "Complete Quest",
			description:
				"Write the first-person quest report and close the quest. Call this once, as the final action on a quest, when your work on it is finished.",
			promptSnippet:
				"complete_quest(report) - write the first-person quest report and close the quest.",
			promptGuidelines: [
				"Call complete_quest exactly once per quest, when work on it is finished — it is the last thing you do on that quest.",
				"The report is your own first-person account: what the quest was, what you decided and why, what you learned, what you produced, what is left.",
				"outcome 'done' or 'abandoned' closes the quest; 'partial' or 'blocked' record the report but leave the quest open.",
				"learned[] are your own suggestions to the Keeper — durable lessons, not a transcript. grade is your self-assessment; the Keeper or captain may override it.",
			],
			parameters: Type.Object({
				report: Type.Object({
					summary: Type.String({
						description:
							"One to a few sentences: what the quest was and how it went.",
					}),
					outcome: Type.Union(
						[
							Type.Literal("done"),
							Type.Literal("blocked"),
							Type.Literal("abandoned"),
							Type.Literal("partial"),
						],
						{
							description:
								"done | blocked | abandoned | partial. 'done' and 'abandoned' close the quest; 'blocked' and 'partial' leave it open.",
						},
					),
					decisions: Type.Array(
						Type.Object({
							decision: Type.String({
								description: "One concise sentence naming the decision.",
							}),
							rationale: Type.Optional(
								Type.String({
									description: "Optional one-sentence rationale.",
								}),
							),
						}),
						{
							description:
								"Explicit decisions made during the quest. Empty array if none worth recording.",
						},
					),
					learned: Type.Array(Type.String(), {
						description:
							"Durable lessons worth keeping — your own suggestions to the Keeper. One assertion per entry.",
					}),
					artifacts: Type.Array(
						Type.Object({
							file: Type.String({
								description: "Path or identifier of the artifact.",
							}),
							role: Type.String({
								description:
									"What happened to it, e.g. created | modified | deleted | documentation.",
							}),
						}),
						{
							description: "Files or other artifacts the quest produced or changed.",
						},
					),
					open_threads: Type.Array(Type.String(), {
						description:
							"Unfinished work, follow-ups, or known gaps left after this quest.",
					}),
					reflection: Type.String({
						description:
							"Brief first-person reflection on how the quest went.",
					}),
					grade: Type.String({
						description:
							"Your self-assessed grade for the quest (e.g. A, B, C). Self-suggested; may be overridden.",
					}),
				}),
			}),
			async execute(_toolCallId, params) {
				const r = params.report;
				const decisions = (Array.isArray(r.decisions) ? r.decisions : [])
					.slice(0, LIST_MAX)
					.map((d) => ({
						decision: trimTo(d.decision ?? "", SENTENCE_MAX),
						rationale: cleanOptional(d.rationale, SENTENCE_MAX),
					}))
					.filter((d) => d.decision.length > 0);
				const artifacts = (Array.isArray(r.artifacts) ? r.artifacts : [])
					.slice(0, LIST_MAX)
					.map((a) => ({
						file: trimTo(a.file ?? "", SENTENCE_MAX),
						role: trimTo(a.role ?? "", ROLE_MAX),
					}))
					.filter((a) => a.file.length > 0);

				const report = {
					summary: trimTo(r.summary ?? "", SUMMARY_MAX),
					outcome: r.outcome,
					decisions,
					learned: cleanStringList(r.learned, SENTENCE_MAX),
					artifacts,
					open_threads: cleanStringList(r.open_threads, SENTENCE_MAX),
					reflection: trimTo(r.reflection ?? "", REFLECTION_MAX),
					grade: trimTo(r.grade ?? "", GRADE_MAX),
				};

				emit({
					type: "event",
					agentId,
					event: {
						type: "quest_report",
						report,
					},
				});

				const closed = report.outcome === "done" || report.outcome === "abandoned";
				return {
					content: [
						{
							type: "text",
							text: closed
								? `Quest report recorded; quest closed as ${report.outcome}.`
								: "Quest report recorded; quest left open.",
						},
					],
					details: { outcome: report.outcome, closed },
				};
			},
		}),
	] as const;
}
