import { describe, expect, it } from "vitest";
import { parseObjectiveReport, type ObjectiveReportView } from "./objectiveStore.svelte";
import type { ObjectiveReportRow } from "../bindings";

/**
 * P6 Slice C (MON-121): the parser is the load-bearing piece — if a payload
 * shape drifts the captain-facing report silently empties. These cover the
 * wire shape the executor emits today plus the two degraded paths the parser
 * must survive (malformed JSON, missing fields).
 */

const baseRow = (payload: string): ObjectiveReportRow => ({
  id: "r1",
  objectiveId: "q1",
  agentId: "a1",
  payload,
  createdAt: "2026-05-22T00:00:00Z",
  updatedAt: "2026-05-22T00:05:00Z",
  distilledByKeeperRunId: null,
});

describe("parseObjectiveReport", () => {
  it("returns the structured payload verbatim", () => {
    const payload = {
      summary: "shipped the auth fix",
      outcome: "done",
      decisions: [{ decision: "used a parallel call", rationale: "latency" }],
      learned: ["sidecar event ordering is not guaranteed"],
      artifacts: [{ file: "src/auth.rs", role: "modified" }],
      open_threads: ["follow up on session refresh"],
      reflection: "tight loop today",
      grade: "A",
    };
    const r = parseObjectiveReport(baseRow(JSON.stringify(payload)));
    expect(r.summary).toBe(payload.summary);
    expect(r.outcome).toBe("done");
    expect(r.decisions).toEqual(payload.decisions);
    expect(r.learned).toEqual(payload.learned);
    expect(r.artifacts).toEqual(payload.artifacts);
    expect(r.open_threads).toEqual(payload.open_threads);
    expect(r.reflection).toBe(payload.reflection);
    expect(r.grade).toBe("A");
    expect(r.raw).toBeUndefined();
  });

  it("supplies empty defaults when fields are absent", () => {
    const r = parseObjectiveReport(baseRow(JSON.stringify({ summary: "only this" })));
    expect(r.summary).toBe("only this");
    expect(r.outcome).toBe("");
    expect(r.decisions).toEqual([]);
    expect(r.learned).toEqual([]);
    expect(r.artifacts).toEqual([]);
    expect(r.open_threads).toEqual([]);
    expect(r.reflection).toBe("");
    expect(r.grade).toBe("");
    expect(r.raw).toBeUndefined();
  });

  it("falls back to raw payload on malformed JSON without throwing", () => {
    const row = baseRow("not json");
    let r!: ObjectiveReportView;
    expect(() => {
      r = parseObjectiveReport(row);
    }).not.toThrow();
    expect(r.raw).toBe("not json");
    // Defaults still populated so the renderer can branch on raw cleanly.
    expect(r.summary).toBe("");
    expect(r.decisions).toEqual([]);
  });

  it("ignores non-array list fields rather than crashing", () => {
    const row = baseRow(
      JSON.stringify({ decisions: "oops", learned: null, artifacts: 5 }),
    );
    const r = parseObjectiveReport(row);
    expect(r.decisions).toEqual([]);
    expect(r.learned).toEqual([]);
    expect(r.artifacts).toEqual([]);
    expect(r.raw).toBeUndefined();
  });

  it("carries row metadata through to the view", () => {
    const row = {
      ...baseRow(JSON.stringify({ summary: "" })),
      distilledByKeeperRunId: 42,
      updatedAt: "2026-05-22T01:00:00Z",
    };
    const r = parseObjectiveReport(row);
    expect(r.distilledByKeeperRunId).toBe(42);
    expect(r.updatedAt).toBe("2026-05-22T01:00:00Z");
  });
});
