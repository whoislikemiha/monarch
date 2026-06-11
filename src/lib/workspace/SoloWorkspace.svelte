<script lang="ts">
  /**
   * The solo workspace — the heart of the Agents view. A 50/50 split between
   * the narrated work TIMELINE (left) and CHAT (right), under a slim header.
   *
   * Owns the live-state binding for this agent/session (one per mount; the
   * parent keys this component by viewKey so a new session remounts cleanly).
   */
  import { onMount, onDestroy } from "svelte";
  import type { Agent } from "$lib/types";
  import ShadowHeader from "./ShadowHeader.svelte";
  import TimelinePane from "./TimelinePane.svelte";
  import ChatColumn from "./ChatColumn.svelte";
  import { LiveBinding } from "./liveBinding.svelte";
  import { chatStore } from "./chatStore.svelte";
  import { layoutStore } from "$lib/layout/layoutStore.svelte";
  import Splitter from "$lib/ui/Splitter.svelte";

  interface Props {
    agent: Agent;
  }
  let { agent }: Props = $props();

  let splitEl: HTMLDivElement | undefined = $state();
  function resizeSplit(dx: number) {
    const w = splitEl?.clientWidth ?? 0;
    if (w > 0) layoutStore.setTimelineFrac(layoutStore.timelineFrac + dx / w);
  }

  const binding = new LiveBinding();
  onMount(() => {
    binding.bind(agent).catch((e) => console.error("bind failed:", e));
  });
  onDestroy(() => binding.destroy());

  /** Timeline action → scope the chat to that piece of work. */
  function askAbout(action: { id: string; intent: string; outcome?: string | null }) {
    const ctx =
      `[The captain is asking about this piece of your work: "${action.intent}"` +
      (action.outcome ? ` (outcome: ${action.outcome})` : "") +
      `. Answer with that context in mind.]`;
    chatStore.setScope(agent.id, {
      id: action.id,
      kind: "action",
      label: action.intent,
      context: ctx,
    });
  }
</script>

<div class="solo">
  <ShadowHeader {agent} {binding} />
  <div class="split" bind:this={splitEl}>
    <section class="pane timeline" style="flex-grow:{layoutStore.timelineFrac}" aria-label="Work timeline">
      <div class="pane-head"><span class="t">Timeline</span></div>
      <div class="pane-body">
        <TimelinePane {agent} onask={askAbout} />
      </div>
    </section>
    <Splitter axis="x" onresize={resizeSplit} />
    <section class="pane chat" style="flex-grow:{1 - layoutStore.timelineFrac}" aria-label="Chat">
      <ChatColumn {agent} {binding} />
    </section>
  </div>
</div>

<style>
  .solo {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
  .split {
    flex: 1;
    display: flex;
    flex-direction: row;
    min-height: 0;
    min-width: 0;
  }
  .pane {
    flex: 1 1 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .pane-head {
    display: flex;
    align-items: center;
    height: 30px;
    flex: none;
    padding: 0 var(--s4);
    border-bottom: 1px solid var(--border-subtle);
  }
  .pane-head .t {
    font-size: 10px; font-weight: 600; letter-spacing: 0.14em;
    text-transform: uppercase; color: var(--text-muted);
  }
  .pane-body { flex: 1; min-height: 0; overflow-y: auto; padding: var(--s4); }
</style>
