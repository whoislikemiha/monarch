<script lang="ts">
  /**
   * One narrated coherent action in the work timeline. The shadow's own
   * narration ("intent") is the headline; outcome is the resolution. The whole
   * row is clickable → opens a chat scoped to this piece of work (wired in
   * slice 5). Drill-in to nested tool calls comes in a later pass.
   */
  interface Props {
    intent: string;
    outcome?: string | null;
    /** "active" = in flight, "done" = resolved, "auto" = auto-closed. */
    state?: "active" | "done" | "auto";
    time?: string | null;
    onask?: () => void;
  }
  let { intent, outcome = null, state = "done", time = null, onask }: Props = $props();
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div class="act" class:active={state === "active"} role={onask ? "button" : undefined} tabindex={onask ? 0 : undefined}
  onclick={() => onask?.()}
  onkeydown={(e) => { if (onask && (e.key === "Enter" || e.key === " ")) { e.preventDefault(); onask(); } }}
>
  <span class="rail" aria-hidden="true">
    <span class="node" class:active={state === "active"} class:auto={state === "auto"}></span>
  </span>
  <div class="body">
    <div class="headline">
      <span class="intent">{intent}</span>
      {#if time}<span class="time mono">{time}</span>{/if}
    </div>
    {#if outcome}<div class="outcome">{outcome}</div>{/if}
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
  }
  .act[role="button"] { cursor: pointer; }
  .act[role="button"]:hover { background: var(--bg-panel); }
  .act:focus-visible { outline: 2px solid var(--focus); outline-offset: -2px; }

  .rail { width: 14px; flex: none; display: flex; justify-content: center; position: relative; }
  .rail::before {
    content: ""; position: absolute; top: 0; bottom: 0; left: 50%;
    width: 1px; background: var(--border-subtle); transform: translateX(-0.5px);
  }
  .node {
    width: 9px; height: 9px; margin-top: 4px; border-radius: var(--r-full);
    background: var(--bg-overlay); border: 1.5px solid var(--border-strong);
    position: relative; z-index: 1; flex: none;
  }
  .node.active { background: var(--status-info); border-color: var(--status-info); }
  .node.auto { border-style: dashed; }

  .body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .headline { display: flex; align-items: baseline; gap: var(--s3); }
  .intent {
    font-size: 12.5px; color: var(--text-primary); font-weight: 500; line-height: 1.45;
    min-width: 0;
  }
  .act.active .intent { color: var(--text-primary); }
  .time { font-size: 9.5px; color: var(--text-muted); margin-left: auto; flex: none; }
  .outcome { font-size: 11.5px; color: var(--text-muted); line-height: 1.5; }

  .ask {
    align-self: center; flex: none;
    display: inline-flex; align-items: center; justify-content: center;
    width: 22px; height: 22px; padding: 0;
    background: transparent; border: 1px solid var(--border-subtle); border-radius: var(--r-sm);
    color: var(--text-muted); cursor: pointer; opacity: 0; transition: opacity 0.12s, color 0.12s, background 0.12s;
  }
  .act:hover .ask { opacity: 1; }
  .ask:hover { background: var(--bg-raised); color: var(--accent); border-color: var(--accent-border-subtle); }
</style>
