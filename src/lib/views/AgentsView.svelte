<script lang="ts">
  /**
   * AGENTS lens (who). Shows the solo workspace for the selected agent, or an
   * invitation when nothing is selected.
   */
  import { agentStore } from "$lib/stores/agentStore.svelte";
  import SoloWorkspace from "$lib/workspace/SoloWorkspace.svelte";

  let activeAgent = $derived(agentStore.getAgent(agentStore.activeTabId ?? ""));
</script>

<div class="view">
  {#if activeAgent}
    {#key activeAgent.viewKey}
      <SoloWorkspace agent={activeAgent} />
    {/key}
  {:else}
    <div class="empty">
      <div class="glyph" aria-hidden="true"></div>
      <h4>No agents yet</h4>
      <p>Select an agent from the rail, or create a new one to begin. This is where you watch and talk to your agents.</p>
    </div>
  {/if}
</div>

<style>
  .view {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    background: var(--bg-base);
  }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    gap: var(--s3);
    padding: var(--s7) var(--s5);
  }
  .empty .glyph {
    width: 42px; height: 42px;
    border: 1.5px solid var(--border-strong);
    transform: rotate(45deg);
    border-radius: var(--r-sm);
    margin-bottom: var(--s2);
  }
  .empty h4 { font-size: 14px; color: var(--text-primary); }
  .empty p { font-size: 12px; color: var(--text-muted); max-width: 38ch; line-height: 1.6; }
</style>
