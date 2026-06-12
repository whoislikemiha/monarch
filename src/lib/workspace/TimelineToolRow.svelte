<script lang="ts">
  /**
   * One bare tool call in the timeline stream (MON-124 flat chronology) — a
   * tool the shadow ran with no narrated action above it. Narration AUGMENTS
   * the timeline; its absence doesn't hide work. Same data-row grammar as the
   * nested tool rows inside action cards, at stream level.
   */
  import type { ToolCallView } from "./timelineModel";
  import { fmtDuration } from "./timelineModel";
  import EventIcon from "$lib/ui/EventIcon.svelte";

  interface Props {
    tool: ToolCallView;
    time?: string | null;
  }
  let { tool, time = null }: Props = $props();
</script>

<div class="trow" class:error={tool.isError} class:running={tool.status === "running"}>
  <span class="rail" aria-hidden="true">
    <span class="node">
      <EventIcon kind="tool" size={11} tone={tool.isError ? "error" : tool.status === "running" ? "info" : "neutral"} muted={!tool.isError && tool.status !== "running"} />
    </span>
  </span>
  <span class="name mono">{tool.toolName}</span>
  {#if tool.target}
    <span class="target mono trunc-head" title={tool.target}>{tool.target}</span>
  {:else if tool.argsPreview}
    <span class="target mono dim" title={tool.argsPreview}>{tool.argsPreview}</span>
  {/if}
  <span class="end mono">
    {#if tool.status === "running"}
      <span class="st run">running…</span>
    {:else if tool.isError}
      <span class="st err">✕ error</span>
    {:else if fmtDuration(tool.durationMs)}
      {fmtDuration(tool.durationMs)}
    {/if}
    {#if time}<span class="time">{time}</span>{/if}
  </span>
</div>

<style>
  .trow {
    display: flex;
    align-items: baseline;
    gap: var(--s3);
    padding: 2px var(--s2) 2px 0;
    min-width: 0;
    font-size: 10.5px;
    border-radius: var(--r-sm);
  }
  .trow:hover { background: var(--bg-panel); }

  .rail { width: 14px; flex: none; display: flex; justify-content: center; position: relative; align-self: stretch; }
  .rail::before {
    content: ""; position: absolute; top: 0; bottom: 0; left: 50%;
    width: 1px; background: var(--border-subtle); transform: translateX(-0.5px);
  }
  .node {
    position: relative; z-index: 1; flex: none; align-self: center;
    display: flex; background: var(--bg-base); padding: 1px 0;
  }

  .name { color: var(--text-secondary); flex: none; min-width: 44px; }
  .trow.error .name { color: var(--status-error); }
  .target {
    color: var(--text-muted); min-width: 0; flex: 1;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .target.dim { opacity: 0.7; }
  .trunc-head { direction: rtl; text-align: left; unicode-bidi: isolate; }

  .end { flex: none; margin-left: auto; font-size: 9.5px; color: var(--text-muted); display: inline-flex; gap: var(--s2); }
  .st.run { color: var(--status-info); }
  .st.err { color: var(--status-error); }
  .time { color: var(--text-muted); }
</style>
