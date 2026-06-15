import { defineTool } from "@mariozechner/pi-coding-agent";
import { Type } from "@mariozechner/pi-ai";
import type { EmitFn } from "./ui-bridge.js";
import { randomUUID } from "node:crypto";

// Gentle length caps. Hard truncation rather than rejection — we never
// want a plan tool call to fail; the executor's intent should land even
// if the title is overlong. Mirrors the pattern in narration-tools.ts.
const TITLE_MAX = 200;
const RATIONALE_MAX = 500;
const REASON_MAX = 300;
const OUTCOME_MAX = 500;
const ITEMS_MAX = 32;

function trimTo(value: string, max: number): string {
	const trimmed = value.trim();
	return trimmed.length <= max ? trimmed : `${trimmed.slice(0, max - 1)}…`;
}

function cleanOptional(value: string | undefined, max: number): string | undefined {
	if (value === undefined) return undefined;
	const trimmed = trimTo(value, max);
	return trimmed.length > 0 ? trimmed : undefined;
}

export function createPlanTools(agentId: string, emit: EmitFn) {
	return [
		defineTool({
			name: "set_plan",
			label: "Set Execution Plan",
			description:
				"Declare or fully replace the current objective's execution plan: an ordered list of intended next steps.",
			promptSnippet:
				"set_plan(items, rationale?) - declare/replace the objective's intended-route plan; items[].title is the only required field per item.",
			promptGuidelines: [
				"Plan items are the *intended route*, not history. Do not record completed work as plan items after the fact.",
				"Granularity: each item is roughly one or a few coherent actions — coarser than tool calls, finer than the objective goal.",
				"Declare a plan early when the task is non-trivial; skip set_plan for trivial single-step tasks.",
				"Titles are short and action-shaped, e.g. 'patch expiry handler', 'run focused tests'. Rationale is optional.",
				"Calling set_plan again replaces the plan. Items whose id matches an existing item keep their status; missing items are dropped; new items start as pending.",
			],
			parameters: Type.Object({
				items: Type.Array(
					Type.Object({
						id: Type.Optional(
							Type.String({
								description:
									"Optional id. Supply only when preserving an existing item across a set_plan call; omit to mint a new one.",
							}),
						),
						title: Type.String({
							description:
								"Short action-shaped title for the step, e.g. 'inspect auth flow'.",
						}),
						rationale: Type.Optional(
							Type.String({
								description: "Optional brief rationale for this step (1 sentence).",
							}),
						),
					}),
					{
						description:
							"Ordered list of plan items. Order in the array determines the plan order.",
					},
				),
				rationale: Type.Optional(
					Type.String({
						description:
							"Optional one-sentence rationale for the plan as a whole, e.g. why these steps in this order.",
					}),
				),
			}),
			async execute(_toolCallId, params) {
				type CleanItem = { id?: string; title: string; rationale?: string };
				const rawItems = Array.isArray(params.items) ? params.items.slice(0, ITEMS_MAX) : [];
				const items: CleanItem[] = [];
				for (const it of rawItems) {
					const title = trimTo(it.title ?? "", TITLE_MAX);
					if (!title) continue;
					// Mint an id up front (unless the caller is preserving one) so it
					// can be returned to the model — start_plan_item needs the id, and
					// it is otherwise invisible until a get_objective round-trip.
					const cleaned: CleanItem = { title, id: it.id?.trim() || randomUUID() };
					const rationale = cleanOptional(it.rationale, RATIONALE_MAX);
					if (rationale !== undefined) cleaned.rationale = rationale;
					items.push(cleaned);
				}
				const rationale = cleanOptional(params.rationale, RATIONALE_MAX);
				emit({
					type: "event",
					agentId,
					event: {
						type: "plan_set",
						items,
						rationale,
					},
				});
				return {
					content: [
						{
							type: "text",
							text:
								items.length === 0
									? "Plan cleared."
									: `Plan set (${items.length} item${items.length === 1 ? "" : "s"}):\n` +
										items.map((it) => `- ${it.title} — id=${it.id}`).join("\n") +
										"\nCall start_plan_item(id) with the id shown to begin an item.",
						},
					],
					details: { itemCount: items.length, rationale },
				};
			},
		}),
		defineTool({
			name: "start_plan_item",
			label: "Start Plan Item",
			description: "Mark a plan item active. Any sibling currently active is silently reset.",
			promptSnippet:
				"start_plan_item(item_id) - declare which plan item you are now working on.",
			promptGuidelines: [
				"Call start_plan_item right before doing the work for an item. The active item gets stamped onto your subsequent coherent actions automatically.",
				"At most one item is active per objective. Starting a new item resets the previous active one to pending.",
				"There is no auto-advance on completion — after complete_plan_item, call start_plan_item again on the next item.",
			],
			parameters: Type.Object({
				item_id: Type.String({
					description: "The id of the plan item to mark active.",
				}),
			}),
			async execute(_toolCallId, params) {
				const itemId = params.item_id.trim();
				if (!itemId) {
					return {
						content: [
							{ type: "text", text: "start_plan_item requires a non-empty item_id." },
						],
						details: { itemId: "", error: "missing_item_id" },
					};
				}
				emit({
					type: "event",
					agentId,
					event: {
						type: "plan_item_start",
						itemId,
					},
				});
				return {
					content: [{ type: "text", text: "Plan item is now active." }],
					details: { itemId, error: "" },
				};
			},
		}),
		defineTool({
			name: "complete_plan_item",
			label: "Complete Plan Item",
			description:
				"Close the currently active plan item with an optional one-sentence outcome.",
			promptSnippet:
				"complete_plan_item(outcome?) - close the active plan item; the next item must be started explicitly.",
			promptGuidelines: [
				"Use complete_plan_item when the active item is finished. Outcome is optional but recommended when the result is non-obvious.",
				"After completing, call start_plan_item to move to the next step. There is no auto-advance.",
			],
			parameters: Type.Object({
				outcome: Type.Optional(
					Type.String({
						description: "Optional one-sentence outcome of the completed item.",
					}),
				),
			}),
			async execute(_toolCallId, params) {
				const outcome = cleanOptional(params.outcome, OUTCOME_MAX);
				emit({
					type: "event",
					agentId,
					event: {
						type: "plan_item_complete",
						outcome,
					},
				});
				return {
					content: [{ type: "text", text: "Plan item completed." }],
					details: { outcome },
				};
			},
		}),
		defineTool({
			name: "skip_plan_item",
			label: "Skip Plan Item",
			description:
				"Mark a plan item skipped. Use when the item is no longer needed (not when it failed — use block_plan_item for that).",
			promptSnippet:
				"skip_plan_item(item_id?, reason?) - mark a plan item skipped; targets the active item when item_id is omitted.",
			promptGuidelines: [
				"Skipping is for items the work no longer needs (e.g. a follow-up that turned out to be unnecessary).",
				"Use block_plan_item, not skip, when an item is stuck on something external.",
				"Do not silently abandon items by leaving them pending — mark them skipped or blocked with a brief reason.",
			],
			parameters: Type.Object({
				item_id: Type.Optional(
					Type.String({
						description:
							"Optional plan item id. Omit to skip the currently active item.",
					}),
				),
				reason: Type.Optional(
					Type.String({
						description: "Optional brief reason the item is being skipped.",
					}),
				),
			}),
			async execute(_toolCallId, params) {
				const itemId = cleanOptional(params.item_id, 200);
				const reason = cleanOptional(params.reason, REASON_MAX);
				emit({
					type: "event",
					agentId,
					event: {
						type: "plan_item_skip",
						itemId,
						reason,
					},
				});
				return {
					content: [{ type: "text", text: "Plan item skipped." }],
					details: { itemId, reason },
				};
			},
		}),
		defineTool({
			name: "block_plan_item",
			label: "Block Plan Item",
			description:
				"Mark a plan item blocked on something external. Reason is required so the captain knows what is needed.",
			promptSnippet:
				"block_plan_item(reason, item_id?) - mark a plan item blocked; reason is required.",
			promptGuidelines: [
				"Use block_plan_item when an item cannot proceed without external input or action — captain decision, environment fix, upstream dependency.",
				"State the blocker explicitly so the captain can resolve it without rereading the timeline.",
			],
			parameters: Type.Object({
				reason: Type.String({
					description: "What the item is blocked on. Required.",
				}),
				item_id: Type.Optional(
					Type.String({
						description:
							"Optional plan item id. Omit to block the currently active item.",
					}),
				),
			}),
			async execute(_toolCallId, params) {
				const reason = trimTo(params.reason ?? "", REASON_MAX);
				if (!reason) {
					return {
						content: [
							{ type: "text", text: "block_plan_item requires a non-empty reason." },
						],
						details: { itemId: undefined, reason: "", error: "missing_reason" },
					};
				}
				const itemId = cleanOptional(params.item_id, 200);
				emit({
					type: "event",
					agentId,
					event: {
						type: "plan_item_block",
						itemId,
						reason,
					},
				});
				return {
					content: [{ type: "text", text: "Plan item blocked." }],
					details: { itemId, reason, error: "" },
				};
			},
		}),
	] as const;
}
