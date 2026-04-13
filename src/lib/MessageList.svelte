<script lang="ts">
  import AssistantMessageComp from "./AssistantMessage.svelte";
  import ToolGroup from "./ToolGroup.svelte";
  import { formatCost } from "./format";
  import type { DisplayItem, AssistantMessage } from "./types";

  let {
    items,
    streamingMessage,
  }: {
    items: DisplayItem[];
    streamingMessage: AssistantMessage | null;
  } = $props();
</script>

<div class="message-list">
  {#each items as item, i (i)}
    {#if item.kind === "user"}
      <div class="message user-message">
        <div class="message-label">You</div>
        <div class="message-content">{item.content}</div>
      </div>
    {:else if item.kind === "assistant"}
      <div class="message assistant-message">
        <div class="message-label">
          Assistant
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
        </div>
        <AssistantMessageComp content={item.content} />
      </div>
    {:else if item.kind === "tool-group"}
      <ToolGroup executions={item.executions} turnComplete={item.turnComplete} />
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
    {@const textContent = streamingMessage.content
      .filter((b): b is { type: "text"; text: string } => b.type === "text")
      .map((b) => b.text)
      .join("")}
    {@const isThinking = streamingMessage.content.some((b) => b.type === "thinking")}
    <div class="message assistant-message streaming">
      <div class="message-label">
        Assistant
        <span class="streaming-indicator"></span>
      </div>
      {#if isThinking && !textContent}
        <div class="streaming-content streaming-thinking">thinking...</div>
      {:else if textContent}
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
    color: var(--accent);
  }

  .model-tag,
  .token-tag,
  .cost-tag {
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
    border: 1px solid var(--border-subtle);
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

  .streaming-thinking {
    color: var(--text-muted);
    font-style: italic;
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
