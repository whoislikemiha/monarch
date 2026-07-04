<script lang="ts">
  /**
   * Renders a live conversation: the DisplayItem list plus the in-flight
   * streaming assistant turn. Auto-scrolls to the newest content.
   */
  import { tick } from "svelte";
  import type { Agent, DisplayItem, AssistantMessage } from "$lib/types";
  import Avatar from "$lib/ui/Avatar.svelte";
  import AssistantBlock from "./AssistantBlock.svelte";
  import ToolActivityChip from "./ToolActivityChip.svelte";
  import ClassificationPill from "./ClassificationPill.svelte";
  import { classifierStore } from "$lib/classifierStore.svelte";

  interface Props {
    agent: Agent;
    items: DisplayItem[];
    streamingMessage: AssistantMessage | null;
    /** MON-82: global user-turn ordinal per item — enables the complexity
     * pill lookup. Omitted in read-only viewers (session history). */
    userOrdinals?: Map<DisplayItem, number> | null;
  }
  let { agent, items, streamingMessage, userOrdinals = null }: Props = $props();

  let classifications = $derived(
    userOrdinals ? classifierStore.byAgent.get(agent.id)?.ordinalMap ?? null : null,
  );

  function classificationFor(item: DisplayItem) {
    if (!classifications || !userOrdinals) return null;
    const ord = userOrdinals.get(item);
    return ord === undefined ? null : classifications.get(ord) ?? null;
  }

  /** MON-130: a completed assistant turn with no visible text (thinking-only
   * — the model went straight to tools) renders compactly, without the
   * avatar/speaker header. A column of empty speaker shells between activity
   * chips is what made the chat read hollow. */
  function hasVisibleText(item: Extract<DisplayItem, { kind: "assistant" }>): boolean {
    return item.content.some((b) => b.type === "text" && b.text.trim().length > 0);
  }

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
      {@const clf = classificationFor(item)}
      <div class="turn user">
        <div class="user-stack">
          <div class="bubble">{item.content}</div>
          {#if clf}
            <ClassificationPill info={clf} />
          {/if}
        </div>
      </div>
    {:else if item.kind === "assistant"}
      {#if item.content.length === 0}
        <!-- Empty assistant turn = the model returned no output (e.g. a codex
             empty/errored completion). Render it as a visible failure instead
             of a blank bubble so it doesn't read as a hang. -->
        <div class="meta note error">
          The model returned an empty response — no output. Retry, or start a new session if it persists.
        </div>
      {:else if hasVisibleText(item)}
        <div class="turn assistant">
          <div class="speaker">
            <Avatar name={agent.name} size={24} avatarType={agent.avatarType} avatarPath={agent.avatarPath} />
            <span class="speaker-name">{agent.name}</span>
          </div>
          <AssistantBlock content={item.content} />
        </div>
      {:else}
        <!-- Thinking-only turn: the dialogue content is the work itself,
             which the following activity chip links to. Keep the (collapsed)
             thinking reachable, drop the speaker shell. -->
        <div class="turn assistant compact">
          <AssistantBlock content={item.content} />
        </div>
      {/if}
    {:else if item.kind === "tool-group"}
      <!-- MON-124: chat is dialogue-only — tool tables live on the timeline.
           This chip marks that work happened here and links to the card. -->
      <div class="turn tools">
        <ToolActivityChip {agent} executions={item.executions} turnComplete={item.turnComplete} />
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
  .user-stack { display: flex; flex-direction: column; align-items: flex-end; gap: 3px; max-width: 85%; }
  .turn.user .bubble {
    max-width: 100%;
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
  .turn.tools { align-items: center; }
  .turn.assistant { gap: var(--s2); }
  /* Thinking-only turns are part of the work trace, not a full dialogue
   * turn — align them with the centered activity chips they precede. */
  .turn.assistant.compact { align-items: center; }
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
