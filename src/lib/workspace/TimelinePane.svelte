<script lang="ts">
  /**
   * The work TIMELINE — the heart of the workspace. A live, narrated view of
   * what the shadow is doing: NOW + plan, then the stream of coherent actions
   * in its own words. Read-only here; slice 5 makes each action open a scoped
   * chat with this same shadow.
   */
  import { onMount } from "svelte";
  import type { Agent } from "$lib/types";
  import { objectiveStore } from "$lib/toolbox/objectiveStore.svelte";
  import { liveAgentStore } from "$lib/toolbox/liveAgentStore.svelte";
  import NowStrip from "./NowStrip.svelte";
  import TimelineAction from "./TimelineAction.svelte";

  interface Props {
    agent: Agent;
    /** Open a chat scoped to a timeline action (slice 5). */
    onask?: (action: { id: string; intent: string; outcome?: string | null }) => void;
  }
  let { agent, onask }: Props = $props();

  let entry = $derived(objectiveStore.byAgent.get(agent.id));
  let workingMemory = $derived(entry?.workingMemory ?? null);
  let streaming = $derived(!!liveAgentStore.byAgent.get(agent.id)?.isStreaming);

  let planItems = $derived(
    workingMemory?.currentObjectiveId
      ? entry?.planItemsByObjective.get(workingMemory.currentObjectiveId) ?? []
      : [],
  );
  let recentActions = $derived(workingMemory?.recentActions ?? []);
  let current = $derived(workingMemory?.currentAction ?? null);
  let hasContent = $derived(!!current || recentActions.length > 0 || planItems.length > 0);

  onMount(() => {
    objectiveStore.ensure(agent.id);
    objectiveStore.refresh(agent.id).catch(() => {});
  });

  function relTime(iso: string | null | undefined): string | null {
    if (!iso) return null;
    const t = Date.parse(iso);
    if (Number.isNaN(t)) return null;
    const s = Math.max(0, Math.floor((Date.now() - t) / 1000));
    if (s < 60) return `${s}s`;
    if (s < 3600) return `${Math.floor(s / 60)}m`;
    if (s < 86400) return `${Math.floor(s / 3600)}h`;
    return `${Math.floor(s / 86400)}d`;
  }
</script>

<div class="timeline">
  <NowStrip {workingMemory} {planItems} {streaming} />

  {#if hasContent}
    <div class="stream">
      {#if current}
        <TimelineAction
          intent={current.intent}
          state="active"
          time={relTime(current.startedAt)}
          onask={onask ? () => onask({ id: current!.eventId, intent: current!.intent }) : undefined}
        />
      {/if}
      {#each [...recentActions].reverse() as action (action.eventId)}
        <TimelineAction
          intent={action.intent}
          outcome={action.outcome}
          state={action.autoClosed ? "auto" : "done"}
          time={relTime(action.completedAt)}
          onask={onask ? () => onask({ id: action.eventId, intent: action.intent, outcome: action.outcome }) : undefined}
        />
      {/each}
    </div>
  {:else}
    <div class="empty mono">
      No work yet. Ask this shadow to do something and watch it narrate here.
    </div>
  {/if}
</div>

<style>
  .timeline {
    display: flex;
    flex-direction: column;
    gap: var(--s3);
  }
  .stream { display: flex; flex-direction: column; }
  .empty { font-size: 11px; color: var(--text-muted); padding: var(--s4) var(--s2); line-height: 1.6; }
</style>
