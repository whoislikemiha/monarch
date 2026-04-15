<script lang="ts">
  import { marked } from "marked";
  import { slide } from "svelte/transition";
  import type { ContentBlock } from "./types";
  import { formatDuration } from "./format";

  let { content }: { content: ContentBlock[] } = $props();

  // MON-71: thinking blocks carry their finalized duration via Rust-injected
  // `_monarch.durationMs`. When present, the toggle label reads
  // "Thought for 15 sec" (Claude.ai/ChatGPT convention); when absent
  // (pre-MON-71 rows, sub-1-sec blocks, live pre-finalize) it falls back
  // to plain "Thinking".
  function thinkingLabel(block: ContentBlock): string {
    if (block.type !== "thinking") return "Thinking";
    const d = formatDuration(block._monarch?.durationMs);
    return d ? `Thought for ${d}` : "Thinking";
  }

  // Collapsed by default — track which blocks the user has expanded.
  let expandedThinking: Set<number> = $state(new Set());

  function toggleThinking(index: number) {
    if (expandedThinking.has(index)) {
      expandedThinking.delete(index);
    } else {
      expandedThinking.add(index);
    }
    expandedThinking = new Set(expandedThinking);
  }

  function renderMarkdown(text: string): string {
    const rendered = marked.parse(escapeInlineHtml(text), {
      async: false,
      breaks: true,
    }) as string;
    return sanitizeRenderedHtml(rendered);
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

  function escapeInlineHtml(text: string): string {
    return text
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;");
  }

  function sanitizeRenderedHtml(html: string): string {
    if (typeof window === "undefined") return html;

    const template = document.createElement("template");
    template.innerHTML = html;

    const blockedTags = new Set(["script", "iframe", "object", "embed", "link", "meta", "style"]);
    const allowedUrlProtocols = new Set(["http:", "https:", "mailto:", "data:", "blob:"]);

    const walk = (node: Node) => {
      if (node.nodeType !== Node.ELEMENT_NODE) {
        for (const child of Array.from(node.childNodes)) walk(child);
        return;
      }

      const element = node as HTMLElement;
      const tag = element.tagName.toLowerCase();

      if (blockedTags.has(tag)) {
        element.remove();
        return;
      }

      for (const attr of Array.from(element.attributes)) {
        const name = attr.name.toLowerCase();
        const value = attr.value.trim();

        if (name.startsWith("on")) {
          element.removeAttribute(attr.name);
          continue;
        }

        if (name === "href" || name === "src") {
          try {
            const url = new URL(value, window.location.href);
            if (!allowedUrlProtocols.has(url.protocol)) {
              element.removeAttribute(attr.name);
            }
          } catch {
            element.removeAttribute(attr.name);
          }
          continue;
        }

        if (name !== "title" && name !== "alt") {
          element.removeAttribute(attr.name);
        }
      }

      if (tag === "a" && element.hasAttribute("href")) {
        element.setAttribute("target", "_blank");
        element.setAttribute("rel", "noopener noreferrer");
      }

      for (const child of Array.from(element.childNodes)) walk(child);
    };

    for (const child of Array.from(template.content.childNodes)) walk(child);
    return template.innerHTML;
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
      {@const expanded = expandedThinking.has(i)}
      <div class="thinking-block">
        <button
          class="thinking-toggle"
          type="button"
          aria-expanded={expanded}
          onclick={() => toggleThinking(i)}
        >
          <span class="toggle-arrow" aria-hidden="true">{expanded ? "▾" : "▸"}</span>
          <span class="toggle-label">{thinkingLabel(block)}</span>
          {#if block.redacted}
            <span class="redacted-tag">redacted</span>
          {/if}
        </button>
        {#if expanded}
          <div
            class="thinking-content"
            transition:slide={{ duration: 180 }}
          >
            {block.thinking}
          </div>
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
    background: var(--bg-panel-2);
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    color: var(--text-muted);
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
    color: var(--text-primary);
    background: var(--bg-panel-3);
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
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
  }

  .thinking-toggle {
    background: var(--bg-panel-2);
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 11px;
    padding: 3px 10px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    transition: color 0.15s, background 0.15s, border-color 0.15s;
  }

  .thinking-toggle:hover {
    color: var(--text-secondary);
    background: var(--bg-panel-3);
    border-color: var(--border-strong);
  }

  .toggle-arrow {
    font-size: 10px;
    width: 10px;
    text-align: center;
  }

  .toggle-label {
    letter-spacing: 0.02em;
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
    font-style: italic;
    border-left: 2px solid var(--border-strong);
    padding: 2px 0 2px 12px;
    margin-left: 4px;
  }
</style>
