<script lang="ts">
  import AssistantMessageComp from "./AssistantMessage.svelte";
  import ToolGroup from "./ToolGroup.svelte";
  import { formatCost, formatDuration } from "./format";
  import type { DisplayItem, AssistantMessage } from "./types";
  import type { PendingImage } from "./ChatInput.svelte";

  let {
    items,
    streamingMessage,
    nowMs,
    agentName = "Assistant",
    sentImages,
    onimageclick,
  }: {
    items: DisplayItem[];
    streamingMessage: AssistantMessage | null;
    /** MON-71: 1Hz live ticker from AgentView; drives in-progress duration counters. */
    nowMs: number;
    agentName?: string;
    /** Ephemeral map of images attached to user messages, keyed by the
     * 0-based index among user messages in `items`. Owned by AgentView and
     * cleared whenever the bound session changes. */
    sentImages?: Map<number, PendingImage[]>;
    onimageclick?: (src: string) => void;
  } = $props();

  // MON-71: for a streaming turn, elapsed is `nowMs - turnStartedAtMs`;
  // for a finalized turn it's the persisted `durationMs`.
  function liveElapsed(startedAtMs?: number | null): number | null {
    if (startedAtMs == null) return null;
    return Math.max(0, nowMs - startedAtMs);
  }

  // Maps items[i] → that user message's 0-based index among user messages.
  // Non-user items are absent from the map.
  let userIndexAt = $derived.by(() => {
    const m = new Map<number, number>();
    let n = 0;
    for (let i = 0; i < items.length; i++) {
      if (items[i].kind === "user") {
        m.set(i, n);
        n++;
      }
    }
    return m;
  });
</script>

