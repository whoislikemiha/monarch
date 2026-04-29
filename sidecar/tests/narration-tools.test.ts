import { describe, expect, it } from "vitest";
import { createNarrationTools } from "../src/narration-tools";

function toolByName(name: string) {
	const emitted: Record<string, unknown>[] = [];
	const tools = createNarrationTools("agent-1", (msg) => emitted.push(msg));
	const tool = tools.find((candidate) => candidate.name === name);
	if (!tool) throw new Error(`Missing tool ${name}`);
	return { emitted, tool: tool as any };
}

describe("narration tools", () => {
	it("set_current_action emits an action_transition event", async () => {
		const { emitted, tool } = toolByName("set_current_action");

		const result = await tool.execute("tc-1", {
			intent: " Inspect the failing auth flow ",
			previous_outcome: " Found the test entry point. ",
		});

		expect(emitted).toEqual([
			{
				type: "event",
				agentId: "agent-1",
				event: {
					type: "action_transition",
					intent: "Inspect the failing auth flow",
					previous_outcome: "Found the test entry point.",
				},
			},
		]);
		expect(result.details).toEqual({
			intent: "Inspect the failing auth flow",
			previous_outcome: "Found the test entry point.",
		});
	});

	it("complete_action emits an action_complete event", async () => {
		const { emitted, tool } = toolByName("complete_action");

		await tool.execute("tc-2", {
			outcome: " Added focused coverage for session restore. ",
		});

		expect(emitted[0]).toEqual({
			type: "event",
			agentId: "agent-1",
			event: {
				type: "action_complete",
				outcome: "Added focused coverage for session restore.",
			},
		});
	});

	it("record_decision omits blank rationale", async () => {
		const { emitted, tool } = toolByName("record_decision");

		const result = await tool.execute("tc-3", {
			decision: " Use the existing persistence queue. ",
			rationale: "   ",
		});

		expect(emitted[0]).toEqual({
			type: "event",
			agentId: "agent-1",
			event: {
				type: "executor_decision",
				decision: "Use the existing persistence queue.",
				rationale: undefined,
			},
		});
		expect(result.details).toEqual({
			decision: "Use the existing persistence queue.",
			rationale: undefined,
		});
	});
});
