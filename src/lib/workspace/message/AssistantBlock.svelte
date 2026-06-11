<script lang="ts">
  /**
   * One assistant turn: thinking (collapsed), markdown text, inline images.
   * Tool executions are rendered separately as tool-group items in the stream.
   */
  import type { ContentBlock } from "$lib/types";
  import { formatDuration } from "$lib/format";
  import { renderMarkdown } from "./markdown";

  interface Props {
    content: ContentBlock[];
    streaming?: boolean;
  }
  let { content, streaming = false }: Props = $props();

  let expanded = $state(new Set<number>());
  function toggle(i: number) {
    if (expanded.has(i)) expanded.delete(i);
    else expanded.add(i);
    expanded = new Set(expanded);
  }

  function thinkingLabel(block: ContentBlock): string {
    if (block.type !== "thinking") return "Thinking";
    const d = formatDuration(block._monarch?.durationMs);
    return d ? `Thought for ${d}` : "Thinking";
  }
</script>

<div class="assistant">
  {#each content as block, i (i)}
    {#if block.type === "thinking"}
      <div class="think">
        <button class="think-toggle" onclick={() => toggle(i)}>
          <span class="chev" class:open={expanded.has(i)} aria-hidden="true">›</span>
          {thinkingLabel(block)}
        </button>
        {#if expanded.has(i)}
          <div class="think-body">{block.thinking}</div>
        {/if}
      </div>
    {:else if block.type === "text"}
      <!-- eslint-disable-next-line svelte/no-at-html-tags -->
      <div class="prose">{@html renderMarkdown(block.text)}</div>
    {:else if block.type === "image"}
      <img class="img" src={`data:${block.mimeType};base64,${block.data}`} alt="attachment" />
    {/if}
  {/each}
  {#if streaming}<span class="caret"></span>{/if}
</div>

<style>
  .assistant { display: flex; flex-direction: column; gap: var(--s2); min-width: 0; }

  .think { border-left: 2px solid var(--border-subtle); padding-left: var(--s3); }
  .think-toggle {
    display: inline-flex; align-items: center; gap: var(--s2);
    background: none; border: none; cursor: pointer; padding: 0;
    font: inherit; font-size: 11px; color: var(--text-muted);
  }
  .think-toggle:hover { color: var(--text-secondary); }
  .chev { transition: transform 0.15s; }
  .chev.open { transform: rotate(90deg); }
  .think-body {
    margin-top: var(--s2); font-size: 11.5px; color: var(--text-muted);
    line-height: 1.6; white-space: pre-wrap; font-family: "JetBrains Mono", monospace;
  }

  .prose { font-size: 13px; color: var(--text-secondary); line-height: 1.65; min-width: 0; word-break: break-word; }
  .prose :global(p) { margin: 0 0 var(--s2); }
  .prose :global(p:last-child) { margin-bottom: 0; }
  .prose :global(h1), .prose :global(h2), .prose :global(h3) { font-size: 13.5px; font-weight: 600; color: var(--text-primary); margin: var(--s3) 0 var(--s2); }
  .prose :global(ul), .prose :global(ol) { margin: 0 0 var(--s2); padding-left: var(--s4); }
  .prose :global(li) { margin: 2px 0; }
  .prose :global(a) { color: var(--accent); text-decoration: none; }
  .prose :global(a:hover) { text-decoration: underline; }
  .prose :global(code) { font-family: "JetBrains Mono", monospace; font-size: 11.5px; background: var(--bg-sink); border: 1px solid var(--border-subtle); border-radius: var(--r-sm); padding: 1px 4px; }
  .prose :global(pre) { background: var(--bg-sink); border: 1px solid var(--border-subtle); border-radius: var(--r-md); padding: var(--s3); overflow-x: auto; margin: 0 0 var(--s2); }
  .prose :global(pre code) { background: none; border: none; padding: 0; font-size: 11.5px; line-height: 1.6; }
  .prose :global(blockquote) { margin: 0 0 var(--s2); padding-left: var(--s3); border-left: 2px solid var(--border); color: var(--text-muted); }

  .img { max-width: 280px; border-radius: var(--r-md); border: 1px solid var(--border-subtle); }
  .caret { display: inline-block; width: 7px; height: 14px; background: var(--accent); vertical-align: -2px; animation: blink 1.1s steps(2, start) infinite; }
  @keyframes blink { 0%, 50% { opacity: 1; } 50.01%, 100% { opacity: 0; } }
</style>
