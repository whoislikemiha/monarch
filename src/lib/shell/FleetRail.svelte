<script lang="ts">
  /**
   * Left rail — the fleet. Slice 1 ships the collapsible frame (header,
   * Active/All filter, Extract, collapse toggle) with a placeholder body;
   * slice 2 fills it with the project-grouped roster.
   */
  import type { Agent, Project } from "$lib/types";
  import { agentStore } from "$lib/stores/agentStore.svelte";
  import ShadowRow from "./ShadowRow.svelte";

  interface Props {
    onextract?: () => void;
  }
  let { onextract }: Props = $props();

  let collapsed = $derived(agentStore.sidebarCollapsed);

  interface Group {
    project?: Project;
    agents: Agent[];
  }

  let groups = $derived.by<Group[]>(() => {
    const projectMap = new Map<string, Project>();
    for (const p of agentStore.projects) projectMap.set(p.id, p);

    const byProject = new Map<string, Group>();
    const ungrouped: Group = { project: undefined, agents: [] };

    for (const agent of agentStore.agents) {
      if (agent.projectId && projectMap.has(agent.projectId)) {
        let g = byProject.get(agent.projectId);
        if (!g) {
          g = { project: projectMap.get(agent.projectId), agents: [] };
          byProject.set(agent.projectId, g);
        }
        g.agents.push(agent);
      } else {
        ungrouped.agents.push(agent);
      }
    }

    const result = [...byProject.values()];
    if (ungrouped.agents.length > 0) result.push(ungrouped);
    return result;
  });
</script>

{#if collapsed}
  <div class="rail collapsed">
    <button class="tab" title="Show fleet" onclick={() => agentStore.toggleSidebarCollapsed()}>
      <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5">
        <path d="M6 3l5 5-5 5" />
      </svg>
    </button>
    <button class="tab" title="Extract a shadow" onclick={() => onextract?.()}>
      <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5">
        <path d="M8 3v10M3 8h10" />
      </svg>
    </button>
  </div>
{:else}
  <aside class="rail">
    <div class="rail-head">
      <span class="title">Fleet</span>
      <span class="count mono">{agentStore.agents.filter((a) => !a.archivedAt).length}</span>
      <div class="grow"></div>
      <div class="filter" role="tablist" aria-label="Roster filter">
        <button
          class="filter-btn"
          class:active={!agentStore.sidebarShowAll}
          onclick={() => agentStore.setSidebarShowAll(false)}
        >Active</button>
        <button
          class="filter-btn"
          class:active={agentStore.sidebarShowAll}
          onclick={() => agentStore.setSidebarShowAll(true)}
        >All</button>
      </div>
      <button class="collapse" title="Hide fleet" onclick={() => agentStore.toggleSidebarCollapsed()}>
        <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.5">
          <path d="M10 3l-5 5 5 5" />
        </svg>
      </button>
    </div>

    <button class="extract" onclick={() => onextract?.()}>
      <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.5">
        <path d="M8 3v10M3 8h10" />
      </svg>
      Extract shadow
    </button>

    <div class="rail-body">
      {#if agentStore.agents.length === 0}
        <div class="rail-empty">No shadows yet. Extract one to begin.</div>
      {:else}
        {#each groups as group (group.project?.id ?? "ungrouped")}
          <div class="group">
            <div class="group-label">
              <span class="slash">/</span>{group.project?.name ?? "Shadows"}
              <span class="group-count mono">{group.agents.length}</span>
            </div>
            <div class="group-rows">
              {#each group.agents as agent (agent.id)}
                <ShadowRow {agent} />
              {/each}
            </div>
          </div>
        {/each}
      {/if}
    </div>
  </aside>
{/if}

<style>
  .rail {
    width: 248px;
    flex: none;
    display: flex;
    flex-direction: column;
    background: var(--bg-sink);
    border-right: 1px solid var(--border-subtle);
    min-height: 0;
    user-select: none;
  }
  .rail.collapsed {
    width: 44px;
    align-items: center;
    padding-top: var(--s2);
    gap: var(--s1);
  }
  .rail.collapsed .tab {
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-md);
    color: var(--text-secondary);
    cursor: pointer;
  }
  .rail.collapsed .tab:hover { background: var(--bg-raised); color: var(--text-primary); }

  .rail-head {
    display: flex;
    align-items: center;
    gap: var(--s2);
    height: 40px;
    flex: none;
    padding: 0 var(--s3);
    border-bottom: 1px solid var(--border-subtle);
  }
  .rail-head .title { font-size: 12px; font-weight: 600; letter-spacing: 0.04em; color: var(--text-primary); }
  .rail-head .count { font-size: 10px; color: var(--text-muted); }
  .rail-head .grow { flex: 1; }

  .filter { display: flex; padding: 2px; background: var(--bg-base); border: 1px solid var(--border-subtle); border-radius: var(--r-md); }
  .filter-btn {
    font: inherit; font-size: 10.5px; font-weight: 500; color: var(--text-muted);
    background: transparent; border: none; padding: 2px var(--s2);
    border-radius: var(--r-sm); cursor: pointer;
  }
  .filter-btn:hover { color: var(--text-secondary); }
  .filter-btn.active { background: var(--bg-overlay); color: var(--text-primary); }

  .collapse {
    width: 24px; height: 24px; display: flex; align-items: center; justify-content: center;
    background: transparent; border: none; color: var(--text-muted); cursor: pointer; border-radius: var(--r-sm);
  }
  .collapse:hover { background: var(--bg-raised); color: var(--text-primary); }

  .extract {
    display: flex; align-items: center; justify-content: center; gap: var(--s2);
    margin: var(--s3); flex: none;
    font: inherit; font-size: 12px; font-weight: 500; color: var(--text-secondary);
    background: var(--bg-base); border: 1px solid var(--border); border-radius: var(--r-md);
    padding: 7px var(--s3); cursor: pointer; transition: background 0.14s, border-color 0.14s, color 0.14s;
  }
  .extract:hover { background: var(--bg-raised); color: var(--text-primary); border-color: var(--border-strong); }

  .rail-body { flex: 1; min-height: 0; overflow-y: auto; padding: 0 var(--s2) var(--s3); }
  .rail-empty { font-size: 11px; color: var(--text-muted); padding: var(--s3); line-height: 1.5; }

  .group { display: flex; flex-direction: column; gap: 2px; margin-top: var(--s2); }
  .group-label {
    display: flex; align-items: center; gap: var(--s2);
    font-size: 9.5px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase;
    color: var(--text-muted); padding: var(--s2) var(--s2) var(--s1);
  }
  .group-label .slash { color: var(--accent); font-weight: 700; }
  .group-label .group-count { margin-left: auto; text-transform: none; letter-spacing: 0; }
  .group-rows { display: flex; flex-direction: column; gap: 1px; }
</style>
