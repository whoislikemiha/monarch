<script lang="ts">
  /**
   * Workspace host: center surface (active view) + a resizable right dock of
   * pinnable inspector panels + the inspector icon rail that toggles them.
   * The dock is shared across views; pinned panels persist across switches.
   */
  import { viewStore } from "./viewStore.svelte";
  import Panel from "./Panel.svelte";
  import AgentsView from "$lib/views/AgentsView.svelte";
  import ProjectsView from "$lib/views/ProjectsView.svelte";
  import { layoutStore } from "$lib/layout/layoutStore.svelte";
  import { PANELS } from "$lib/layout/panelRegistry";
  import { agentStore } from "$lib/stores/agentStore.svelte";
  import { liveAgentStore } from "$lib/toolbox/liveAgentStore.svelte";
  import type { AgentContext } from "$lib/toolbox/types";
  import Splitter from "$lib/ui/Splitter.svelte";

  let activeAgent = $derived(agentStore.getAgent(agentStore.activeTabId ?? ""));
  let live = $derived(activeAgent ? liveAgentStore.byAgent.get(activeAgent.id) ?? null : null);
  let project = $derived(
    activeAgent?.projectId ? agentStore.projects.find((p) => p.id === activeAgent!.projectId) : undefined,
  );
  let agentContext: AgentContext = $derived(
    activeAgent && live
      ? {
          agentId: activeAgent.id,
          agent: activeAgent,
          live,
          setup: { customPrompt: null, projectInstructions: project?.instructions ?? null },
        }
      : null,
  );

  // Dock is on the right; dragging the handle left (negative dx) grows it.
  function resizeDock(dx: number) {
    layoutStore.setWidth(layoutStore.dockWidth - dx);
  }
</script>

<div class="host">
  <div class="center">
    {#if viewStore.activeView === "agents"}
      <AgentsView />
    {:else}
      <ProjectsView />
    {/if}
  </div>

  {#if layoutStore.openPanels.length > 0}
    <Splitter axis="x" onresize={resizeDock} />
    <aside class="dock" style="width:{layoutStore.dockWidth}px">
      {#each layoutStore.openPanels as id, i (id)}
        {@const last = i === layoutStore.openPanels.length - 1}
        <div class="dock-item" style={last ? "flex:1 1 auto" : `height:${layoutStore.panelHeight(id)}px`}>
          <Panel panelId={id} {agentContext} />
        </div>
        {#if !last}
          <Splitter axis="y" onresize={(d) => layoutStore.setPanelHeight(id, layoutStore.panelHeight(id) + d)} />
        {/if}
      {/each}
    </aside>
  {/if}

  <nav class="rail" aria-label="Inspectors">
    {#each PANELS as panel (panel.id)}
      <button
        class="rail-btn"
        class:on={layoutStore.isOpen(panel.id)}
        title={panel.title}
        aria-label={panel.title}
        aria-pressed={layoutStore.isOpen(panel.id)}
        onclick={() => layoutStore.toggle(panel.id)}
      >
        {@html panel.icon}
      </button>
    {/each}
  </nav>
</div>

<style>
  .host {
    flex: 1;
    display: flex;
    flex-direction: row;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
  .center {
    flex: 1;
    display: flex;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
  .dock {
    flex: none;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg-base);
    border-left: 1px solid var(--border-subtle);
    overflow: hidden;
  }
  .dock-item { flex: none; min-height: 0; display: flex; overflow: hidden; }
  .rail {
    width: 44px;
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--s1);
    padding-top: var(--s2);
    background: var(--bg-sink);
    border-left: 1px solid var(--border-subtle);
  }
  .rail-btn {
    width: 32px;
    height: 32px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--r-md);
    color: var(--text-muted);
    cursor: pointer;
    transition: background 0.12s, color 0.12s, border-color 0.12s;
  }
  .rail-btn :global(svg) { width: 16px; height: 16px; }
  .rail-btn:hover { background: var(--bg-raised); color: var(--text-secondary); }
  .rail-btn.on { background: var(--bg-overlay); color: var(--accent); border-color: var(--accent-border-subtle); }
</style>
