<script lang="ts">
  /**
   * The solo workspace — the heart of the Agents view. The timeline and chat
   * panes are one arrangeable tile stack: drag any tile's grip to reorder
   * (timeline included), drag dividers to resize, toggle row/column orientation.
   *
   * Owns the live-state binding for this agent/session (one per mount; the
   * parent keys this component by viewKey so a new session remounts cleanly).
   */
  import { onMount, onDestroy } from "svelte";
  import type { Agent } from "$lib/types";
  import { invoke } from "$lib/api";
  import ShadowHeader from "./ShadowHeader.svelte";
  import TimelinePane from "./TimelinePane.svelte";
  import type { AskPayload } from "./timelineModel";
  import ChatThread from "./ChatThread.svelte";
  import { LiveBinding } from "./liveBinding.svelte";
  import { chatStore } from "./chatStore.svelte";
  import { layoutStore } from "$lib/layout/layoutStore.svelte";
  import TileStack from "$lib/ui/TileStack.svelte";
  import ExtensionDialog from "$lib/ExtensionDialog.svelte";

  interface Props {
    agent: Agent;
  }
  let { agent }: Props = $props();

  let tiles = $derived([...chatStore.tiles(agent.id)]);
  let orient = $derived(layoutStore.workspaceOrient);

  function tileKey(id: string) {
    return `tile:${agent.id}:${id}`;
  }
  const size = (id: string) => layoutStore.panelHeight(tileKey(id));
  const setSize = (id: string, px: number) => layoutStore.setPanelHeight(tileKey(id), px);
  const reorder = (from: number, to: number) => chatStore.reorderTiles(agent.id, from, to);

  const binding = new LiveBinding();
  onMount(() => {
    chatStore.ensure(agent.id);
    binding.bind(agent).catch((e) => console.error("bind failed:", e));
  });
  onDestroy(() => binding.destroy());

  /** Timeline action → open a chat tile scoped to that piece of work, and
   * record a durable `chat_spawned` event under the action so "the captain
   * intervened here" stays on the work record (MON-124) — chips on the card
   * outlive the pane. Recording is best-effort and never blocks the chat. */
  function askAbout(action: AskPayload) {
    const ctx =
      `[The captain is asking about this piece of your work: "${action.intent}"` +
      (action.outcome ? ` (outcome: ${action.outcome})` : "") +
      `. Answer with that context in mind.]`;
    const wasOpen = chatStore.hasScopedPane(agent.id, action.id);
    chatStore.openScopedPane(agent.id, { id: action.id, kind: "action", label: action.intent, context: ctx });
    if (!wasOpen && !action.spawned && action.objectiveId) {
      invoke("db_record_objective_event", {
        payload: {
          objectiveId: action.objectiveId,
          eventType: "chat_spawned",
          actor: "captain",
          author: "captain",
          parentEventId: action.id,
          payloadJson: JSON.stringify({ scope_id: action.id, label: action.intent }),
        },
      }).catch(() => {});
    }
  }

  /** Chat chip on a card → re-open / focus that scoped conversation. */
  function reopenChat(scopeId: string, label: string) {
    const ctx =
      `[The captain re-opened the conversation about this piece of your work: "${label}". ` +
      `Answer with that context in mind.]`;
    chatStore.openScopedPane(agent.id, { id: scopeId, kind: "action", label, context: ctx });
  }
</script>

<div class="solo">
  <ShadowHeader {agent} {binding} onnewchat={() => chatStore.addPane(agent.id)} />

  <TileStack ids={tiles} axis={orient} onreorder={reorder} {size} {setSize}>
    {#snippet header(id)}
      {#if chatStore.isTimeline(id)}
        <span class="th">Timeline</span>
      {:else}
        {@const pane = chatStore.pane(agent.id, id)}
        {#if pane?.scope}<span class="scope-dot" aria-hidden="true"></span>{/if}
        <span class="th chat">{pane?.title ?? "Chat"}</span>
        {#if chatStore.chatCount(agent.id) > 1}
          <button class="close" title="Close chat" aria-label="Close chat" onclick={() => chatStore.closePane(agent.id, id)}>×</button>
        {/if}
      {/if}
    {/snippet}

    {#snippet body(id)}
      {#if chatStore.isTimeline(id)}
        <div class="tl-scroll">
          <TimelinePane {agent} onask={askAbout} onopenchat={reopenChat} />
        </div>
      {:else}
        {@const pane = chatStore.pane(agent.id, id)}
        {#if pane}
          <ChatThread {agent} {binding} {pane} />
        {/if}
      {/if}
    {/snippet}
  </TileStack>
</div>

{#if binding.pendingExtension}
  <ExtensionDialog
    request={binding.pendingExtension}
    onrespond={(v) => binding.respondExtension(v)}
    oncancel={() => binding.cancelExtension()}
  />
{/if}

<style>
  .solo {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
  .th { font-size: 10px; font-weight: 600; letter-spacing: 0.12em; text-transform: uppercase; color: var(--text-muted); flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .th.chat { text-transform: none; letter-spacing: 0; font-size: 11px; color: var(--text-secondary); }
  .scope-dot { width: 6px; height: 6px; border-radius: var(--r-full); background: var(--accent); flex: none; }
  .close {
    width: 18px; height: 18px; display: inline-flex; align-items: center; justify-content: center;
    background: none; border: none; border-radius: var(--r-sm); color: var(--text-muted);
    font-size: 14px; line-height: 1; cursor: pointer; flex: none;
  }
  .close:hover { background: var(--bg-raised); color: var(--status-error); }
  /* MON-124: the timeline owns its scrolling (bottom-anchored like the
   * chat); this wrapper just sizes it. */
  .tl-scroll { flex: 1; min-height: 0; display: flex; padding: var(--s4); }
</style>
