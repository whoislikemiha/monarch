<script lang="ts">
  /**
   * Workspace host: center surface (active view) + a resizable right dock of
   * pinnable inspector panels + the inspector icon rail that toggles them.
   * The dock is shared across views; pinned panels persist across switches.
   */
  import { viewStore } from "./viewStore.svelte";
  import AgentsView from "$lib/views/AgentsView.svelte";
  import ProjectsView from "$lib/views/ProjectsView.svelte";
  import { layoutStore } from "$lib/layout/layoutStore.svelte";
  import { PANELS, getPanel } from "$lib/layout/panelRegistry";
  import { agentStore } from "$lib/stores/agentStore.svelte";
  import { liveAgentStore } from "$lib/toolbox/liveAgentStore.svelte";
  import type { AgentContext } from "$lib/toolbox/types";
  import Splitter from "$lib/ui/Splitter.svelte";
  import TileStack from "$lib/ui/TileStack.svelte";

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
    <aside class="dock" style="width:{layoutStore.dockWidth}px" data-density="compact">
      <TileStack
        ids={layoutStore.openPanels}
        axis="v"
        onreorder={(from, to) => layoutStore.reorderPanels(from, to)}
        size={(id) => layoutStore.panelHeight(id)}
        setSize={(id, px) => layoutStore.setPanelHeight(id, px)}
      >
        {#snippet header(id)}
          {@const def = getPanel(id)}
          <span class="panel-title">{def?.title ?? id}</span>
          <button class="panel-btn" class:on={layoutStore.isPinned(id)} title={layoutStore.isPinned(id) ? "Unpin" : "Pin (keep open)"} aria-label="Pin" onclick={() => layoutStore.togglePin(id)}>
            <svg viewBox="0 0 16 16" width="11" height="11" fill={layoutStore.isPinned(id) ? "currentColor" : "none"} stroke="currentColor" stroke-width="1.4"><path d="M6 2h4l-.5 4 2 2v1H4.5v-1l2-2L6 2z"/><path d="M8 9v5"/></svg>
          </button>
          <button class="panel-btn" title="Close" aria-label="Close" onclick={() => layoutStore.close(id)}>
            <svg viewBox="0 0 16 16" width="11" height="11" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M4 4l8 8M12 4l-8 8"/></svg>
          </button>
        {/snippet}
        {#snippet body(id)}
          {@const def = getPanel(id)}
          {#if def}
            {@const Content = def.component}
            <div class="panel-content"><Content {agentContext} /></div>
          {/if}
        {/snippet}
      </TileStack>
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
  .panel-title { font-size: 10px; font-weight: 600; letter-spacing: 0.12em; text-transform: uppercase; color: var(--text-secondary); flex: 1; min-width: 0; }
  .panel-btn {
    width: 22px; height: 22px; display: inline-flex; align-items: center; justify-content: center;
    background: none; border: none; border-radius: var(--r-sm); color: var(--text-muted); cursor: pointer; flex: none;
  }
  .panel-btn:hover { background: var(--bg-raised); color: var(--text-primary); }
  .panel-btn.on { color: var(--accent); }
  .panel-content { flex: 1; min-height: 0; overflow-y: auto; }
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
