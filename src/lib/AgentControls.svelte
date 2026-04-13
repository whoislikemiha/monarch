<script lang="ts">
  import type { DisplayItem, SessionStats, Usage } from "./types";

  let {
    isStreaming,
    items,
    lastUsage,
    contextWindow,
    thinkingLevel,
    model,
    sessionStats,
    onabort,
    onthinking,
  }: {
    isStreaming: boolean;
    items: DisplayItem[];
    lastUsage?: Usage;
    contextWindow?: number;
    thinkingLevel?: string;
    model?: string;
    sessionStats?: SessionStats;
    onabort: () => void;
    onthinking: (level: string) => void;
  } = $props();

  const DEFAULT_CONTEXT_WINDOW = 128000;
  const CHARS_PER_TOKEN = 4;
  const thinkingLevels = ["off", "minimal", "low", "medium", "high", "xhigh"];
  let showThinkingPicker = $state(false);

  function formatTokens(n: number): string {
    if (n >= 1000000) return (n / 1000000).toFixed(1) + "M";
    if (n >= 1000) return (n / 1000).toFixed(1) + "k";
    return n.toString();
  }

  function estimateTokens(text: string): number {
    return Math.ceil(text.length / CHARS_PER_TOKEN);
  }

  function stringifyResult(result: unknown): string {
    if (result == null) return "";
    if (typeof result === "string") return result;
    if (typeof result === "object" && result && "content" in result && Array.isArray((result as any).content)) {
      return (result as any).content
        .map((part: any) => (part?.type === "text" ? part.text : `[${part?.type || "content"}]`))
        .join("\n");
    }
    return JSON.stringify(result);
  }

  let estimatedContextTokens = $derived.by(() =>
    items.reduce((total, item) => {
      if (item.kind === "user") {
        return total + estimateTokens(item.content);
      }

      if (item.kind === "assistant") {
        return total + item.content.reduce((messageTotal, block) => {
          if (block.type === "text") return messageTotal + estimateTokens(block.text);
          if (block.type === "thinking") return messageTotal + estimateTokens(block.thinking);
          return messageTotal;
        }, 0);
      }

      if (item.kind === "tool-group") {
        return total + item.executions.reduce((groupTotal, execution) => {
          const argsText = execution.args ? JSON.stringify(execution.args) : "";
          const resultText = stringifyResult(execution.result);
          return groupTotal + estimateTokens(argsText) + estimateTokens(resultText);
        }, 0);
      }

      return total;
    }, 0)
  );

  // Live context occupancy: what's sitting in the model's context right now,
  // taken from the most recent assistant message's usage. NOT session-lifetime
  // billing — that accumulates across every turn and massively overstates
  // occupancy after a few messages.
  let liveContextTokens = $derived.by(() => {
    if (lastUsage) {
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
    }
    return estimatedContextTokens;
  });

  // MON-50: session-total cost, summed over every assistant turn in the
  // currently displayed conversation. sessionStats.totalCost comes from the
  // DB and lags behind (it's only refreshed at bind/reset, not per-turn), so
  // use it only as a floor for the rare case where items haven't loaded yet.
  let itemsCostTotal = $derived(
    items.reduce((total, item) => {
      if (item.kind === "assistant") {
        return total + (item.usage?.cost?.total ?? 0);
      }
      return total;
    }, 0)
  );
  let displayCost = $derived(
    itemsCostTotal > 0 ? itemsCostTotal : sessionStats?.totalCost
  );
  let hasContextMeter = $derived(
    contextWindow != null || sessionStats != null || lastUsage != null || items.length > 0
  );
  let resolvedContextWindow = $derived(contextWindow ?? DEFAULT_CONTEXT_WINDOW);
  let isEstimatedContextWindow = $derived(contextWindow == null);
  let isEstimatedOccupancy = $derived(!lastUsage && estimatedContextTokens > 0);
  let usedRatio = $derived(
    resolvedContextWindow > 0
      ? Math.min(liveContextTokens / resolvedContextWindow, 1)
      : 0
  );
  let usedPct = $derived(Math.round(usedRatio * 100));
  let freeTokens = $derived(Math.max(resolvedContextWindow - liveContextTokens, 0));
  let freePct = $derived(Math.max(100 - usedPct, 0));
  let contextFill = $derived(
    Math.max((1 - usedRatio) * 100, 0)
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
        title={`Context snapshot: ${liveContextTokens.toLocaleString()} / ${resolvedContextWindow.toLocaleString()} tokens in context, ${freeTokens.toLocaleString()} free (${freePct}% headroom)${isEstimatedOccupancy ? " — occupancy estimated from restored content" : ""}${isEstimatedContextWindow ? " — window is estimated" : ""}`}
      >
        <span class="context-label">ctx</span>
        <div class="context-track">
          <div class="context-fill" style:width={`${contextFill}%`}></div>
        </div>
        <span class="context-value">
          {formatTokens(liveContextTokens)}/{formatTokens(resolvedContextWindow)}
          · {freePct}% free
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
    border: 1px solid var(--border-subtle);
    background: var(--bg-panel-2);
  }

  .context-meter.warning {
    border-color: var(--border-subtle);
  }

  .context-meter.critical {
    border-color: var(--border-subtle);
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
    background: var(--context-track-bg);
  }

  .context-fill {
    height: 100%;
    border-radius: inherit;
    background: var(--success);
    transition: width 0.25s ease, background 0.2s ease;
  }

  .context-meter.warning .context-fill {
    background: var(--warning);
  }

  .context-meter.critical .context-fill {
    background: var(--error);
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
    background: var(--bg-panel-2);
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
    color: var(--accent);
  }

  .control-tag.model {
    color: var(--accent);
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
    color: var(--text-on-accent);
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
