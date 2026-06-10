<script lang="ts">
  /**
   * MON-99 Slice A — Memory Inspector v0.
   * Browse-only tree of the agent's memories, grouped by scope, indented
   * by parent_id depth, with a detail panel for the selected memory.
   *
   * Read-only in Slice A. No edit / archive / promote affordances —
   * those are P12. Selecting a memory shows full provenance (source
   * objective, file refs, supersedes chain) so the captain can verify the
   * Keeper's writes once Slice B (MON-100) lands.
   */
  import { invoke } from "$lib/api";
  import type { MemoryRow } from "$lib/bindings";
  import type { ToolProps } from "../types";

  let { agentContext }: ToolProps = $props();

  let memories = $state<MemoryRow[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let selectedId = $state<number | null>(null);

  const SCOPES = ["self", "project", "captain"] as const;

  let byScope = $derived.by(() => {
    const map: Record<string, MemoryRow[]> = {
      self: [],
      project: [],
      captain: [],
    };
    for (const m of memories) {
      const bucket = map[m.scope] ?? (map[m.scope] = []);
      bucket.push(m);
    }
    return map;
  });

  let childrenByParent = $derived.by(() => {
    const map = new Map<number | null, MemoryRow[]>();
    for (const m of memories) {
      const key = m.parentId ?? null;
      if (!map.has(key)) map.set(key, []);
      map.get(key)!.push(m);
    }
    for (const arr of map.values()) {
      arr.sort((a, b) => (a.title || "").localeCompare(b.title || ""));
    }
    return map;
  });

  let memoriesById = $derived(new Map(memories.map((m) => [m.id, m])));

  let selected = $derived(
    selectedId !== null ? memoriesById.get(selectedId) ?? null : null,
  );

  let supersedesChain = $derived.by(() => {
    if (!selected) return [];
    const chain: MemoryRow[] = [];
    let cursor: MemoryRow | undefined = selected;
    const seen = new Set<number>();
    while (cursor && cursor.supersedesId !== null && !seen.has(cursor.id)) {
      seen.add(cursor.id);
      const next = memoriesById.get(cursor.supersedesId);
      if (!next) break;
      chain.push(next);
      cursor = next;
    }
    return chain;
  });

  function rootsForScope(scope: string): MemoryRow[] {
    // A "root" within a scope = a memory in that scope whose parent is
    // either null or lives in a different scope (so we display it at the
    // top level of its own scope rather than orphaning it).
    return (byScope[scope] ?? []).filter((m) => {
      if (m.parentId === null) return true;
      const parent = memoriesById.get(m.parentId);
      return !parent || parent.scope !== scope;
    });
  }

  async function refresh() {
    if (!agentContext) {
      memories = [];
      return;
    }
    loading = true;
    error = null;
    try {
      memories = await invoke<MemoryRow[]>("db_list_memories_for_agent", {
        agentId: agentContext.agentId,
      });
      // Drop selection if the row went away.
      if (selectedId !== null && !memoriesById.get(selectedId)) {
        selectedId = null;
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    // Reload whenever the active agent changes.
    if (agentContext) {
      void refresh();
    } else {
      memories = [];
      selectedId = null;
    }
  });

  function fmtTime(s: string | null): string {
    if (!s) return "—";
    try {
      return new Date(s).toLocaleString();
    } catch {
      return s;
    }
  }
</script>

<div class="inspector">
  {#if !agentContext}
    <p class="empty">Select an agent to view its memories.</p>
  {:else}
    <header class="header">
      <span class="count">{memories.length} memor{memories.length === 1 ? "y" : "ies"}</span>
      <button class="btn-ghost" onclick={refresh} disabled={loading}>
        {loading ? "…" : "Refresh"}
      </button>
    </header>

    <div class="panes">
      <aside class="tree">
        {#if memories.length === 0 && !loading}
          <p class="empty">
            No memories yet. The Keeper will write some when a objective closes
            (MON-100). For now you can use <code>memory_smoke_insert</code>
            from the dev console to populate one.
          </p>
        {:else}
          {#each SCOPES as scope (scope)}
            {@const roots = rootsForScope(scope)}
            <section class="scope">
              <div class="scope-title">
                {scope}
                <span class="scope-count">{byScope[scope]?.length ?? 0}</span>
              </div>
              {#if roots.length === 0}
                <div class="scope-empty">empty</div>
              {:else}
                <ul class="tree-list">
                  {#each roots as root (root.id)}
                    {@render node(root, 0)}
                  {/each}
                </ul>
              {/if}
            </section>
          {/each}
        {/if}
      </aside>

      <section class="detail">
        {#if selected}
          <div class="detail-header">
            <h3 class="detail-title">{selected.title || "(untitled)"}</h3>
            <div class="badges">
              <span class="badge badge-scope">{selected.scope}</span>
              {#if selected.kind}
                <span class="badge badge-kind">{selected.kind}</span>
              {/if}
              <span class="badge badge-layer">{selected.layer}</span>
            </div>
          </div>

          {#if selected.summary}
            <div class="block">
              <div class="block-title">Summary</div>
              <p class="block-body">{selected.summary}</p>
            </div>
          {/if}

          {#if selected.content}
            <div class="block">
              <div class="block-title">Content</div>
              <pre class="block-body mono">{selected.content}</pre>
            </div>
          {/if}

          <div class="block">
            <div class="block-title">Provenance</div>
            <dl class="kv">
              <dt>Memory id</dt><dd class="mono">{selected.id}</dd>
              <dt>Created</dt><dd>{fmtTime(selected.createdAt)}</dd>
              <dt>Last accessed</dt><dd>{fmtTime(selected.lastAccessedAt)}</dd>
              <dt>Access count</dt><dd>{selected.accessCount}</dd>
              {#if selected.sourceObjectiveId}
                <dt>Source objective</dt><dd class="mono">{selected.sourceObjectiveId}</dd>
              {/if}
              {#if selected.sourceSessionId}
                <dt>Source session</dt><dd class="mono">{selected.sourceSessionId}</dd>
              {/if}
              {#if selected.sourceEvents}
                <dt>Source events</dt><dd class="mono small">{selected.sourceEvents}</dd>
              {/if}
              {#if selected.embeddingModelId}
                <dt>Embedding model</dt><dd class="mono">{selected.embeddingModelId}</dd>
              {/if}
              {#if selected.parentId !== null}
                <dt>Parent</dt><dd class="mono">#{selected.parentId}</dd>
              {/if}
              {#if selected.manualOverride}
                <dt>Manual override</dt><dd>yes</dd>
              {/if}
            </dl>
          </div>

          {#if selected.fileRefs}
            <div class="block">
              <div class="block-title">File refs</div>
              <pre class="block-body mono small">{selected.fileRefs}</pre>
            </div>
          {/if}

          {#if supersedesChain.length > 0}
            <div class="block">
              <div class="block-title">Supersedes chain</div>
              <ul class="chain">
                {#each supersedesChain as ancestor (ancestor.id)}
                  <li>
                    <button
                      class="chain-link"
                      onclick={() => (selectedId = ancestor.id)}
                    >
                      #{ancestor.id} — {ancestor.title || "(untitled)"}
                    </button>
                  </li>
                {/each}
              </ul>
            </div>
          {/if}
        {:else}
          <p class="empty">Select a memory from the tree to view its details.</p>
        {/if}
      </section>
    </div>
  {/if}

  {#if error}
    <pre class="error">{error}</pre>
  {/if}
</div>

{#snippet node(memory: MemoryRow, depth: number)}
  {@const children = childrenByParent.get(memory.id) ?? []}
  <li class="node">
    <button
      class="row"
      class:selected={memory.id === selectedId}
      style:--depth={depth}
      onclick={() => (selectedId = memory.id)}
    >
      <span class="row-title">{memory.title || `(memory #${memory.id})`}</span>
      {#if memory.kind}
        <span class="row-kind">{memory.kind}</span>
      {/if}
    </button>
    {#if children.length > 0}
      <ul class="tree-list">
        {#each children as child (child.id)}
          {@render node(child, depth + 1)}
        {/each}
      </ul>
    {/if}
  </li>
{/snippet}

<style>
  .inspector {
    display: flex;
    flex-direction: column;
    gap: 8px;
    height: 100%;
    min-height: 0;
  }
  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    flex-shrink: 0;
  }
  .count {
    font-size: 10px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .btn-ghost {
    padding: 3px 8px;
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    background: transparent;
    color: var(--text-secondary);
    font-family: inherit;
    font-size: 10px;
    cursor: pointer;
  }
  .btn-ghost:hover:not(:disabled) {
    background: var(--bg-panel-2);
    color: var(--text-primary);
  }
  .btn-ghost:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .panes {
    display: grid;
    grid-template-columns: minmax(180px, 1fr) minmax(220px, 1.5fr);
    gap: 8px;
    flex: 1;
    min-height: 0;
  }
  .tree {
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    padding: 6px;
    overflow-y: auto;
    background: var(--bg-panel-2);
  }
  .detail {
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    padding: 10px 12px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 10px;
    background: var(--bg-panel-2);
  }

  .scope {
    margin-bottom: 6px;
  }
  .scope-title {
    display: flex;
    justify-content: space-between;
    align-items: center;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-size: 10px;
    padding: 4px 6px;
  }
  .scope-count {
    color: var(--text-muted);
    font-size: 10px;
  }
  .scope-empty {
    padding: 2px 12px;
    font-size: 10px;
    color: var(--text-muted);
    font-style: italic;
  }

  .tree-list {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .node {
    display: flex;
    flex-direction: column;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    padding: 4px 6px;
    padding-left: calc(8px + var(--depth, 0) * 12px);
    border: none;
    background: transparent;
    color: var(--text-primary);
    font-family: inherit;
    font-size: 11px;
    text-align: left;
    cursor: pointer;
    border-radius: 4px;
  }
  .row:hover {
    background: var(--bg-panel);
  }
  .row.selected {
    background: var(--accent-bg-hover);
    color: var(--accent);
  }
  .row-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row-kind {
    color: var(--text-muted);
    font-size: 9px;
    text-transform: uppercase;
  }

  .detail-header {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .detail-title {
    margin: 0;
    font-size: 13px;
    color: var(--text-primary);
  }
  .badges {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .badge {
    font-size: 9px;
    padding: 2px 6px;
    border-radius: 4px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    background: var(--bg-panel);
    color: var(--text-secondary);
  }
  .badge-scope {
    background: var(--accent-bg-hover);
    color: var(--accent);
  }

  .block {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .block-title {
    font-size: 10px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .block-body {
    margin: 0;
    color: var(--text-primary);
    font-size: 11px;
    line-height: 1.45;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .block-body.mono {
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    font-size: 10px;
    background: var(--bg-panel);
    padding: 6px 8px;
    border-radius: 4px;
    max-height: 14rem;
    overflow: auto;
  }

  .kv {
    display: grid;
    grid-template-columns: max-content 1fr;
    column-gap: 10px;
    row-gap: 4px;
    margin: 0;
    font-size: 11px;
  }
  .kv dt {
    color: var(--text-muted);
  }
  .kv dd {
    margin: 0;
    color: var(--text-primary);
    word-break: break-word;
  }
  .mono {
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }
  .small {
    font-size: 10px;
  }

  .chain {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .chain-link {
    background: transparent;
    border: none;
    color: var(--accent);
    font-family: inherit;
    font-size: 11px;
    cursor: pointer;
    padding: 0;
    text-align: left;
  }
  .chain-link:hover {
    text-decoration: underline;
  }

  .empty {
    color: var(--text-muted);
    font-style: italic;
    font-size: 11px;
    margin: 0;
    padding: 6px;
  }
  .empty code {
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    background: var(--bg-panel);
    padding: 1px 4px;
    border-radius: 3px;
    font-style: normal;
  }

  .error {
    margin: 0;
    padding: 6px 8px;
    background: var(--error-bg-faint);
    color: var(--error-light);
    font-size: 10px;
    white-space: pre-wrap;
    border-radius: 4px;
  }
</style>
