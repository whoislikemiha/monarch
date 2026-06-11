<script lang="ts">
  /**
   * Dockable panel chrome: title · pin · close, wrapping a registered inspector
   * component. Compact density (narrow inspector). Content is the panel's
   * registered component, fed the current agent context.
   */
  import type { AgentContext } from "$lib/toolbox/types";
  import { getPanel } from "$lib/layout/panelRegistry";
  import { layoutStore } from "$lib/layout/layoutStore.svelte";

  interface Props {
    panelId: string;
    agentContext: AgentContext;
  }
  let { panelId, agentContext }: Props = $props();

  let def = $derived(getPanel(panelId));
  let pinned = $derived(layoutStore.isPinned(panelId));
</script>

{#if def}
  {@const Content = def.component}
  <section class="panel" data-density="compact">
    <header class="panel-head">
      <span class="title">{def.title}</span>
      <div class="grow"></div>
      <button class="hbtn" class:on={pinned} title={pinned ? "Unpin" : "Pin (keep open)"} aria-label="Pin panel" onclick={() => layoutStore.togglePin(panelId)}>
        <svg viewBox="0 0 16 16" width="12" height="12" fill={pinned ? "currentColor" : "none"} stroke="currentColor" stroke-width="1.4"><path d="M6 2h4l-.5 4 2 2v1H4.5v-1l2-2L6 2z"/><path d="M8 9v5"/></svg>
      </button>
      <button class="hbtn" title="Close" aria-label="Close panel" onclick={() => layoutStore.close(panelId)}>
        <svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M4 4l8 8M12 4l-8 8"/></svg>
      </button>
    </header>
    <div class="panel-content">
      <Content {agentContext} />
    </div>
  </section>
{/if}

<style>
  .panel {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1 1 0;
    border-bottom: 1px solid var(--border-subtle);
    overflow: hidden;
  }
  .panel-head {
    display: flex;
    align-items: center;
    gap: var(--s2);
    height: 30px;
    flex: none;
    padding: 0 var(--s2) 0 var(--s3);
    background: var(--bg-sink);
    border-bottom: 1px solid var(--border-subtle);
  }
  .title { font-size: 10px; font-weight: 600; letter-spacing: 0.12em; text-transform: uppercase; color: var(--text-secondary); }
  .grow { flex: 1; }
  .hbtn {
    width: 22px; height: 22px; display: inline-flex; align-items: center; justify-content: center;
    background: none; border: none; border-radius: var(--r-sm); color: var(--text-muted); cursor: pointer;
  }
  .hbtn:hover { background: var(--bg-raised); color: var(--text-primary); }
  .hbtn.on { color: var(--accent); }
  .panel-content { flex: 1; min-height: 0; overflow-y: auto; }
</style>
