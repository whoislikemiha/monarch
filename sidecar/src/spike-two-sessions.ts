/**
 * MON-128 Slice A — Pi-concurrency spike.
 *
 * Two AgentSessions for one agent in one Node process, built through the SAME
 * path runtime-manager.ts uses (DefaultResourceLoader + createAgentSession +
 * SessionManager.inMemory + customTools), so the findings transfer.
 *
 * Probes:
 *   1. concurrency      — two sessions stream at the same time; events stay per-session.
 *   2. abort isolation  — aborting one mid-stream leaves the other untouched.
 *   3. pause gate       — an extensionFactories `tool_call` handler parks the
 *                         executor at a tool boundary and resume releases it.
 *   4. abort while parked — abort() during a parked tool call ends the turn cleanly.
 *
 * Run:  npm run build && node dist/spike-two-sessions.js [provider] [model]
 * Defaults to anthropic / claude-haiku-4-5-20251001. Uses whatever auth the
 * production sidecar would use; if the model can't be set, the spike exits 2.
 *
 * Not wired into the app. Delete or keep as a regression probe once P3 ships.
 */

import {
	createAgentSession,
	DefaultResourceLoader,
	SessionManager,
	defineTool,
	type AgentSession,
} from "@mariozechner/pi-coding-agent";
import { Type } from "@mariozechner/pi-ai";
import { resolveModel } from "./model-resolver.js";

const provider = process.argv[2] ?? "anthropic";
const modelId = process.argv[3] ?? "claude-haiku-4-5-20251001";
const cwd = process.cwd();

interface TaggedEvent {
	role: string;
	type: string;
	at: number;
}

const eventLog: TaggedEvent[] = [];
const failures: string[] = [];

function check(probe: string, ok: boolean, detail: string): void {
	const mark = ok ? "PASS" : "FAIL";
	console.log(`  [${mark}] ${detail}`);
	if (!ok) failures.push(`${probe}: ${detail}`);
}

function createMarkerTool(role: string, calls: number[]) {
	return defineTool({
		name: `spike_marker_${role}`,
		label: `Spike marker (${role})`,
		description: `Test marker tool for the ${role} session. Records the call and returns ok.`,
		parameters: Type.Object({
			note: Type.String({ description: "Any short string." }),
		}),
		async execute(_toolCallId, _params) {
			calls.push(Date.now());
			return {
				content: [{ type: "text" as const, text: "ok" }],
				details: {},
			};
		},
	});
}

interface GateControls {
	pause(): void;
	resume(): void;
	parkedCount(): number;
	toolCallsSeen(): number;
}

/** The P3 pause-gate candidate: a programmatic extension whose tool_call
 * handler parks while paused. Exactly what Slice E would ship. */
function createPauseGate(): { factory: (pi: any) => void; controls: GateControls } {
	let paused = false;
	let parked = 0;
	let seen = 0;
	let releases: Array<() => void> = [];

	const factory = (pi: any) => {
		pi.on("tool_call", async (_event: unknown) => {
			seen += 1;
			if (paused) {
				parked += 1;
				await new Promise<void>((resolve) => releases.push(resolve));
			}
			return undefined;
		});
	};

	return {
		factory,
		controls: {
			pause: () => {
				paused = true;
			},
			resume: () => {
				paused = false;
				for (const release of releases) release();
				releases = [];
			},
			parkedCount: () => parked,
			toolCallsSeen: () => seen,
		},
	};
}

