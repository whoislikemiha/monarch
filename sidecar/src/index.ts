/**
 * Monarch Sidecar — Entry point
 *
 * Long-lived Node process that hosts Pi SDK agent sessions.
 * Communicates with Rust backend via stdin/stdout JSONL.
 * stderr is used for sidecar diagnostics only.
 */

import { createInterface } from "node:readline";
import { RuntimeManager } from "./runtime-manager.js";
import type { SidecarCommand } from "./protocol.js";

function emit(msg: Record<string, unknown>): void {
	process.stdout.write(JSON.stringify(msg) + "\n");
}

const manager = new RuntimeManager(emit);

const rl = createInterface({
	input: process.stdin,
	terminal: false,
});

rl.on("line", async (line: string) => {
	if (!line.trim()) return;

	let cmd: SidecarCommand;
	try {
		cmd = JSON.parse(line) as SidecarCommand;
	} catch {
		process.stderr.write(`[sidecar] Invalid JSON: ${line}\n`);
		return;
	}

	try {
		await handleCommand(cmd);
	} catch (err) {
		const agentId = "agentId" in cmd ? (cmd.agentId as string) : "unknown";
		emit({
			type: "error",
			agentId,
			error: `Command ${cmd.type} failed: ${err instanceof Error ? err.message : String(err)}`,
		});
	}
});

async function handleCommand(cmd: SidecarCommand): Promise<void> {
	switch (cmd.type) {
		case "create_session":
			await manager.createSession(cmd);
			break;
		case "destroy_session":
			await manager.destroySession(cmd.agentId);
			break;
		case "prompt":
			await manager.prompt(cmd.agentId, cmd.message, cmd.classifier);
			break;
		case "abort":
			await manager.abort(cmd.agentId);
			break;
		case "set_model":
			await manager.setModel(cmd.agentId, cmd.provider, cmd.modelId, cmd.contextWindow);
			break;
		case "set_thinking_level":
			manager.setThinkingLevel(cmd.agentId, cmd.level);
			break;
		case "new_session":
			await manager.newSession(cmd.agentId);
			break;
		case "compact":
			await manager.compact(cmd.agentId);
			break;
		case "load_session":
			manager.loadSession(cmd.agentId, cmd.messages);
			break;
		case "extension_ui_response":
			manager.handleUIResponse(cmd.agentId, cmd.requestId, cmd.value);
			break;
		case "set_custom_prompt":
			manager.setCustomPrompt(cmd.agentId, cmd.prompt, cmd.projectInstructions, cmd.captainIdentityPayload, cmd.shadowIdentityPayload);
			break;
		case "keeper_run":
			// MON-100: don't await — the Keeper round trip can take seconds and
			// the readline pump must stay free for in-flight prompts/aborts.
			void manager.keeperRun(cmd);
			break;
		default:
			process.stderr.write(
				`[sidecar] Unknown command type: ${(cmd as any).type}\n`,
			);
	}
}

// Graceful shutdown
async function shutdown(): Promise<void> {
	process.stderr.write("[sidecar] Shutting down...\n");
	await manager.disposeAll();
	process.exit(0);
}

process.on("SIGTERM", shutdown);
process.on("SIGINT", shutdown);

// Handle stdin close (Rust process died)
rl.on("close", shutdown);

process.stderr.write("[sidecar] Ready\n");
