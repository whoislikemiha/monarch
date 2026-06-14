/**
 * MON-129 — agent-authored objective tree: authoring + navigation tools.
 *
 * The agent owns its work structure. Writes (create / activate / update) are
 * fire-and-forget events persisted Rust-side via the objective_* persist
 * appliers; `create_objective` mints the id here so the model gets it back
 * with no round-trip. Reads (get_tree / get_objective) go through the
 * objective-query bridge — Rust formats the snapshot/detail and answers.
 *
 * Field names on emitted events are camelCase to match the Rust
 * `rename_all_fields = "camelCase"` inner-event decoding.
 */

import { defineTool } from "@mariozechner/pi-coding-agent";
import { Type } from "@mariozechner/pi-ai";
import { randomUUID } from "node:crypto";
import type { EmitFn } from "./ui-bridge.js";

/** Bridge to Rust for a navigation read; resolves to formatted text. */
export type ObjectiveQueryFn = (
	kind: "tree" | "detail",
	objectiveId?: string,
) => Promise<string>;

export function createObjectiveTools(
	agentId: string,
	emit: EmitFn,
	query: ObjectiveQueryFn,
) {
	return [
		defineTool({
			name: "create_objective",
			label: "Create Objective",
			description:
				"Author a new objective on the work tree. Omit parentId for a top-level objective; set it to nest a sub-objective. Auto-activates (becomes your current objective) unless activate is false. Returns the new objective id.",
			promptSnippet:
				"create_objective(title, description?, parentId?, direction?, activate?) - author an objective; auto-activates; returns its id.",
			promptGuidelines: [
				"Create an objective when you and the captain have agreed on a concrete piece of work — not for chit-chat or trivial single-step tasks (those stay on the scratch objective).",
				"Prefer ONE objective with a flat plan (set_plan) over deep trees. Only nest a sub-objective when it is independently completable and reportable on its own.",
				"Pass activate=false to author a sub-objective for later without moving your focus off the current one.",
			],
			parameters: Type.Object({
				title: Type.String({
					description: "Short, action-shaped objective title.",
				}),
				description: Type.Optional(
					Type.String({ description: "Optional fuller description of the work." }),
				),
				parentId: Type.Optional(
					Type.String({
						description: "Parent objective id to nest under; omit for a top-level objective.",
					}),
				),
				direction: Type.Optional(
					Type.String({ description: "Optional one-line approach / direction." }),
				),
				activate: Type.Optional(
					Type.Boolean({ description: "Make this your current objective (default true)." }),
				),
			}),
			async execute(_toolCallId, params) {
				const id = randomUUID();
				const activate = params.activate ?? true;
				emit({
					type: "event",
					agentId,
					event: {
						type: "objective_created",
						id,
						title: params.title,
						description: params.description ?? null,
						parentId: params.parentId ?? null,
						direction: params.direction ?? null,
						activate,
					},
				});
				return {
					content: [
						{
							type: "text" as const,
							text: `Created objective "${params.title}"${
								activate ? " (now current)" : ""
							}. id=${id}`,
						},
					],
					details: { id, activate },
				};
			},
		}),
		defineTool({
			name: "activate_objective",
			label: "Activate Objective",
			description:
				"Move your focus to an existing objective. Subsequent work, narration, and plan updates attach to it.",
			promptSnippet:
				"activate_objective(objectiveId) - set your current objective to an existing one (find ids with get_tree).",
			parameters: Type.Object({
				objectiveId: Type.String({
					description: "The objective id to focus on (from get_tree).",
				}),
			}),
			async execute(_toolCallId, params) {
				emit({
					type: "event",
					agentId,
					event: { type: "objective_activated", objectiveId: params.objectiveId },
				});
				return {
					content: [
						{ type: "text" as const, text: `Current objective set to ${params.objectiveId}.` },
					],
					details: {},
				};
			},
		}),
		defineTool({
			name: "update_objective",
			label: "Update Objective",
			description:
				"Update an objective's direction, scope, or status. Targets your current objective unless objectiveId is given. Use status to mark work done/abandoned or to re-scope (superseded).",
			promptSnippet:
				"update_objective(objectiveId?, direction?, scope?, status?) - patch the current (or named) objective.",
			promptGuidelines: [
				"status is one of: pending | in_progress | done | abandoned | superseded.",
				"To close finished work, prefer complete_objective (it also writes a report); use status='done' only for a bare close.",
			],
			parameters: Type.Object({
				objectiveId: Type.Optional(
					Type.String({ description: "Target objective; omit for your current one." }),
				),
				direction: Type.Optional(
					Type.String({ description: "New approach / direction." }),
				),
				scope: Type.Optional(Type.String({ description: "New scope statement." })),
				status: Type.Optional(
					Type.String({
						description: "pending | in_progress | done | abandoned | superseded.",
					}),
				),
			}),
			async execute(_toolCallId, params) {
				emit({
					type: "event",
					agentId,
					event: {
						type: "objective_updated",
						objectiveId: params.objectiveId ?? null,
						direction: params.direction ?? null,
						scope: params.scope ?? null,
						status: params.status ?? null,
					},
				});
				return {
					content: [{ type: "text" as const, text: "Objective updated." }],
					details: {},
				};
			},
		}),
		defineTool({
			name: "get_tree",
			label: "Get Objective Tree",
			description:
				"Read a cheap snapshot of your active objectives (title, status, id), indented by nesting, with your current objective marked. Use it to orient before planning, or to find an objective to activate or drill into.",
			promptSnippet: "get_tree() - snapshot of active objectives; drill in with get_objective(id).",
			parameters: Type.Object({}),
			async execute(_toolCallId, _params) {
				const text = await query("tree");
				return { content: [{ type: "text" as const, text }], details: {} };
			},
		}),
		defineTool({
			name: "get_objective",
			label: "Get Objective",
			description:
				"Read full detail for one objective: description, direction, plan items and their status, recent events, artifacts, and report.",
			promptSnippet: "get_objective(objectiveId) - full detail for one objective.",
			parameters: Type.Object({
				objectiveId: Type.String({ description: "The objective id (from get_tree)." }),
			}),
			async execute(_toolCallId, params) {
				const text = await query("detail", params.objectiveId);
				return { content: [{ type: "text" as const, text }], details: {} };
			},
		}),
	];
}
