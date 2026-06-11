<script lang="ts">
  /**
   * A stack of arrangeable tiles. Drag a tile's grip to reorder (pointer-based,
   * so it works in the Tauri/WebKit webview where HTML5 drag-and-drop does not),
   * drag the dividers between tiles to resize. One mechanism, used by both the
   * agent workspace (timeline + chats) and the inspector dock (panels).
   *
   * The parent supplies the ordered ids, a header + body snippet per tile, and
   * size getters/setters; this owns the grip, drag math, and splitters.
   */
  import type { Snippet } from "svelte";
  import Splitter from "./Splitter.svelte";

  interface Props {
    ids: string[];
    axis?: "h" | "v";
    onreorder: (from: number, to: number) => void;
    size: (id: string) => number;
    setSize: (id: string, px: number) => void;
    header: Snippet<[string]>;
    body: Snippet<[string]>;
  }
  let { ids, axis = "v", onreorder, size, setSize, header, body }: Props = $props();

  let containerEl: HTMLDivElement | undefined = $state();
  let from = $state<number | null>(null);
  let over = $state<number | null>(null);

  function tileEls(): HTMLElement[] {
    return containerEl ? [...containerEl.querySelectorAll<HTMLElement>(":scope > [data-tile]")] : [];
  }

  function gripDown(e: PointerEvent) {
    e.preventDefault();
    const tileEl = (e.currentTarget as HTMLElement).closest("[data-tile]");
    from = tileEls().findIndex((el) => el === tileEl);
    over = from;
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }
  function move(e: PointerEvent) {
    if (from === null) return;
    const els = tileEls();
    const pos = axis === "v" ? e.clientY : e.clientX;
    let idx = els.length - 1;
    for (let i = 0; i < els.length; i++) {
      const r = els[i].getBoundingClientRect();
      const mid = axis === "v" ? (r.top + r.bottom) / 2 : (r.left + r.right) / 2;
      if (pos < mid) { idx = i; break; }
    }
    over = idx;
  }
  function up() {
    if (from !== null && over !== null && over !== from) onreorder(from, over);
    from = null;
    over = null;
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
  }
</script>

<div class="stack {axis}" bind:this={containerEl}>
  {#each ids as id, i (id)}
    {@const last = i === ids.length - 1}
    <div
      class="tile"
      data-tile
      class:dragging={from === i}
      class:over={over === i && from !== null && from !== i}
      style={last ? "flex:1 1 0" : `flex:0 0 ${size(id)}px`}
    >
      <header class="tile-head">
        <button class="grip" title="Drag to move" aria-label="Drag to move" onpointerdown={gripDown}>
          <svg viewBox="0 0 16 16" width="11" height="11" fill="currentColor"><circle cx="6" cy="4" r="1"/><circle cx="10" cy="4" r="1"/><circle cx="6" cy="8" r="1"/><circle cx="10" cy="8" r="1"/><circle cx="6" cy="12" r="1"/><circle cx="10" cy="12" r="1"/></svg>
        </button>
        {@render header(id)}
      </header>
      <div class="tile-body">{@render body(id)}</div>
    </div>
    {#if !last}
      <Splitter axis={axis === "v" ? "y" : "x"} onresize={(d) => setSize(id, size(id) + d)} />
    {/if}
  {/each}
</div>

<style>
  .stack { flex: 1; display: flex; min-height: 0; min-width: 0; overflow: hidden; }
  .stack.v { flex-direction: column; }
  .stack.h { flex-direction: row; }
  .tile {
    display: flex; flex-direction: column; min-height: 0; min-width: 0; overflow: hidden;
    position: relative;
  }
  .tile.dragging { opacity: 0.45; }
  .tile.over::after {
    content: ""; position: absolute; inset: 0; pointer-events: none;
    outline: 2px solid var(--accent); outline-offset: -2px; border-radius: var(--r-sm);
  }
  .tile-head {
    display: flex; align-items: center; gap: var(--s2);
    height: 28px; flex: none; padding: 0 var(--s2) 0 4px;
    background: var(--bg-sink); border-bottom: 1px solid var(--border-subtle);
  }
  .grip {
    display: inline-flex; align-items: center; justify-content: center;
    width: 18px; height: 22px; padding: 0; flex: none;
    background: none; border: none; color: var(--text-muted); cursor: grab; touch-action: none;
  }
  .grip:active { cursor: grabbing; }
  .grip:hover { color: var(--text-secondary); }
  .tile-body { flex: 1; min-height: 0; min-width: 0; display: flex; flex-direction: column; overflow: hidden; }
</style>
