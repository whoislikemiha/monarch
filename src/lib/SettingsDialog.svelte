<script lang="ts">
  /**
   * App settings — built on the design-system Modal with a section nav.
   * Appearance (theme + zoom, moved here from the TopBar), Keybindings,
   * and Memory. Theme selection goes through viewStore so every consumer
   * of the active theme stays in sync.
   */
  import Modal from "./ui/Modal.svelte";
  import { listThemes, type ThemeId } from "./themes";
  import { viewStore } from "./shell/viewStore.svelte";
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

  const sections = [
    { id: "appearance", label: "Appearance" },
    { id: "keybindings", label: "Keybindings" },
    { id: "memory", label: "Memory" },
  ] as const;
  type SectionId = (typeof sections)[number]["id"];

  let active = $state<SectionId>("appearance");
  const themes = listThemes();

  function selectTheme(id: ThemeId) {
    viewStore.setTheme(id);
  }

  let zoomPercent = $derived(Math.round(zoomLevel * 100));
</script>

<Modal title="Settings" {onclose} width={760} flush>
  <div class="settings">
    <nav class="nav" aria-label="Settings sections">
      {#each sections as section (section.id)}
        <button
          class="nav-btn"
          class:active={active === section.id}
          onclick={() => (active = section.id)}
        >
          {section.label}
        </button>
      {/each}
    </nav>

    <div class="content">
      {#if active === "appearance"}
        <span class="head">Theme</span>
        <div class="theme-grid">
          {#each themes as { id, label, theme } (id)}
            <button
              class="theme-card"
              class:active={viewStore.themeId === id}
              onclick={() => selectTheme(id)}
            >
              <div class="preview" style:background={theme.bgApp}>
                <div class="p-side" style:background={theme.bgSidebar}></div>
                <div class="p-main" style:background={theme.bgPanel}>
                  <div
                    class="p-head"
                    style:background={theme.bgSidebar}
                    style:border-bottom="1px solid {theme.borderSubtle}"
                  ></div>
                  <div class="p-body">
                    <div class="p-line" style:background={theme.textMuted}></div>
                    <div class="p-line short" style:background={theme.accent}></div>
                    <div class="p-line" style:background={theme.textMuted}></div>
                  </div>
                  <div
                    class="p-input"
                    style:background={theme.bgPanel2}
                    style:border-top="1px solid {theme.borderSubtle}"
                  ></div>
                </div>
              </div>
              <span class="theme-name">
                {label}
                {#if viewStore.themeId === id}
                  <span class="active-dot" aria-label="Active"></span>
                {/if}
              </span>
            </button>
          {/each}
        </div>

        <span class="head">Window</span>
        <div class="setting-row">
          <div class="setting-info">
            <span class="setting-label">Zoom</span>
            <span class="setting-hint">Ctrl+Plus / Ctrl+Minus · Ctrl+0 resets</span>
          </div>
          <div class="zoom-controls">
            <button
              class="zoom-btn"
              aria-label="Zoom out"
              onclick={() => onzoom(zoomLevel - 0.05)}
              disabled={zoomPercent <= 50}>−</button
            >
            <span class="zoom-value mono">{zoomPercent}%</span>
            <button
              class="zoom-btn"
              aria-label="Zoom in"
              onclick={() => onzoom(zoomLevel + 0.05)}
              disabled={zoomPercent >= 200}>+</button
            >
            <button class="zoom-reset" onclick={() => onzoom(1.0)} disabled={zoomPercent === 100}>
              Reset
            </button>
          </div>
        </div>
      {:else if active === "keybindings"}
        <KeybindingsSettings />
      {:else if active === "memory"}
        <MemorySettings />
      {/if}
    </div>
  </div>
</Modal>

<style>
  .settings {
    display: flex;
    flex: 1;
    min-width: 0;
    height: min(540px, calc(100vh - 160px));
  }

  .nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 168px;
    flex: none;
    padding: var(--s3);
    border-right: 1px solid var(--border-subtle);
    background: var(--bg-base);
    overflow-y: auto;
  }
  .nav-btn {
    font: inherit;
    font-size: 12px;
    font-weight: 500;
    text-align: left;
    color: var(--text-secondary);
    background: transparent;
    border: none;
    border-radius: var(--r-sm);
    padding: var(--s2) var(--s3);
    cursor: pointer;
    transition: background 0.14s, color 0.14s;
  }
  .nav-btn:hover {
    background: var(--bg-raised);
    color: var(--text-primary);
  }
  .nav-btn.active {
    background: var(--bg-overlay);
    color: var(--text-primary);
  }
  .nav-btn:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
  }

  .content {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: var(--s3);
    padding: var(--s4) var(--s5);
    overflow-y: auto;
  }

  .head {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--text-muted);
  }
  .head:not(:first-child) {
    margin-top: var(--s3);
  }

  .mono {
    font-family: "JetBrains Mono", monospace;
  }

  /* --- Theme cards --- */
  .theme-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: var(--s3);
  }
  .theme-card {
    display: flex;
    flex-direction: column;
    gap: var(--s2);
    padding: var(--s2);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-md);
    background: var(--bg-raised);
    cursor: pointer;
    text-align: left;
    transition: border-color 0.14s, background 0.14s;
  }
  .theme-card:hover {
    border-color: var(--border-strong);
  }
  .theme-card.active {
    border-color: var(--accent);
  }
  .theme-card:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
  }

  .preview {
    display: flex;
    height: 68px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-sm);
    overflow: hidden;
  }
  .p-side {
    width: 26px;
    flex: none;
  }
  .p-main {
    flex: 1;
    display: flex;
    flex-direction: column;
  }
  .p-head {
    height: 9px;
    flex: none;
  }
  .p-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: var(--s2);
    justify-content: center;
  }
  .p-line {
    height: 3px;
    border-radius: 2px;
    width: 80%;
    opacity: 0.5;
  }
  .p-line.short {
    width: 50%;
    opacity: 0.9;
  }
  .p-input {
    height: 9px;
    flex: none;
  }

  .theme-name {
    display: flex;
    align-items: center;
    gap: var(--s2);
    font-size: 12px;
    font-weight: 500;
    color: var(--text-primary);
  }
  .active-dot {
    width: 6px;
    height: 6px;
    border-radius: var(--r-full);
    background: var(--accent);
  }

  /* --- Setting rows --- */
  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s3);
    padding: var(--s2) 0;
  }
  .setting-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .setting-label {
    font-size: 12px;
    color: var(--text-primary);
  }
  .setting-hint {
    font-size: 10px;
    color: var(--text-muted);
  }

  .zoom-controls {
    display: flex;
    align-items: center;
    gap: var(--s2);
    flex: none;
  }
  .zoom-btn {
    width: 26px;
    height: 26px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font: inherit;
    font-size: 14px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--bg-raised);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background 0.14s, color 0.14s;
  }
  .zoom-btn:hover:not(:disabled) {
    background: var(--bg-overlay);
    color: var(--text-primary);
  }
  .zoom-btn:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .zoom-value {
    min-width: 44px;
    text-align: center;
    font-size: 11px;
    color: var(--text-primary);
    font-variant-numeric: tabular-nums;
  }
  .zoom-reset {
    font: inherit;
    font-size: 11px;
    padding: 4px var(--s2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    transition: background 0.14s, color 0.14s;
  }
  .zoom-reset:hover:not(:disabled) {
    background: var(--bg-raised);
    color: var(--text-primary);
  }
  .zoom-reset:disabled {
    opacity: 0.35;
    cursor: default;
  }
</style>
