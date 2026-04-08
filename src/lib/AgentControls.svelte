<script lang="ts">
  import type { Usage, SessionStats } from "./types";

  let {
    isStreaming,
    lastUsage,
    thinkingLevel,
    model,
    sessionStats,
    onabort,
    onthinking,
  }: {
    isStreaming: boolean;
    lastUsage?: Usage;
    thinkingLevel?: string;
    model?: string;
    sessionStats?: SessionStats;
    onabort: () => void;
    onthinking: (level: string) => void;
  } = $props();

  const thinkingLevels = ["off", "minimal", "low", "medium", "high", "xhigh"];
  let showThinkingPicker = $state(false);

  function formatTokens(n: number): string {
    if (n >= 1000000) return (n / 1000000).toFixed(1) + "M";
    if (n >= 1000) return (n / 1000).toFixed(1) + "k";
    return n.toString();
  }

  let displayTokens = $derived(
    sessionStats?.totalTokens || lastUsage?.totalTokens
  );
  let displayCost = $derived(
    sessionStats?.totalCost || lastUsage?.cost?.total
  );
</script>

<div class="controls">
  <div class="controls-left">
    {#if model}
      <span class="control-tag model" title={model}>{model}</span>
    {/if}

    <div class="thinking-wrap">
      <button
        class="control-btn"
        onclick={() => (showThinkingPicker = !showThinkingPicker)}
        title="Set thinking level"
      >
        thinking: {thinkingLevel || "off"}
      </button>
      {#if showThinkingPicker}
        <div class="thinking-dropdown">
          {#each thinkingLevels as level}
            <button
              class="thinking-option"
              class:active={level === (thinkingLevel || "off")}
              onclick={() => { onthinking(level); showThinkingPicker = false; }}
            >
              {level}
            </button>
          {/each}
        </div>
      {/if}
    </div>

    {#if displayTokens}
      <span class="control-tag tokens">
        {formatTokens(displayTokens)} tok
        {#if displayCost}
          &middot; ${displayCost.toFixed(4)}
        {/if}
      </span>
    {/if}
  </div>

  <div class="controls-right">
    {#if isStreaming}
      <button class="abort-btn" onclick={onabort}>Abort</button>
    {/if}
  </div>
</div>

<style>
  .controls {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 8px;
    gap: 8px;
  }

  .controls-left {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .controls-right {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .control-tag {
    font-size: 10px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    padding: 3px 8px;
    border-radius: 4px;
    background: var(--bg-panel-2);
    color: var(--text-muted);
  }

  .control-tag.tokens {
    color: var(--text-muted);
  }

  .control-btn {
    font-size: 10px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    padding: 3px 8px;
    border-radius: 4px;
    background: var(--bg-panel-2);
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s;
  }

  .control-btn:hover {
    background: var(--bg-panel-3);
    border-color: var(--border-strong);
  }

  .thinking-wrap {
    position: relative;
  }

  .thinking-dropdown {
    position: absolute;
    bottom: 100%;
    left: 0;
    margin-bottom: 4px;
    background: var(--bg-panel-2, #201734);
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    padding: 4px;
    z-index: 50;
    display: flex;
    flex-direction: column;
    min-width: 100px;
  }

  .thinking-option {
    padding: 5px 10px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    text-align: left;
    transition: background 0.1s;
  }

  .thinking-option:hover {
    background: var(--bg-panel-3);
  }

  .thinking-option.active {
    color: var(--accent-purple);
  }

  .control-tag.model {
    color: var(--accent-purple);
    max-width: 300px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .abort-btn {
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    padding: 4px 12px;
    border-radius: 6px;
    background: var(--error);
    border: none;
    color: #190f24;
    cursor: pointer;
    font-weight: 600;
    transition: filter 0.15s;
  }

  .abort-btn:hover {
    filter: brightness(1.08);
  }
</style>
