<script lang="ts">
  import type { Agent, DisplayItem, SessionStats, Usage } from "./types";
  import { ShadowAvatar } from "./avatar";

  export type PortraitCorner = "top-left" | "top-right" | "bottom-left" | "bottom-right";

  let {
    agent,
    projectName,
    isStreaming,
    items,
    lastUsage,
    contextWindow,
    thinkingLevel,
    model,
    sessionStats,
    onabort,
    onthinking,
    onprompt,
    onhistory,
    oncompact,
    onnewsession,
    onprojectedit,
    onmove,
    corner = "bottom-right",
  }: {
    agent: Agent;
    projectName?: string;
    isStreaming: boolean;
    items: DisplayItem[];
    lastUsage?: Usage;
    contextWindow?: number;
    thinkingLevel?: string;
    model?: string;
    sessionStats?: SessionStats;
    onabort: () => void;
    onthinking: (level: string) => void;
    onprompt?: () => void;
    onhistory?: () => void;
    oncompact?: () => void;
    onnewsession?: () => void;
    onprojectedit?: () => void;
    onmove?: (corner: PortraitCorner) => void;
    corner?: PortraitCorner;
  } = $props();

  // --- Drag to reposition ---
  let portraitEl: HTMLDivElement | undefined = $state(undefined);
  let dragState: {
    startX: number;
    startY: number;
    pointerId: number;
  } | null = null;
  let dragOffsetX = $state(0);
  let dragOffsetY = $state(0);
  let isDragging = $state(false);

  function handleHandleDown(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragState = { startX: e.clientX, startY: e.clientY, pointerId: e.pointerId };
    isDragging = true;
    showCommandMenu = false;
    showThinkingPicker = false;
  }

  function handleHandleMove(e: PointerEvent) {
    if (!dragState || e.pointerId !== dragState.pointerId) return;
    dragOffsetX = e.clientX - dragState.startX;
    dragOffsetY = e.clientY - dragState.startY;
  }

  function handleHandleUp(e: PointerEvent) {
    if (!dragState || e.pointerId !== dragState.pointerId) return;
    (e.currentTarget as HTMLElement).releasePointerCapture?.(e.pointerId);

    const anchor = portraitEl?.closest(".messages-area") as HTMLElement | null;
    if (anchor && portraitEl) {
      const bounds = anchor.getBoundingClientRect();
      const pr = portraitEl.getBoundingClientRect();
      const cx = pr.left + pr.width / 2;
      const cy = pr.top + pr.height / 2;
      const midX = bounds.left + bounds.width / 2;
      const midY = bounds.top + bounds.height / 2;
      const horiz: "left" | "right" = cx < midX ? "left" : "right";
      const vert: "top" | "bottom" = cy < midY ? "top" : "bottom";
      const next: PortraitCorner = `${vert}-${horiz}` as PortraitCorner;
      onmove?.(next);
    }

    dragState = null;
    dragOffsetX = 0;
    dragOffsetY = 0;
    isDragging = false;
  }

  const DEFAULT_CONTEXT_WINDOW = 128000;
  const CHARS_PER_TOKEN = 4;
  const thinkingLevels = ["off", "minimal", "low", "medium", "high", "xhigh"];
  let showThinkingPicker = $state(false);
  let showCommandMenu = $state(false);

  function runCommand(fn?: () => void) {
    showCommandMenu = false;
    fn?.();
  }

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
      if (lastUsage.totalTokens) {
        const output = lastUsage.output ?? 0;
        return Math.max(lastUsage.totalTokens - output, 0);
      }
    }
    return estimatedContextTokens;
  });

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
  let contextFill = $derived(Math.max((1 - usedRatio) * 100, 0));
  let contextState = $derived(
    usedRatio >= 0.9 ? "critical" : usedRatio >= 0.7 ? "warning" : "healthy"
  );

  // Rolling sparkline of context occupancy over the last N samples.
  // Only push when the value actually changes so we don't stack duplicates
  // on every snapshot bump.
  const SPARK_SAMPLES = 24;
  const SPARK_W = 160;
  const SPARK_H = 22;
  let ctxHistory = $state<number[]>([]);
  let lastPushedCtx = $state<number | null>(null);

  $effect(() => {
    const v = liveContextTokens;
    if (v === lastPushedCtx) return;
    lastPushedCtx = v;
    ctxHistory = [...ctxHistory, v].slice(-SPARK_SAMPLES);
  });

  let sparkPath = $derived.by(() => {
    if (ctxHistory.length < 2) return "";
    const max = Math.max(resolvedContextWindow, ...ctxHistory, 1);
    const stepX = SPARK_W / (SPARK_SAMPLES - 1);
    const startIdx = SPARK_SAMPLES - ctxHistory.length;
    return ctxHistory
      .map((v, i) => {
        const x = (startIdx + i) * stepX;
        const y = SPARK_H - (v / max) * SPARK_H;
        return `${i === 0 ? "M" : "L"} ${x.toFixed(1)} ${y.toFixed(1)}`;
      })
      .join(" ");
  });

  function shortenPath(path: string): string {
    return path.replace(/^\/home\/[^/]+/, "~");
  }

  let avatarTooltip = $derived.by(() => {
    const parts: string[] = [];
    if (agent.shadow?.shadowName) parts.push(agent.shadow.shadowName);
    else if (agent.name) parts.push(agent.name);
    if (agent.shadow?.shadowTitle) parts.push(agent.shadow.shadowTitle);
    if (projectName) parts.push(`/${projectName}`);
    else if (agent.cwd) parts.push(shortenPath(agent.cwd));
    return parts.join(" · ");
  });
