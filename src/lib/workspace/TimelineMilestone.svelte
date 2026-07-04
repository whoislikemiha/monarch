<script lang="ts">
  /**
   * One non-action event in the work timeline (MON-124): plan mutations,
   * status transitions, manual notes/blockers/questions. Visually THINNER
   * than action cards — these are punctuation in the work record, not work.
   * Status changes render as section dividers ("objective started" /
   * "objective done"); blockers carry the warning tone.
   */
  import type { ObjectiveEventRow } from "$lib/bindings";
  import { clockTime } from "./timelineModel";
  import EventIcon from "$lib/ui/EventIcon.svelte";

  interface Props {
    event: ObjectiveEventRow;
    payload: Record<string, unknown>;
    /** Resolve a plan item id to its title (best-effort, cache-backed). */
    resolveTitle?: (itemId: string) => string | null;
  }
  let { event, payload, resolveTitle }: Props = $props();

  const s = (v: unknown): string | null => (typeof v === "string" && v.length ? v : null);

  let isDivider = $derived(event.eventType === "status_change");
  let isBlocker = $derived(event.eventType === "blocker");

  let dividerLabel = $derived.by(() => {
    const to = s(payload.to);
    switch (to) {
      case "in_progress":
        return "objective started";
      case "done":
      case "verified":
        return "objective done";
      case "claimed_done":
        return "claimed done";
      case "abandoned":
        return "objective abandoned";
      default:
        return `${s(payload.from) ?? "?"} → ${to ?? "?"}`;
    }
  });

  let itemTitle = $derived.by(() => {
    const id = s(payload.item_id) ?? event.planItemId;
    return id ? resolveTitle?.(id) ?? null : null;
  });

  let label = $derived.by(() => {
    switch (event.eventType) {
      case "plan_created": {
        const n = Array.isArray(payload.item_ids) ? payload.item_ids.length : null;
        return n ? `plan set · ${n} step${n === 1 ? "" : "s"}` : "plan set";
      }
      case "plan_changed":
        return "plan changed";
      case "plan_item_started":
        return itemTitle ? `step started: ${itemTitle}` : "plan step started";
      case "plan_item_completed":
        return itemTitle ? `step done: ${itemTitle}` : "plan step done";
      case "plan_item_skipped":
        return itemTitle ? `step skipped: ${itemTitle}` : "plan step skipped";
      case "plan_item_blocked":
        return itemTitle ? `step blocked: ${itemTitle}` : "plan step blocked";
      case "note":
      case "blocker":
      case "blocker_resolved":
      case "question":
      case "answer":
        return s(payload.title) ?? event.eventType.replace(/_/g, " ");
      default:
        return event.eventType.replace(/_/g, " ");
    }
  });

  /** Secondary text — outcome/reason/body, shown dim after the label. */
  let detail = $derived.by(() => {
    switch (event.eventType) {
      case "plan_item_completed":
        return s(payload.outcome);
      case "plan_item_skipped":
      case "plan_item_blocked":
        return s(payload.reason);
      case "plan_created":
      case "plan_changed":
        return s(payload.rationale);
      case "note":
      case "blocker":
      case "blocker_resolved":
      case "question":
      case "answer":
        return s(payload.text);
      default:
        return null;
    }
  });

  let expanded = $state(false);
</script>

{#if isDivider}
  <div class="divider" role="separator">
    <span class="line" aria-hidden="true"></span>
    <span class="d-label">{dividerLabel}</span>
    <span class="line" aria-hidden="true"></span>
    <span class="d-time mono">{clockTime(event.createdAt)}</span>
  </div>
{:else}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <!-- tabindex and role are both gated on `detail`: interactive only when expandable -->
  <div
    class="mile"
    class:blocker={isBlocker}
    class:expandable={!!detail}
    onclick={() => detail && (expanded = !expanded)}
    onkeydown={(e) => {
      if (detail && (e.key === "Enter" || e.key === " ")) {
        e.preventDefault();
        expanded = !expanded;
      }
    }}
    role={detail ? "button" : undefined}
    tabindex={detail ? 0 : undefined}
  >
    <span class="mark">
      <EventIcon
        kind={event.eventType}
        size={11}
        tone={isBlocker ? "warning" : "neutral"}
        muted={!isBlocker}
      />
    </span>
    <span class="body" class:expanded>
      <span class="label">{label}</span>
      {#if detail}<span class="detail">{detail}</span>{/if}
    </span>
    <span class="time mono">{clockTime(event.createdAt)}</span>
  </div>
{/if}

<style>
  .divider {
    display: flex; align-items: center; gap: var(--s2);
    padding: var(--s2) 0;
    min-width: 0;
  }
  .divider .line { flex: 1; height: 1px; background: var(--border-subtle); }
  .divider .line:first-child { max-width: 14px; flex: none; width: 14px; }
  .d-label {
    flex: none;
    font-size: 9.5px; font-weight: 600; letter-spacing: 0.1em; text-transform: uppercase;
    color: var(--text-muted);
  }
  .d-time { flex: none; font-size: 9.5px; color: var(--text-muted); }

  .mile {
    display: flex;
    align-items: baseline;
    gap: var(--s2);
    padding: 3px var(--s2) 3px 0;
    border-radius: var(--r-sm);
    min-width: 0;
  }
  .mile.expandable { cursor: pointer; }
  .mile.expandable:hover { background: var(--bg-panel); }
  .mile:focus-visible { outline: 2px solid var(--focus); outline-offset: -2px; }
  .mark { flex: none; align-self: center; display: inline-flex; width: 14px; justify-content: center; }

  .body { display: flex; gap: var(--s2); align-items: baseline; min-width: 0; flex: 1; }
  .label { font-size: 11px; color: var(--text-muted); line-height: 1.5; flex: none; }
  .mile.blocker .label { color: var(--status-warning); font-weight: 500; }
  .detail {
    font-size: 11px; color: var(--text-muted); opacity: 0.8; line-height: 1.5;
    min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .body.expanded { flex-direction: column; }
  .body.expanded .detail { white-space: normal; overflow: visible; }
  .time { font-size: 9.5px; color: var(--text-muted); margin-left: auto; flex: none; }
</style>
