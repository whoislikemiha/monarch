<script lang="ts">
  /**
   * The chat side of the solo workspace. Hosts conversation tabs and a
   * "New chat" button so you can start talking to the shadow about anything the
   * moment you select it. Slice 4 ships the live conversation + new-chat;
   * slice 5 adds scoped tabs spawned from timeline actions.
   */
  import type { Agent } from "$lib/types";
  import { agentStore } from "$lib/stores/agentStore.svelte";
  import type { LiveBinding } from "./liveBinding.svelte";
  import { chatStore } from "./chatStore.svelte";
  import ChatThread from "./ChatThread.svelte";
  import ExtensionDialog from "$lib/ExtensionDialog.svelte";

  interface Props {
    agent: Agent;
    binding: LiveBinding;
  }
  let { agent, binding }: Props = $props();

  let scope = $derived(chatStore.getScope(agent.id));

  function newChat() {
    // Fresh conversation with the same shadow. For a running agent this opens a
    // new session (cheap); a stopped agent starts one on first message.
    chatStore.clearScope(agent.id);
    agentStore.newConversation(agent.id).catch((e) => console.error("new chat failed:", e));
  }
</script>

<div class="chat">
  <div class="tabs">
    <button class="tab" class:active={!scope} onclick={() => chatStore.clearScope(agent.id)}>
      <span class="tab-label">Chat</span>
    </button>
    {#if scope}
      <div class="tab active scoped" title={scope.label}>
        <span class="scope-dot" aria-hidden="true"></span>
        <span class="tab-label">{scope.label}</span>
        <button class="scope-x" title="Back to general chat" aria-label="Clear scope" onclick={() => chatStore.clearScope(agent.id)}>×</button>
      </div>
    {/if}
    <button class="new-chat" title="New chat" onclick={newChat}>
      <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M8 3v10M3 8h10" /></svg>
      New chat
    </button>
  </div>

  <ChatThread {agent} {binding} />
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
  .tabs {
    display: flex;
    align-items: center;
    gap: var(--s1);
    height: 30px;
    flex: none;
    padding: 0 var(--s2) 0 var(--s3);
    border-bottom: 1px solid var(--border-subtle);
  }
  .tab {
    display: flex; align-items: center; gap: var(--s2);
    padding: 3px var(--s3); border-radius: var(--r-sm);
    font-size: 11px; color: var(--text-muted);
    background: none; border: none; cursor: pointer; font: inherit;
    max-width: 240px;
  }
  .tab:hover { color: var(--text-secondary); }
  .tab.active { color: var(--text-primary); background: var(--bg-overlay); }
  .tab.scoped { color: var(--accent); }
  .tab-label { font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .scope-dot { width: 6px; height: 6px; border-radius: var(--r-full); background: var(--accent); flex: none; }
  .scope-x {
    background: none; border: none; cursor: pointer; color: var(--text-muted);
    font-size: 14px; line-height: 1; padding: 0 0 0 2px; flex: none;
  }
  .scope-x:hover { color: var(--status-error); }
  .new-chat {
    display: inline-flex; align-items: center; gap: 5px;
    margin-left: auto;
    font: inherit; font-size: 11px; font-weight: 500; color: var(--text-secondary);
    background: transparent; border: 1px solid var(--border-subtle); border-radius: var(--r-sm);
    padding: 3px var(--s2); cursor: pointer; transition: background 0.12s, color 0.12s, border-color 0.12s;
  }
  .new-chat:hover { background: var(--bg-raised); color: var(--text-primary); border-color: var(--border); }
</style>