<div class="message-list">
  {#each items as item, i (i)}
    {#if item.kind === "user"}
      {@const userIdx = userIndexAt.get(i)}
      {@const imgs = userIdx != null ? sentImages?.get(userIdx) : undefined}
      <div class="message user-message">
        <div class="message-label">You</div>
        {#if item.content}
          <div class="message-content">{item.content}</div>
        {/if}
        {#if imgs && imgs.length > 0}
          <div class="sent-image-strip" class:text-above={!!item.content}>
            {#each imgs as img (img.id)}
              <button
                type="button"
                class="sent-thumb"
                onclick={() => onimageclick?.(img.dataUrl)}
                aria-label="View image"
              >
                <img src={img.dataUrl} alt="attachment" />
              </button>
            {/each}
          </div>
        {/if}
      </div>
    {:else if item.kind === "assistant"}
      <div class="message assistant-message">
        <div class="message-label">
          {agentName}
          {#if item.model}
            <span class="model-tag">{item.model}</span>
          {/if}
          {#if item.usage?.totalTokens != null}
            <span class="token-tag"
              >{item.usage.totalTokens.toLocaleString()} tokens</span
            >
          {/if}
          {#if formatCost(item.usage?.cost?.total)}
            <span class="cost-tag">{formatCost(item.usage?.cost?.total)}</span>
          {/if}
          {#if formatDuration(item.durationMs)}
            <span class="duration-tag">{formatDuration(item.durationMs)}</span>
          {/if}
        </div>
        <AssistantMessageComp content={item.content} />
      </div>
    {:else if item.kind === "tool-group"}
      <ToolGroup executions={item.executions} turnComplete={item.turnComplete} {nowMs} />
    {:else if item.kind === "status"}
      <div class="status-message">{item.text}</div>
    {:else if item.kind === "notification"}
      <div class="notification-message {item.level}">
        {#if item.level === "warning"}⚠{:else if item.level === "error"}✕{:else}ℹ{/if}
        {item.text}
      </div>
    {/if}
  {/each}

  {#if streamingMessage}
    {@const thinkingContent = streamingMessage.content
      .filter((b): b is { type: "thinking"; thinking: string; redacted?: boolean } => b.type === "thinking")
      .map((b) => b.thinking)
      .join("\n")}
    {@const textContent = streamingMessage.content
      .filter((b): b is { type: "text"; text: string } => b.type === "text")
      .map((b) => b.text)
      .join("")}
    {@const liveTurnDuration = formatDuration(liveElapsed(streamingMessage.turnStartedAtMs))}
    <div class="message assistant-message streaming">
      <div class="message-label">
        {agentName}
        <span class="streaming-indicator"></span>
        {#if liveTurnDuration}
          <span class="duration-tag">{liveTurnDuration}</span>
        {/if}
      </div>
      {#if thinkingContent && !textContent}
        <!-- Thinking-only phase: render text live so the user can follow along. -->
        <div class="streaming-thinking-live" aria-live="polite">
          <div class="streaming-thinking-label">
            <span class="thinking-dots" aria-hidden="true">
              <span class="dot"></span>
              <span class="dot"></span>
              <span class="dot"></span>
            </span>
            <span>Thinking</span>
          </div>
          <div class="streaming-thinking-text">{thinkingContent}</div>
        </div>
      {:else if thinkingContent}
        <!-- Text has started: collapse thinking to a static label so local
             models that stuff their whole response into thinking don't push
             the actual reply off-screen. The finalized bubble in
             AssistantMessage exposes the content once the turn lands. -->
        <div class="streaming-thinking-collapsed">
          <span class="toggle-arrow" aria-hidden="true">▸</span>
          <span>Thinking</span>
        </div>
      {/if}
      {#if textContent}
        <div class="streaming-content">{textContent}</div>
      {/if}
    </div>
  {/if}

  {#if items.length === 0 && !streamingMessage}
    <div class="empty">
      <span class="empty-icon">&gt;_</span>
      <p>Send a message to get started</p>
    </div>
  {/if}
</div>

<style>
  .message-list {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .message {
    max-width: 100%;
  }

  .message-label {
    font-size: 11px;
    font-weight: 600;
    margin-bottom: 6px;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .user-message .message-label {
    color: var(--accent-blue);
  }

  .assistant-message .message-label {
    color: var(--shadow-name);
  }

  .model-tag,
  .token-tag,
  .cost-tag,
  .duration-tag {
    font-weight: 400;
    color: var(--text-muted);
    font-size: 10px;
  }

  .message-content {
    color: var(--text-primary);
    font-size: 13px;
    line-height: 1.6;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .user-message .message-content {
    padding: 10px 14px;
    background: var(--bg-panel-2);
    border-radius: 8px;
    border: 1px solid var(--accent-blue-border);
  }

  .sent-image-strip {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .sent-image-strip.text-above {
    margin-top: 6px;
  }

  .sent-thumb {
    width: 72px;
    height: 72px;
    padding: 0;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    overflow: hidden;
    background: transparent;
    cursor: zoom-in;
    flex-shrink: 0;
    transition: border-color 0.15s;
  }

  .sent-thumb:hover {
    border-color: var(--accent-blue);
  }

  .sent-thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .streaming-indicator {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent-blue);
    animation: pulse 1s ease-in-out infinite;
  }

  @keyframes pulse {
    0%,
    100% {
      opacity: 0.4;
    }
    50% {
      opacity: 1;
    }
  }

  .status-message {
    color: var(--text-muted);
    font-size: 12px;
    font-style: italic;
    text-align: center;
    padding: 8px;
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    min-height: 200px;
    color: var(--text-muted);
  }

  .empty-icon {
    font-size: 48px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    margin-bottom: 12px;
    color: var(--accent);
  }

  .streaming-content {
    color: var(--text-primary);
    font-size: 13px;
    line-height: 1.6;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .streaming-thinking-live {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 6px;
  }

  .streaming-thinking-label {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    color: var(--text-muted);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    letter-spacing: 0.02em;
  }

  .streaming-thinking-text {
    color: var(--text-muted);
    font-size: 12px;
    line-height: 1.5;
    white-space: pre-wrap;
    font-style: italic;
    border-left: 2px solid var(--border-strong);
    padding: 2px 0 2px 12px;
    margin-left: 4px;
  }

  .streaming-thinking-collapsed {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: var(--bg-panel-2);
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    color: var(--text-muted);
    font-size: 11px;
    padding: 3px 10px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    letter-spacing: 0.02em;
    margin-bottom: 6px;
    align-self: flex-start;
  }

  .streaming-thinking-collapsed .toggle-arrow {
    font-size: 10px;
    width: 10px;
    text-align: center;
  }

  .thinking-dots {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .thinking-dots .dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--text-muted);
    opacity: 0.35;
    animation: thinking-pulse 1.1s ease-in-out infinite;
  }

  .thinking-dots .dot:nth-child(2) {
    animation-delay: 0.18s;
  }

  .thinking-dots .dot:nth-child(3) {
    animation-delay: 0.36s;
  }

  @keyframes thinking-pulse {
    0%, 80%, 100% {
      opacity: 0.25;
      transform: scale(0.85);
    }
    40% {
      opacity: 1;
      transform: scale(1);
    }
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }

  .streaming-tool {
    color: var(--accent-cyan);
    font-size: 12px;
  }

  .empty p {
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    margin: 0;
  }

  .notification-message {
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    padding: 8px 12px;
    border-radius: 6px;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .notification-message.info {
    color: var(--accent-blue);
    background: var(--accent-blue-bg-subtle);
    border: 1px solid var(--accent-blue-border-subtle);
  }

  .notification-message.warning {
    color: var(--warning);
    background: var(--warning-bg-subtle);
    border: 1px solid var(--warning-border-subtle);
  }

  .notification-message.error {
    color: var(--error);
    background: var(--error-bg-subtle);
    border: 1px solid var(--error-border-subtle);
  }
</style>
