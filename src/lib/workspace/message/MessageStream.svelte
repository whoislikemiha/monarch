<script lang="ts">
  /**
   * Renders a live conversation: the DisplayItem list plus the in-flight
   * streaming assistant turn. Auto-scrolls to the newest content.
   */
  import { tick } from "svelte";
  import type { Agent, DisplayItem, AssistantMessage } from "$lib/types";
  import Avatar from "$lib/ui/Avatar.svelte";
  import AssistantBlock from "./AssistantBlock.svelte";
  import ToolGroupBlock from "./ToolGroupBlock.svelte";

  interface Props {
    agent: Agent;
    items: DisplayItem[];
    streamingMessage: AssistantMessage | null;
  }
  let { agent, items, streamingMessage }: Props = $props();

  let scroller: HTMLDivElement | undefined = $state();
  let pinned = true;

  function onScroll() {
    if (!scroller) return;
    pinned = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 60;
  }

  $effect(() => {
    // Re-run on any item / streaming change; honor the user's scroll position.
    items.length;
    streamingMessage?.content;
    if (!pinned) return;
    tick().then(() => {
      if (scroller) scroller.scrollTop = scroller.scrollHeight;
    });
  });
</script>

<div class="stream" bind:this={scroller} onscroll={onScroll}>
  {#each items as item, i (i)}
    {#if item.kind === "user"}
      <div class="turn user">
        <div class="bubble">{item.content}</div>
      </div>
    {:else if item.kind === "assistant"}
      {#if item.content.length === 0}
        <!-- Empty assistant turn = the model returned no output (e.g. a codex
             empty/errored completion). Render it as a visible failure instead
             of a blank bubble so it doesn't read as a hang. -->
        <div class="meta note error">
          The model returned an empty response — no output. Retry, or start a new session if it persists.
        </div>
      {:else}
        <div class="turn assistant">
          <div class="speaker">
            <Avatar name={agent.name} size={24} avatarType={agent.avatarType} avatarPath={agent.avatarPath} />
            <span class="speaker-name">{agent.name}</span>
          </div>
          <AssistantBlock content={item.content} />
        </div>
      {/if}
    {:else if item.kind === "tool-group"}
      <div class="turn tools">
        <ToolGroupBlock executions={item.executions} />
      </div>
    {:else if item.kind === "status"}
      <div class="meta">{item.text}</div>
    {:else if item.kind === "notification"}
      <div class="meta note {item.level}">{item.text}</div>
    {/if}
  {/each}

  {#if streamingMessage}
    <div class="turn assistant">
      <div class="speaker">
        <Avatar name={agent.name} size={24} avatarType={agent.avatarType} avatarPath={agent.avatarPath} />
        <span class="speaker-name">{agent.name}</span>
      </div>
      <AssistantBlock content={streamingMessage.content} streaming />
    </div>
  {/if}
</div>

<style>
  .stream {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--s4);
    padding: var(--s4);
  }
  .turn { display: flex; min-width: 0; }
  .turn.user { justify-content: flex-end; }
  .turn.user .bubble {
    max-width: 85%;
    background: var(--accent-bg-subtle);
    border: 1px solid var(--accent-border-subtle);
    border-radius: var(--r-md);
    padding: var(--s2) var(--s3);
    font-size: 13px;
    color: var(--text-primary);
    line-height: 1.55;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .turn.assistant, .turn.tools { flex-direction: column; }
  .turn.assistant { gap: var(--s2); }
  .speaker { display: flex; align-items: center; gap: var(--s2); }
  .speaker-name { font-size: 11.5px; font-weight: 600; color: var(--text-primary); }
  .meta {
    font-size: 10.5px;
    color: var(--text-muted);
    font-family: "JetBrains Mono", monospace;
    text-align: center;
  }
  .meta.note.warning { color: var(--status-warning); }
  .meta.note.error { color: var(--status-error); }
</style>
