<script lang="ts">
  import { sortedTools } from "./registry";

  let {
    openToolIds,
    ontoggle,
    onsettings,
  }: {
    openToolIds: string[];
    ontoggle: (id: string) => void;
    onsettings: () => void;
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
  <button
    type="button"
    class="rail-btn settings-btn"
    title="Settings"
    aria-label="Settings"
    onclick={onsettings}
  >
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
    </svg>
  </button>
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
    background: var(--bg-sidebar);
    border-left: 1px solid var(--border-subtle);
    overflow: hidden;
  }

  .settings-btn {
    margin-top: auto;
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
    color: var(--text-muted);
    cursor: pointer;
    transition: background 0.15s, color 0.15s, border-color 0.15s;
  }

  .rail-btn:hover {
    background: var(--bg-panel-2);
    color: var(--text-primary);
  }

  .rail-btn:focus-visible {
    outline: none;
    border-color: var(--accent);
  }

  .rail-btn.active {
    color: var(--accent);
    background: var(--accent-bg-hover);
  }

  .rail-btn :global(svg) {
    width: 18px;
    height: 18px;
  }
</style>
