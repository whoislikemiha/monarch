<script lang="ts">
  /**
   * Persistent top chrome: brand · context breadcrumb · Agents⇄Projects lens
   * toggle · ⌘K (stub) · theme switch · captain chip.
   *
   * Pure presentation + the view/theme stores. No agent logic lives here.
   */
  import { viewStore, type ViewId } from "./viewStore.svelte";
  import { listThemes, type ThemeId } from "$lib/themes";

  interface Props {
    /** Breadcrumb crumbs, left → right (e.g. ["monarch", "Onyx"]). */
    crumbs?: string[];
    onCommandPalette?: () => void;
  }
  let { crumbs = [], onCommandPalette }: Props = $props();

  const lenses: { id: ViewId; label: string }[] = [
    { id: "agents", label: "Agents" },
    { id: "projects", label: "Projects" },
  ];

  const themeOptions = listThemes();

  function onThemeChange(e: Event) {
    viewStore.setTheme((e.currentTarget as HTMLSelectElement).value as ThemeId);
  }
</script>

<header class="topbar">
  <div class="brand">
    <span class="mark" aria-hidden="true"></span>
    <span class="word">monarch</span>
  </div>

  <nav class="crumbs" aria-label="Context">
    {#each crumbs as crumb, i (i)}
      {#if i > 0}<span class="sep" aria-hidden="true">›</span>{/if}
      <span class="crumb" class:leaf={i === crumbs.length - 1}>{crumb}</span>
    {/each}
  </nav>

  <div class="spacer"></div>

  <div class="lens" role="tablist" aria-label="View">
    {#each lenses as lens (lens.id)}
      <button
        role="tab"
        aria-selected={viewStore.activeView === lens.id}
        class="lens-btn"
        class:active={viewStore.activeView === lens.id}
        onclick={() => viewStore.setView(lens.id)}
      >
        {lens.label}
      </button>
    {/each}
  </div>

  <button class="cmdk" onclick={() => onCommandPalette?.()} title="Command palette (coming soon)" disabled>
    <span class="mono">⌘K</span>
  </button>

  <label class="theme" title="Theme">
    <select class="theme-select mono" value={viewStore.themeId} onchange={onThemeChange}>
      {#each themeOptions as t (t.id)}
        <option value={t.id}>{t.label}</option>
      {/each}
    </select>
  </label>

  <div class="captain" title="Captain">
    <span class="cap-dot" aria-hidden="true"></span>
    <span class="cap-label">Captain</span>
  </div>
</header>

<style>
  .topbar {
    display: flex;
    align-items: center;
    gap: var(--s3);
    height: 44px;
    flex: none;
    padding: 0 var(--s3);
    background: var(--bg-sink);
    border-bottom: 1px solid var(--border-subtle);
    user-select: none;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: var(--s2);
    padding-right: var(--s2);
  }
  .brand .mark {
    width: 12px;
    height: 12px;
    background: var(--accent);
    border-radius: var(--r-sm);
    transform: rotate(45deg);
  }
  .brand .word {
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--text-primary);
  }

  .crumbs {
    display: flex;
    align-items: center;
    gap: var(--s2);
    min-width: 0;
    overflow: hidden;
  }
  .crumbs .sep { color: var(--text-muted); font-size: 12px; }
  .crumb {
    font-size: 12px;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .crumb.leaf { color: var(--text-secondary); font-weight: 500; }

  .spacer { flex: 1; }

  .lens {
    display: flex;
    padding: 2px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-md);
  }
  .lens-btn {
    font: inherit;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-muted);
    background: transparent;
    border: none;
    padding: 4px var(--s3);
    border-radius: var(--r-sm);
    cursor: pointer;
    transition: background 0.14s, color 0.14s;
  }
  .lens-btn:hover { color: var(--text-secondary); }
  .lens-btn.active {
    background: var(--bg-overlay);
    color: var(--text-primary);
  }
  .lens-btn:focus-visible { outline: 2px solid var(--focus); outline-offset: 1px; }

  .cmdk {
    display: flex;
    align-items: center;
    height: 26px;
    padding: 0 var(--s2);
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-md);
    color: var(--text-muted);
    font-size: 11px;
    cursor: not-allowed;
    opacity: 0.7;
  }

  .theme { display: flex; }
  .theme-select {
    font-size: 11px;
    color: var(--text-secondary);
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-md);
    padding: 4px 22px 4px var(--s2);
    cursor: pointer;
    appearance: none;
    background-image: linear-gradient(45deg, transparent 50%, var(--text-muted) 50%),
      linear-gradient(135deg, var(--text-muted) 50%, transparent 50%);
    background-position: calc(100% - 13px) 11px, calc(100% - 9px) 11px;
    background-size: 4px 4px, 4px 4px;
    background-repeat: no-repeat;
  }
  .theme-select:focus-visible { outline: 2px solid var(--focus); outline-offset: 1px; }

  .captain {
    display: flex;
    align-items: center;
    gap: var(--s2);
    padding: 3px var(--s2) 3px 3px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-full);
  }
  .captain .cap-dot {
    width: 18px;
    height: 18px;
    border-radius: var(--r-full);
    background: var(--bg-overlay);
    border: 1px solid var(--accent-border-subtle);
  }
  .captain .cap-label { font-size: 11px; color: var(--text-secondary); }
</style>
