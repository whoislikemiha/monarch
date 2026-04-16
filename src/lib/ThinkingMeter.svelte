<script lang="ts">
  // Five-cell power meter. Cell N fills when intensity >= N, with warmer hue
  // the higher you go. The top cell pulses at max (xhigh) to communicate
  // "this is the heavy one". Everything is CSS — no external assets.
  let {
    intensity,
    size = "sm",
  }: {
    intensity: number;
    size?: "sm" | "md";
  } = $props();

  const cells = [1, 2, 3, 4, 5];
  const off = $derived(intensity <= 0);
</script>

<span class="meter" class:off class:sm={size === "sm"} class:md={size === "md"}>
  {#each cells as cell}
    <span
      class="cell"
      class:filled={cell <= intensity}
      class:pulse={cell === 5 && intensity >= 5}
      data-rank={cell}
    ></span>
  {/each}
</span>

<style>
  .meter {
    display: inline-flex;
    align-items: flex-end;
    gap: 2px;
    line-height: 0;
  }

  .meter.sm {
    height: 10px;
  }

  .meter.md {
    height: 14px;
    gap: 3px;
  }

  .cell {
    width: 3px;
    background: var(--text-muted);
    opacity: 0.22;
    border-radius: 1px;
    transition: opacity 0.15s, background 0.15s, box-shadow 0.15s;
  }

  .sm .cell[data-rank="1"] { height: 3px; }
  .sm .cell[data-rank="2"] { height: 5px; }
  .sm .cell[data-rank="3"] { height: 7px; }
  .sm .cell[data-rank="4"] { height: 9px; }
  .sm .cell[data-rank="5"] { height: 10px; }

  .md .cell {
    width: 4px;
  }
  .md .cell[data-rank="1"] { height: 4px; }
  .md .cell[data-rank="2"] { height: 7px; }
  .md .cell[data-rank="3"] { height: 10px; }
  .md .cell[data-rank="4"] { height: 12px; }
  .md .cell[data-rank="5"] { height: 14px; }

  .cell.filled {
    opacity: 1;
  }

  /* Cool → hot gradient as intensity ladders up. */
  .cell[data-rank="1"].filled { background: var(--accent-cyan); }
  .cell[data-rank="2"].filled { background: var(--accent-blue); }
  .cell[data-rank="3"].filled { background: var(--accent); }
  .cell[data-rank="4"].filled { background: var(--accent); box-shadow: 0 0 4px rgba(190, 149, 255, 0.35); }
  .cell[data-rank="5"].filled { background: var(--warning); box-shadow: 0 0 6px var(--warning-glow); }

  .cell.pulse {
    animation: pulse 1.6s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% {
      box-shadow: 0 0 6px var(--warning-glow);
      opacity: 1;
    }
    50% {
      box-shadow: 0 0 10px var(--warning-glow), 0 0 2px var(--warning);
      opacity: 0.85;
    }
  }

  .meter.off .cell {
    opacity: 0.15;
  }
</style>
