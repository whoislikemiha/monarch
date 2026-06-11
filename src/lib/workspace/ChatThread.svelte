<script lang="ts">
  /**
   * One chat pane: the shared session's stream filtered to this pane's turns,
   * plus a composer that tags new turns to this pane (and injects scope context
   * on the first scoped message).
   */
  import type { Agent, DisplayItem } from "$lib/types";
  import { liveAgentStore, detachedLiveState } from "$lib/toolbox/liveAgentStore.svelte";
  import type { LiveBinding } from "./liveBinding.svelte";
  import { chatStore, type ChatPane } from "./chatStore.svelte";
  import MessageStream from "./message/MessageStream.svelte";
  import Composer from "./Composer.svelte";

  interface Props {
    agent: Agent;
    binding: LiveBinding;
    pane: ChatPane;
  }
  let { agent, binding, pane }: Props = $props();

  const DETACHED = detachedLiveState();
  let live = $derived(liveAgentStore.byAgent.get(agent.id) ?? DETACHED);

  // Filter the shared stream to the turns that belong to this pane.
  let items = $derived.by<DisplayItem[]>(() => {
    const out: DisplayItem[] = [];
    let ord = -1;
    for (const item of live.items) {
      if (item.kind === "user") ord++;
      const owner = ord < 0 ? "general" : chatStore.paneForOrdinal(agent.id, ord);
      if (owner === pane.id) out.push(item);
    }
    return out;
  });

  let userCount = $derived(live.items.filter((i) => i.kind === "user").length);
  // The streaming reply belongs to the most recent user turn.
  let streamingMine = $derived(
    live.isStreaming &&
      userCount > 0 &&
      chatStore.paneForOrdinal(agent.id, userCount - 1) === pane.id,
  );
  let hasMessages = $derived(items.some((i) => i.kind === "user" || i.kind === "assistant"));

  let composer: Composer | undefined = $state();
  // Focus a scoped pane when it's freshly opened from a timeline action.
  $effect(() => {
    if (pane.scope) composer?.focus();
  });

  function send(text: string) {
    // Tag the upcoming user turn (its ordinal = current user count) to this pane.
    chatStore.assignTurn(agent.id, userCount, pane.id);
    let message = text;
    if (pane.scope) {
      const primer = chatStore.consumePrimer(agent.id, pane.scope);
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
    <MessageStream {items} streamingMessage={streamingMine ? live.streamingMessage : null} />
  {:else}
    <div class="blank">
      <p class="hint">
        {#if pane.scope}
          Ask about “{pane.scope.label}” — {agent.name} answers with that work in mind.
        {:else}
          Talk to {agent.name}. Shares the same memory as the shadow doing the work.
        {/if}
      </p>
    </div>
  {/if}

  <Composer
    bind:this={composer}
    streaming={streamingMine}
    onsend={send}
    onstop={stop}
    placeholder={pane.scope ? `Ask about “${pane.scope.label}”…` : `Message ${agent.name}…`}
  />
</div>

<style>
  .thread { flex: 1; display: flex; flex-direction: column; min-height: 0; min-width: 0; }
  .blank { flex: 1; display: flex; align-items: center; justify-content: center; padding: var(--s4); }
  .blank .hint { font-size: 12px; color: var(--text-muted); max-width: 36ch; line-height: 1.6; text-align: center; }
</style>
