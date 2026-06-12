<script lang="ts">
  /**
   * MON-124 chat de-tooling: the one-line trace of tool work in a chat pane.
   * Chat is dialogue-only — the actual tool calls live on the timeline (the
   * two-surface rule). This chip says "work happened here" and, when the
   * corresponding action card is in the loaded feed, links to it
   * (scroll + flash). Non-linking when the action isn't loaded.
   */
  import type { Agent, ToolExecution } from "$lib/types";
  import { timelineStore } from "../timelineStore.svelte";
  import { liveAgentStore } from "$lib/toolbox/liveAgentStore.svelte";
  import { FALLBACK_ACTION_ID } from "../timelineModel";
  import EventIcon from "$lib/ui/EventIcon.svelte";

  interface Props {
    agent: Agent;
    executions: ToolExecution[];
    turnComplete: boolean;
  }
  let { agent, executions, turnComplete }: Props = $props();

  let running = $derived(!turnComplete || executions.some((e) => e.status === "running"));
  let errored = $derived(executions.some((e) => e.status === "error"));
  let actionId = $derived.by(() => {
    const persisted = timelineStore.findActionForToolCalls(
      agent.id,
      executions.map((e) => e.toolCallId),
    );
    if (persisted) return persisted;
    // No persisted record (project-less agent / unnarrated work) — link to
    // the timeline's synthesized fallback card if these tools are live there.
    const liveTools = liveAgentStore.byAgent.get(agent.id)?.toolExecutions;
    if (liveTools && executions.some((e) => liveTools.has(e.toolCallId))) {
      return FALLBACK_ACTION_ID;
    }
    return null;
  });

  function jump() {
    if (actionId) timelineStore.focusAction(agent.id, actionId);
  }
</script>

<button
  class="activity"
  class:linked={!!actionId}
  class:running
  onclick={jump}
  disabled={!actionId}
  title={actionId ? "Show this work on the timeline" : "Tool activity (not in the loaded timeline)"}
>
  <EventIcon kind="tool" size={10} tone={errored ? "error" : running ? "info" : "neutral"} muted={!errored && !running} />
  <span class="label">
    {running ? "working" : "worked"} · {executions.length} tool call{executions.length === 1 ? "" : "s"}
    {#if errored}<span class="err">· error</span>{/if}
  </span>
  {#if running}<span class="pulse" aria-hidden="true"></span>{/if}
  {#if actionId}<span class="link">view in timeline</span>{/if}
</button>

<style>
  .activity {
    align-self: center;
    display: inline-flex;
    align-items: center;
    gap: var(--s2);
    padding: 2px var(--s3);
    background: none;
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-sm);
    font: inherit;
    cursor: default;
  }
  .activity.linked { cursor: pointer; }
  .activity.linked:hover { background: var(--bg-panel); border-color: var(--border-strong); }
  .activity:focus-visible { outline: 2px solid var(--focus); outline-offset: 2px; }
  .label {
    font-family: "JetBrains Mono", monospace;
    font-size: 10px;
    color: var(--text-muted);
  }
  .err { color: var(--status-error); }
  .pulse {
    width: 6px; height: 6px; border-radius: var(--r-full);
    background: var(--status-info); flex: none;
    animation: chip-pulse 1.4s ease-in-out infinite;
  }
  @keyframes chip-pulse { 0%, 100% { opacity: 0.35; } 50% { opacity: 1; } }
  .link { font-size: 10px; color: var(--accent); }
  .activity.linked:hover .link { text-decoration: underline; }

  @media (prefers-reduced-motion: reduce) {
    .pulse { animation: none; }
  }
</style>
