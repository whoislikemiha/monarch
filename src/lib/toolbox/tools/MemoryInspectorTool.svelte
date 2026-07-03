<script lang="ts">
  /**
   * Memory inspector: search box over the agent's distilled long-term memory
   * (FTS + vector via `memory_search_for_agent`) above a scope-grouped tree
   * (self / project / supervisor). Selecting a memory expands its detail —
   * summary, content, provenance, supersedes chain — inline under the row.
   * Read-only: memories are the Keeper's artifact; editing is a later slice.
   */
  import { invoke } from "$lib/api";
  import type { MemoryRow, MemorySearchResult } from "$lib/bindings";
  import type { ToolProps } from "../types";

  let { agentContext }: ToolProps = $props();

  let memories = $state<MemoryRow[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let selectedId = $state<number | null>(null);

  // -- search --
  let query = $state("");
  let results = $state<MemorySearchResult[] | null>(null);
  let searching = $state(false);
  let searchSeq = 0;
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  const SCOPES = ["self", "project", "captain"] as const;
  const SCOPE_LABELS: Record<string, string> = {
    self: "Self",
    project: "Project",
    captain: "Supervisor",
  };
  const scopeLabel = (scope: string): string => SCOPE_LABELS[scope] ?? scope;

  let byScope = $derived.by(() => {
    const map: Record<string, MemoryRow[]> = { self: [], project: [], captain: [] };
    for (const m of memories) (map[m.scope] ?? (map[m.scope] = [])).push(m);
    return map;
  });

  let childrenByParent = $derived.by(() => {
    const map = new Map<number | null, MemoryRow[]>();
    for (const m of memories) {
      const key = m.parentId ?? null;
      if (!map.has(key)) map.set(key, []);
      map.get(key)!.push(m);
    }
    for (const arr of map.values()) arr.sort((a, b) => (a.title || "").localeCompare(b.title || ""));
    return map;
  });

  let memoriesById = $derived(new Map(memories.map((m) => [m.id, m])));
  let selected = $derived(selectedId !== null ? (memoriesById.get(selectedId) ?? null) : null);

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
    // A root within a scope = parent is null or lives in a different scope.
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
      if (selectedId !== null && !memories.some((m) => m.id === selectedId)) selectedId = null;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function runSearch(q: string) {
    if (!agentContext || !q.trim()) {
      results = null;
      return;
    }
    const seq = ++searchSeq;
    searching = true;
    try {
      const r = await invoke<MemorySearchResult[]>("memory_search_for_agent", {
        agentId: agentContext.agentId,
        query: q.trim(),
        topK: 12,
      });
      if (seq === searchSeq) results = r;
    } catch (e) {
      if (seq === searchSeq) {
        error = String(e);
        results = [];
      }
    } finally {
      if (seq === searchSeq) searching = false;
    }
  }

  function onQueryInput() {
    clearTimeout(debounceTimer);
    if (!query.trim()) {
      results = null;
      return;
    }
    debounceTimer = setTimeout(() => void runSearch(query), 250);
  }

  function clearSearch() {
    query = "";
    results = null;
  }

  $effect(() => {
    if (agentContext?.agentId) {
      clearSearch();
      selectedId = null;
      void refresh();
    } else {
      memories = [];
      selectedId = null;
      results = null;
    }
  });

  function fmtTime(s: string | null): string {
    if (!s) return "—";
    try {
      const d = new Date(s);
      return (
        d.toLocaleDateString([], { month: "short", day: "numeric" }) +
        " " +
        d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
      );
    } catch {
      return s;
    }
  }

  const shortId = (s: string): string => (s.length > 18 ? s.slice(0, 18) + "…" : s);
</script>

<div class="mem">
  {#if !agentContext}
    <div class="blank">Select an agent to browse its memory.</div>
  {:else}
    <div class="bar">
      <div class="searchwrap">
        <svg class="mag" viewBox="0 0 16 16" width="11" height="11" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="7" cy="7" r="4.5"/><path d="M10.5 10.5L14 14"/></svg>
        <input
          class="search"
          placeholder="Search memory…"
          bind:value={query}
          oninput={onQueryInput}
          onkeydown={(e) => { if (e.key === "Escape") clearSearch(); }}
        />
        {#if query}
          <button class="clear" aria-label="Clear search" onclick={clearSearch}>✕</button>
        {/if}
      </div>
      <span class="count mono">{memories.length}</span>
    </div>

    {#if error}<div class="err">{error}</div>{/if}

    {#if results !== null}
      <!-- search results -->
      <div class="list">
        {#if searching && results.length === 0}
          <div class="blank">Searching…</div>
        {:else if results.length === 0}
          <div class="blank">No matches for “{query}”.</div>
        {:else}
          {#each results as r (r.memory.id)}
            <button
              class="row hit"
              class:sel={selectedId === r.memory.id}
              onclick={() => (selectedId = selectedId === r.memory.id ? null : r.memory.id)}
            >
              <span class="row-main">
                <span class="row-title">{r.memory.title || `memory #${r.memory.id}`}</span>
                {#if r.memory.summary}<span class="row-sub">{r.memory.summary}</span>{/if}
              </span>
              <span class="tag mono">{r.source}</span>
            </button>
            {#if selectedId === r.memory.id && selected}
              {@render detail(selected)}
            {/if}
          {/each}
        {/if}
      </div>
    {:else}
      <!-- scope-grouped tree -->
      <div class="list">
        {#if memories.length === 0 && !loading}
          <div class="empty">
            <div class="glyph"></div>
            <h4>Nothing distilled yet</h4>
            <p>The Keeper writes long-term memories when objectives close and sessions end.</p>
          </div>
        {:else}
          {#each SCOPES as scope (scope)}
            {@const roots = rootsForScope(scope)}
            {#if roots.length > 0}
              <div class="scope-head">
                <span class="st">{scopeLabel(scope)}</span>
                <span class="rule"></span>
                <span class="sc mono">{byScope[scope]?.length ?? 0}</span>
              </div>
              {#each roots as root (root.id)}
                {@render node(root, 0)}
              {/each}
            {/if}
          {/each}
        {/if}
      </div>
    {/if}
  {/if}
</div>

{#snippet node(memory: MemoryRow, depth: number)}
  {@const children = childrenByParent.get(memory.id) ?? []}
  <button
    class="row"
    class:sel={memory.id === selectedId}
    style:--depth={depth}
    onclick={() => (selectedId = selectedId === memory.id ? null : memory.id)}
  >
    <span class="row-main">
      <span class="row-title">{memory.title || `memory #${memory.id}`}</span>
    </span>
    {#if memory.kind}<span class="tag mono">{memory.kind}</span>{/if}
  </button>
  {#if selectedId === memory.id && selected}
    {@render detail(selected)}
  {/if}
  {#each children as child (child.id)}
    {@render node(child, depth + 1)}
  {/each}
{/snippet}

{#snippet detail(m: MemoryRow)}
  <div class="card">
    <div class="badges">
      <span class="chip chip-scope">{scopeLabel(m.scope)}</span>
      {#if m.kind}<span class="chip">{m.kind}</span>{/if}
      {#if m.manualOverride}<span class="chip">manual</span>{/if}
    </div>

    {#if m.summary}<p class="sum">{m.summary}</p>{/if}
    {#if m.content}<pre class="content mono">{m.content}</pre>{/if}

    <div class="prov">
      <div class="pr"><span class="pk">Created</span><span class="pv mono">{fmtTime(m.createdAt)}</span></div>
      <div class="pr"><span class="pk">Accessed</span><span class="pv mono">{m.accessCount}× · {fmtTime(m.lastAccessedAt)}</span></div>
      {#if m.sourceObjectiveId}
        <div class="pr"><span class="pk">Objective</span><span class="pv mono" title={m.sourceObjectiveId}>{shortId(m.sourceObjectiveId)}</span></div>
      {/if}
      {#if m.sourceSessionId}
        <div class="pr"><span class="pk">Session</span><span class="pv mono" title={m.sourceSessionId}>{shortId(m.sourceSessionId)}</span></div>
      {/if}
      {#if m.embeddingModelId}
        <div class="pr"><span class="pk">Embedding</span><span class="pv mono">{m.embeddingModelId}</span></div>
      {/if}
    </div>

    {#if supersedesChain.length > 0}
      <div class="chain">
        <span class="pk">Supersedes</span>
        {#each supersedesChain as ancestor (ancestor.id)}
          <button class="chain-link" onclick={() => (selectedId = ancestor.id)}>
            #{ancestor.id} {ancestor.title || "(untitled)"}
          </button>
        {/each}
      </div>
    {/if}
  </div>
{/snippet}

<style>
  .mem { display: flex; flex-direction: column; min-height: 0; height: 100%; }

  .bar {
    display: flex; align-items: center; gap: var(--s2);
    padding: var(--s2) var(--s3); flex: none;
    border-bottom: 1px solid var(--border-subtle);
  }
  .searchwrap { position: relative; flex: 1; display: flex; align-items: center; }
  .mag { position: absolute; left: 7px; color: var(--text-muted); pointer-events: none; }
  .search {
    font: inherit; font-size: 11.5px; color: var(--text-primary);
    background: var(--bg-raised); border: 1px solid var(--border);
    border-radius: var(--r-md); padding: 4px var(--s5) 4px 22px; width: 100%;
  }
  .search::placeholder { color: var(--text-muted); }
  .search:focus { outline: 2px solid var(--focus); outline-offset: 1px; border-color: var(--accent); }
  .clear {
    position: absolute; right: 4px; width: 16px; height: 16px;
    display: inline-flex; align-items: center; justify-content: center;
    background: none; border: none; color: var(--text-muted);
    font-size: 9px; cursor: pointer; border-radius: var(--r-sm);
  }
  .clear:hover { color: var(--text-primary); background: var(--bg-overlay); }
  .count { font-size: 10px; color: var(--text-muted); flex: none; }

  .err { padding: var(--s2) var(--s3); font-size: 11px; color: var(--status-error); }
  .blank { padding: var(--s4); text-align: center; font-size: 11px; color: var(--text-muted); }

  .list { overflow-y: auto; min-height: 0; flex: 1; padding: var(--s1); display: flex; flex-direction: column; gap: 1px; }

  /* scope heads */
  .scope-head { display: flex; align-items: center; gap: var(--s2); padding: var(--s2) var(--s2) var(--s1); }
  .st { font-size: 9.5px; font-weight: 600; letter-spacing: 0.14em; text-transform: uppercase; color: var(--text-muted); }
  .rule { flex: 1; height: 1px; background: var(--border-subtle); }
  .sc { font-size: 9.5px; color: var(--text-muted); }

  /* rows */
  .row {
    display: flex; align-items: center; gap: var(--s2);
    width: 100%; text-align: left; font: inherit; cursor: pointer;
    background: none; border: 1px solid transparent; border-radius: var(--r-sm);
    padding: 4px var(--s2);
    padding-left: calc(var(--s2) + var(--depth, 0) * 14px);
    color: var(--text-secondary);
  }
  .row:hover { background: var(--bg-raised); }
  .row.sel { background: var(--bg-raised); border-color: var(--border); }
  .row-main { min-width: 0; flex: 1; display: flex; flex-direction: column; gap: 1px; }
  .row-title { font-size: 12px; color: var(--text-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .row-sub { font-size: 10.5px; color: var(--text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .tag {
    flex: none; font-size: 9px; letter-spacing: 0.04em; text-transform: uppercase;
    color: var(--text-muted); border: 1px solid var(--border-subtle);
    border-radius: var(--r-sm); padding: 1px 5px;
  }
  .row.hit .tag { color: var(--accent-2); border-color: color-mix(in srgb, var(--accent-2) 30%, transparent); }

  /* inline detail card */
  .card {
    margin: 2px var(--s1) var(--s2);
    border: 1px solid var(--border-subtle); border-radius: var(--r-sm);
    background: var(--bg-sink); padding: var(--s3);
    display: flex; flex-direction: column; gap: var(--s2);
  }
  .badges { display: flex; gap: var(--s1); flex-wrap: wrap; }
  .chip {
    display: inline-flex; align-items: center; font-size: 9.5px; font-weight: 500;
    text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-secondary);
    padding: 1px 6px; border-radius: var(--r-sm);
    border: 1px solid var(--border); background: var(--bg-raised);
  }
  .chip-scope { color: var(--accent-2); border-color: color-mix(in srgb, var(--accent-2) 30%, transparent); }

  .sum { margin: 0; font-size: 11.5px; line-height: 1.5; color: var(--text-primary); }
  .content {
    margin: 0; font-size: 10.5px; line-height: 1.6; color: var(--text-secondary);
    background: var(--bg-base); border: 1px solid var(--border-subtle);
    border-radius: var(--r-sm); padding: var(--s2) var(--s3);
    white-space: pre-wrap; word-break: break-word; max-height: 200px; overflow: auto;
  }

  .prov { display: flex; flex-direction: column; }
  .pr { display: flex; align-items: baseline; gap: var(--s3); padding: 2px 0; }
  .pk { font-size: 10px; color: var(--text-muted); width: 62px; flex: none; }
  .pv { font-size: 10px; color: var(--text-secondary); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .chain { display: flex; flex-direction: column; gap: 2px; }
  .chain-link {
    background: none; border: none; padding: 0; text-align: left; cursor: pointer;
    font: inherit; font-size: 10.5px; color: var(--accent);
  }
  .chain-link:hover { text-decoration: underline; }

  .mono { font-family: "JetBrains Mono", monospace; }

  /* empty state (atom pattern) */
  .empty { display: flex; flex-direction: column; align-items: center; text-align: center; gap: var(--s2); padding: var(--s6) var(--s4); }
  .empty .glyph { width: 34px; height: 34px; border: 1.5px solid var(--border-strong); transform: rotate(45deg); border-radius: var(--r-sm); margin-bottom: var(--s2); }
  .empty h4 { margin: 0; font-size: 13px; color: var(--text-primary); }
  .empty p { margin: 0; font-size: 11px; color: var(--text-muted); max-width: 30ch; }
</style>
