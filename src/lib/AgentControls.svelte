<script lang="ts">
  import type { Usage, SessionStats } from "./types";

  let {
    isStreaming,
    lastUsage,
    contextWindow,
    thinkingLevel,
    model,
    sessionStats,
    onabort,
    onthinking,
  }: {
    isStreaming: boolean;
    lastUsage?: Usage;
    contextWindow?: number;
    thinkingLevel?: string;
    model?: string;
    sessionStats?: SessionStats;
    onabort: () => void;
    onthinking: (level: string) => void;
  } = $props();

  const DEFAULT_CONTEXT_WINDOW = 128000;
  const thinkingLevels = ["off", "minimal", "low", "medium", "high", "xhigh"];
  let showThinkingPicker = $state(false);

  function formatTokens(n: number): string {
    if (n >= 1000000) return (n / 1000000).toFixed(1) + "M";
    if (n >= 1000) return (n / 1000).toFixed(1) + "k";
    return n.toString();
  }

  // Live context occupancy: what's sitting in the model's context right now,
  // taken from the most recent assistant message's usage. NOT session-lifetime
  // billing — that accumulates across every turn and massively overstates
  // occupancy after a few messages.
  let liveContextTokens = $derived.by(() => {
    if (!lastUsage) return 0;
    const cached = (lastUsage.cacheRead ?? 0) + (lastUsage.cacheWrite ?? 0);
    if (lastUsage.input && lastUsage.input > 0) {
      return lastUsage.input + cached;
    }
    // Pi SDK / LM Studio sometimes only populates totalTokens. Back out output
    // to get what was in the prompt, since totalTokens = input + output.
    if (lastUsage.totalTokens) {
      const output = lastUsage.output ?? 0;
      return Math.max(lastUsage.totalTokens - output, 0);
    }
    return 0;
  });

  let displayCost = $derived(
    sessionStats?.totalCost ?? lastUsage?.cost?.total
  );
  let hasContextMeter = $derived(
    contextWindow != null || sessionStats != null || lastUsage != null
  );
  let resolvedContextWindow = $derived(contextWindow ?? DEFAULT_CONTEXT_WINDOW);
  let isEstimatedContextWindow = $derived(contextWindow == null);
  let usedRatio = $derived(
    resolvedContextWindow > 0
      ? Math.min(liveContextTokens / resolvedContextWindow, 1)
      : 0
  );
  let usedPct = $derived(Math.round(usedRatio * 100));
  let contextFill = $derived(
    liveContextTokens > 0 ? Math.max(usedRatio * 100, 4) : 0
  );
  // Critical/warning thresholds on how FULL the context is (not how much is left).
  let contextState = $derived(
    usedRatio >= 0.9 ? "critical" : usedRatio >= 0.7 ? "warning" : "healthy"
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

    {#if hasContextMeter}
      <div
        class="context-meter"
        class:warning={contextState === "warning"}
        class:critical={contextState === "critical"}
        title={`Context used: ${liveContextTokens.toLocaleString()} / ${resolvedContextWindow.toLocaleString()} tokens (${usedPct}%)${isEstimatedContextWindow ? " — window is estimated, set it in the spawn dialog for LM Studio" : ""}`}
      >
        <span class="context-label">ctx</span>
        <div class="context-track">
          <div class="context-fill" style:width={`${contextFill}%`}></div>
        </div>
        <span class="context-value">
          {formatTokens(liveContextTokens)}/{formatTokens(resolvedContextWindow)}
          ({usedPct}%)
        </span>
      </div>

      {#if sessionStats && sessionStats.totalTokens > 0}
        <span
          class="control-tag billing"
          title={`Session-lifetime billing total (not current context occupancy)`}
        >
          Σ {formatTokens(sessionStats.totalTokens)}
          {#if displayCost}
            · ${displayCost.toFixed(4)}
          {/if}
        </span>
      {:else if displayCost}
        <span class="control-tag tokens">${displayCost.toFixed(4)}</span>
      {/if}
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

  .context-meter {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-width: min(320px, 52vw);
    padding: 4px 9px;
    border-radius: 999px;
    border: 1px solid color-mix(in srgb, var(--success) 32%, var(--border-subtle));
    background:
      linear-gradient(180deg, rgba(66, 190, 101, 0.08), rgba(66, 190, 101, 0.02)),
      var(--bg-panel-2);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.04);
  }

  .context-meter.warning {
    border-color: color-mix(in srgb, var(--warning) 50%, var(--border-subtle));
    background:
      linear-gradient(180deg, rgba(255, 233, 123, 0.08), rgba(255, 233, 123, 0.02)),
      var(--bg-panel-2);
  }

  .context-meter.critical {
    border-color: color-mix(in srgb, var(--error) 48%, var(--border-subtle));
    background:
      linear-gradient(180deg, rgba(238, 83, 150, 0.1), rgba(238, 83, 150, 0.03)),
      var(--bg-panel-2);
  }

  .context-label,
  .context-value {
    font-size: 10px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    line-height: 1;
    white-space: nowrap;
  }

  .context-label {
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .context-value {
    color: var(--text-secondary);
  }

  .context-track {
    position: relative;
    flex: 1;
    min-width: 88px;
    height: 8px;
    overflow: hidden;
    border-radius: 999px;
    background:
      linear-gradient(90deg, rgba(255, 255, 255, 0.05), rgba(255, 255, 255, 0.02)),
      rgba(9, 6, 16, 0.72);
    box-shadow:
      inset 0 0 0 1px rgba(255, 255, 255, 0.04),
      inset 0 1px 3px rgba(0, 0, 0, 0.45);
  }

  .context-fill {
    height: 100%;
    border-radius: inherit;
    background:
      linear-gradient(90deg, color-mix(in srgb, var(--success) 76%, white 10%), var(--success));
    box-shadow:
      0 0 14px rgba(66, 190, 101, 0.36),
      inset 0 0 12px rgba(255, 255, 255, 0.2);
    transition: width 0.25s ease, background 0.2s ease, box-shadow 0.2s ease;
  }

  .context-meter.warning .context-fill {
    background:
      linear-gradient(90deg, color-mix(in srgb, var(--warning) 88%, white 10%), var(--warning));
    box-shadow:
      0 0 14px rgba(255, 233, 123, 0.32),
      inset 0 0 10px rgba(255, 255, 255, 0.16);
  }

  .context-meter.critical .context-fill {
    background:
      linear-gradient(90deg, color-mix(in srgb, var(--error) 78%, white 8%), var(--error));
    box-shadow:
      0 0 16px rgba(238, 83, 150, 0.32),
      inset 0 0 12px rgba(255, 255, 255, 0.16);
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

  @media (max-width: 720px) {
    .context-meter {
      min-width: 100%;
      order: 10;
    }
  }
</style>
