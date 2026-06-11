<script lang="ts">
  /**
   * S3 — objective detail. Brief (scope / direction / rationale), the plan (the
   * intended route, visually distinct from execution), notes & refs, and the
   * first-person report once the objective is closed.
   */
  import type { ObjectiveRow } from "$lib/bindings";
  import { objectiveStore } from "$lib/toolbox/objectiveStore.svelte";
  import { objectiveStatus, planItemStatus } from "$lib/ui/status";

  interface Props {
    agentId: string;
    objective: ObjectiveRow;
  }
  let { agentId, objective }: Props = $props();

  let entry = $derived(objectiveStore.byAgent.get(agentId));
  let plan = $derived(entry?.planItemsByObjective.get(objective.id) ?? []);
  let refs = $derived(entry?.refsByObjective.get(objective.id) ?? []);
  let report = $derived(entry?.reportsByObjective.get(objective.id) ?? null);
  let s = $derived(objectiveStatus(objective.status));

  // Lazy-load detail slices when the selected objective changes.
  $effect(() => {
    const id = objective.id;
    objectiveStore.loadPlanItems(agentId, id).catch(() => {});
    objectiveStore.loadRefs(agentId, id).catch(() => {});
    objectiveStore.loadReport(agentId, id).catch(() => {});
  });

  function fmtDate(iso: string | null): string {
    if (!iso) return "—";
    const t = Date.parse(iso);
    return Number.isNaN(t) ? "—" : new Date(t).toLocaleString();
  }
</script>

