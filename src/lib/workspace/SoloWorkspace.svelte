<script lang="ts">
  /**
   * The solo workspace — the heart of the Agents view. Timeline + chat, arranged
   * by the captain: side-by-side or stacked, in either order (persisted).
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
  let orient = $derived(layoutStore.workspaceOrient);
  let timelineFirst = $derived(layoutStore.timelineFirst);

  function resizeSplit(delta: number) {
    const size = (orient === "h" ? splitEl?.clientWidth : splitEl?.clientHeight) ?? 0;
    if (size <= 0) return;
    // The splitter delta grows whichever region sits first.
    const d = (delta / size) * (timelineFirst ? 1 : -1);
    layoutStore.setTimelineFrac(layoutStore.timelineFrac + d);
  }

  const binding = new LiveBinding();
  onMount(() => {
    binding.bind(agent).catch((e) => console.error("bind failed:", e));
  });
  onDestroy(() => binding.destroy());

  /** Timeline action → open a chat pane scoped to that piece of work. */
  function askAbout(action: { id: string; intent: string; outcome?: string | null }) {
    const ctx =
      `[The captain is asking about this piece of your work: "${action.intent}"` +
      (action.outcome ? ` (outcome: ${action.outcome})` : "") +
      `. Answer with that context in mind.]`;
    chatStore.openScopedPane(agent.id, {
      id: action.id,
      kind: "action",
      label: action.intent,
      context: ctx,
    });
  }
</script>

{#snippet timelinePane()}
  <section class="pane timeline" style="flex-grow:{layoutStore.timelineFrac}" aria-label="Work timeline">
    <div class="pane-head">
      <span class="t">Timeline</span>
      <div class="grow"></div>
      <button class="arr" title={orient === "h" ? "Stack vertically" : "Place side by side"} aria-label="Toggle orientation" onclick={() => layoutStore.toggleOrient()}>
        {#if orient === "h"}
          <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.4"><rect x="2.5" y="2.5" width="11" height="4.5" rx="1"/><rect x="2.5" y="9" width="11" height="4.5" rx="1"/></svg>
        {:else}
          <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.4"><rect x="2.5" y="2.5" width="4.5" height="11" rx="1"/><rect x="9" y="2.5" width="4.5" height="11" rx="1"/></svg>
        {/if}
      </button>
      <button class="arr" title="Swap timeline and chat" aria-label="Swap" onclick={() => layoutStore.swapWorkspace()}>
        <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.4"><path d="M4 5h8l-2-2M12 11H4l2 2"/></svg>
      </button>
    </div>
    <div class="pane-body">
      <TimelinePane {agent} onask={askAbout} />
    </div>
  </section>
{/snippet}

{#snippet chatPane()}
  <section class="pane chat" style="flex-grow:{1 - layoutStore.timelineFrac}" aria-label="Chat">
    <ChatColumn {agent} {binding} />
  </section>
{/snippet}

<div class="solo">
  <ShadowHeader {agent} {binding} />
  <div class="split" class:vert={orient === "v"} bind:this={splitEl}>
    {#if timelineFirst}
      {@render timelinePane()}
      <Splitter axis={orient === "h" ? "x" : "y"} onresize={resizeSplit} />
      {@render chatPane()}
    {:else}
      {@render chatPane()}
      <Splitter axis={orient === "h" ? "x" : "y"} onresize={resizeSplit} />
      {@render timelinePane()}
    {/if}
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
  .split.vert { flex-direction: column; }
  .pane {
    flex: 1 1 0;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .pane-head {
    display: flex;
    align-items: center;
    gap: var(--s1);
    height: 30px;
    flex: none;
    padding: 0 var(--s2) 0 var(--s4);
    border-bottom: 1px solid var(--border-subtle);
  }
  .pane-head .t {
    font-size: 10px; font-weight: 600; letter-spacing: 0.14em;
    text-transform: uppercase; color: var(--text-muted);
  }
  .pane-head .grow { flex: 1; }
  .arr {
    width: 22px; height: 22px; display: inline-flex; align-items: center; justify-content: center;
    background: none; border: none; border-radius: var(--r-sm); color: var(--text-muted); cursor: pointer;
  }
  .arr:hover { background: var(--bg-raised); color: var(--text-primary); }
  .pane-body { flex: 1; min-height: 0; overflow-y: auto; padding: var(--s4); }
</style>
