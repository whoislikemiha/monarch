<script lang="ts">
  import { invoke } from "$lib/api";
  import { listThemes, applyTheme, getActiveTheme, type ThemeId, type Theme } from "./themes";
  import KeybindingsSettings from "./KeybindingsSettings.svelte";
  import MemorySettings from "./MemorySettings.svelte";

  let {
    onclose,
    zoomLevel,
    onzoom,
  }: {
    onclose: () => void;
    zoomLevel: number;
    onzoom: (level: number) => void;
  } = $props();

  const categories = [
    { id: "general", label: "General" },
    { id: "appearance", label: "Appearance" },
    { id: "agent-defaults", label: "Agent Defaults" },
    { id: "memory", label: "Memory" },
    { id: "keybindings", label: "Keybindings" },
  ];

  let activeCategory = $state("general");
  let activeThemeId = $state(getActiveTheme().name);
  let themes = $derived(listThemes());

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      onclose();
      e.stopPropagation();
    }
  }

  function selectTheme(id: ThemeId) {
    const resolvedId = applyTheme(id);
    activeThemeId = resolvedId;
    invoke("db_set_ui_state", { key: "theme", value: JSON.stringify(resolvedId) }).catch(() => {});
  }

  let zoomPercent = $derived(Math.round(zoomLevel * 100));
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

        {#if activeCategory === "appearance"}
          <div class="theme-section">
            <span class="section-label">Theme</span>
            <div class="theme-grid">
              {#each themes as { id, label, theme } (id)}
                <button
                  class="theme-card"
                  class:active={activeThemeId === id}
                  onclick={() => selectTheme(id)}
                >
                  <div class="theme-preview">
                    <div class="preview-sidebar" style:background={theme.bgSidebar}></div>
                    <div class="preview-main" style:background={theme.bgPanel}>
                      <div class="preview-header" style:background={theme.bgSidebar} style:border-bottom="1px solid {theme.borderSubtle}"></div>
                      <div class="preview-content">
                        <div class="preview-line" style:background={theme.textMuted}></div>
                        <div class="preview-line short" style:background={theme.accent}></div>
                        <div class="preview-line" style:background={theme.textMuted}></div>
                      </div>
                      <div class="preview-input" style:background={theme.bgPanel2} style:border-top="1px solid {theme.borderSubtle}"></div>
                    </div>
                  </div>
                  <span class="theme-label">{label}</span>
                  {#if activeThemeId === id}
                    <span class="theme-active-badge">Active</span>
                  {/if}
                </button>
              {/each}
            </div>
          </div>

          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">Zoom Level</span>
              <span class="setting-hint">Ctrl+Plus / Ctrl+Minus / Ctrl+0 to reset</span>
            </div>
            <div class="zoom-controls">
              <button class="zoom-btn" onclick={() => onzoom(zoomLevel - 0.05)} disabled={zoomPercent <= 50}>−</button>
              <span class="zoom-value">{zoomPercent}%</span>
              <button class="zoom-btn" onclick={() => onzoom(zoomLevel + 0.05)} disabled={zoomPercent >= 200}>+</button>
              <button class="zoom-reset" onclick={() => onzoom(1.0)} disabled={zoomPercent === 100}>Reset</button>
            </div>
          </div>
        {:else if activeCategory === "memory"}
          <MemorySettings />
        {:else if activeCategory === "keybindings"}
          <KeybindingsSettings />
        {:else}
          <p class="placeholder-text">No settings configured yet.</p>
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: var(--overlay-backdrop);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }

  .dialog {
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
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
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }

  .dialog-header h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
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
    border-right: 1px solid var(--border-subtle);
    overflow-y: auto;
  }

  .category-btn {
    padding: 8px 12px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    text-align: left;
    transition: background 0.15s, color 0.15s;
  }

  .category-btn:hover {
    background: var(--bg-panel-2);
    color: var(--text-primary);
  }

  .category-btn.active {
    background: var(--accent-bg-hover);
    color: var(--accent);
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
    color: var(--text-primary);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .placeholder-text {
    font-size: 12px;
    color: var(--text-muted);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    margin: 0;
  }

  .btn-close {
    padding: 6px 12px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn-close:hover {
    background: var(--bg-panel-2);
  }

  /* Appearance tab */
  .theme-section {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .section-label {
    font-size: 11px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .theme-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 12px;
  }

  .theme-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    border: 2px solid var(--border-subtle);
    border-radius: 10px;
    background: var(--bg-panel-2);
    cursor: pointer;
    transition: border-color 0.15s, background 0.15s;
    text-align: left;
  }

  .theme-card:hover {
    border-color: var(--border-strong);
    background: var(--bg-panel-3);
  }

  .theme-card.active {
    border-color: var(--accent);
  }

  .theme-preview {
    display: flex;
    border-radius: 6px;
    overflow: hidden;
    height: 72px;
    border: 1px solid var(--border-subtle);
  }

  .preview-sidebar {
    width: 28px;
    flex-shrink: 0;
  }

  .preview-main {
    flex: 1;
    display: flex;
    flex-direction: column;
  }

  .preview-header {
    height: 10px;
    flex-shrink: 0;
  }

  .preview-content {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px 8px;
    justify-content: center;
  }

  .preview-line {
    height: 3px;
    border-radius: 2px;
    width: 80%;
    opacity: 0.5;
  }

  .preview-line.short {
    width: 50%;
    opacity: 0.8;
  }

  .preview-input {
    height: 10px;
    flex-shrink: 0;
  }

  .theme-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .theme-active-badge {
    font-size: 9px;
    color: var(--accent);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  /* --- Settings rows --- */

  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 0;
    border-bottom: 1px solid var(--border-subtle);
  }

  .setting-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .setting-label {
    font-size: 12px;
    color: var(--text-primary);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .setting-hint {
    font-size: 10px;
    color: var(--text-muted);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .zoom-controls {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .zoom-btn {
    width: 28px;
    height: 28px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 14px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background 0.15s, color 0.15s;
  }

  .zoom-btn:hover:not(:disabled) {
    background: var(--bg-panel-2);
    color: var(--text-primary);
  }

  .zoom-btn:disabled {
    opacity: 0.3;
    cursor: default;
  }

  .zoom-value {
    min-width: 44px;
    text-align: center;
    font-size: 12px;
    color: var(--text-primary);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    font-variant-numeric: tabular-nums;
  }

  .zoom-reset {
    padding: 4px 10px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: transparent;
    color: var(--text-muted);
    font-size: 10px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    margin-left: 4px;
    transition: background 0.15s, color 0.15s;
  }

  .zoom-reset:hover:not(:disabled) {
    background: var(--bg-panel-2);
    color: var(--text-primary);
  }

  .zoom-reset:disabled {
    opacity: 0.3;
    cursor: default;
  }
</style>
