/**
 * MON-124: view-model helpers for the execution timeline. The feed delivers
 * raw `objective_events` rows (top-level entries + their nested children);
 * these helpers parse the per-type JSON payloads and group rows into the
 * shapes the timeline components render. Pure functions — no state here.
 */
import type { ObjectiveEventRow, ObjectiveRow } from "$lib/bindings";

/** Parsed `payload_json` of a `tool_call` event (start fields always present,
 * end fields merged in-place by the backend when the call resolves). */
export interface ToolCallView {
  eventId: string;
  toolCallId: string;
  toolName: string;
  /** MON-124: normalized path/command extracted at record time. */
  target: string | null;
  argsPreview: string;
  resultPreview: string | null;
  status: "running" | "done" | "error";
  isError: boolean;
  startedAt: string | null;
  completedAt: string | null;
  durationMs: number | null;
}

export interface DecisionView {
  eventId: string;
  decision: string;
  rationale: string | null;
  createdAt: string;
}

export interface ChatSpawnedView {
  eventId: string;
  scopeId: string;
  label: string;
  createdAt: string;
}

/** One coherent action assembled from its event row + nested children. */
export interface ActionView {
  eventId: string;
  objectiveId: string;
  intent: string;
  startedAt: string | null;
  /** From the `action_outcome` child, when the action has closed. */
  outcome: string | null;
  autoClosed: boolean;
  completedAt: string | null;
  toolCalls: ToolCallView[];
  decisions: DecisionView[];
  chatsSpawned: ChatSpawnedView[];
  /** Distinct file paths touched by mutating tools (edit/write). */
  filesTouched: string[];
  planItemId: string | null;
}

/** Payload of the "ask about this action" affordance on a timeline card. */
export interface AskPayload {
  id: string;
  intent: string;
  outcome?: string | null;
  objectiveId: string;
  /** True when a chat was already spawned from this action (don't re-record). */
  spawned: boolean;
}

export type TimelineItem =
  | { kind: "action"; event: ObjectiveEventRow; action: ActionView }
  | { kind: "milestone"; event: ObjectiveEventRow; payload: Record<string, unknown> };

/** A run of consecutive items on the same objective, newest-first. */
export interface TimelineSegment {
  objectiveId: string;
  objective: ObjectiveRow | null;
  items: TimelineItem[];
}

export function parsePayload(row: ObjectiveEventRow): Record<string, unknown> {
  if (!row.payloadJson) return {};
  try {
    const v = JSON.parse(row.payloadJson);
    return v && typeof v === "object" ? (v as Record<string, unknown>) : {};
  } catch {
    return {};
  }
}

const str = (v: unknown): string | null => (typeof v === "string" && v.length ? v : null);

/** Tools whose `target` is a file path that was (potentially) modified. */
const MUTATING_TOOLS = new Set(["write", "edit", "multi_edit", "apply_patch"]);

export function toToolCallView(row: ObjectiveEventRow): ToolCallView {
  const p = parsePayload(row);
  const status = p.status === "done" || p.status === "error" ? p.status : "running";
  return {
    eventId: row.id,
    toolCallId: str(p.tool_call_id) ?? row.id,
    toolName: str(p.tool_name) ?? "tool",
    target: str(p.target),
    argsPreview: str(p.args_preview) ?? "",
    resultPreview: str(p.result_preview),
    status,
    isError: p.is_error === true,
    startedAt: str(p.started_at),
    completedAt: str(p.completed_at),
    durationMs: typeof p.duration_ms === "number" ? p.duration_ms : null,
  };
}

export function buildActionView(
  entry: ObjectiveEventRow,
  children: ObjectiveEventRow[],
): ActionView {
  const p = parsePayload(entry);
  const action: ActionView = {
    eventId: entry.id,
    objectiveId: entry.objectiveId,
    intent: str(p.intent) ?? "(unnarrated work)",
    startedAt: str(p.started_at) ?? entry.createdAt,
    outcome: null,
    autoClosed: false,
    completedAt: null,
    toolCalls: [],
    decisions: [],
    chatsSpawned: [],
    filesTouched: [],
    planItemId: entry.planItemId,
  };
  for (const child of children) {
    const cp = parsePayload(child);
    switch (child.eventType) {
      case "action_outcome":
        action.outcome = str(cp.outcome);
        action.autoClosed = cp.auto_closed === true;
        action.completedAt = child.createdAt;
        break;
      case "tool_call":
        action.toolCalls.push(toToolCallView(child));
        break;
      case "executor_decision":
        action.decisions.push({
          eventId: child.id,
          decision: str(cp.decision) ?? "",
          rationale: str(cp.rationale),
          createdAt: child.createdAt,
        });
        break;
      case "chat_spawned":
        action.chatsSpawned.push({
          eventId: child.id,
          scopeId: str(cp.scope_id) ?? entry.id,
          label: str(cp.label) ?? "chat",
          createdAt: child.createdAt,
        });
        break;
      default:
        break;
    }
  }
  const files = new Set<string>();
  for (const tc of action.toolCalls) {
    if (tc.target && MUTATING_TOOLS.has(tc.toolName)) files.add(tc.target);
  }
  action.filesTouched = [...files];
  return action;
}

/**
 * Group the feed (top-level entries newest-first) into objective segments —
 * consecutive runs on the same objective. Splitting on transition (not a
 * global group-by) keeps interleaved work honest: A → B → A renders as three
 * segments, which is what actually happened.
 */
