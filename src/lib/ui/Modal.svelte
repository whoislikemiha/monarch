<script lang="ts">
  /**
   * Flat modal shell on the design system: scrim + bordered panel + header.
   * No shadow (house rule) — depth is the scrim + 1px border. Esc / scrim-click
   * close. Body is a snippet so callers compose their own content.
   */
  import type { Snippet } from "svelte";

  interface Props {
    title: string;
    onclose: () => void;
    width?: number;
    /** Remove body padding — caller owns the full body layout (e.g. nav + content columns). */
    flush?: boolean;
    children: Snippet;
  }
  let { title, onclose, width = 520, flush = false, children }: Props = $props();

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      onclose();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
<div class="scrim" role="presentation" onclick={onclose}>
  <div
    class="modal"
    role="dialog"
    aria-modal="true"
    aria-label={title}
    tabindex="-1"
    style="width:min({width}px, calc(100vw - 48px))"
    onclick={(e) => e.stopPropagation()}
  >
    <header class="m-head">
      <h2>{title}</h2>
      <button class="m-close" title="Close" aria-label="Close" onclick={onclose}>
        <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M4 4l8 8M12 4l-8 8" /></svg>
      </button>
    </header>
    <div class="m-body" class:flush>
      {@render children()}
    </div>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 100;
    background: var(--scrim);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--s5);
    overflow-y: auto;
  }
  .modal {
    background: var(--bg-panel);
    border: 1px solid var(--border-strong);
    border-radius: var(--r-lg);
    max-height: calc(100vh - 48px);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .m-head {
    display: flex;
    align-items: center;
    height: 44px;
    flex: none;
    padding: 0 var(--s2) 0 var(--s4);
    border-bottom: 1px solid var(--border-subtle);
  }
  .m-head h2 { margin: 0; font-size: 13px; font-weight: 600; color: var(--text-primary); flex: 1; }
  .m-close {
    width: 28px; height: 28px; display: inline-flex; align-items: center; justify-content: center;
    background: none; border: none; border-radius: var(--r-sm); color: var(--text-muted); cursor: pointer;
  }
  .m-close:hover { background: var(--bg-raised); color: var(--text-primary); }
  .m-body { flex: 1; min-height: 0; overflow-y: auto; padding: var(--s4); }
  .m-body.flush { padding: 0; overflow: hidden; display: flex; }
</style>