</script>

<svelte:window onclick={() => { showThinkingPicker = false; showCommandMenu = false; }} />

<div
  class="portrait"
  class:streaming={isStreaming}
  class:dragging={isDragging}
  data-corner={corner}
  bind:this={portraitEl}
  style:transform={isDragging ? `translate(${dragOffsetX}px, ${dragOffsetY}px)` : undefined}
>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="drag-handle"
    title="Drag to reposition"
    onpointerdown={handleHandleDown}
    onpointermove={handleHandleMove}
    onpointerup={handleHandleUp}
    onpointercancel={handleHandleUp}
  >
    <span class="drag-dots">&bull; &bull; &bull;</span>
  </div>

  {#if hasContextMeter}
    <div
      class="context-meter"
      class:warning={contextState === "warning"}
      class:critical={contextState === "critical"}
      title={`Context snapshot: ${liveContextTokens.toLocaleString()} / ${resolvedContextWindow.toLocaleString()} tokens in context, ${freeTokens.toLocaleString()} free (${freePct}% headroom)${isEstimatedOccupancy ? " — occupancy estimated from restored content" : ""}${isEstimatedContextWindow ? " — window is estimated" : ""}`}
    >
      <div class="context-row">
        <span class="context-label">ctx</span>
        <span class="context-value">{usedPct}%</span>
      </div>
      <div class="context-track">
        <div class="context-fill" style:width={`${contextFill}%`}></div>
      </div>
      {#if sparkPath}
        <svg class="context-spark" viewBox="0 0 {SPARK_W} {SPARK_H}" preserveAspectRatio="none" aria-hidden="true">
          <path d={sparkPath} fill="none" stroke="currentColor" stroke-width="1" stroke-linejoin="round" stroke-linecap="round" />
        </svg>
      {/if}
      <div class="context-row context-row-sub">
        <span class="context-tokens">{formatTokens(liveContextTokens)}/{formatTokens(resolvedContextWindow)}</span>
        <span class="context-free">{freePct}% free</span>
      </div>
    </div>
  {/if}

  <div class="avatar-frame" title={avatarTooltip}>
    <button
      class="avatar-btn"
      onclick={(e: MouseEvent) => { e.stopPropagation(); showCommandMenu = !showCommandMenu; showThinkingPicker = false; }}
      title="Agent actions"
      aria-label="Agent actions"
      aria-haspopup="menu"
      aria-expanded={showCommandMenu}
    >
      <ShadowAvatar
        agentId={agent.id}
        size={180}
        avatarType={agent.avatarType}
        avatarPath={agent.avatarPath}
      />
    </button>
    <div class="avatar-caption">
      <span class="caption-name">{agent.shadow?.shadowName || agent.name}</span>
      {#if agent.shadow?.shadowTitle}
        <span class="caption-title">{agent.shadow.shadowTitle}</span>
      {/if}
    </div>

    {#if showCommandMenu}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div class="command-menu" onclick={(e: MouseEvent) => e.stopPropagation()} role="menu" tabindex="-1">
        {#if isStreaming}
          <button class="command-item danger" onclick={() => runCommand(onabort)} role="menuitem">
            Abort
          </button>
          <div class="command-divider"></div>
        {/if}
        {#if onnewsession}
          <button class="command-item" onclick={() => runCommand(onnewsession)} role="menuitem">
            New chat
          </button>
        {/if}
        {#if oncompact}
          <button class="command-item" onclick={() => runCommand(oncompact)} role="menuitem">
            Compact context
          </button>
        {/if}
        {#if onhistory}
          <button class="command-item" onclick={() => runCommand(onhistory)} role="menuitem">
            Session history
          </button>
        {/if}
        <div class="command-divider"></div>
        {#if onprompt}
          <button class="command-item" onclick={() => runCommand(onprompt)} role="menuitem">
            System prompt
          </button>
        {/if}
        {#if onprojectedit && projectName}
          <button class="command-item" onclick={() => runCommand(onprojectedit)} role="menuitem">
            Project instructions
          </button>
        {/if}
      </div>
    {/if}
  </div>

  <div class="stack">
    {#if model}
      <span class="model" title={model}>{model}</span>
    {/if}

    <div class="thinking-wrap">
      <button
        class="thinking-btn"
        onclick={(e: MouseEvent) => { e.stopPropagation(); showThinkingPicker = !showThinkingPicker; }}
        title="Set thinking level"
      >
        thinking: {thinkingLevel || "off"}
      </button>
      {#if showThinkingPicker}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div class="thinking-dropdown" onclick={(e: MouseEvent) => e.stopPropagation()} role="menu" tabindex="-1">
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
      {#if sessionStats && sessionStats.totalTokens > 0}
        <span
          class="billing-tag"
          title="Session-lifetime billing total (not current context occupancy)"
        >
          Σ {formatTokens(sessionStats.totalTokens)}{#if displayCost} · ${displayCost.toFixed(4)}{/if}
        </span>
      {:else if displayCost}
        <span class="billing-tag">${displayCost.toFixed(4)}</span>
      {/if}
    {/if}

  </div>
</div>

<style>
  .portrait {
    position: relative;
    display: flex;
    flex-direction: column;
    width: 196px;
    gap: 7px;
    padding: 8px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--bg-panel) 88%, transparent);
    border: 1px solid var(--border-subtle);
    backdrop-filter: blur(8px);
    box-shadow: 0 8px 24px var(--shadow-dark, rgba(0, 0, 0, 0.35));
    pointer-events: auto;
    user-select: none;
    will-change: transform;
  }

  .portrait.dragging {
    cursor: grabbing;
    opacity: 0.92;
    transition: none;
  }

  .drag-handle {
    height: 14px;
    margin: -4px -4px 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
    cursor: grab;
    border-radius: 6px;
    opacity: 0.55;
    transition: opacity 0.12s, background 0.12s;
  }

  .drag-handle:hover {
    opacity: 1;
    background: var(--bg-panel-2);
  }

  .portrait.dragging .drag-handle {
    cursor: grabbing;
  }

  .drag-dots {
    font-size: 14px;
    line-height: 1;
    letter-spacing: 2px;
  }

  .portrait.streaming {
    border-color: var(--accent);
    animation: portrait-breath 1.8s ease-in-out infinite;
  }

  .portrait.streaming .avatar-frame {
    border-color: var(--accent);
    animation: avatar-glow 1.8s ease-in-out infinite;
  }

  @keyframes portrait-breath {
    0%, 100% {
      box-shadow:
        0 8px 24px var(--shadow-dark, rgba(0, 0, 0, 0.35)),
        0 0 0 0 color-mix(in srgb, var(--accent) 35%, transparent);
    }
    50% {
      box-shadow:
        0 8px 24px var(--shadow-dark, rgba(0, 0, 0, 0.35)),
        0 0 0 6px color-mix(in srgb, var(--accent) 0%, transparent);
    }
  }

  @keyframes avatar-glow {
    0%, 100% {
      box-shadow: inset 0 0 12px color-mix(in srgb, var(--accent) 30%, transparent);
    }
    50% {
      box-shadow: inset 0 0 24px color-mix(in srgb, var(--accent) 55%, transparent);
    }
  }

  .avatar-frame {
    position: relative;
    align-self: center;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    overflow: hidden;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-panel-2);
    line-height: 0;
  }

  .avatar-btn {
    all: unset;
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    line-height: 0;
  }

  .avatar-btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .command-menu {
    position: absolute;
    min-width: 170px;
    padding: 4px;
    background: var(--bg-panel-2);
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    box-shadow: 0 12px 32px var(--shadow-dark, rgba(0, 0, 0, 0.4));
    z-index: 60;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  /* Menu opens toward the chat center based on which corner the portrait sits in. */
  .portrait[data-corner$="-right"] .command-menu {
    right: calc(100% + 8px);
  }
  .portrait[data-corner$="-left"] .command-menu {
    left: calc(100% + 8px);
  }
  .portrait[data-corner^="bottom-"] .command-menu {
    bottom: 0;
  }
  .portrait[data-corner^="top-"] .command-menu {
    top: 0;
  }

  .command-item {
    padding: 7px 10px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    text-align: left;
    cursor: pointer;
    transition: background 0.12s, color 0.12s;
    white-space: nowrap;
  }

  .command-item:hover {
    background: var(--bg-panel-3);
    color: var(--text-primary);
  }

  .command-item.danger {
    color: var(--error, #eb5757);
  }
  .command-item.danger:hover {
    background: var(--error, #eb5757);
    color: var(--bg-panel);
  }

  .command-divider {
    height: 1px;
    background: var(--border-subtle);
    margin: 2px 4px;
  }

  .avatar-caption {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    padding: 12px 8px 6px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    background: linear-gradient(
      to top,
      rgba(0, 0, 0, 0.8) 0%,
      rgba(0, 0, 0, 0.55) 40%,
      rgba(0, 0, 0, 0) 100%
    );
    pointer-events: none;
    line-height: 1.2;
  }

  .caption-name {
    font-size: 12px;
    font-weight: 600;
    color: #fff;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.6);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .caption-title {
    font-size: 10px;
    color: color-mix(in srgb, var(--accent) 80%, #fff);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.6);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .stack {
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
  }

  .model {
    font-size: 10px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    color: var(--accent);
    padding: 3px 6px;
    background: var(--bg-panel-2);
    border-radius: 4px;
    text-align: center;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .thinking-wrap {
    position: relative;
  }

  .thinking-btn {
    width: 100%;
    font-size: 10px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    padding: 3px 6px;
    border-radius: 4px;
    background: var(--bg-panel-2);
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s;
  }

  .thinking-btn:hover {
    background: var(--bg-panel-3);
    border-color: var(--border-strong);
  }

  .thinking-dropdown {
    position: absolute;
    bottom: 100%;
    left: 0;
    right: 0;
    margin-bottom: 4px;
    background: var(--bg-panel-2);
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    padding: 4px;
    z-index: 60;
    display: flex;
    flex-direction: column;
  }

  .thinking-option {
    padding: 5px 8px;
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

  .context-meter {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 4px 6px;
    border-radius: 6px;
    border: 1px solid var(--border-subtle);
    background: var(--bg-panel-2);
  }

  .context-row {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    font-size: 10px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    line-height: 1;
  }

  .context-row-sub {
    font-size: 9px;
  }

  .context-label {
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .context-value {
    color: var(--text-secondary);
  }

  .context-tokens {
    color: var(--text-muted);
  }

  .context-free {
    color: var(--text-muted);
  }

  .context-track {
    position: relative;
    width: 100%;
    height: 6px;
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

  .context-spark {
    width: 100%;
    height: 22px;
    color: color-mix(in srgb, var(--success) 70%, transparent);
    opacity: 0.85;
  }

  .context-meter.warning .context-spark {
    color: color-mix(in srgb, var(--warning) 75%, transparent);
  }

  .context-meter.critical .context-spark {
    color: color-mix(in srgb, var(--error) 80%, transparent);
  }

  .billing-tag {
    font-size: 9px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    color: var(--text-muted);
    padding: 3px 6px;
    border-radius: 4px;
    background: var(--bg-panel-2);
    text-align: center;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

</style>
