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
      <span class="caret" aria-hidden="true">{direction === "up" ? "▴" : "▾"}</span>
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
  .picker {
    position: relative;
    display: inline-flex;
  }

  .trigger {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    padding: 6px 10px;
    border-radius: 6px;
    background: var(--bg-panel-2);
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s, color 0.15s, box-shadow 0.15s;
  }

  .trigger:hover {
    background: var(--bg-panel-3);
    border-color: var(--border-strong);
  }

  .trigger.level-cool {
    border-color: var(--accent-blue-border-subtle);
    color: var(--accent-blue);
  }

  .trigger.level-warm {
    border-color: var(--accent-border-subtle);
    color: var(--accent);
  }

  .trigger.level-hot {
    border-color: var(--accent-border-hover);
    color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent-border-subtle) inset;
  }

  .trigger.level-max {
    border-color: var(--warning-border-subtle);
    color: var(--warning);
    box-shadow: 0 0 6px var(--warning-glow), 0 0 0 1px var(--warning-border-faint) inset;
  }

  .trigger-label {
    letter-spacing: 0.2px;
  }

  .caret {
    font-size: 9px;
    opacity: 0.55;
  }

  .menu {
    position: absolute;
    left: 0;
    background: var(--bg-panel-2);
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    padding: 4px;
    z-index: 50;
    display: flex;
    flex-direction: column;
    min-width: 160px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
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
    gap: 10px;
    padding: 6px 10px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    text-align: left;
    transition: background 0.1s, color 0.1s;
  }

  .option:hover {
    background: var(--bg-panel-3);
    color: var(--text-primary);
  }

  .option.active {
    color: var(--accent);
    background: var(--accent-bg-subtle);
  }

  .opt-label {
    flex: 1;
  }
</style>
