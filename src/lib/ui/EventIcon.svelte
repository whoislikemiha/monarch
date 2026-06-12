<script lang="ts">
  /**
   * Event-type icon for the execution timeline (MON-124). One glyph per event
   * kind — differentiation is by SHAPE, not color (house style: status is
   * never color-alone). Tones reuse existing status/accent tokens sparingly;
   * neutral is the default.
   *
   * Glyph decisions carried in from the visual-direction prompt pack:
   * - `action` (coherent_action) is the HEADLINE glyph — a weighted
   *   play-in-circle, deliberately heavier than the tool/outcome marks nested
   *   under it (the "headline-action icon" backport fix).
   * - `decision` is a branching fork, NOT a diamond — the diamond stays
   *   reserved for objective-tree nodes (the "diamond collision" fix).
   */
  export type EventKind =
    | "action"
    | "tool"
    | "outcome"
    | "decision"
    | "plan"
    | "status"
    | "note"
    | "blocker"
    | "blocker_resolved"
    | "question"
    | "answer"
    | "chat"
    | "report"
    | "event";

  interface Props {
    /** Semantic kind, or a raw `objective_events.event_type` (auto-mapped). */
    kind: EventKind | string;
    /** Square size in px. */
    size?: number;
    tone?: "neutral" | "accent" | "success" | "warning" | "error" | "info";
    /** Dims the glyph (auto-closed / unresolved states). */
    muted?: boolean;
  }
  let { kind, size = 14, tone = "neutral", muted = false }: Props = $props();

  const KIND_MAP: Record<string, EventKind> = {
    coherent_action: "action",
    tool_call: "tool",
    action_outcome: "outcome",
    executor_decision: "decision",
    plan_created: "plan",
    plan_changed: "plan",
    plan_item_started: "plan",
    plan_item_completed: "plan",
    plan_item_skipped: "plan",
    plan_item_blocked: "plan",
    status_change: "status",
    scope_change: "status",
    direction_change: "status",
    objective_rationale_change: "status",
    grade_change: "status",
    objective_summary_change: "status",
    note: "note",
    blocker: "blocker",
    blocker_resolved: "blocker_resolved",
    question: "question",
    answer: "answer",
    chat_spawned: "chat",
    report: "report",
  };

  let resolved = $derived((KIND_MAP[kind] ?? (kind as EventKind)) as EventKind);
</script>

<span
  class="evi tone-{tone}"
  class:muted
  style="width:{size}px;height:{size}px"
  aria-hidden="true"
>
  {#if resolved === "action"}
    <!-- weighted play-in-circle: the headline action mark -->
    <svg viewBox="0 0 16 16"><circle cx="8" cy="8" r="6.25" fill="none" stroke="currentColor" stroke-width="1.5" /><path d="M6.6 5.4 L10.6 8 L6.6 10.6 Z" fill="currentColor" /></svg>
  {:else if resolved === "tool"}
    <!-- terminal prompt -->
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3.5 4.5 L7 8 L3.5 11.5" /><path d="M8.5 11.5 H12.5" /></svg>
  {:else if resolved === "outcome"}
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.7"><path d="M3.5 8.5 L6.5 11.5 L12.5 4.5" /></svg>
  {:else if resolved === "decision"}
    <!-- branching fork (NOT a diamond — that's the objective-node glyph) -->
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M8 13.5 V8" /><path d="M8 8 C8 5.5, 4.5 6.5, 4.5 3.5" /><path d="M8 8 C8 5.5, 11.5 6.5, 11.5 3.5" /><circle cx="4.5" cy="3" r="1.1" fill="currentColor" stroke="none" /><circle cx="11.5" cy="3" r="1.1" fill="currentColor" stroke="none" /></svg>
  {:else if resolved === "plan"}
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M6 4.5 H13" /><path d="M6 8 H13" /><path d="M6 11.5 H13" /><circle cx="3.3" cy="4.5" r="0.9" fill="currentColor" stroke="none" /><circle cx="3.3" cy="8" r="0.9" fill="currentColor" stroke="none" /><circle cx="3.3" cy="11.5" r="0.9" fill="currentColor" stroke="none" /></svg>
  {:else if resolved === "status"}
    <!-- transition arrows -->
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 5.5 H11.5 M11.5 5.5 L9 3" /><path d="M13 10.5 H4.5 M4.5 10.5 L7 13" /></svg>
  {:else if resolved === "note"}
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M4 2.5 h6 l3 3 v8 h-9 z" /><path d="M10 2.5 v3 h3" /></svg>
  {:else if resolved === "blocker"}
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M8 2.5 L14 13 H2 Z" /><path d="M8 6.5 V9.5" stroke-width="1.7" /><circle cx="8" cy="11.4" r="0.8" fill="currentColor" stroke="none" /></svg>
  {:else if resolved === "blocker_resolved"}
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M8 2.5 L14 13 H2 Z" /><path d="M5.8 9.4 L7.4 11 L10.4 7.4" stroke-width="1.4" /></svg>
  {:else if resolved === "question"}
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M5.5 5.5 C5.5 3.8, 7 3, 8 3 C9.3 3, 10.5 3.9, 10.5 5.2 C10.5 7.2, 8 7, 8 9.2" /><circle cx="8" cy="12.2" r="0.9" fill="currentColor" stroke="none" /></svg>
  {:else if resolved === "answer"}
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M13 4 V7.5 C13 9, 12 10, 10.5 10 H3.5" /><path d="M6 7 L3 10 L6 13" /></svg>
  {:else if resolved === "chat"}
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 4h10v6H7l-3 2.5V10H3z" /></svg>
  {:else if resolved === "report"}
    <!-- flag: the close-of-objective report -->
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M4 14 V2.5" /><path d="M4 3 H12 L10 5.75 L12 8.5 H4" /></svg>
  {:else}
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="8" cy="8" r="3" /></svg>
  {/if}
</span>

<style>
  .evi {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    color: var(--text-secondary);
  }
  .evi :global(svg) { width: 100%; height: 100%; display: block; }
  .evi.muted { color: var(--text-muted); opacity: 0.75; }
  .evi.tone-accent { color: var(--accent); }
  .evi.tone-success { color: var(--status-success); }
  .evi.tone-warning { color: var(--status-warning); }
  .evi.tone-error { color: var(--status-error); }
  .evi.tone-info { color: var(--status-info); }
</style>
