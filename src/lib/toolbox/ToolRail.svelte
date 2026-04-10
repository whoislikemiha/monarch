<script lang="ts">
  import { sortedTools } from "./registry";

  let {
    openToolIds,
    ontoggle,
  }: {
    openToolIds: string[];
    ontoggle: (id: string) => void;
  } = $props();

  let tools = $derived(sortedTools());
  let openSet = $derived(new Set(openToolIds));
</script>

<div class="tool-rail" role="toolbar" aria-label="Toolbox rail">
  {#each tools as tool (tool.id)}
    <button
      type="button"
      class="rail-btn"
      class:active={openSet.has(tool.id)}
      title={tool.title}
      aria-label={tool.title}
      aria-pressed={openSet.has(tool.id)}
      onclick={() => ontoggle(tool.id)}
    >
      {@html tool.icon}
    </button>
  {/each}
</div>

<style>
  .tool-rail {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    width: 44px;
    flex-shrink: 0;
    padding: 8px 0;
    background: var(--bg-sidebar, #0c0816);
    border-left: 1px solid var(--border-subtle, #35274f);
    overflow: hidden;
  }

  .rail-btn {
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: 1px solid transparent;
    border-radius: 6px;
    background: transparent;
    color: var(--text-muted, #9aa0a6);
    cursor: pointer;
    transition: background 0.15s, color 0.15s, border-color 0.15s;
  }

  .rail-btn:hover {
    background: var(--bg-panel-2, #201734);
    color: var(--text-primary, #f2f4f8);
  }

  .rail-btn:focus-visible {
    outline: none;
    border-color: var(--accent-purple, #be95ff);
  }

  .rail-btn.active {
    color: var(--accent-purple, #be95ff);
    background: rgba(190, 149, 255, 0.12);
  }

  .rail-btn :global(svg) {
    width: 18px;
    height: 18px;
  }
</style>