async function makeSession(
	role: string,
	markerCalls: number[],
	gateFactory?: (pi: any) => void,
): Promise<AgentSession> {
	const promptRef = {
		current:
			"You are a spike probe. Follow instructions literally and tersely. Never ask questions.",
	};
	const resourceLoader = new DefaultResourceLoader({
		cwd,
		agentDir: cwd,
		systemPromptOverride: () => promptRef.current,
		noExtensions: true,
		noSkills: true,
		noPromptTemplates: true,
		noThemes: true,
		...(gateFactory ? { extensionFactories: [gateFactory] } : {}),
	});
	await resourceLoader.reload();

	const { session } = await createAgentSession({
		cwd,
		thinkingLevel: "off",
		sessionManager: SessionManager.inMemory(cwd),
		resourceLoader,
		customTools: [createMarkerTool(role, markerCalls)],
	});

	const model = resolveModel(session, provider, modelId, null);
	if (!model) {
		console.error(`Model not found in registry: ${provider}/${modelId}`);
		process.exit(2);
	}
	await session.setModel(model);

	session.subscribe((event) => {
		eventLog.push({ role, type: event.type, at: Date.now() });
	});

	return session;
}

function lastAssistantText(session: AgentSession): string {
	const messages = session.messages;
	for (let i = messages.length - 1; i >= 0; i--) {
		const message = messages[i] as { role?: string; content?: unknown };
		if (message.role !== "assistant") continue;
		const content = message.content;
		if (typeof content === "string") return content;
		if (Array.isArray(content)) {
			return content
				.filter((block: any) => block.type === "text")
				.map((block: any) => block.text)
				.join("");
		}
	}
	return "";
}

function eventWindow(role: string): { first: number; last: number } {
	const events = eventLog.filter((entry) => entry.role === role);
	return {
		first: events.length ? events[0].at : 0,
		last: events.length ? events[events.length - 1].at : 0,
	};
}

