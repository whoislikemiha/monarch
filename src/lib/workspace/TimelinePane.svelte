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
  import type { Agent } from "$lib/types";
  import { objectiveStore } from "$lib/toolbox/objectiveStore.svelte";
  import { liveAgentStore } from "$lib/toolbox/liveAgentStore.svelte";
  import { timelineStore } from "./timelineStore.svelte";
  import {
    buildSegments,
    mergeLiveTools,
    relTime,
    type ActionView,
    type AskPayload,
    type ToolCallView,
  } from "./timelineModel";
  import NowStrip from "./NowStrip.svelte";
  import TimelineAction from "./TimelineAction.svelte";

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

  /** Unnarrated fallback: running tools with no narrated action to live under.
   * Work must never silently disappear from the timeline. */
  let orphanTools = $derived.by(() => {
    if (!live || activeActionId) return [];
    const orphans = [...live.toolExecutions.values()].filter(
      (t) => t.status === "running" && !persistedToolCallIds.has(t.toolCallId),
    );
    return orphans.length ? mergeLiveTools([], orphans) : [];
  });
  let fallbackAction = $derived.by((): ActionView | null => {
    if (!orphanTools.length) return null;
    return {
      eventId: "__fallback__",
      objectiveId: "",
      intent: "working…",
      startedAt: null,
      outcome: null,
      autoClosed: false,
      completedAt: null,
      toolCalls: orphanTools,
      decisions: [],
      chatsSpawned: [],
      filesTouched: [],
      planItemId: null,
    };
  });

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

  /** Human label for milestone rows — slice 6 gives these full treatment. */
  function milestoneLabel(eventType: string, payload: Record<string, unknown>): string {
    switch (eventType) {
      case "status_change":
        return `status: ${payload.from ?? "?"} → ${payload.to ?? "?"}`;
      case "plan_created":
        return "plan set";
      case "plan_changed":
        return "plan changed";
      case "plan_item_started":
        return "plan step started";
      case "plan_item_completed":
        return "plan step completed";
      case "plan_item_skipped":
        return "plan step skipped";
      case "plan_item_blocked":
        return "plan step blocked";
      case "note":
      case "blocker":
      case "blocker_resolved":
      case "question":
      case "answer":
        return `${eventType.replace(/_/g, " ")}${typeof payload.text === "string" && payload.text ? `: ${payload.text}` : ""}`;
      default:
        return eventType.replace(/_/g, " ");
    }
  }
</script>

<div class="timeline">
  <NowStrip {workingMemory} {planItems} {streaming} />

  {#if tl?.loading}
    <div class="empty mono">Loading the work record…</div>
  {:else if hasContent}
    <div class="stream">
      {#if fallbackAction}
        <TimelineAction action={fallbackAction} phase="active" {nowMs} />
      {/if}

      {#each segments as segment, si (segment.objectiveId + ":" + si)}
        <div class="segment">
          <div class="seg-head">
            <span class="seg-title">{segment.objective?.title ?? "Objective"}</span>
            {#if segment.objective}
              <span class="seg-status mono">{statusLabel(segment.objective.status)}</span>
            {/if}
          </div>
          {#each segment.items as item (item.event.id)}
            {#if item.kind === "action"}
              {@const a = item.action}
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
            {:else}
              <div class="mile">
                <span class="mile-mark" aria-hidden="true"></span>
                <span class="mile-label">{milestoneLabel(item.event.eventType, item.payload)}</span>
                <span class="mile-time mono">{relTime(item.event.createdAt)}</span>
              </div>
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

  .mile {
    display: flex;
    align-items: baseline;
    gap: var(--s2);
    padding: 3px var(--s2) 3px 3px;
    min-width: 0;
  }
  .mile-mark {
    width: 5px; height: 5px; border-radius: var(--r-full);
    background: var(--border-strong); flex: none; align-self: center;
    margin: 0 4px 0 2px;
  }
  .mile-label {
    font-size: 11px; color: var(--text-muted); line-height: 1.5;
    min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .mile-time { font-size: 9.5px; color: var(--text-muted); margin-left: auto; flex: none; }

  .more, .end {
    display: flex; justify-content: center;
    padding: var(--s3);
    font-size: 10px; color: var(--text-muted);
  }
  .empty { font-size: 11px; color: var(--text-muted); padding: var(--s4) var(--s2); line-height: 1.6; }
  .err { font-size: 10px; color: var(--status-error); padding: var(--s2); }
</style>
