<script lang="ts">
  /**
   * A single conversation with the shadow: the live message stream + composer.
   * For now every chat projects the agent's live session; scoped chats (slice 5)
   * add a context primer on top of the same session.
   */
  import type { Agent } from "$lib/types";
  import { liveAgentStore, detachedLiveState } from "$lib/toolbox/liveAgentStore.svelte";
  import type { LiveBinding } from "./liveBinding.svelte";
  import { chatStore } from "./chatStore.svelte";
  import MessageStream from "./message/MessageStream.svelte";
  import Composer from "./Composer.svelte";

  interface Props {
    agent: Agent;
    binding: LiveBinding;
  }
  let { agent, binding }: Props = $props();

  const DETACHED = detachedLiveState();
  let live = $derived(liveAgentStore.byAgent.get(agent.id) ?? DETACHED);
  let hasMessages = $derived(live.items.some((i) => i.kind === "user" || i.kind === "assistant"));
  let scope = $derived(chatStore.getScope(agent.id));

  let composer: Composer | undefined = $state();
  // Focus the composer when a timeline action scopes the chat.
  $effect(() => {
    if (scope) composer?.focus();
  });

  function send(text: string) {
    let message = text;
    if (scope) {
      const primer = chatStore.consumePrimer(agent.id, scope);
      if (primer) message = `${primer}\n\n${text}`;
    }
    binding.sendPrompt(agent, message).catch((e) => console.error("send failed:", e));
  }
  function stop() {
    binding.abort(agent).catch(() => {});
  }
</script>

<div class="thread">
  {#if hasMessages}
    <MessageStream items={live.items} streamingMessage={live.streamingMessage} />
  {:else}
    <div class="blank">
      <p class="lead">Talk to {agent.name}</p>
      <p class="hint">
        {#if scope}
          Ask about “{scope.label}” — {agent.name} answers with that work in mind.
        {:else}
          Ask anything — it shares the same memory as the shadow doing the work.
        {/if}
      </p>
    </div>
  {/if}

  <Composer
    bind:this={composer}
    streaming={live.isStreaming}
    onsend={send}
    onstop={stop}
    placeholder={scope ? `Ask about “${scope.label}”…` : `Message ${agent.name}…`}
  />
</div>

<style>
  .thread { flex: 1; display: flex; flex-direction: column; min-height: 0; min-width: 0; }
  .blank {
    flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: var(--s2); text-align: center; padding: var(--s5);
  }
  .blank .lead { font-size: 14px; color: var(--text-primary); }
  .blank .hint { font-size: 12px; color: var(--text-muted); max-width: 34ch; line-height: 1.6; }
</style>