async function main(): Promise<void> {
	console.log(`spike: two sessions, one process — ${provider}/${modelId}\n`);

	// ── Probe 1: concurrency ────────────────────────────────────────────────
	console.log("Probe 1 — concurrent streaming, per-session event routing");
	const callsA: number[] = [];
	const callsB: number[] = [];
	const sessionA = await makeSession("A", callsA);
	const sessionB = await makeSession("B", callsB);

	await Promise.all([
		sessionA.prompt("Reply with exactly: ALPHA-DONE"),
		sessionB.prompt("Reply with exactly: BETA-DONE"),
	]);

	const textA = lastAssistantText(sessionA);
	const textB = lastAssistantText(sessionB);
	check("concurrency", textA.includes("ALPHA-DONE"), `session A answered its own prompt (got: ${JSON.stringify(textA.slice(0, 40))})`);
	check("concurrency", textB.includes("BETA-DONE"), `session B answered its own prompt (got: ${JSON.stringify(textB.slice(0, 40))})`);
	check(
		"concurrency",
		!textA.includes("BETA") && !textB.includes("ALPHA"),
		"no cross-session bleed in assistant output",
	);
	const windowA = eventWindow("A");
	const windowB = eventWindow("B");
	const overlapped = windowA.first < windowB.last && windowB.first < windowA.last;
	check("concurrency", overlapped, `event windows overlapped (A ${windowA.first}–${windowA.last}, B ${windowB.first}–${windowB.last})`);

	// ── Probe 2: abort isolation ────────────────────────────────────────────
	console.log("\nProbe 2 — abort one session mid-stream; the other finishes");
	eventLog.length = 0;
	const longPrompt = sessionA.prompt(
		"Count from 1 to 200, one number per line. Do not stop early.",
	);
	// Give A time to start streaming, then abort it while B runs to completion.
	await new Promise((resolve) => setTimeout(resolve, 1500));
	const shortPrompt = sessionB.prompt("Reply with exactly: STILL-ALIVE");
	await sessionA.abort();
	await Promise.allSettled([longPrompt]);
	await shortPrompt;

	check("abort", !sessionA.isStreaming, "session A is no longer streaming after abort");
	check("abort", lastAssistantText(sessionB).includes("STILL-ALIVE"), "session B completed normally despite A's abort");

	await sessionA.prompt("Reply with exactly: RECOVERED");
	check("abort", lastAssistantText(sessionA).includes("RECOVERED"), "session A accepts a new prompt after abort");

	// ── Probe 3: pause gate (park at tool boundary, resume releases) ────────
	console.log("\nProbe 3 — pause gate parks at a tool boundary; resume releases");
	const callsC: number[] = [];
	const gate = createPauseGate();
	const sessionC = await makeSession("C", callsC, gate.factory);

	const PARK_MS = 3000;
	const gatedPrompt = sessionC.prompt(
		"Call the spike_marker_C tool exactly 3 times (sequentially, note='x'), then reply with exactly: GATED-DONE",
	);
	// Engage the pause once the first marker call has landed.
	while (callsC.length < 1) await new Promise((resolve) => setTimeout(resolve, 50));
	gate.controls.pause();
	const pausedAt = Date.now();
	await new Promise((resolve) => setTimeout(resolve, PARK_MS));
	const callsDuringPark = callsC.filter((at) => at > pausedAt).length;
	gate.controls.resume();
	await gatedPrompt;

	check("gate", callsDuringPark === 0, `no marker tool executed while parked (${callsDuringPark} during park)`);
	check("gate", gate.controls.parkedCount() >= 1, `at least one tool call parked (${gate.controls.parkedCount()})`);
	check("gate", callsC.length === 3, `all 3 marker calls completed after resume (${callsC.length})`);
	check("gate", lastAssistantText(sessionC).includes("GATED-DONE"), "gated turn completed normally after resume");

	// ── Probe 4: abort while parked ─────────────────────────────────────────
	// Known sharp edge: AgentSession's beforeToolCall bridge does not pass the
	// abort signal into extension tool_call handlers, so a parked handler
	// blocks turn settlement — abort() alone hangs. The mitigation (what
	// stop_executor must do) is: release the gate, THEN abort.
	console.log("\nProbe 4 — abort while parked: abort() alone hangs; release-then-abort settles");
	const callsD: number[] = [];
	const gateD = createPauseGate();
	const sessionD = await makeSession("D", callsD, gateD.factory);

	gateD.controls.pause();
	const parkedPrompt = sessionD.prompt(
		"Call the spike_marker_D tool once (note='x'), then reply with exactly: NEVER",
	);
	while (gateD.controls.parkedCount() < 1) await new Promise((resolve) => setTimeout(resolve, 50));

	const abortPromise = sessionD.abort();
	const abortAloneSettled = await Promise.race([
		abortPromise.then(() => true),
		new Promise<boolean>((resolve) => setTimeout(() => resolve(false), 3000)),
	]);
	check(
		"abort-parked",
		!abortAloneSettled,
		"confirmed: abort() alone does NOT settle while a tool call is parked (gate must release first)",
	);

	gateD.controls.resume(); // the mitigation: release the gate
	const settledAfterRelease = await Promise.race([
		Promise.allSettled([abortPromise, parkedPrompt]).then(() => true),
		new Promise<boolean>((resolve) => setTimeout(() => resolve(false), 10_000)),
	]);
	check("abort-parked", settledAfterRelease, "release-then-abort settles cleanly (no hang)");
	check("abort-parked", !sessionD.isStreaming, "session D not streaming after release-then-abort");

	// ── Verdict ─────────────────────────────────────────────────────────────
	console.log("\n────────────────────────────────────────");
	if (failures.length === 0) {
		console.log("VERDICT: GO — two sessions per agent are viable; pause gate works at the extension layer.");
	} else {
		console.log(`VERDICT: ${failures.length} failure(s):`);
		for (const failure of failures) console.log(`  - ${failure}`);
	}

	sessionA.dispose();
	sessionB.dispose();
	sessionC.dispose();
	sessionD.dispose();
	// exitCode (not process.exit) so piped stdout flushes — and so a session
	// holding the event loop open after dispose() shows up as a hang here.
	process.exitCode = failures.length === 0 ? 0 : 1;
}

main().catch((err) => {
	console.error("spike crashed:", err);
	process.exit(2);
});
