/**
 * MON-128 (P3) — executor pause gate.
 *
 * A programmatic Pi extension whose `tool_call` handler parks while the gate
 * is engaged. The executor halts at the next tool boundary (the in-flight
 * tool call completes; the next one waits) — exactly the pause semantics
 * flows.md specifies. Resume releases every parked call.
 *
 * SHARP EDGE (verified in the Slice A spike, `spike-two-sessions.ts`):
 * AgentSession's beforeToolCall bridge does NOT pass the abort signal into
 * extension handlers, so `session.abort()` alone hangs while a call is
 * parked. Every abort/destroy path on a gated session MUST call
 * `controls.release()` first — see RuntimeManager.stopExecutor and
 * destroyByKey.
 */

import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";

export interface PauseGateControls {
	/** Engage the gate: the next tool call (and all after it) parks. */
	pause(reason?: string): void;
	/** Disengage and release every parked call. */
	resume(): void;
	/** Release parked calls WITHOUT disengaging — the abort path. */
	release(): void;
	paused(): boolean;
	pauseReason(): string | undefined;
}

export function createPauseGate(): {
	factory: (pi: ExtensionAPI) => void;
	controls: PauseGateControls;
} {
	let paused = false;
	let reason: string | undefined;
	let releases: Array<() => void> = [];

	const releaseAll = () => {
		const pending = releases;
		releases = [];
		for (const release of pending) release();
	};

	const factory = (pi: ExtensionAPI) => {
		pi.on("tool_call", async () => {
			if (paused) {
				await new Promise<void>((resolve) => releases.push(resolve));
			}
			return undefined;
		});
	};

	return {
		factory,
		controls: {
			pause: (r?: string) => {
				paused = true;
				reason = r;
			},
			resume: () => {
				paused = false;
				reason = undefined;
				releaseAll();
			},
			release: releaseAll,
			paused: () => paused,
			pauseReason: () => reason,
		},
	};
}
