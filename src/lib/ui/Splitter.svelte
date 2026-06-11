<script lang="ts">
  /**
   * Draggable splitter. Delta-based: reports the pointer movement in px since
   * the last event so the parent can apply it to whatever it's sizing (a flex
   * fraction, a width, a height). Axis sets the orientation + cursor.
   */
  interface Props {
    axis?: "x" | "y";
    onresize: (deltaPx: number) => void;
  }
  let { axis = "x", onresize }: Props = $props();

  let dragging = $state(false);
  let last = 0;

  function down(e: PointerEvent) {
    dragging = true;
    last = axis === "x" ? e.clientX : e.clientY;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function move(e: PointerEvent) {
    if (!dragging) return;
    const cur = axis === "x" ? e.clientX : e.clientY;
    onresize(cur - last);
    last = cur;
  }
  function up(e: PointerEvent) {
    dragging = false;
    (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
  }
</script>

<div
  class="splitter {axis}"
  class:dragging
  role="separator"
  aria-orientation={axis === "x" ? "vertical" : "horizontal"}
  aria-label="Resize"
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
></div>

<style>
  .splitter { flex: none; background: var(--border-subtle); transition: background 0.12s; touch-action: none; }
  .splitter.x { width: 1px; cursor: col-resize; position: relative; }
  .splitter.y { height: 1px; cursor: row-resize; position: relative; }
  /* Invisible wider hit area so the 1px line is easy to grab. */
  .splitter::after { content: ""; position: absolute; }
  .splitter.x::after { inset: 0 -3px; }
  .splitter.y::after { inset: -3px 0; }
  .splitter:hover, .splitter.dragging { background: var(--accent); }
</style>
