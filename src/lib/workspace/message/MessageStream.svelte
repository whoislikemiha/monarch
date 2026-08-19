<script lang="ts">
  /**
   * Renders a live conversation as DIALOGUE ONLY (MON-130): user messages and
   * assistant turns that actually say something. Tool work never renders here
   * — the timeline pane is the work record; while a run is in flight one
   * pulsing "working" row shows the current narrated action, switching in
   * place as the agent re-narrates. Auto-scrolls to the newest content.
   */
  import { tick } from "svelte";
  import type { Agent, DisplayItem, AssistantMessage } from "$lib/types";
  import Avatar from "$lib/ui/Avatar.svelte";
  import AssistantBlock from "./AssistantBlock.svelte";
  import ClassificationPill from "./ClassificationPill.svelte";
  import { classifierStore } from "$lib/classifierStore.svelte";

  interface Props {
    agent: Agent;
    items: DisplayItem[];
    streamingMessage: AssistantMessage | null;
    /** MON-82: global user-turn ordinal per item — enables the complexity
     * pill lookup. Omitted in read-only viewers (session history). */
    userOrdinals?: Map<DisplayItem, number> | null;
    /** MON-130: a run is in flight for this pane — show the working row. */
    working?: boolean;
    /** Current narrated action intent (from working memory), live-updated. */
    workingIntent?: string | null;
    /** Jump to the active action card on the timeline. */
    onviewwork?: (() => void) | null;
  }
  let {
    agent,
    items,
    streamingMessage,
    userOrdinals = null,
    working = false,
    workingIntent = null,
    onviewwork = null,
  }: Props = $props();

  let classifications = $derived(
    userOrdinals ? classifierStore.byAgent.get(agent.id)?.ordinalMap ?? null : null,
  );

  function classificationFor(item: DisplayItem) {
    if (!classifications || !userOrdinals) return null;
    const ord = userOrdinals.get(item);
    return ord === undefined ? null : classifications.get(ord) ?? null;
  }

  /** Dialogue rule: an assistant turn renders only when it says something.
   * Thinking-only / tool-only turns are work, and work lives on the timeline. */
  function hasVisibleText(content: { type: string; text?: string }[]): boolean {
    return content.some((b) => b.type === "text" && (b.text ?? "").trim().length > 0);
  }

  let scroller: HTMLDivElement | undefined = $state();
  let pinned = true;

  function onScroll() {
    if (!scroller) return;
    pinned = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 60;
  }

  $effect(() => {
    // Re-run on any item / streaming / working change; honor the user's scroll.
    items.length;
    streamingMessage?.content;
    working;
    workingIntent;
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
      {:else if hasVisibleText(item.content)}
        <div class="turn assistant">
          <div class="speaker">
            <Avatar name={agent.name} size={24} avatarType={agent.avatarType} avatarPath={agent.avatarPath} provider={agent.provider} />
            <span class="speaker-name">{agent.name}</span>
          </div>
          <AssistantBlock content={item.content} />
        </div>
      {/if}
      <!-- Thinking-only turns render nothing: that work is on the timeline. -->
    {:else if item.kind === "status"}
      <div class="meta">{item.text}</div>
    {:else if item.kind === "notification"}
      <div class="meta note {item.level}">{item.text}</div>
    {/if}
  {/each}

  {#if streamingMessage && hasVisibleText(streamingMessage.content)}
    <div class="turn assistant">
      <div class="speaker">
        <Avatar name={agent.name} size={24} avatarType={agent.avatarType} avatarPath={agent.avatarPath} provider={agent.provider} />
        <span class="speaker-name">{agent.name}</span>
      </div>
      <AssistantBlock content={streamingMessage.content} streaming />
    </div>
  {/if}

  {#if working}
    <!-- MON-130: the ONE live work row — pulsing, current narrated action,
         switches in place. History of the work lives on the timeline. -->
    <div class="working">
      <span class="pulse" aria-hidden="true"></span>
      <span class="working-label">{workingIntent ?? "Working…"}</span>
      {#if onviewwork}
        <button class="working-link" onclick={() => onviewwork?.()}>view in timeline</button>
      {/if}
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
  .turn.assistant { flex-direction: column; gap: var(--s2); }
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

  .working {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--s2);
    min-width: 0;
  }
  .pulse {
    width: 7px; height: 7px; border-radius: var(--r-full);
    background: var(--status-info); flex: none;
    animation: work-pulse 1.4s ease-in-out infinite;
  }
  @keyframes work-pulse { 0%, 100% { opacity: 0.35; } 50% { opacity: 1; } }
  @media (prefers-reduced-motion: reduce) { .pulse { animation: none; } }
  .working-label {
    font-size: 11.5px;
    color: var(--text-secondary);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .working-link {
    background: none; border: none; padding: 0; margin: 0;
    font: inherit; font-size: 10px; color: var(--accent);
    cursor: pointer; flex: none;
  }
  .working-link:hover { text-decoration: underline; }
  .working-link:focus-visible { outline: 2px solid var(--focus); outline-offset: 2px; border-radius: var(--r-sm); }
</style>
