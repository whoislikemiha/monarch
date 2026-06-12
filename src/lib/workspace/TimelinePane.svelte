<script lang="ts">
  /**
   * The work TIMELINE — the heart of the workspace. A live, narrated view of
   * what the shadow is doing AND a scrollable record of everything it did:
   * NOW + plan up top, then the stream of coherent actions (newest-first)
   * grouped into objective segments, paged from `db_list_agent_timeline`
   * (MON-124). ~20 entries preload; older pages lazy-load as the captain
   * scrolls toward the past. The active card live-merges running tools from
   * the agent's in-flight state so the timeline never lags the work.
   */
  import { onMount } from "svelte";
  import type { Agent, ToolExecution } from "$lib/types";
  import { objectiveStore, type ObjectiveReportView } from "$lib/toolbox/objectiveStore.svelte";
  import { liveAgentStore } from "$lib/toolbox/liveAgentStore.svelte";
  import { timelineStore } from "./timelineStore.svelte";
  import {
    buildSegments,
    FALLBACK_ACTION_ID,
    mergeAllLiveTools,
    mergeLiveTools,
    relTime,
    type ActionView,
    type AskPayload,
    type ToolCallView,
  } from "./timelineModel";
  import NowStrip from "./NowStrip.svelte";
  import TimelineAction from "./TimelineAction.svelte";
  import TimelineMilestone from "./TimelineMilestone.svelte";
  import TimelineReportCard from "./TimelineReportCard.svelte";

  interface Props {
    agent: Agent;
    /** Open a chat scoped to a timeline action. */
    onask?: (action: AskPayload) => void;
    /** Re-open / focus a chat previously spawned from an action. */
    onopenchat?: (scopeId: string, label: string) => void;
  }
  let { agent, onask, onopenchat }: Props = $props();

  let entry = $derived(objectiveStore.byAgent.get(agent.id));
  let workingMemory = $derived(entry?.workingMemory ?? null);
  let live = $derived(liveAgentStore.byAgent.get(agent.id));
  let streaming = $derived(!!live?.isStreaming);

  let planItems = $derived(
    workingMemory?.currentObjectiveId
      ? entry?.planItemsByObjective.get(workingMemory.currentObjectiveId) ?? []
      : [],
  );

  /** Metadata for the objective the shadow is going after right now — feed
   * cache first, objective trees as fallback. */
  let currentObjective = $derived.by(() => {
    const oid = workingMemory?.currentObjectiveId;
    if (!oid) return null;
    const fromFeed = timelineStore.byAgent.get(agent.id)?.objectivesById.get(oid);
    if (fromFeed) return fromFeed;
    for (const tree of entry?.treesByRoot.values() ?? []) {
      const hit = tree.find((o) => o.id === oid);
      if (hit) return hit;
    }
    return null;
  });

  let tl = $derived(timelineStore.byAgent.get(agent.id));
  let segments = $derived(
    tl ? buildSegments(tl.entries, tl.childrenByParent, tl.objectivesById) : [],
  );
  let newestEntryId = $derived(tl?.entries[0]?.id ?? null);

  /** The in-flight action (newest entry, coherent_action, no outcome yet). */
  let activeActionId = $derived.by(() => {
    const first = segments[0]?.items[0];
    if (!first || first.kind !== "action") return null;
    const a = first.action;
    return first.event.id === newestEntryId && !a.outcome && !a.completedAt ? a.eventId : null;
  });

  /** Tool calls already persisted as children anywhere in the loaded feed. */
  let persistedToolCallIds = $derived.by(() => {
    const ids = new Set<string>();
    if (!tl) return ids;
    for (const children of tl.childrenByParent.values()) {
      for (const c of children) {
        if (c.eventType !== "tool_call" || !c.payloadJson) continue;
        try {
          const p = JSON.parse(c.payloadJson) as { tool_call_id?: string };
          if (p.tool_call_id) ids.add(p.tool_call_id);
        } catch {
          /* ignore */
        }
      }
    }
    return ids;
  });

  /** Live tools for the active card (persisted ⊕ in-flight, by toolCallId). */
  function liveToolsFor(action: ActionView): ToolCallView[] {
    if (action.eventId !== activeActionId || !live) return action.toolCalls;
    return mergeLiveTools(action.toolCalls, live.toolExecutions.values());
  }

  /** Every tool execution this agent's loaded history knows about: restored
   * chat-history tool groups first (durable — the messages table is written
   * project or not), overlaid by the live map (fresh status mid-turn, and
   * cleared on restore). This is what makes project-less work a real part of
   * the record: it rides the agent's chat history, not objective_events. */
  let sessionToolExecutions = $derived.by(() => {
    const map = new Map<string, ToolExecution>();
    if (!live) return map;
    for (const item of live.items) {
      if (item.kind === "tool-group") {
        for (const e of item.executions) map.set(e.toolCallId, e);
      }
    }
    for (const e of live.toolExecutions.values()) map.set(e.toolCallId, e);
    return map;
  });

  /** Unnarrated fallback: tool executions with no persisted objective_events
   * record — no objective (project-less agent) or no narrated action to live
   * under. Running AND finished ones both render: work must never silently
   * disappear from the timeline. History-sourced executions are only safe to
   * classify as unrecorded once the feed is fully loaded (a partially-loaded
   * feed can't prove an old tool call wasn't narrated on a deeper page) —
   * until then, only the live in-flight map feeds the card. */
  let orphanTools = $derived.by(() => {
    if (!live || activeActionId) return [];
    const feedComplete = !tl || (!tl.hasMore && !tl.loading);
    const source = feedComplete ? sessionToolExecutions.values() : live.toolExecutions.values();
    const orphans = [...source].filter((t) => !persistedToolCallIds.has(t.toolCallId));
    return orphans.length ? mergeAllLiveTools(orphans) : [];
  });
  let fallbackAction = $derived.by((): ActionView | null => {
    if (!orphanTools.length) return null;
    const running = orphanTools.some((t) => t.status === "running");
    const errors = orphanTools.filter((t) => t.isError).length;
    return {
      eventId: FALLBACK_ACTION_ID,
      objectiveId: "",
      intent: running ? "working…" : "unnarrated work",
      startedAt: null,
      outcome: running
        ? null
        : errors > 0
          ? `${errors} of ${orphanTools.length} tool call${orphanTools.length === 1 ? "" : "s"} failed`
          : `${orphanTools.length} tool call${orphanTools.length === 1 ? "" : "s"}`,
      autoClosed: false,
      completedAt: null,
      toolCalls: orphanTools,
      decisions: [],
      chatsSpawned: [],
      filesTouched: [],
      planItemId: null,
    };
  });
  let fallbackRunning = $derived(orphanTools.some((t) => t.status === "running"));

  let hasContent = $derived(
    segments.length > 0 || !!fallbackAction || (planItems.length > 0 && !!workingMemory),
  );

  /** 1s wall clock driving the active card's elapsed timer and live durations. */
  let nowMs = $state(Date.now());
  $effect(() => {
    if (!streaming && !activeActionId) return;
    nowMs = Date.now();
    const t = setInterval(() => (nowMs = Date.now()), 1000);
    return () => clearInterval(t);
  });

  onMount(() => {
    objectiveStore.ensure(agent.id);
    objectiveStore.refresh(agent.id).catch(() => {});
    timelineStore.init(agent.id).catch(() => {});
  });

  /** Chat activity chips request "show me this work": scroll the card into
   * view and flash it. */
  let rootEl = $state<HTMLElement | null>(null);
  let flashId = $state<string | null>(null);
  $effect(() => {
    const req = timelineStore.focusRequest;
    if (!req || req.agentId !== agent.id || !rootEl) return;
    const el = rootEl.querySelector(`[data-action-id="${req.actionId}"]`);
    if (!el) return;
    el.scrollIntoView({ block: "center", behavior: "smooth" });
    flashId = req.actionId;
    const t = setTimeout(() => (flashId = null), 1600);
    return () => clearTimeout(t);
  });

  /** Infinite scroll: when the sentinel at the old end becomes visible, pull
   * the next page. The scroll container clips it, so viewport-root IO works. */
  let sentinel = $state<HTMLElement | null>(null);
  $effect(() => {
    const el = sentinel;
    const agentId = agent.id;
    if (!el) return;
    const io = new IntersectionObserver(
      (hits) => {
        if (hits.some((h) => h.isIntersecting)) {
          timelineStore.loadMore(agentId).catch(() => {});
        }
      },
      { rootMargin: "200px" },
    );
    io.observe(el);
    return () => io.disconnect();
  });

  function statusLabel(status: string): string {
    return status.replace(/_/g, " ");
  }

  /** Best-effort plan-item title lookup across the cached plan slices. */
  function resolvePlanTitle(itemId: string): string | null {
    for (const items of entry?.planItemsByObjective.values() ?? []) {
      const hit = items.find((i) => i.id === itemId);
      if (hit) return hit.title;
    }
    return null;
  }

  const TERMINAL = new Set(["done", "verified", "claimed_done", "abandoned"]);

  /** Lazily pull reports for closed objectives that entered the feed. */
  $effect(() => {
    const state = timelineStore.byAgent.get(agent.id);
    if (!state) return;
    for (const obj of state.objectivesById.values()) {
      if (TERMINAL.has(obj.status)) timelineStore.loadReport(agent.id, obj.id).catch(() => {});
    }
  });

  /** Segment index → report, for the NEWEST segment of each closed objective
   * (the close lives at the top of that segment in a newest-first stream). */
  let reportBySegment = $derived.by(() => {
    const map = new Map<number, ObjectiveReportView>();
    const state = timelineStore.byAgent.get(agent.id);
    if (!state) return map;
    const seen = new Set<string>();
    segments.forEach((segment, i) => {
      if (seen.has(segment.objectiveId)) return;
      seen.add(segment.objectiveId);
      const obj = segment.objective;
      if (!obj || !TERMINAL.has(obj.status)) return;
      const report = state.reportsByObjective.get(segment.objectiveId);
      if (report) map.set(i, report);
    });
    return map;
  });
