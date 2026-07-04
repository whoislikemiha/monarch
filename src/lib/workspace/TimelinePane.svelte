<script lang="ts">
  /**
   * The work TIMELINE — the heart of the workspace. A live, narrated view of
   * what the agent is doing AND a scrollable record of everything it did:
   * NOW + plan up top, then the stream of coherent actions (newest-first)
   * grouped into objective segments, paged from `db_list_agent_timeline`
   * (MON-124). ~20 entries preload; older pages lazy-load as the user
   * scrolls toward the past. The active card live-merges running tools from
   * the agent's in-flight state so the timeline never lags the work.
   */
  import { onMount, tick } from "svelte";
  import type { Agent, ToolExecution } from "$lib/types";
  import { objectiveStore, type ObjectiveReportView } from "$lib/toolbox/objectiveStore.svelte";
  import { liveAgentStore } from "$lib/toolbox/liveAgentStore.svelte";
  import { timelineStore } from "./timelineStore.svelte";
  import {
    buildSegments,
    mergeAllLiveTools,
    mergeLiveTools,
    META_TOOLS,
    relTime,
    type ActionView,
    type AskPayload,
    type ExtraToolRow,
    type ToolCallView,
  } from "./timelineModel";
  import NowStrip from "./NowStrip.svelte";
  import TimelineAction from "./TimelineAction.svelte";
  import TimelineMilestone from "./TimelineMilestone.svelte";
  import TimelineReportCard from "./TimelineReportCard.svelte";
  import TimelineToolRow from "./TimelineToolRow.svelte";

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

  /** Metadata for the objective the agent is going after right now — feed
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
  let newestEntryId = $derived(tl?.entries[0]?.id ?? null);

  /** Tool calls already persisted anywhere in the loaded feed — nested under
   * actions OR as top-level rows. */
  let persistedToolCallIds = $derived.by(() => {
    const ids = new Set<string>();
    if (!tl) return ids;
    const collect = (rows: Iterable<import("$lib/bindings").ObjectiveEventRow>) => {
      for (const c of rows) {
        if (c.eventType !== "tool_call" || !c.payloadJson) continue;
        try {
          const p = JSON.parse(c.payloadJson) as { tool_call_id?: string };
          if (p.tool_call_id) ids.add(p.tool_call_id);
        } catch {
          /* ignore */
        }
      }
    };
    for (const children of tl.childrenByParent.values()) collect(children);
    collect(tl.entries);
    return ids;
  });

  /** The in-flight action: newest top-level entry, no outcome child, AND the
   * agent is actually working. An unclosed action on an idle agent is not
   * active — it renders unresolved (dashed), with no ticking clock. */
  let activeActionId = $derived.by(() => {
    if (!streaming) return null;
    const first = tl?.entries[0];
    if (!first || first.eventType !== "coherent_action") return null;
    const children = tl?.childrenByParent.get(first.id) ?? [];
    return children.some((c) => c.eventType === "action_outcome") ? null : first.id;
  });

  /** Live tools for the active card (persisted ⊕ in-flight, by toolCallId). */
  function liveToolsFor(action: ActionView): ToolCallView[] {
    if (action.eventId !== activeActionId || !live) return action.toolCalls;
    return mergeLiveTools(action.toolCalls, live.toolExecutions.values());
  }

  /** Overlay live status onto a top-level tool row (running ticker before
   * the persisted row's end-mutation ping lands). */
  function liveToolOverlay(tool: ToolCallView): ToolCallView {
    if (tool.status !== "running" || !live) return tool;
    const exec = live.toolExecutions.get(tool.toolCallId);
    if (!exec || exec.status === "running") return tool;
    return {
      ...tool,
      status: exec.status,
      isError: exec.status === "error",
      durationMs: exec.durationMs ?? tool.durationMs,
    };
  }

  /** Every tool execution this agent's loaded history knows about: restored
   * chat-history tool groups (durable — the messages table is written
   * project or not), overlaid by the live map (fresh status mid-turn). */
  let sessionToolExecutions = $derived.by(() => {
    const map = new Map<string, ToolExecution>();
    if (!live) return map;
    for (const item of live.items) {
      if (item.kind === "tool-group") {
        for (const e of item.executions) {
          if (!META_TOOLS.has(e.toolName)) map.set(e.toolCallId, e);
        }
      }
    }
    for (const e of live.toolExecutions.values()) {
      if (!META_TOOLS.has(e.toolName)) map.set(e.toolCallId, e);
    }
    return map;
  });

  /** MON-124 flat chronology: tool executions with no persisted
   * objective_events record (pre-scratch history, or in-flight latency)
   * interleave into the stream as bare tool rows — never grouped, never
   * dropped. History-sourced executions only qualify once the feed is fully
   * loaded (a partial feed can't prove an old call wasn't recorded deeper);
   * until then only live in-flight tools ride along. */
  let extraToolRows = $derived.by((): ExtraToolRow[] => {
    if (!live) return [];
    const feedComplete = !tl || (!tl.hasMore && !tl.loading);
    const source = feedComplete
      ? sessionToolExecutions.values()
      : [...live.toolExecutions.values()].filter((t) => t.status === "running");
    const orphans = [...source].filter((t) => !persistedToolCallIds.has(t.toolCallId));
    return mergeAllLiveTools(orphans).map((view) => ({
      view,
      createdAt: view.startedAt ?? "",
    }));
  });

  /** Chronological, oldest-first — the timeline reads top-down like the
   * chat; you scroll up for the past. The store keeps newest-first pages
   * (that's the cursor direction); display reverses. */
  let segments = $derived(
    buildSegments(
      tl ? [...tl.entries].reverse() : [],
      tl?.childrenByParent ?? new Map(),
      tl?.objectivesById ?? new Map(),
      extraToolRows,
    ),
  );

  let hasContent = $derived(segments.length > 0 || (planItems.length > 0 && !!workingMemory));

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
    // The user asked to look at THIS card — stop following the live head, or
    // the next feed refresh snaps back to the bottom mid-scroll (MON-130).
    pinned = false;
    el.scrollIntoView({ block: "center", behavior: "smooth" });
    flashId = req.actionId;
    const t = setTimeout(() => (flashId = null), 1600);
    return () => clearTimeout(t);
  });

  /** The pane owns its scroller and anchors to the BOTTOM like the chat:
   * newest at the bottom, scroll up for the past. */
  let scroller = $state<HTMLElement | null>(null);
  let pinned = true;
  function onScroll() {
    const el = scroller;
    if (!el) return;
    pinned = el.scrollHeight - el.scrollTop - el.clientHeight < 60;
  }
  /** Clicking anything in the stream (expanding a tool, toggling a card)
   * means the user is reading — stop following the live head. Scrolling back
   * to the bottom re-pins via onScroll, same as the chat. */
  function onStreamClick() {
    pinned = false;
  }
  $effect(() => {
    // Re-run on stream growth; honor the user's scroll position.
    segments;
    if (!pinned) return;
    tick().then(() => {
      const el = scroller;
      if (el) el.scrollTop = el.scrollHeight;
    });
  });

  /** Infinite scroll INTO THE PAST: the sentinel sits at the top; loading an
   * older page prepends content, so restore the scroll offset afterwards to
   * keep the viewport visually still. */
  let loadingOlder = false;
  async function loadOlder() {
    const el = scroller;
    if (loadingOlder || !el) return;
    loadingOlder = true;
    const prevHeight = el.scrollHeight;
    const prevTop = el.scrollTop;
    try {
      await timelineStore.loadMore(agent.id);
      await tick();
      el.scrollTop = el.scrollHeight - prevHeight + prevTop;
    } catch {
      /* surfaced via store error */
    } finally {
      loadingOlder = false;
    }
  }
  let sentinel = $state<HTMLElement | null>(null);
  $effect(() => {
    const el = sentinel;
    if (!el) return;
    const io = new IntersectionObserver(
      (hits) => {
        if (hits.some((h) => h.isIntersecting)) void loadOlder();
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

  /** Segment index → report, for the LAST segment of each closed objective —
   * in a chronological stream the close (and its report) reads at the end. */
  let reportBySegment = $derived.by(() => {
    const map = new Map<number, ObjectiveReportView>();
    const state = timelineStore.byAgent.get(agent.id);
    if (!state) return map;
    const seen = new Set<string>();
    for (let i = segments.length - 1; i >= 0; i--) {
      const segment = segments[i];
      if (seen.has(segment.objectiveId)) continue;
      seen.add(segment.objectiveId);
      const obj = segment.objective;
      if (!obj || !TERMINAL.has(obj.status)) continue;
      const report = state.reportsByObjective.get(segment.objectiveId);
      if (report) map.set(i, report);
    }
    return map;
  });
</script>

<div class="timeline" bind:this={rootEl}>
  <NowStrip {workingMemory} {planItems} {streaming} {currentObjective} />

  {#if tl?.loading}
    <div class="empty mono">Loading the work record…</div>
  {:else if hasContent}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="stream-scroll" bind:this={scroller} onscroll={onScroll} onclick={onStreamClick}>
      {#if tl?.hasMore}
        <div class="more" bind:this={sentinel}>
          <span class="mono">{tl.loadingMore ? "loading earlier work…" : "earlier work"}</span>
        </div>
      {:else if (tl?.entries.length ?? 0) > 0}
        <div class="end mono">start of the record</div>
      {/if}

      <div class="stream">
      {#each segments as segment, si (segment.objectiveId + ":" + si)}
        <div class="segment">
          {#if segment.objectiveId !== ""}
            <div class="seg-head">
              <span class="seg-title">{segment.objective?.title ?? "Objective"}</span>
              {#if segment.objective}
                <span class="seg-status mono">{statusLabel(segment.objective.status)}</span>
              {/if}
            </div>
          {/if}
          {#each segment.items as item (item.event.id)}
            {#if item.kind === "action"}
              {@const a = item.action}
              <div class="act-wrap" class:flash={flashId === a.eventId} data-action-id={a.eventId}>
                <TimelineAction
                  action={a}
                  phase={a.eventId === activeActionId
                    ? "active"
                    : a.autoClosed || (!a.outcome && !a.completedAt)
                      ? "auto"
                      : "done"}
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
            {:else if item.kind === "tool"}
              <div class="act-wrap" class:flash={flashId === item.event.id} data-action-id={item.event.id}>
                <TimelineToolRow tool={liveToolOverlay(item.tool)} time={relTime(item.event.createdAt || item.tool.startedAt)} />
              </div>
            {:else}
              <TimelineMilestone
                event={item.event}
                payload={item.payload}
                resolveTitle={resolvePlanTitle}
              />
            {/if}
          {/each}
          {#if reportBySegment.has(si)}
            <TimelineReportCard report={reportBySegment.get(si)!} />
          {/if}
        </div>
      {/each}
      </div>
    </div>
  {:else}
    <div class="empty mono">
      No work yet. Ask this agent to do something and watch it narrate here.
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
    flex: 1;
    min-height: 0;
    /* Without this the pane's automatic min width is the widest unbreakable
     * row (mono paths, bash args) — it silently outgrows the tile and every
     * line looks "cut off" at the edge instead of wrapping. */
    min-width: 0;
  }
  .stream-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
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
    overflow-wrap: anywhere;
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