<div class="detail">
  <header class="brief">
    <div class="brief-top">
      <span class="sdot {s.dot}" title={s.label}></span>
      <h3 class="title">{objective.title}</h3>
      {#if objective.grade}<span class="gchip mono">{objective.grade}</span>{/if}
      <span class="status mono tone-{s.tone}">{s.label}</span>
    </div>
    <div class="meta">
      <span class="k">Created</span><span class="v mono">{fmtDate(objective.createdAt)}</span>
      {#if objective.startedAt}<span class="k">Started</span><span class="v mono">{fmtDate(objective.startedAt)}</span>{/if}
      {#if objective.completedAt}<span class="k">Completed</span><span class="v mono">{fmtDate(objective.completedAt)}</span>{/if}
    </div>
    {#if objective.scope}<p class="field"><span class="lbl">Scope</span>{objective.scope}</p>{/if}
    {#if objective.currentDirection}<p class="field"><span class="lbl">Direction</span>{objective.currentDirection}</p>{/if}
    {#if objective.rationale}<p class="field"><span class="lbl">Rationale</span>{objective.rationale}</p>{/if}
    {#if objective.description}<p class="field"><span class="lbl">Brief</span>{objective.description}</p>{/if}
  </header>

  <section class="block">
    <div class="block-head"><span class="t">Plan</span><span class="hint">intended route</span></div>
    {#if plan.length === 0}
      <div class="empty mono">No plan items.</div>
    {:else}
      <ol class="plan">
        {#each plan as item (item.id)}
          {@const ps = planItemStatus(item.status)}
          <li class="pitem">
            <span class="sdot {ps.dot}" title={ps.label}></span>
            <span class="pi-title">{item.title}</span>
            <span class="pi-status mono tone-{ps.tone}">{ps.label}</span>
          </li>
        {/each}
      </ol>
    {/if}
  </section>

  {#if refs.length}
    <section class="block">
      <div class="block-head"><span class="t">Notes &amp; refs</span></div>
      <ul class="refs">
        {#each refs as ref (ref.id)}
          <li class="ref">
            <span class="ref-type mono">{ref.refType}</span>
            <span class="ref-target mono">{ref.label ?? ref.target}</span>
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  {#if report}
    <section class="block report">
      <div class="block-head"><span class="t">Report</span>{#if report.grade}<span class="gchip mono">{report.grade}</span>{/if}</div>
      {#if report.raw}
        <div class="codeblock"><pre>{report.raw}</pre></div>
      {:else}
        {#if report.summary}<p class="field"><span class="lbl">Summary</span>{report.summary}</p>{/if}
        {#if report.outcome}<p class="field"><span class="lbl">Outcome</span>{report.outcome}</p>{/if}
        {#if report.learned.length}
          <p class="field"><span class="lbl">Learned</span></p>
          <ul class="bullets">{#each report.learned as l}<li>{l}</li>{/each}</ul>
        {/if}
        {#if report.reflection}<p class="field"><span class="lbl">Reflection</span>{report.reflection}</p>{/if}
      {/if}
    </section>
  {/if}
</div>

<style>
  .detail { display: flex; flex-direction: column; gap: var(--s4); padding: var(--s4); }

  .brief { display: flex; flex-direction: column; gap: var(--s2); padding-bottom: var(--s3); border-bottom: 1px solid var(--border-subtle); }
  .brief-top { display: flex; align-items: center; gap: var(--s2); }
  .title { font-size: 15px; font-weight: 600; color: var(--text-primary); margin: 0; flex: 1; min-width: 0; }
  .gchip {
    display: inline-flex; align-items: center; justify-content: center;
    min-width: 18px; height: 18px; padding: 0 4px; border-radius: var(--r-sm);
    font-size: 10px; font-weight: 700; color: var(--accent-2);
    border: 1px solid color-mix(in srgb, var(--accent-2) 35%, transparent);
  }
  .status { font-size: 10px; }
  .meta { display: grid; grid-template-columns: auto 1fr; gap: 2px var(--s3); align-items: baseline; }
  .meta .k { font-size: 10px; color: var(--text-muted); }
  .meta .v { font-size: 10px; color: var(--text-secondary); }

  .field { font-size: 12px; color: var(--text-secondary); line-height: 1.6; margin: 0; }
  .field .lbl { display: block; font-size: 9.5px; font-weight: 600; letter-spacing: 0.1em; text-transform: uppercase; color: var(--text-muted); margin-bottom: 2px; }

  .block { display: flex; flex-direction: column; gap: var(--s2); }
  .block-head { display: flex; align-items: baseline; gap: var(--s2); }
  .block-head .t { font-size: 10px; font-weight: 600; letter-spacing: 0.12em; text-transform: uppercase; color: var(--text-secondary); }
  .block-head .hint { font-size: 10px; color: var(--text-muted); }
  .empty { font-size: 11px; color: var(--text-muted); }

  .plan { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; border: 1px solid var(--border-subtle); border-radius: var(--r-md); overflow: hidden; }
  .pitem { display: flex; align-items: center; gap: var(--s2); padding: 5px var(--s3); border-bottom: 1px solid var(--border-subtle); }
  .pitem:last-child { border-bottom: none; }
  .pi-title { font-size: 12px; color: var(--text-primary); flex: 1; min-width: 0; }
  .pi-status { font-size: 9.5px; flex: none; }

  .refs { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  .ref { display: flex; gap: var(--s2); font-size: 11px; }
  .ref-type { color: var(--text-muted); flex: none; }
  .ref-target { color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; }

  .bullets { margin: 0; padding-left: var(--s4); font-size: 12px; color: var(--text-secondary); line-height: 1.6; }
  .codeblock { background: var(--bg-sink); border: 1px solid var(--border-subtle); border-radius: var(--r-sm); }
  .codeblock pre { margin: 0; padding: var(--s3); font-family: "JetBrains Mono", monospace; font-size: 11px; line-height: 1.6; color: var(--text-secondary); white-space: pre-wrap; word-break: break-word; }

  .tone-muted { color: var(--text-muted); }
  .tone-info { color: var(--status-info); }
  .tone-success { color: var(--status-success); }
  .tone-warning { color: var(--status-warning); }
  .tone-error { color: var(--status-error); }
</style>
