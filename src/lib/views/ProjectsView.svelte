<script lang="ts">
  /**
   * PROJECTS lens (what). Project picker → campaign tree (S2) + objective
   * detail (S3). The campaign is per-project; we drive objectiveStore through a
   * "lens" agent in that project (the active shadow when it belongs there, else
   * the project's first shadow).
   */
  import type { ObjectiveRow } from "$lib/bindings";
  import { agentStore } from "$lib/stores/agentStore.svelte";
  import { objectiveStore } from "$lib/toolbox/objectiveStore.svelte";
  import { layoutStore } from "$lib/layout/layoutStore.svelte";
  import Splitter from "$lib/ui/Splitter.svelte";
  import CampaignTree from "$lib/board/CampaignTree.svelte";
  import ObjectiveDetail from "$lib/board/ObjectiveDetail.svelte";

  let boardEl: HTMLDivElement | undefined = $state();
  function resizeBoard(dx: number) {
    const w = boardEl?.clientWidth ?? 0;
    if (w > 0) layoutStore.setBoardFrac(layoutStore.boardFrac + dx / w);
  }

  let selectedProjectId = $state<string | null>(null);
  let selectedObjective = $state<ObjectiveRow | null>(null);

  let projects = $derived(agentStore.projects);

  // Default the selected project to the active agent's project, else the first.
  $effect(() => {
    if (selectedProjectId && projects.some((p) => p.id === selectedProjectId)) return;
    const active = agentStore.getAgent(agentStore.activeTabId ?? "");
    selectedProjectId = active?.projectId ?? projects[0]?.id ?? null;
  });

  let lensAgentId = $derived.by(() => {
    if (!selectedProjectId) return null;
    const active = agentStore.getAgent(agentStore.activeTabId ?? "");
    if (active?.projectId === selectedProjectId) return active.id;
    return agentStore.agents.find((a) => a.projectId === selectedProjectId)?.id ?? null;
  });

  // Load the campaign tree for the lens agent.
  $effect(() => {
    const id = lensAgentId;
    selectedObjective = null;
    if (!id) return;
    objectiveStore.ensure(id);
    objectiveStore.refresh(id).catch(() => {});
  });

  let selectedProject = $derived(projects.find((p) => p.id === selectedProjectId));
</script>

<div class="view">
  {#if projects.length === 0}
    <div class="empty">
      <div class="glyph" aria-hidden="true"></div>
      <h4>No projects yet</h4>
      <p>Extract a shadow inside a git project and its campaign will appear here.</p>
    </div>
  {:else}
    <header class="phead">
      <div class="projects">
        {#each projects as p (p.id)}
          <button class="pchip" class:active={p.id === selectedProjectId} onclick={() => (selectedProjectId = p.id)}>
            <span class="slash">/</span>{p.name}
          </button>
        {/each}
      </div>
    </header>

    <div class="board" bind:this={boardEl}>
      <section class="tree-pane" style="flex-grow:{layoutStore.boardFrac}" aria-label="Campaign">
        <div class="pane-head"><span class="t">Campaign</span></div>
        <div class="pane-body">
          {#if lensAgentId}
            <CampaignTree
              agentId={lensAgentId}
              selectedId={selectedObjective?.id ?? null}
              onselect={(o) => (selectedObjective = o)}
            />
          {:else}
            <div class="hint mono">No shadow assigned to {selectedProject?.name ?? "this project"} yet.</div>
          {/if}
        </div>
      </section>
      <Splitter axis="x" onresize={resizeBoard} />
      <section class="detail-pane" style="flex-grow:{1 - layoutStore.boardFrac}" aria-label="Objective">
        {#if selectedObjective && lensAgentId}
          {#key selectedObjective.id}
            <ObjectiveDetail agentId={lensAgentId} objective={selectedObjective} />
          {/key}
        {:else}
          <div class="detail-empty">
            <p>Select an objective to see its brief, plan, and report.</p>
          </div>
        {/if}
      </section>
    </div>
  {/if}
</div>

<style>
  .view { flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column; background: var(--bg-base); }

  .phead { display: flex; align-items: center; height: 40px; flex: none; padding: 0 var(--s4); border-bottom: 1px solid var(--border-subtle); }
  .projects { display: flex; gap: var(--s2); overflow-x: auto; }
  .pchip {
    font: inherit; font-size: 12px; font-weight: 500; color: var(--text-muted);
    background: transparent; border: 1px solid var(--border-subtle); border-radius: var(--r-md);
    padding: 3px var(--s3); cursor: pointer; white-space: nowrap;
  }
  .pchip:hover { color: var(--text-secondary); background: var(--bg-raised); }
  .pchip.active { color: var(--text-primary); border-color: var(--accent-border-subtle); background: var(--bg-overlay); }
  .pchip .slash { color: var(--accent); font-weight: 700; margin-right: 2px; }

  .board { flex: 1; display: flex; flex-direction: row; min-height: 0; min-width: 0; }
  .tree-pane { flex: 1 1 0; display: flex; flex-direction: column; min-width: 0; min-height: 0; }
  .detail-pane { flex: 1 1 0; min-width: 0; min-height: 0; overflow-y: auto; }
  .pane-head { display: flex; align-items: center; height: 30px; flex: none; padding: 0 var(--s4); border-bottom: 1px solid var(--border-subtle); }
  .pane-head .t { font-size: 10px; font-weight: 600; letter-spacing: 0.14em; text-transform: uppercase; color: var(--text-muted); }
  .pane-body { flex: 1; min-height: 0; overflow-y: auto; padding: var(--s4); }
  .hint { font-size: 11px; color: var(--text-muted); padding: var(--s3); }

  .detail-empty { display: flex; align-items: center; justify-content: center; height: 100%; padding: var(--s6); }
  .detail-empty p { font-size: 12px; color: var(--text-muted); max-width: 32ch; text-align: center; line-height: 1.6; }

  .empty { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; text-align: center; gap: var(--s3); padding: var(--s7) var(--s5); }
  .empty .glyph { width: 42px; height: 42px; border: 1.5px solid var(--border-strong); border-radius: var(--r-sm); margin-bottom: var(--s2); }
  .empty h4 { font-size: 14px; color: var(--text-primary); }
  .empty p { font-size: 12px; color: var(--text-muted); max-width: 38ch; line-height: 1.6; }
</style>
