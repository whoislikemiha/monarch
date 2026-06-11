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

  // --- Dock resize (drag the handle on the dock's left edge) ---
  let resizing = $state(false);
  function startResize(e: PointerEvent) {
    resizing = true;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function onResize(e: PointerEvent) {
    if (!resizing) return;
    // Dock is on the right; width grows as the pointer moves left.
    layoutStore.setWidth(window.innerWidth - e.clientX - 44);
  }
  function endResize(e: PointerEvent) {
    resizing = false;
    (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
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
    <div
      class="resize"
      class:active={resizing}
      role="separator"
      aria-label="Resize panels"
      onpointerdown={startResize}
      onpointermove={onResize}
      onpointerup={endResize}
    ></div>
    <aside class="dock" style="width:{layoutStore.dockWidth}px">
      {#each layoutStore.openPanels as id (id)}
        <Panel panelId={id} {agentContext} />
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
  .resize {
    width: 4px;
    flex: none;
    cursor: col-resize;
    background: var(--border-subtle);
    transition: background 0.12s;
  }
  .resize:hover, .resize.active { background: var(--accent); }
  .dock {
    flex: none;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg-base);
    border-left: 1px solid var(--border-subtle);
    overflow: hidden;
  }
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
