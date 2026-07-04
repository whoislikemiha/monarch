<script lang="ts">
  /**
   * Inline first-person report at the close of an objective (MON-124). The
   * timeline segment for a finished objective ends (visually: begins —
   * newest-first) with the agent's own account: summary, grade, and an
   * expandable breakdown of decisions / learnings / artifacts / open threads.
   * Read-only — the report is the agent's artifact.
   */
  import type { ObjectiveReportView } from "$lib/toolbox/objectiveStore.svelte";
  import EventIcon from "$lib/ui/EventIcon.svelte";

  interface Props {
    report: ObjectiveReportView;
  }
  let { report }: Props = $props();

  let expanded = $state(false);
  let hasDetail = $derived(
    report.decisions.length > 0 ||
      report.learned.length > 0 ||
      report.artifacts.length > 0 ||
      report.open_threads.length > 0 ||
      !!report.reflection,
  );
</script>

<div class="report">
  <div class="head">
    <EventIcon kind="report" size={12} tone="accent" />
    <span class="tag">REPORT</span>
    {#if report.outcome}<span class="outcome mono">{report.outcome}</span>{/if}
    {#if report.grade}
      <span
        class="grade mono"
        style="color: var(--grade-{report.grade.toLowerCase()}); border-color: var(--grade-{report.grade.toLowerCase()})"
      >{report.grade}</span>
    {/if}
  </div>

  {#if report.raw}
    <p class="summary mono">{report.raw}</p>
  {:else if report.summary}
    <p class="summary">{report.summary}</p>
  {/if}

  {#if hasDetail}
    <button class="toggle" onclick={() => (expanded = !expanded)} aria-expanded={expanded}>
      {expanded ? "less" : "decisions · learned · artifacts"}
    </button>
  {/if}

  {#if expanded}
    <div class="detail">
      {#if report.decisions.length}
        <div class="block">
          <span class="bt">decisions</span>
          <ul>
            {#each report.decisions as d, i (i)}
              <li>{d.decision}{d.rationale ? ` — ${d.rationale}` : ""}</li>
            {/each}
          </ul>
        </div>
      {/if}
      {#if report.learned.length}
        <div class="block">
          <span class="bt">learned</span>
          <ul>
            {#each report.learned as l, i (i)}<li>{l}</li>{/each}
          </ul>
        </div>
      {/if}
      {#if report.artifacts.length}
        <div class="block">
          <span class="bt">artifacts</span>
          <ul>
            {#each report.artifacts as a, i (i)}
              <li><span class="mono path" title={a.file}>{a.file}</span>{a.role ? ` — ${a.role}` : ""}</li>
            {/each}
          </ul>
        </div>
      {/if}
      {#if report.open_threads.length}
        <div class="block">
          <span class="bt">open threads</span>
          <ul>
            {#each report.open_threads as t, i (i)}<li>{t}</li>{/each}
          </ul>
        </div>
      {/if}
      {#if report.reflection}
        <p class="reflection">{report.reflection}</p>
      {/if}
    </div>
  {/if}
</div>

<style>
  .report {
    display: flex; flex-direction: column; gap: var(--s2);
    padding: var(--s3) var(--s4);
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-md);
    margin: var(--s1) 0;
  }
  .head { display: flex; align-items: center; gap: var(--s2); }
  .tag { font-size: 9px; font-weight: 700; letter-spacing: 0.14em; color: var(--text-muted); }
  .outcome { font-size: 10px; color: var(--text-secondary); }
  .grade {
    margin-left: auto; flex: none;
    font-size: 10px; font-weight: 600;
    border: 1px solid; border-radius: var(--r-sm);
    padding: 0 6px; line-height: 1.6;
  }
  .summary { margin: 0; font-size: 11.5px; color: var(--text-secondary); line-height: 1.55; }
  .summary.mono { font-size: 10.5px; }

  .toggle {
    align-self: flex-start;
    background: none; border: none; padding: 0;
    font-size: 10px; color: var(--accent); cursor: pointer;
  }
  .toggle:hover { text-decoration: underline; }
  .toggle:focus-visible { outline: 2px solid var(--focus); outline-offset: 2px; border-radius: var(--r-sm); }

  .detail { display: flex; flex-direction: column; gap: var(--s2); padding-top: var(--s1); border-top: 1px solid var(--border-subtle); }
  .block { display: flex; flex-direction: column; gap: 2px; }
  .bt { font-size: 9px; font-weight: 700; letter-spacing: 0.12em; text-transform: uppercase; color: var(--text-muted); }
  .block ul { margin: 0; padding-left: var(--s4); display: flex; flex-direction: column; gap: 1px; }
  .block li { font-size: 11px; color: var(--text-secondary); line-height: 1.5; }
  /* Only visible in the expanded detail — show full paths, wrapped. */
  .path {
    font-size: 10px;
    overflow-wrap: anywhere;
    display: inline-block; max-width: 100%; vertical-align: bottom;
  }
  .reflection { margin: 0; font-size: 11px; color: var(--text-muted); font-style: italic; line-height: 1.55; }
</style>
