<script lang="ts">
  /**
   * The "now" strip: what the shadow is doing this instant + its plan position.
   * Sits at the top of the timeline pane. Pure projection of working memory +
   * the current objective's plan items.
   */
  import type { PlanItemRow, WorkingMemoryPayload } from "$lib/bindings";

  interface Props {
    workingMemory: WorkingMemoryPayload | null;
    planItems: PlanItemRow[];
    streaming: boolean;
  }
  let { workingMemory, planItems, streaming }: Props = $props();

  let activePlanItem = $derived(
    workingMemory?.activePlanItemId
      ? planItems.find((i) => i.id === workingMemory!.activePlanItemId) ?? null
      : null,
  );
  let nextPlanItems = $derived.by(() => {
    const ids = workingMemory?.nextPlanItemIds ?? [];
    return ids
      .map((id) => planItems.find((i) => i.id === id))
      .filter((i): i is PlanItemRow => !!i);
  });

  let current = $derived(workingMemory?.currentAction ?? null);
  let path = $derived(workingMemory?.currentObjectivePath ?? []);
  let planProgress = $derived.by(() => {
    if (!planItems.length) return null;
    const done = planItems.filter((i) => i.status === "completed").length;
    return { done, total: planItems.length };
  });
</script>

{#if current || activePlanItem || nextPlanItems.length}
  <div class="now">
    <div class="now-line">
      <span class="tag" class:live={streaming}>NOW</span>
      {#if current}
        <span class="intent">{current.intent}</span>
      {:else}
        <span class="intent idle">Idle</span>
      {/if}
      {#if streaming}<span class="pulse" aria-hidden="true"></span>{/if}
    </div>
    {#if path.length}
      <div class="path mono">{path.join(" / ")}</div>
    {/if}

    {#if activePlanItem || nextPlanItems.length}
      <div class="plan">
        <span class="tag">PLAN</span>
        {#if planProgress}<span class="prog mono">{planProgress.done}/{planProgress.total}</span>{/if}
        <div class="plan-items">
          {#if activePlanItem}
            <span class="pi active" title={activePlanItem.rationale ?? activePlanItem.title}>
              <span class="pi-mark" aria-hidden="true"></span>{activePlanItem.title}
            </span>
          {/if}
          {#each nextPlanItems as item (item.id)}
            <span class="pi next" title={item.rationale ?? item.title}>{item.title}</span>
          {/each}
        </div>
      </div>
    {/if}
  </div>
{/if}

<style>
  .now {
    display: flex;
    flex-direction: column;
    gap: var(--s2);
    padding: var(--s3) var(--s4);
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-md);
  }
  .now-line { display: flex; align-items: center; gap: var(--s2); }
  .tag {
    font-size: 9px; font-weight: 700; letter-spacing: 0.14em;
    color: var(--text-muted); flex: none;
  }
  .tag.live { color: var(--status-info); }
  .intent { font-size: 12.5px; color: var(--text-primary); font-weight: 500; }
  .intent.idle { color: var(--text-muted); font-weight: 400; }
  .pulse {
    width: 7px; height: 7px; border-radius: var(--r-full);
    background: var(--status-info); flex: none;
    animation: now-pulse 1.4s ease-in-out infinite;
  }
  @keyframes now-pulse { 0%, 100% { opacity: 0.35; } 50% { opacity: 1; } }
  .path { font-size: 10px; color: var(--text-muted); padding-left: calc(9px + var(--s2)); }

  .plan { display: flex; align-items: baseline; gap: var(--s2); flex-wrap: wrap; padding-top: var(--s1); border-top: 1px solid var(--border-subtle); }
  .prog { font-size: 10px; color: var(--text-muted); }
  .plan-items { display: flex; gap: var(--s2); flex-wrap: wrap; min-width: 0; }
  .pi { display: inline-flex; align-items: center; gap: 5px; font-size: 11px; color: var(--text-muted); }
  .pi.active { color: var(--text-secondary); }
  .pi.active .pi-mark {
    width: 6px; height: 6px; border-radius: var(--r-full); background: var(--accent); flex: none;
  }
  .pi.next { opacity: 0.7; }
  .pi.next::before { content: "›"; margin-right: 3px; color: var(--text-muted); }
</style>