export function buildSegments(
  entries: ObjectiveEventRow[],
  childrenByParent: ReadonlyMap<string, ObjectiveEventRow[]>,
  objectivesById: ReadonlyMap<string, ObjectiveRow>,
): TimelineSegment[] {
  const segments: TimelineSegment[] = [];
  let current: TimelineSegment | null = null;
  for (const entry of entries) {
    if (!current || current.objectiveId !== entry.objectiveId) {
      current = {
        objectiveId: entry.objectiveId,
        objective: objectivesById.get(entry.objectiveId) ?? null,
        items: [],
      };
      segments.push(current);
    }
    if (entry.eventType === "coherent_action") {
      current.items.push({
        kind: "action",
        event: entry,
        action: buildActionView(entry, childrenByParent.get(entry.id) ?? []),
      });
    } else {
      current.items.push({ kind: "milestone", event: entry, payload: parsePayload(entry) });
    }
  }
  return segments;
}

/** Frontend mirror of the backend's target extraction, for live (not yet
 * persisted) tool executions where we hold the full args. */
export function extractClientTarget(toolName: string, args: unknown): string | null {
  if (!args || typeof args !== "object") return null;
  const o = args as Record<string, unknown>;
  const pick = (k: string) => str(o[k]);
  const path = pick("path") ?? pick("file_path") ?? pick("filePath") ?? pick("absolute_path");
  const raw = toolName === "bash" ? (pick("command") ?? path) : (path ?? pick("command") ?? pick("url"));
  if (!raw) return null;
  const compact = raw.split(/\s+/).join(" ");
  return compact.length <= 200 ? compact : `${compact.slice(0, 197)}...`;
}

/** Card id of the synthesized "unnarrated session work" fallback card. Chat
 * activity chips link here when their tool calls have no persisted record. */
export const FALLBACK_ACTION_ID = "__fallback__";

/** Minimal shape of a live tool execution (from liveAgentStore) we merge in. */
export interface LiveToolExecution {
  toolCallId: string;
  toolName: string;
  args: unknown;
  status: "running" | "done" | "error";
  startedAtMs?: number | null;
  durationMs?: number | null;
}

function liveToolToView(exec: LiveToolExecution): ToolCallView {
  return {
    eventId: `live:${exec.toolCallId}`,
    toolCallId: exec.toolCallId,
    toolName: exec.toolName,
    target: extractClientTarget(exec.toolName, exec.args),
    argsPreview: "",
    resultPreview: null,
    status: exec.status,
    isError: exec.status === "error",
    startedAt: exec.startedAtMs ? new Date(exec.startedAtMs).toISOString() : null,
    completedAt: null,
    durationMs: exec.durationMs ?? null,
  };
}

/** Every live execution as a view row, finished ones included — feeds the
 * unnarrated-work fallback card, where there is no persisted baseline. */
export function mergeAllLiveTools(live: Iterable<LiveToolExecution>): ToolCallView[] {
  return [...live]
    .sort((a, b) => (a.startedAtMs ?? 0) - (b.startedAtMs ?? 0))
    .map(liveToolToView);
}

/**
 * Overlay live executions onto an action's persisted tool calls: a live exec
 * matching a persisted `toolCallId` freshens its status/duration (the DB row
 * wins once the objective-event ping re-fetches it as done); unmatched RUNNING
 * execs append — they're this action's tools whose start event hasn't landed
 * yet. Finished unmatched execs are ignored: they belong to older actions.
 */
export function mergeLiveTools(
  persisted: ToolCallView[],
  live: Iterable<LiveToolExecution>,
): ToolCallView[] {
  const byId = new Map(persisted.map((t) => [t.toolCallId, t]));
  const merged = [...persisted];
  for (const exec of live) {
    const have = byId.get(exec.toolCallId);
    if (have) {
      if (have.status === "running" && exec.status !== "running") {
        const i = merged.indexOf(have);
        merged[i] = {
          ...have,
          status: exec.status,
          isError: exec.status === "error",
          durationMs: exec.durationMs ?? have.durationMs,
        };
      }
    } else if (exec.status === "running") {
      merged.push(liveToolToView(exec));
    }
  }
  return merged;
}

/** Compact relative time ("8s", "4m", "2h", "3d"). */
export function relTime(iso: string | null | undefined, nowMs?: number): string | null {
  if (!iso) return null;
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return null;
  const s = Math.max(0, Math.floor(((nowMs ?? Date.now()) - t) / 1000));
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h`;
  return `${Math.floor(s / 86400)}d`;
}

/** Live elapsed clock ("0:42", "12:05", "1:03:09") for the active action. */
export function elapsedClock(iso: string | null | undefined, nowMs: number): string | null {
  if (!iso) return null;
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return null;
  const s = Math.max(0, Math.floor((nowMs - t) / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const mm = h > 0 ? String(m).padStart(2, "0") : String(m);
  return `${h > 0 ? `${h}:` : ""}${mm}:${String(sec).padStart(2, "0")}`;
}

/** Compact duration for finished tool calls ("0.3s", "4.1s", "2m 10s"). */
export function fmtDuration(ms: number | null | undefined): string | null {
  if (ms == null || ms < 0) return null;
  if (ms < 10_000) return `${(ms / 1000).toFixed(1)}s`;
  if (ms < 120_000) return `${Math.round(ms / 1000)}s`;
  return `${Math.floor(ms / 60_000)}m ${Math.round((ms % 60_000) / 1000)}s`;
}
