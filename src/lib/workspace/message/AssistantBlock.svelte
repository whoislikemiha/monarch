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

  /**
   * Client-side start anchors for in-flight thinking blocks so the toggle can
   * show a live counter before Rust injects the real `durationMs` at block
   * end (which then replaces the approximation).
   */
  const thinkingStartedAt = new Map<number, number>();
  $effect(() => {
    if (!streaming) return;
    const last = content.length - 1;
    if (content[last]?.type === "thinking" && !thinkingStartedAt.has(last)) {
      thinkingStartedAt.set(last, Date.now());
    }
  });

  function thinkingSeconds(block: ContentBlock, i: number): string | null {
    if (block.type !== "thinking") return null;
    const d = block._monarch?.durationMs;
    if (d != null) return formatDuration(d);
    // Still thinking — tick from the moment the block first appeared.
    if (streaming && i === content.length - 1) {
      const start = thinkingStartedAt.get(i);
      if (start != null) return formatElapsed(now - start);
    }
    return null;
  }

  // Live elapsed ticker — only runs while this turn is streaming.
  let now = $state(Date.now());
  $effect(() => {
    if (!streaming) return;
    const id = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(id);
  });

  // Like formatDuration but shows "0 sec" from the first tick instead of
  // hiding sub-1-second values — a live counter that starts blank reads broken.
  function formatElapsed(ms: number): string {
    const sec = Math.max(0, Math.floor(ms / 1000));
    if (sec < 60) return `${sec} sec`;
    return formatDuration(sec * 1000) ?? `${sec} sec`;
  }
</script>

<div class="assistant">
  {#each content as block, i (i)}
    {#if block.type === "thinking"}
      <div class="think">
        <button class="think-toggle" onclick={() => toggle(i)}>
          <span class="chev" class:open={expanded.has(i)} aria-hidden="true">›</span>
          Thinking
          {#if thinkingSeconds(block, i)}
            <span class="think-sep" aria-hidden="true">·</span>
            <span class="think-secs mono">{thinkingSeconds(block, i)}</span>
          {/if}
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
  {#if streaming}
    <div class="foot">
      <span class="caret"></span>
    </div>
  {/if}
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
  .think-sep { color: var(--border); }
  .think-secs { font-size: 10px; color: var(--text-muted); }
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

  .foot { display: flex; align-items: center; gap: var(--s2); }
  .caret { display: inline-block; width: 7px; height: 14px; background: var(--accent); vertical-align: -2px; animation: blink 1.1s steps(2, start) infinite; }
  @keyframes blink { 0%, 50% { opacity: 1; } 50.01%, 100% { opacity: 0; } }
</style>
