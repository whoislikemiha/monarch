<script lang="ts">
  /**
   * The "now" strip: what the shadow is doing this instant, which objective
   * it's going after, and its position in the plan. Sits at the top of the
   * timeline pane. Pure projection of working memory + the current
   * objective's plan items. Always rendered — "conversational, no objective"
   * is a designed state, not an absence (MON-124).
   */
  import type { ObjectiveRow, PlanItemRow, WorkingMemoryPayload } from "$lib/bindings";

  interface Props {
    workingMemory: WorkingMemoryPayload | null;
    planItems: PlanItemRow[];
    streaming: boolean;
    /** Metadata of the objective the shadow is currently going after. */
    currentObjective?: ObjectiveRow | null;
  }
  let { workingMemory, planItems, streaming, currentObjective = null }: Props = $props();

  let current = $derived(workingMemory?.currentAction ?? null);
  let path = $derived(workingMemory?.currentObjectivePath ?? []);
  let hasObjective = $derived(!!workingMemory?.currentObjectiveId);

  let activeId = $derived(workingMemory?.activePlanItemId ?? null);
  let planProgress = $derived.by(() => {
    if (!planItems.length) return null;
    const done = planItems.filter((i) => i.status === "completed").length;
    return { done, total: planItems.length };
  });

  /** Whole plan, expanded. Collapsed shows the active item + what's next. */
  let planOpen = $state(false);
  let collapsedItems = $derived.by(() => {
    if (!planItems.length) return [];
    const active = planItems.find((i) => i.id === activeId);
    const nextIds = new Set(workingMemory?.nextPlanItemIds ?? []);
    const next = planItems.filter((i) => nextIds.has(i.id));
    return active ? [active, ...next] : next;
  });

  function mark(status: string, isActive: boolean): string {
    if (isActive || status === "active") return "●";
    switch (status) {
      case "completed":
        return "✓";
      case "skipped":
        return "~";
      case "blocked":
        return "⊘";
      default:
        return "›";
    }
  }
</script>

<div class="now">
  <div class="now-line">
    <span class="tag" class:live={streaming}>NOW</span>
    {#if current}
      <span class="intent" title={current.intent}>{current.intent}</span>
    {:else if streaming}
      <span class="intent idle">Working</span>
    {:else}
      <span class="intent idle">Idle</span>
    {/if}
    {#if streaming}<span class="pulse" aria-hidden="true"></span>{/if}
  </div>

  {#if hasObjective}
    <div class="obj">
      <span class="path mono">{path.length ? path.join(" / ") : currentObjective?.title ?? "objective"}</span>
      {#if currentObjective}
        <span class="status mono">{currentObjective.status.replace(/_/g, " ")}</span>
      {/if}
    </div>
  {:else}
    <div class="obj">
      <span class="no-obj">conversational — no objective</span>
    </div>
  {/if}

  {#if planItems.length}
    <div class="plan">
      <button
        class="plan-head"
        onclick={() => (planOpen = !planOpen)}
        aria-expanded={planOpen}
        title={planOpen ? "Collapse plan" : "Show full plan"}
      >
        <span class="tag">PLAN</span>
        {#if planProgress}<span class="prog mono">{planProgress.done}/{planProgress.total}</span>{/if}
        <span class="caret" class:open={planOpen} aria-hidden="true">▸</span>
      </button>

      {#if planOpen}
        <ol class="plan-full">
          {#each planItems as item (item.id)}
            {@const isActive = item.id === activeId || item.status === "active"}
            <li
              class="pf-item"
              class:active={isActive}
              class:done={item.status === "completed"}
              class:skipped={item.status === "skipped"}
              class:blocked={item.status === "blocked"}
              title={item.rationale ?? item.title}
            >
              <span class="pf-mark mono" aria-hidden="true">{mark(item.status, isActive)}</span>
              <span class="pf-title">{item.title}</span>
              {#if item.status === "skipped" || item.status === "blocked"}
                <span class="pf-state mono">{item.status}</span>
              {/if}
            </li>
          {/each}
        </ol>
      {:else if collapsedItems.length}
        <div class="plan-items">
          {#each collapsedItems as item (item.id)}
            {@const isActive = item.id === activeId || item.status === "active"}
            <span class="pi" class:active={isActive} class:next={!isActive} title={item.rationale ?? item.title}>
              {#if isActive}<span class="pi-mark" aria-hidden="true"></span>{/if}{item.title}
            </span>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</div>

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
  .intent {
    font-size: 12.5px; color: var(--text-primary); font-weight: 500; min-width: 0;
    display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;
  }
  .intent.idle { color: var(--text-muted); font-weight: 400; }
  .pulse {
    width: 7px; height: 7px; border-radius: var(--r-full);
    background: var(--status-info); flex: none;
    animation: now-pulse 1.4s ease-in-out infinite;
  }
  @keyframes now-pulse { 0%, 100% { opacity: 0.35; } 50% { opacity: 1; } }

  .obj {
    display: flex; align-items: baseline; gap: var(--s3);
    padding-left: calc(9px + var(--s2)); min-width: 0;
  }
  .path {
    font-size: 10px; color: var(--text-muted);
    min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    /* Head truncation — keep the leaf objective visible, clip the ancestry. */
    direction: rtl; text-align: left; unicode-bidi: isolate;
  }
  .status { font-size: 9.5px; color: var(--text-muted); flex: none; margin-left: auto; }
  .no-obj { font-size: 10px; color: var(--text-muted); font-style: italic; }

  .plan { display: flex; flex-direction: column; gap: var(--s1); padding-top: var(--s1); border-top: 1px solid var(--border-subtle); }
  .plan-head {
    display: flex; align-items: baseline; gap: var(--s2);
    background: none; border: none; padding: 0; margin: 0;
    cursor: pointer; font: inherit; color: inherit; text-align: left;
  }
  .plan-head:focus-visible { outline: 2px solid var(--focus); outline-offset: 2px; border-radius: var(--r-sm); }
  .prog { font-size: 10px; color: var(--text-muted); }
  .caret { font-size: 9px; color: var(--text-muted); transition: transform 0.12s; }
  .caret.open { transform: rotate(90deg); }

  .plan-items { display: flex; gap: var(--s2); flex-wrap: wrap; min-width: 0; }
  .pi { display: inline-flex; align-items: center; gap: 5px; font-size: 11px; color: var(--text-muted); }
  .pi.active { color: var(--text-secondary); }
  .pi.active .pi-mark {
    width: 6px; height: 6px; border-radius: var(--r-full); background: var(--accent); flex: none;
  }
  .pi.next { opacity: 0.7; }
  .pi.next::before { content: "›"; margin-right: 3px; color: var(--text-muted); }

  .plan-full { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  .pf-item { display: flex; align-items: baseline; gap: var(--s2); font-size: 11px; color: var(--text-muted); min-width: 0; }
  .pf-mark { flex: none; width: 12px; text-align: center; font-size: 10px; }
  .pf-title { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pf-item.active { color: var(--text-primary); }
  .pf-item.active .pf-mark { color: var(--accent); }
  .pf-item.done { opacity: 0.65; }
  .pf-item.done .pf-mark { color: var(--status-success); }
  .pf-item.skipped { opacity: 0.55; }
  .pf-item.skipped .pf-title { text-decoration: line-through; }
  .pf-item.blocked .pf-mark { color: var(--status-warning); }
  .pf-state { flex: none; margin-left: auto; font-size: 9px; color: var(--text-muted); }
</style>
