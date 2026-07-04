import type { AgentSession } from "@mariozechner/pi-coding-agent";

/**
 * MON-130: deterministic narration cadence.
 *
 * The system prompt asks the agent to group work into coherent actions via
 * `set_current_action`, but nothing enforces it — in the first field run the
 * agent narrated once and then ran 30 tool calls under a single never-closed
 * action. This module composes onto Pi's `afterToolCall` hook and appends a
 * `<system-reminder>` text block to tool results after a run of consecutive
 * non-meta tool calls with no narration, so the model is re-anchored to the
 * narration contract exactly when it is drifting — without polluting the
 * chat (tool results are not rendered as dialogue).
 */

/** Narration/plan meta tools — never counted, never nudged. Mirrors the
 * frontend's META_TOOLS in `src/lib/workspace/timelineModel.ts`. */
const META_TOOLS = new Set([
	"set_current_action",
	"complete_action",
	"record_decision",
	"set_plan",
	"update_plan",
	"start_plan_item",
	"complete_plan_item",
	"skip_plan_item",
	"block_plan_item",
	"complete_objective",
	"suggest_memory",
]);

/** Consecutive un-narrated tool calls before the first nudge fires. */
const NUDGE_AFTER = 6;
/** After the first nudge, re-nudge every N further calls (not every call —
 * a reminder on every result reads as noise and gets ignored). */
const NUDGE_EVERY = 5;

type AfterToolCall = NonNullable<AgentSession["agent"]["afterToolCall"]>;
type AfterToolCallResult = Awaited<ReturnType<AfterToolCall>>;

function reminderText(count: number, intent: string | null): string {
	const body = intent
		? `You have made ${count} tool calls since declaring the action "${intent}". ` +
			`If the work has moved to a different chunk, call set_current_action with the new intent ` +
			`(pass previous_outcome to close this one). If this chunk just finished, call complete_action ` +
			`with its outcome. If "${intent}" still accurately describes what you are doing right now, ` +
			`keep going — no call needed.`
		: `You have made ${count} tool calls with no current action declared. Call set_current_action ` +
			`with one short sentence describing this chunk of work so the timeline groups it for the supervisor.`;
	return `<system-reminder>${body}</system-reminder>`;
}

/**
 * Wrap the session's `afterToolCall` hook (already installed by Pi for
 * extensions; Monarch runs with extensions disabled so the base hook is a
 * fast no-op). Install once per session, right after creation — Pi installs
 * its own hooks in the AgentSession constructor and never reinstalls them.
 */
export function installNarrationNudge(session: AgentSession): void {
	let sinceNarration = 0;
	let currentIntent: string | null = null;

	const base = session.agent.afterToolCall;
	const wrapped: AfterToolCall = async (ctx, signal) => {
		const baseResult: AfterToolCallResult = base ? await base(ctx, signal) : undefined;
		const name = ctx.toolCall.name;

		if (META_TOOLS.has(name)) {
			if (name === "set_current_action") {
				const args = ctx.args as { intent?: string } | undefined;
				currentIntent = args?.intent?.trim() || null;
				sinceNarration = 0;
			} else if (name === "complete_action") {
				currentIntent = null;
				sinceNarration = 0;
			}
			return baseResult;
		}

		sinceNarration += 1;
		const overdue = sinceNarration - NUDGE_AFTER;
		if (overdue < 0 || overdue % NUDGE_EVERY !== 0) {
			return baseResult;
		}

		const content = [
			...(baseResult?.content ?? ctx.result.content),
			{ type: "text" as const, text: reminderText(sinceNarration, currentIntent) },
		];
		return { ...(baseResult ?? {}), content };
	};
	session.agent.afterToolCall = wrapped;
}
