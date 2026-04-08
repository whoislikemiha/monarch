<script lang="ts">
  import { marked } from "marked";
  import type { ContentBlock } from "./types";

  let { content }: { content: ContentBlock[] } = $props();

  let collapsedThinking: Set<number> = $state(new Set());

  function toggleThinking(index: number) {
    if (collapsedThinking.has(index)) {
      collapsedThinking.delete(index);
    } else {
      collapsedThinking.add(index);
    }
    collapsedThinking = new Set(collapsedThinking);
  }

  function renderMarkdown(text: string): string {
    return marked.parse(text, { async: false, breaks: true }) as string;
  }

  let copied = $state(false);

  function copyToClipboard(text: string) {
    navigator.clipboard.writeText(text);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }

  // Get all text content for full message copy
  function getAllText(): string {
    return content
      .filter((b): b is { type: "text"; text: string } => b.type === "text")
      .map((b) => b.text)
      .join("\n\n");
  }
</script>

<div class="assistant-content">
  <button
    class="copy-msg-btn"
    onclick={() => copyToClipboard(getAllText())}
    title="Copy message"
  >
    {copied ? "copied" : "copy"}
  </button>
  {#each content as block, i}
    {#if block.type === "text"}
      <div class="text-block markdown">
        {@html renderMarkdown(block.text)}
      </div>
    {:else if block.type === "thinking"}
      <div class="thinking-block">
        <button class="thinking-toggle" onclick={() => toggleThinking(i)}>
          <span class="toggle-arrow"
            >{collapsedThinking.has(i) ? "▸" : "▾"}</span
          >
          Thinking
          {#if block.redacted}
            <span class="redacted-tag">redacted</span>
          {/if}
        </button>
        {#if !collapsedThinking.has(i)}
          <div class="thinking-content">{block.thinking}</div>
        {/if}
      </div>
    {:else if block.type === "toolCall"}
      <!-- Tool calls rendered in ToolGroup, skip inline display -->
    {/if}
  {/each}
</div>

<style>
  .assistant-content {
    display: flex;
    flex-direction: column;
    gap: 8px;
    position: relative;
  }

  .copy-msg-btn {
    position: absolute;
    top: 0;
    right: 0;
    padding: 2px 8px;
    background: var(--bg-panel-2, #201734);
    border: 1px solid var(--border-subtle, #35274f);
    border-radius: 4px;
    color: var(--text-muted, #8f7aa8);
    font-size: 10px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.15s, color 0.15s;
    z-index: 1;
  }

  .assistant-content:hover .copy-msg-btn {
    opacity: 1;
  }

  .copy-msg-btn:hover {
    color: var(--text-primary, #f2f4f8);
    background: var(--bg-panel-3, #2a1e45);
  }

  .text-block {
    color: var(--text-primary);
    font-size: 13px;
    line-height: 1.6;
    word-break: break-word;
  }

  /* Markdown styles */
  .markdown :global(p) {
    margin: 0 0 8px 0;
  }
  .markdown :global(p:last-child) {
    margin-bottom: 0;
  }
  .markdown :global(h1),
  .markdown :global(h2),
  .markdown :global(h3) {
    margin: 12px 0 6px 0;
    color: var(--text-primary);
  }
  .markdown :global(h1) {
    font-size: 18px;
  }
  .markdown :global(h2) {
    font-size: 15px;
  }
  .markdown :global(h3) {
    font-size: 14px;
  }
  .markdown :global(strong) {
    color: var(--text-primary);
    font-weight: 600;
  }
  .markdown :global(em) {
    color: var(--text-secondary);
  }
  .markdown :global(a) {
    color: var(--accent-blue);
    text-decoration: none;
  }
  .markdown :global(a:hover) {
    text-decoration: underline;
  }
  .markdown :global(ul),
  .markdown :global(ol) {
    margin: 4px 0;
    padding-left: 20px;
  }
  .markdown :global(li) {
    margin: 2px 0;
  }
  .markdown :global(code) {
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    font-size: 12px;
    background: var(--bg-panel-2);
    padding: 2px 6px;
    border-radius: 4px;
    color: var(--accent-cyan);
  }
  .markdown :global(pre) {
    background: var(--bg-panel-2);
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    padding: 12px 14px;
    overflow-x: auto;
    margin: 8px 0;
    position: relative;
  }
  .markdown :global(pre code) {
    background: none;
    padding: 0;
    font-size: 11px;
    color: var(--text-secondary);
    line-height: 1.5;
  }
  .markdown :global(blockquote) {
    border-left: 3px solid var(--border-strong);
    margin: 8px 0;
    padding: 4px 12px;
    color: var(--text-muted);
  }
  .markdown :global(hr) {
    border: none;
    border-top: 1px solid var(--border-subtle);
    margin: 12px 0;
  }
  .markdown :global(table) {
    border-collapse: collapse;
    width: 100%;
    margin: 8px 0;
    font-size: 12px;
  }
  .markdown :global(th),
  .markdown :global(td) {
    border: 1px solid var(--border-subtle);
    padding: 6px 10px;
    text-align: left;
  }
  .markdown :global(th) {
    background: var(--bg-panel-2);
    color: var(--text-primary);
    font-weight: 600;
  }

  .thinking-block {
    border-left: 2px solid var(--border-strong);
    padding-left: 12px;
  }

  .thinking-toggle {
    background: none;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 11px;
    padding: 2px 0;
    display: flex;
    align-items: center;
    gap: 6px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .thinking-toggle:hover {
    color: var(--text-secondary);
  }

  .toggle-arrow {
    font-size: 10px;
    width: 10px;
  }

  .redacted-tag {
    color: var(--error);
    font-size: 10px;
  }

  .thinking-content {
    color: var(--text-muted);
    font-size: 12px;
    line-height: 1.5;
    white-space: pre-wrap;
    margin-top: 4px;
    font-style: italic;
  }

  .tool-call-inline {
    color: var(--text-muted);
    font-size: 12px;
  }

  .tool-name {
    color: var(--accent-cyan);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }
</style>
