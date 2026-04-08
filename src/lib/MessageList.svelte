<script lang="ts">
  import AssistantMessageComp from "./AssistantMessage.svelte";
  import ToolCallCard from "./ToolCallCard.svelte";
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
          {#if item.usage}
            <span class="token-tag"
              >{item.usage.totalTokens.toLocaleString()} tokens</span
            >
          {/if}
        </div>
        <AssistantMessageComp content={item.content} />
      </div>
    {:else if item.kind === "tool"}
      <ToolCallCard execution={item.execution} />
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
    <div class="message assistant-message streaming">
      <div class="message-label">
        Assistant
        <span class="streaming-indicator"></span>
      </div>
      <AssistantMessageComp content={streamingMessage.content} />
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
    color: var(--accent-purple);
  }

  .model-tag,
  .token-tag {
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
    background: var(--bg-panel-2, #201734);
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
    color: var(--accent-purple);
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
    background: rgba(51, 177, 255, 0.06);
    border: 1px solid rgba(51, 177, 255, 0.15);
  }

  .notification-message.warning {
    color: var(--warning);
    background: rgba(255, 233, 123, 0.06);
    border: 1px solid rgba(255, 233, 123, 0.15);
  }

  .notification-message.error {
    color: var(--error);
    background: rgba(238, 83, 150, 0.06);
    border: 1px solid rgba(238, 83, 150, 0.15);
  }
</style>
