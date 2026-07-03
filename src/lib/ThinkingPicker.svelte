<script lang="ts">
  import {
    availableLevels,
    displayLabel,
    levelIntensity,
    supportsThinking,
    type ThinkingLevel,
  } from "./thinking";
  import ThinkingMeter from "./ThinkingMeter.svelte";

  let {
    provider,
    model,
    value = $bindable<string>("off"),
    onchange,
    direction = "down",
  }: {
    provider: string;
    model: string;
    value?: string;
    onchange?: (level: string) => void;
    direction?: "up" | "down";
  } = $props();

  let open = $state(false);
  let rootEl: HTMLDivElement | undefined = $state(undefined);

  let levels = $derived(availableLevels(provider, model));
  let enabled = $derived(supportsThinking(provider, model));
  let currentLevel = $derived((value || "off") as ThinkingLevel);
  let currentLabel = $derived(displayLabel(provider, model, currentLevel));
  let currentIntensity = $derived(levelIntensity(currentLevel));

  function handleDocumentClick(e: MouseEvent) {
    if (!open || !rootEl) return;
    if (!rootEl.contains(e.target as Node)) open = false;
  }

  function pick(level: ThinkingLevel) {
    value = level;
    onchange?.(level);
    open = false;
  }
</script>

<svelte:document on:click={handleDocumentClick} />

{#if enabled}
  <div class="picker" class:dir-up={direction === "up"} bind:this={rootEl}>
    <button
      type="button"
      class="trigger"
      class:level-max={currentIntensity >= 5}
      class:level-hot={currentIntensity === 4}
      class:level-warm={currentIntensity === 3}
      class:level-cool={currentIntensity > 0 && currentIntensity < 3}
      onclick={() => (open = !open)}
      aria-haspopup="listbox"
      aria-expanded={open}
      title="Set thinking level"
    >
      <ThinkingMeter intensity={currentIntensity} />
      <span class="trigger-label">{currentLabel}</span>
      <!-- "chev", not "caret" — .caret is the app-wide streaming-cursor atom (atoms.css). -->
      <span class="chev" aria-hidden="true">{direction === "up" ? "▴" : "▾"}</span>
    </button>
    {#if open}
      <div class="menu" role="listbox">
        {#each levels as level}
          {@const optIntensity = levelIntensity(level)}
          <button
            type="button"
            role="option"
            aria-selected={level === currentLevel}
            class="option"
            class:active={level === currentLevel}
            onclick={() => pick(level)}
          >
            <ThinkingMeter intensity={optIntensity} size="md" />
            <span class="opt-label">{displayLabel(provider, model, level)}</span>
          </button>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  /* Flat design-system restyle — depth = elevation + border, no shadows/glows. */
  .picker {
    position: relative;
    display: inline-flex;
  }

  .trigger {
    display: inline-flex;
    align-items: center;
    gap: var(--s2);
    font: inherit;
    font-size: 11.5px;
    font-weight: 500;
    padding: 5px var(--s2);
    border-radius: var(--r-md);
    background: var(--bg-raised);
    border: 1px solid var(--border);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background 0.14s, border-color 0.14s, color 0.14s;
  }

  .trigger:hover {
    background: var(--bg-overlay);
    border-color: var(--border-strong);
  }

  .trigger:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
  }

  .trigger.level-cool {
    border-color: color-mix(in srgb, var(--status-info) 35%, transparent);
    color: var(--status-info);
  }

  .trigger.level-warm {
    border-color: color-mix(in srgb, var(--accent) 35%, transparent);
    color: var(--accent);
  }

  .trigger.level-hot {
    border-color: var(--accent);
    color: var(--accent);
  }

  .trigger.level-max {
    border-color: var(--status-warning);
    color: var(--status-warning);
  }

  .trigger-label {
    letter-spacing: 0.01em;
  }

  .chev {
    font-size: 9px;
    opacity: 0.55;
  }

  .menu {
    position: absolute;
    left: 0;
    background: var(--bg-overlay);
    border: 1px solid var(--border-strong);
    border-radius: var(--r-md);
    padding: var(--s1);
    z-index: 50;
    display: flex;
    flex-direction: column;
    min-width: 160px;
  }

  .picker:not(.dir-up) .menu {
    top: calc(100% + 4px);
  }

  .picker.dir-up .menu {
    bottom: calc(100% + 4px);
  }

  .option {
    display: flex;
    align-items: center;
    gap: var(--s2);
    padding: 6px var(--s2);
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text-secondary);
    font: inherit;
    font-size: 11.5px;
    cursor: pointer;
    text-align: left;
    transition: background 0.1s, color 0.1s;
  }

  .option:hover {
    background: var(--bg-raised);
    color: var(--text-primary);
  }

  .option.active {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }

  .opt-label {
    flex: 1;
  }
</style>
