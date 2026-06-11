<script lang="ts">
  /**
   * The chat side of the workspace: a stack of chat panes the captain arranges.
   * Each pane is a conversation thread (general or scoped to a timeline action).
   * Drag a pane's handle to reorder; drag the dividers to resize; New chat adds
   * a pane.
   */
  import type { Agent } from "$lib/types";
  import { layoutStore } from "$lib/layout/layoutStore.svelte";
  import Splitter from "$lib/ui/Splitter.svelte";
  import type { LiveBinding } from "./liveBinding.svelte";
  import { chatStore } from "./chatStore.svelte";
  import ChatThread from "./ChatThread.svelte";
  import ExtensionDialog from "$lib/ExtensionDialog.svelte";

  interface Props {
    agent: Agent;
    binding: LiveBinding;
  }
  let { agent, binding }: Props = $props();

  let panes = $derived(chatStore.panes(agent.id));

  // --- drag-to-reorder (native DnD on the pane handle) ---
  let dragIndex = $state<number | null>(null);
  let overIndex = $state<number | null>(null);

  function onDragStart(e: DragEvent, i: number) {
    dragIndex = i;
    e.dataTransfer?.setData("text/plain", String(i));
    if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
  }
  function onDragOver(e: DragEvent, i: number) {
    if (dragIndex === null) return;
    e.preventDefault();
    overIndex = i;
  }
  function onDrop(e: DragEvent, i: number) {
    e.preventDefault();
    if (dragIndex !== null && dragIndex !== i) chatStore.reorder(agent.id, dragIndex, i);
    dragIndex = null;
    overIndex = null;
  }
  function onDragEnd() {
    dragIndex = null;
    overIndex = null;
  }

  function heightKey(id: string) {
    return `chat:${agent.id}:${id}`;
  }
</script>

<div class="chat">
  <div class="chat-head">
    <span class="t">Chat</span>
    <span class="count mono">{panes.length}</span>
    <button class="new-chat" title="New chat" onclick={() => chatStore.addPane(agent.id)}>
      <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M8 3v10M3 8h10" /></svg>
      New chat
    </button>
  </div>

  <div class="stack">
    {#each panes as pane, i (pane.id)}
      {@const last = i === panes.length - 1}
      <div
        class="pane"
        class:dragover={overIndex === i && dragIndex !== null && dragIndex !== i}
        class:dragging={dragIndex === i}
        style={last ? "flex:1 1 auto" : `height:${layoutStore.panelHeight(heightKey(pane.id))}px`}
        ondragover={(e) => onDragOver(e, i)}
        ondrop={(e) => onDrop(e, i)}
        role="group"
      >
        <div
          class="pane-head"
          draggable="true"
          ondragstart={(e) => onDragStart(e, i)}
          ondragend={onDragEnd}
          role="button"
          tabindex="0"
          title="Drag to reorder"
        >
          <span class="grip" aria-hidden="true">
            <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><circle cx="6" cy="4" r="1"/><circle cx="10" cy="4" r="1"/><circle cx="6" cy="8" r="1"/><circle cx="10" cy="8" r="1"/><circle cx="6" cy="12" r="1"/><circle cx="10" cy="12" r="1"/></svg>
          </span>
          {#if pane.scope}<span class="scope-dot" aria-hidden="true"></span>{/if}
          <span class="pane-title">{pane.title}</span>
          {#if panes.length > 1}
            <button class="pane-x" title="Close chat" aria-label="Close chat" onclick={() => chatStore.closePane(agent.id, pane.id)}>×</button>
          {/if}
        </div>
        <ChatThread {agent} {binding} {pane} />
      </div>
      {#if !last}
        <Splitter
          axis="y"
          onresize={(d) => layoutStore.setPanelHeight(heightKey(pane.id), layoutStore.panelHeight(heightKey(pane.id)) + d)}
        />
      {/if}
    {/each}
  </div>
</div>

{#if binding.pendingExtension}
  <ExtensionDialog
    request={binding.pendingExtension}
    onrespond={(v) => binding.respondExtension(v)}
    oncancel={() => binding.cancelExtension()}
  />
{/if}

<style>
  .chat { flex: 1; display: flex; flex-direction: column; min-height: 0; min-width: 0; }
  .chat-head {
    display: flex; align-items: center; gap: var(--s2);
    height: 30px; flex: none; padding: 0 var(--s2) 0 var(--s4);
    border-bottom: 1px solid var(--border-subtle);
  }
  .chat-head .t { font-size: 10px; font-weight: 600; letter-spacing: 0.14em; text-transform: uppercase; color: var(--text-muted); }
  .chat-head .count { font-size: 10px; color: var(--text-muted); }
  .new-chat {
    display: inline-flex; align-items: center; gap: 5px; margin-left: auto;
    font: inherit; font-size: 11px; font-weight: 500; color: var(--text-secondary);
    background: transparent; border: 1px solid var(--border-subtle); border-radius: var(--r-sm);
    padding: 3px var(--s2); cursor: pointer; transition: background 0.12s, color 0.12s, border-color 0.12s;
  }
  .new-chat:hover { background: var(--bg-raised); color: var(--text-primary); border-color: var(--border); }

  .stack { flex: 1; display: flex; flex-direction: column; min-height: 0; overflow: hidden; }
  .pane {
    flex: none; min-height: 0; display: flex; flex-direction: column; overflow: hidden;
    transition: box-shadow 0s;
  }
  .pane.dragging { opacity: 0.5; }
  .pane.dragover { outline: 2px solid var(--accent); outline-offset: -2px; }
  .pane-head {
    display: flex; align-items: center; gap: var(--s2);
    height: 28px; flex: none; padding: 0 var(--s2) 0 var(--s3);
    background: var(--bg-sink); border-bottom: 1px solid var(--border-subtle);
    cursor: grab; user-select: none;
  }
  .pane-head:active { cursor: grabbing; }
  .grip { display: inline-flex; color: var(--text-muted); flex: none; }
  .scope-dot { width: 6px; height: 6px; border-radius: var(--r-full); background: var(--accent); flex: none; }
  .pane-title {
    font-size: 11px; font-weight: 500; color: var(--text-secondary);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis; flex: 1; min-width: 0;
  }
  .pane-x {
    width: 18px; height: 18px; display: inline-flex; align-items: center; justify-content: center;
    background: none; border: none; border-radius: var(--r-sm); color: var(--text-muted);
    font-size: 14px; line-height: 1; cursor: pointer; flex: none;
  }
  .pane-x:hover { background: var(--bg-raised); color: var(--status-error); }
</style>
