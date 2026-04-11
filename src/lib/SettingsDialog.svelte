<script lang="ts">
  let {
    onclose,
  }: {
    onclose: () => void;
  } = $props();

  const categories = [
    { id: "general", label: "General" },
    { id: "appearance", label: "Appearance" },
    { id: "agent-defaults", label: "Agent Defaults" },
    { id: "keybindings", label: "Keybindings" },
  ];

  let activeCategory = $state("general");

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      onclose();
      e.stopPropagation();
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="overlay" onclick={onclose} role="presentation">
  <div
    class="dialog"
    onclick={(e: MouseEvent) => e.stopPropagation()}
    onkeydown={handleKeydown}
    role="dialog"
    tabindex="-1"
  >
    <div class="dialog-header">
      <h2>Settings</h2>
      <button class="btn-close" onclick={onclose}>Close</button>
    </div>
    <div class="dialog-body">
      <nav class="category-nav">
        {#each categories as cat (cat.id)}
          <button
            class="category-btn"
            class:active={activeCategory === cat.id}
            onclick={() => (activeCategory = cat.id)}
          >
            {cat.label}
          </button>
        {/each}
      </nav>
      <div class="category-content">
        <h3>{categories.find((c) => c.id === activeCategory)?.label}</h3>
        <p class="placeholder-text">No settings configured yet.</p>
      </div>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }

  .dialog {
    background: var(--bg-panel, #171126);
    border: 1px solid var(--border-subtle, #35274f);
    border-radius: 12px;
    width: 720px;
    max-width: 90vw;
    height: 520px;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .dialog-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid var(--border-subtle, #35274f);
    flex-shrink: 0;
  }

  .dialog-header h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary, #f2f4f8);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .dialog-body {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .category-nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 12px;
    width: 180px;
    flex-shrink: 0;
    border-right: 1px solid var(--border-subtle, #35274f);
    overflow-y: auto;
  }

  .category-btn {
    padding: 8px 12px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text-secondary, #dde1e6);
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    text-align: left;
    transition: background 0.15s, color 0.15s;
  }

  .category-btn:hover {
    background: var(--bg-panel-2, #201734);
    color: var(--text-primary, #f2f4f8);
  }

  .category-btn.active {
    background: rgba(190, 149, 255, 0.12);
    color: var(--accent-purple, #be95ff);
  }

  .category-content {
    flex: 1;
    padding: 20px 24px;
    overflow-y: auto;
  }

  .category-content h3 {
    margin: 0 0 16px 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary, #f2f4f8);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .placeholder-text {
    font-size: 12px;
    color: var(--text-muted, #8f7aa8);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    margin: 0;
  }

  .btn-close {
    padding: 6px 12px;
    border: 1px solid var(--border-subtle, #35274f);
    border-radius: 6px;
    background: transparent;
    color: var(--text-secondary, #dde1e6);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn-close:hover {
    background: var(--bg-panel-2, #201734);
  }
</style>
