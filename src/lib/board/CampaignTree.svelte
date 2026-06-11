<script lang="ts">
  /**
   * S2 — the campaign tree (the living backlog). Renders an agent's campaign
   * root + nested objectives, with status shape+label and grade chips. Planned
   * objectives are first-class, not greyed afterthoughts. Row → select (detail).
   */
  import type { ObjectiveRow } from "$lib/bindings";
  import { objectiveStore } from "$lib/toolbox/objectiveStore.svelte";
  import { objectiveStatus } from "$lib/ui/status";
  import { SvelteSet } from "svelte/reactivity";

  interface Props {
    agentId: string;
    selectedId: string | null;
    onselect: (objective: ObjectiveRow) => void;
  }
  let { agentId, selectedId, onselect }: Props = $props();

  let entry = $derived(objectiveStore.byAgent.get(agentId));
  let roots = $derived(entry?.roots ?? []);

  // Build a parentId → children index across all loaded trees.
  let childrenOf = $derived.by(() => {
    const map = new Map<string, ObjectiveRow[]>();
    if (!entry) return map;
    for (const tree of entry.treesByRoot.values()) {
      for (const node of tree) {
        const key = node.parentId ?? "__root__";
        const arr = map.get(key) ?? [];
        arr.push(node);
        map.set(key, arr);
      }
    }
    return map;
  });

  function kids(id: string): ObjectiveRow[] {
    return childrenOf.get(id) ?? [];
  }

  let collapsed = new SvelteSet<string>();
  function toggle(id: string) {
    if (collapsed.has(id)) collapsed.delete(id);
    else collapsed.add(id);
  }

  function progress(root: ObjectiveRow): { done: number; total: number } {
    const tree = entry?.treesByRoot.get(root.rootId) ?? [];
    const objectives = tree.filter((n) => n.kind !== "campaign");
    const done = objectives.filter((n) => n.status === "completed" || n.status === "done").length;
    return { done, total: objectives.length };
  }
</script>

<div class="tree">
  {#if roots.length === 0}
    <div class="empty mono">No campaign yet for this shadow.</div>
  {/if}
  {#each roots as root (root.id)}
    {@const p = progress(root)}
    <div class="campaign">
      <div class="campaign-head">
        <span class="ttl">{root.title}</span>
        <span class="prog mono">{p.done}/{p.total}</span>
      </div>
      <div class="rows">
        {#each kids(root.id) as node (node.id)}
          {@render objectiveRow(node, 0)}
        {/each}
        {#if kids(root.id).length === 0}
          <div class="empty mono">No objectives yet.</div>
        {/if}
      </div>
    </div>
  {/each}
</div>

{#snippet objectiveRow(node: ObjectiveRow, depth: number)}
  {@const s = objectiveStatus(node.status)}
  {@const children = kids(node.id)}
  {@const isOpen = !collapsed.has(node.id)}
  <div class="node">
    <div
      class="row"
      class:selected={node.id === selectedId}
      role="button"
      tabindex="0"
      style="padding-left:{12 + depth * 16}px"
      onclick={() => onselect(node)}
      onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onselect(node); } }}
    >
      {#if children.length}
        <button class="chev" class:open={isOpen} aria-label="Toggle" onclick={(e) => { e.stopPropagation(); toggle(node.id); }}>›</button>
      {:else}
        <span class="chev-spacer"></span>
      {/if}
      <span class="sdot {s.dot}" title={s.label}></span>
      {#if node.grade}<span class="gchip mono">{node.grade}</span>{/if}
      <span class="title">{node.title}</span>
      <span class="status mono tone-{s.tone}">{s.label}</span>
    </div>
    {#if children.length && isOpen}
      {#each children as child (child.id)}
        {@render objectiveRow(child, depth + 1)}
      {/each}
    {/if}
  </div>
{/snippet}

<style>
  .tree { display: flex; flex-direction: column; gap: var(--s4); }
  .empty { font-size: 11px; color: var(--text-muted); padding: var(--s3); }

  .campaign { border: 1px solid var(--border-subtle); border-radius: var(--r-md); overflow: hidden; background: var(--bg-panel); }
  .campaign-head {
    display: flex; align-items: center; gap: var(--s2);
    padding: var(--s2) var(--s3); border-bottom: 1px solid var(--border-subtle); background: var(--bg-sink);
  }
  .campaign-head .ttl { font-size: 12px; font-weight: 600; color: var(--text-primary); }
  .campaign-head .prog { margin-left: auto; font-size: 10px; color: var(--text-muted); }
  .rows { display: flex; flex-direction: column; }

  .row {
    display: flex; align-items: center; gap: var(--s2);
    padding: 5px var(--s3); cursor: pointer;
    border-bottom: 1px solid var(--border-subtle);
  }
  .row:last-child { border-bottom: none; }
  .row:hover { background: var(--bg-raised); }
  .row.selected { background: var(--bg-overlay); }
  .row:focus-visible { outline: 2px solid var(--focus); outline-offset: -2px; }
  .chev { background: none; border: none; color: var(--text-muted); cursor: pointer; font-size: 13px; line-height: 1; padding: 0; width: 12px; flex: none; transition: transform 0.15s; }
  .chev.open { transform: rotate(90deg); }
  .chev-spacer { width: 12px; flex: none; }
  .sdot { flex: none; }
  .gchip {
    flex: none; display: inline-flex; align-items: center; justify-content: center;
    min-width: 16px; height: 16px; padding: 0 3px; border-radius: var(--r-sm);
    font-size: 9.5px; font-weight: 700; color: var(--accent-2);
    border: 1px solid color-mix(in srgb, var(--accent-2) 35%, transparent);
  }
  .title { font-size: 12px; color: var(--text-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; flex: 1; }
  .status { font-size: 9.5px; flex: none; }
  .tone-muted { color: var(--text-muted); }
  .tone-info { color: var(--status-info); }
  .tone-success { color: var(--status-success); }
  .tone-warning { color: var(--status-warning); }
  .tone-error { color: var(--status-error); }
</style>