</script>

<div class="timeline" bind:this={rootEl}>
  <NowStrip {workingMemory} {planItems} {streaming} {currentObjective} />

  {#if tl?.loading}
    <div class="empty mono">Loading the work record…</div>
  {:else if hasContent}
    <div class="stream">
      {#if fallbackAction}
        <div
          class="act-wrap"
          class:flash={flashId === FALLBACK_ACTION_ID}
          data-action-id={FALLBACK_ACTION_ID}
        >
          <TimelineAction action={fallbackAction} phase={fallbackRunning ? "active" : "auto"} {nowMs} />
        </div>
      {/if}

      {#each segments as segment, si (segment.objectiveId + ":" + si)}
        <div class="segment">
          <div class="seg-head">
            <span class="seg-title">{segment.objective?.title ?? "Objective"}</span>
            {#if segment.objective}
              <span class="seg-status mono">{statusLabel(segment.objective.status)}</span>
            {/if}
          </div>
          {#if reportBySegment.has(si)}
            <TimelineReportCard report={reportBySegment.get(si)!} />
          {/if}
          {#each segment.items as item (item.event.id)}
            {#if item.kind === "action"}
              {@const a = item.action}
              <div class="act-wrap" class:flash={flashId === a.eventId} data-action-id={a.eventId}>
                <TimelineAction
                  action={a}
                  phase={a.eventId === activeActionId ? "active" : a.autoClosed ? "auto" : "done"}
                  tools={liveToolsFor(a)}
                  {nowMs}
                  onask={onask
                    ? () =>
                        onask({
                          id: a.eventId,
                          intent: a.intent,
                          outcome: a.outcome,
                          objectiveId: a.objectiveId,
                          spawned: a.chatsSpawned.length > 0,
                        })
                    : undefined}
                  {onopenchat}
                />
              </div>
            {:else}
              <TimelineMilestone
                event={item.event}
                payload={item.payload}
                resolveTitle={resolvePlanTitle}
              />
            {/if}
          {/each}
        </div>
      {/each}

      {#if tl?.hasMore}
        <div class="more" bind:this={sentinel}>
          <span class="mono">{tl.loadingMore ? "loading earlier work…" : "earlier work"}</span>
        </div>
      {:else if (tl?.entries.length ?? 0) > 0}
        <div class="end mono">start of the record</div>
      {/if}
    </div>
  {:else}
    <div class="empty mono">
      No work yet. Ask this shadow to do something and watch it narrate here.
    </div>
  {/if}

  {#if tl?.error}
    <div class="err mono">{tl.error}</div>
  {/if}
</div>

<style>
  .timeline {
    display: flex;
    flex-direction: column;
    gap: var(--s3);
  }
  .stream { display: flex; flex-direction: column; gap: var(--s2); }

  .segment { display: flex; flex-direction: column; }
  .seg-head {
    display: flex;
    align-items: baseline;
    gap: var(--s3);
    padding: var(--s2) 0 var(--s1);
    border-bottom: 1px solid var(--border-subtle);
    margin-bottom: var(--s1);
    min-width: 0;
  }
  .seg-title {
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .seg-status { font-size: 9.5px; color: var(--text-muted); flex: none; margin-left: auto; }

  .act-wrap { border-radius: var(--r-md); }
  .act-wrap.flash { animation: tl-flash 1.6s ease-out; }
  @keyframes tl-flash {
    0%, 30% { background: var(--accent-bg-subtle); }
    100% { background: transparent; }
  }
  @media (prefers-reduced-motion: reduce) {
    .act-wrap.flash { animation: none; background: var(--accent-bg-subtle); }
  }

  .more, .end {
    display: flex; justify-content: center;
    padding: var(--s3);
    font-size: 10px; color: var(--text-muted);
  }
  .empty { font-size: 11px; color: var(--text-muted); padding: var(--s4) var(--s2); line-height: 1.6; }
  .err { font-size: 10px; color: var(--status-error); padding: var(--s2); }
</style>
