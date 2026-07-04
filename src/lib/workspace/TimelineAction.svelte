<script lang="ts">
  /**
   * One coherent action in the work timeline, as a drill-in CARD. The agent's
   * narration ("intent") is the headline; the outcome is the resolution. The
   * card expands to a typed child list — tool calls (mono data-rows), explicit
   * decisions, spawned chats — and carries artifact chips in its footer.
   * Children are heterogeneous by design: future delegated-run groups slot
   * into the same list (Arc II seam). The active card auto-expands and ticks.
   */
  import type { ActionView, ToolCallView } from "./timelineModel";
  import { clockTime, elapsedClock, fmtDuration } from "./timelineModel";
  import EventIcon from "$lib/ui/EventIcon.svelte";
  import ToolCallDetail from "./ToolCallDetail.svelte";
  import { SvelteSet } from "svelte/reactivity";

  interface Props {
    action: ActionView;
    /** "active" = in flight, "done" = resolved, "auto" = auto-closed. */
    phase?: "active" | "done" | "auto";
    /** Tool calls to render — persisted children, live-merged for the active card. */
    tools?: ToolCallView[];
    /** Ticking wall clock (ms) from the pane — drives the active elapsed timer. */
    nowMs?: number;
    /** Open a chat scoped to this action. */
    onask?: () => void;
    /** Re-open / focus a chat previously spawned from this action. */
    onopenchat?: (scopeId: string, label: string) => void;
  }
  let { action, phase = "done", tools, nowMs = Date.now(), onask, onopenchat }: Props = $props();

  let toolList = $derived(tools ?? action.toolCalls);
  let childCount = $derived(toolList.length + action.decisions.length);

  /** Active cards open by default; finished cards collapse to the summary. */
  let manualExpand = $state<boolean | null>(null);
  let expanded = $derived(manualExpand ?? phase === "active");

  /** MON-130: per-tool expanded detail (full input/output on demand). */
  let openTools = new SvelteSet<string>();
  function toggleTool(toolCallId: string) {
    if (openTools.has(toolCallId)) openTools.delete(toolCallId);
    else openTools.add(toolCallId);
  }

  let time = $derived(
    phase === "active" ? elapsedClock(action.startedAt, nowMs) : clockTime(action.completedAt ?? action.startedAt, nowMs),
  );

  function toggle() {
    manualExpand = !expanded;
  }

  /** The whole card is a toggle target — not just the headline. Clicks on
   * real controls (buttons, chips) and inside the expanded child list keep
   * their own meaning; selecting text never collapses the card. */
  function onCardClick(e: MouseEvent) {
    const el = e.target as HTMLElement;
    if (el.closest("button, a, pre, .children")) return;
    if (window.getSelection()?.toString()) return;
    toggle();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<!-- keyboard toggle lives on the headline button; the card click is a bonus hit area -->
<div class="act" class:active={phase === "active"} class:expanded onclick={onCardClick}>
  <span class="rail" aria-hidden="true">
    <span class="node">
      <EventIcon kind="action" size={13} tone={phase === "active" ? "info" : "neutral"} muted={phase === "auto"} />
    </span>
  </span>

  <div class="body">
    <button
      class="headline"
      onclick={toggle}
      aria-expanded={expanded}
      title={action.intent}
    >
      <span class="intent" class:clamped={!expanded}>{action.intent}</span>
      {#if time}<span class="time mono" class:live={phase === "active"}>{time}</span>{/if}
    </button>

    {#if action.outcome && !action.autoClosed}
      <div class="outcome">{action.outcome}</div>
    {/if}

    {#if childCount > 0 || action.filesTouched.length > 0 || action.chatsSpawned.length > 0}
      <div class="meta">
        {#if !expanded && childCount > 0}
          <button class="chip count" onclick={toggle}>
            {toolList.length} tool{toolList.length === 1 ? "" : "s"}{action.decisions.length
              ? ` · ${action.decisions.length} decision${action.decisions.length === 1 ? "" : "s"}`
              : ""}
          </button>
        {/if}
        {#if action.filesTouched.length > 0}
          <button class="chip files" onclick={toggle} title={action.filesTouched.join("\n")}>
            <svg viewBox="0 0 16 16" width="10" height="10" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true">
              <path d="M4 2h5l3 3v9H4z" /><path d="M9 2v3h3" />
            </svg>
            {action.filesTouched.length} file{action.filesTouched.length === 1 ? "" : "s"}
          </button>
        {/if}
        {#each action.chatsSpawned as chat (chat.eventId)}
          <button
            class="chip chat"
            title="Re-open this chat"
            onclick={(e) => {
              e.stopPropagation();
              onopenchat?.(chat.scopeId, chat.label);
            }}
          >
            <svg viewBox="0 0 16 16" width="10" height="10" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true">
              <path d="M3 4h10v6H7l-3 2.5V10H3z" />
            </svg>
            chat
          </button>
        {/each}
      </div>
    {/if}

    {#if expanded}
      <div class="children">
        {#each toolList as tool (tool.eventId)}
          <button
            class="tool"
            class:error={tool.isError}
            class:running={tool.status === "running"}
            onclick={() => toggleTool(tool.toolCallId)}
            aria-expanded={openTools.has(tool.toolCallId)}
            title={openTools.has(tool.toolCallId) ? "Collapse" : "Show full input"}
          >
            <EventIcon kind="tool" size={10} tone={tool.isError ? "error" : "neutral"} muted={!tool.isError} />
            <span class="t-name mono">{tool.toolName}</span>
            {#if tool.target}
              <span class="t-target mono" class:trunc-head={!openTools.has(tool.toolCallId)} class:wrap={openTools.has(tool.toolCallId)} title={tool.target}>{tool.target}</span>
            {:else if tool.argsPreview}
              <span class="t-target mono dim" class:wrap={openTools.has(tool.toolCallId)} title={tool.argsPreview}>{tool.argsPreview}</span>
            {/if}
            <span class="t-end mono">
              {#if tool.status === "running"}
                <span class="t-status run">running…</span>
              {:else if tool.isError}
                <span class="t-status err">✕ error</span>
              {:else if fmtDuration(tool.durationMs)}
                {fmtDuration(tool.durationMs)}
              {:else}
                <span class="t-status ok">done</span>
              {/if}
            </span>
          </button>
          {#if openTools.has(tool.toolCallId)}
            <ToolCallDetail {tool} />
          {/if}
        {/each}
        {#each action.decisions as d (d.eventId)}
          <div class="decision">
            <EventIcon kind="decision" size={11} muted />
            <span class="d-label">decision</span>
            <span class="d-text">{d.decision}{d.rationale ? ` — ${d.rationale}` : ""}</span>
          </div>
        {/each}
        {#if action.filesTouched.length > 0}
          <div class="files-list">
            {#each action.filesTouched as f (f)}
              <span class="f-path mono">{f}</span>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  </div>

  {#if onask}
    <button class="ask" title="Ask about this" aria-label="Ask about this work" onclick={(e) => { e.stopPropagation(); onask?.(); }}>
      <svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.5">
        <path d="M3 4h10v6H7l-3 2.5V10H3z" />
      </svg>
    </button>
  {/if}
</div>

<style>
  .act {
    display: flex;
    gap: var(--s3);
    padding: var(--s2) var(--s2) var(--s2) 0;
    border-radius: var(--r-md);
    position: relative;
    cursor: pointer;
  }
  .act:hover { background: var(--bg-panel); }
  /* The expanded child list has its own controls + selectable text. */
  .act .children { cursor: auto; }

  .rail { width: 14px; flex: none; display: flex; justify-content: center; position: relative; }
  .rail::before {
    content: ""; position: absolute; top: 0; bottom: 0; left: 50%;
    width: 1px; background: var(--border-subtle); transform: translateX(-0.5px);
  }
  .node {
    margin-top: 3px; position: relative; z-index: 1; flex: none;
    display: flex; align-items: center; justify-content: center;
    background: var(--bg-base); padding: 1px 0;
  }

  .body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; }

  .headline {
    display: flex; align-items: baseline; gap: var(--s3);
    background: none; border: none; padding: 0; margin: 0;
    text-align: left; cursor: pointer; min-width: 0; width: 100%;
    font: inherit; color: inherit;
  }
  .headline:focus-visible { outline: 2px solid var(--focus); outline-offset: 2px; border-radius: var(--r-sm); }
  .intent {
    font-size: 12.5px; color: var(--text-primary); font-weight: 500; line-height: 1.45;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  /* Collapsed headlines are headlines — clamp; expanding the card shows the
   * full intent (MON-130). */
  .intent.clamped {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .time { font-size: 9.5px; color: var(--text-muted); margin-left: auto; flex: none; }
  .time.live { color: var(--status-info); font-variant-numeric: tabular-nums; }

  .outcome { font-size: 11.5px; color: var(--text-muted); line-height: 1.5; overflow-wrap: anywhere; }

  .meta { display: flex; gap: var(--s2); flex-wrap: wrap; padding-top: 1px; }
  .chip {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 1px 7px;
    background: var(--bg-raised); border: 1px solid var(--border-subtle);
    border-radius: var(--r-sm);
    font-size: 9.5px; color: var(--text-muted);
    cursor: pointer; line-height: 1.6;
  }
  .chip:hover { color: var(--text-secondary); border-color: var(--border-strong); }
  .chip.chat:hover { color: var(--accent); border-color: var(--accent-border-subtle); }

  .children {
    display: flex; flex-direction: column; gap: 2px;
    margin-top: var(--s1);
    padding: var(--s2) 0 var(--s1) var(--s3);
    border-left: 1px solid var(--border-subtle);
  }
  .tool {
    display: flex; align-items: baseline; gap: var(--s3);
    min-width: 0; padding: 1px 0; width: 100%;
    background: none; border: none; margin: 0;
    font: inherit; color: inherit; text-align: left; cursor: pointer;
    border-radius: var(--r-sm);
  }
  .tool:hover { background: var(--bg-raised); }
  .tool:focus-visible { outline: 2px solid var(--focus); outline-offset: 1px; }
  .t-name { color: var(--text-secondary); flex: none; min-width: 44px; font-size: 10.5px; }
  .t-target {
    color: var(--text-muted); min-width: 0; flex: 1;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-size: 10.5px;
  }
  .t-target.dim { opacity: 0.7; }
  /* Head truncation: paths keep their TAIL visible (…/module.rs), never the
   * head — the "path truncation" backport fix. RTL-ellipsis trick. */
  .trunc-head { direction: rtl; text-align: left; unicode-bidi: isolate; }
  /* An open tool row shows everything — wrap instead of clipping. */
  .t-target.wrap { white-space: normal; overflow-wrap: anywhere; direction: ltr; }
  .t-end { flex: none; margin-left: auto; font-size: 9.5px; color: var(--text-muted); }
  .t-status.run { color: var(--status-info); }
  .t-status.err { color: var(--status-error); }
  .t-status.ok { color: var(--text-muted); }
  .tool.error .t-name { color: var(--status-error); }

  .decision { display: flex; gap: var(--s2); align-items: baseline; font-size: 11px; padding: 1px 0; min-width: 0; }
  .d-label {
    flex: none; font-size: 9px; font-weight: 700; letter-spacing: 0.1em;
    text-transform: uppercase; color: var(--text-muted);
  }
  .d-text { color: var(--text-secondary); line-height: 1.5; min-width: 0; }

  .files-list { display: flex; flex-direction: column; gap: 1px; padding-top: 2px; }
  /* Only visible when the card is expanded — show full paths, wrapped. */
  .f-path {
    font-size: 10px; color: var(--text-muted);
    overflow-wrap: anywhere;
  }

  .ask {
    align-self: flex-start; margin-top: 3px; flex: none;
    display: inline-flex; align-items: center; justify-content: center;
    width: 22px; height: 22px; padding: 0;
    background: transparent; border: 1px solid var(--border-subtle); border-radius: var(--r-sm);
    color: var(--text-muted); cursor: pointer; opacity: 0; transition: opacity 0.12s, color 0.12s, background 0.12s;
  }
  .act:hover .ask { opacity: 1; }
  .ask:hover { background: var(--bg-raised); color: var(--accent); border-color: var(--accent-border-subtle); }
</style>
