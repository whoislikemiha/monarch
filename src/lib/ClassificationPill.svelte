<script lang="ts">
  /**
   * MON-82 — Slice 1: complexity pill shown next to each user message.
   * Read-only. Click expands a popover with rationale/model/tokens/latency.
   * Monarch override is deferred to Slice 3 where the label drives Architect
   * invocation.
   */
  import type { ClassificationInfo } from "./classifierStore.svelte";
  import {
    COMPLEXITY_COLORS,
    COMPLEXITY_DESCRIPTIONS,
  } from "./classifier-types";

  let { info }: { info: ClassificationInfo } = $props();

  let open = $state(false);

  const isFailed = $derived(!!info.error || !info.complexity);
  const color = $derived(
    isFailed ? "#6b6b6b" : COMPLEXITY_COLORS[info.complexity!] ?? "#6b6b6b",
  );
  const label = $derived(isFailed ? "failed" : info.complexity!);
  const confPct = $derived(
    info.confidence != null ? Math.round(info.confidence * 100) : null,
  );
</script>

<button
  type="button"
  class="pill"
  class:failed={isFailed}
  style="--pill-color: {color};"
  aria-expanded={open}
  onclick={() => (open = !open)}
  title={isFailed
    ? info.error ?? "Classifier failed"
    : COMPLEXITY_DESCRIPTIONS[info.complexity!]}
>
  <span class="dot"></span>
  <span class="label">{label}</span>
  {#if confPct != null && !isFailed}
    <span class="conf">{confPct}%</span>
  {/if}
</button>

{#if open}
  <div class="popover" role="dialog">
    {#if isFailed}
      <div class="row"><span class="k">status</span><span class="v">failed</span></div>
      {#if info.error}
        <div class="row"><span class="k">error</span><span class="v err">{info.error}</span></div>
      {/if}
    {:else}
      <div class="row">
        <span class="k">complexity</span>
        <span class="v">{info.complexity}</span>
      </div>
      {#if info.rationale}
        <div class="row"><span class="k">rationale</span><span class="v">{info.rationale}</span></div>
      {/if}
    {/if}
    {#if info.model}
      <div class="row"><span class="k">model</span><span class="v">{info.model}</span></div>
    {/if}
    {#if info.tokensIn != null || info.tokensOut != null}
      <div class="row">
        <span class="k">tokens</span>
        <span class="v">in {info.tokensIn ?? "?"} · out {info.tokensOut ?? "?"}</span>
      </div>
    {/if}
    {#if info.latencyMs != null}
      <div class="row"><span class="k">latency</span><span class="v">{info.latencyMs}ms</span></div>
    {/if}
  </div>
{/if}

<style>
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.1rem 0.5rem;
    border-radius: 999px;
    border: 1px solid color-mix(in srgb, var(--pill-color) 45%, transparent);
    background: color-mix(in srgb, var(--pill-color) 12%, transparent);
    color: var(--pill-color);
    font-size: 0.72rem;
    font-weight: 500;
    line-height: 1.4;
    cursor: pointer;
    user-select: none;
  }
  .pill:hover {
    background: color-mix(in srgb, var(--pill-color) 20%, transparent);
  }
  .pill.failed {
    opacity: 0.7;
  }
  .dot {
    width: 0.4rem;
    height: 0.4rem;
    border-radius: 50%;
    background: var(--pill-color);
  }
  .conf {
    opacity: 0.7;
    font-variant-numeric: tabular-nums;
  }
  .popover {
    display: inline-block;
    margin-top: 0.25rem;
    padding: 0.5rem 0.6rem;
    background: var(--bg-elevated, #1b1b1b);
    border: 1px solid var(--border, #2a2a2a);
    border-radius: 6px;
    font-size: 0.75rem;
    max-width: 28rem;
  }
  .row {
    display: grid;
    grid-template-columns: 5rem 1fr;
    gap: 0.5rem;
    padding: 0.1rem 0;
  }
  .row .k {
    opacity: 0.55;
  }
  .row .v {
    word-break: break-word;
  }
  .row .v.err {
    color: #eb5757;
  }
</style>
