<script lang="ts">
  import type { Agent, Project, ShadowIdentity } from "./types";
  import { agentStore } from "./stores/agentStore.svelte";
  import { formatCost } from "./format";

  interface ProjectGroup {
    project?: Project;
    agents: Agent[];
  }

  export interface TemplateSource {
    name: string;
    provider?: string;
    model?: string;
    thinkingLevel?: string;
    cwd?: string;
    shadow?: ShadowIdentity;
  }

  let {
    oncreate,
    ondismiss,
    ondelete,
    oneditproject,
    onsavetemplate,
    oneditAgent,
  }: {
    oncreate: () => void;
    ondismiss: (id: string) => void;
    ondelete: (id: string) => void;
    oneditproject?: (project: Project) => void;
    onsavetemplate?: (source: TemplateSource) => void;
    oneditAgent?: (agentId: string) => void;
  } = $props();

  let contextMenu: {
    x: number;
    y: number;
    agentId: string;
    archived: boolean;
    source: TemplateSource;
  } | null = $state(null);

  function openAgentMenu(e: MouseEvent, agent: Agent) {
    e.preventDefault();
    contextMenu = {
      x: e.clientX,
      y: e.clientY,
      agentId: agent.id,
      archived: !!agent.archivedAt,
      source: {
        name: agent.shadow?.shadowName || agent.name,
        provider: agent.provider,
        model: agent.model,
        thinkingLevel: agent.thinkingLevel,
        cwd: agent.cwd,
        shadow: agent.shadow,
      },
    };
  }

  function closeContextMenu() {
    contextMenu = null;
  }

  function handleSaveTemplate() {
    if (!contextMenu) return;
    onsavetemplate?.(contextMenu.source);
    closeContextMenu();
  }

  function handleSummon() {
    if (!contextMenu) return;
    agentStore.summonAgent(contextMenu.agentId);
    closeContextMenu();
  }

  function handleDeletePermanent() {
    if (!contextMenu) return;
    ondelete(contextMenu.agentId);
    closeContextMenu();
  }

  function handleEditAgent() {
    if (!contextMenu) return;
    oneditAgent?.(contextMenu.agentId);
    closeContextMenu();
  }

  function shortenPath(path: string): string {
    return path.replace(/^\/home\/[^/]+/, "~");
  }

  let projectGroups = $derived.by<ProjectGroup[]>(() => {
    const projectMap = new Map<string, Project>();
    for (const p of agentStore.projects) projectMap.set(p.id, p);

    const groups = new Map<string, ProjectGroup>();
    const ungrouped: ProjectGroup = { project: undefined, agents: [] };

    function ensureGroup(projectId: string): ProjectGroup {
      if (!groups.has(projectId)) {
        groups.set(projectId, { project: projectMap.get(projectId), agents: [] });
      }
      return groups.get(projectId)!;
    }

    for (const agent of agentStore.agents) {
      if (agent.projectId && projectMap.has(agent.projectId)) {
        ensureGroup(agent.projectId).agents.push(agent);
      } else {
        ungrouped.agents.push(agent);
      }
    }

    const result: ProjectGroup[] = [...groups.values()];
    if (ungrouped.agents.length > 0) {
      result.push(ungrouped);
    }
    return result;
  });
</script>

<div class="roster" class:collapsed={agentStore.sidebarCollapsed}>
  <div class="roster-head">
    <h1 class="brand">Monarch</h1>
    <div class="view-toggle" role="group" aria-label="Shadow roster view">
      <button
        class="view-toggle-btn"
        class:selected={!agentStore.sidebarShowAll}
        onclick={() => agentStore.setSidebarShowAll(false)}
        aria-pressed={!agentStore.sidebarShowAll}
        title="Show only active shadows"
      >
        Active
      </button>
      <button
        class="view-toggle-btn"
        class:selected={agentStore.sidebarShowAll}
        onclick={() => agentStore.setSidebarShowAll(true)}
        aria-pressed={agentStore.sidebarShowAll}
        title="Include archived shadows"
      >
        All
      </button>
    </div>
    <button class="btn-new" onclick={oncreate} title="Extract Shadow (Ctrl+N)">+</button>
  </div>

  {#if agentStore.agents.length === 0}
    <div class="empty">No shadows extracted — click + to extract one.</div>
  {:else}
    <div class="groups">
      {#each projectGroups as group}
        <div class="group">
          <div class="group-label">
            {#if group.project}
              <button
                class="group-project-btn"
                onclick={() => oneditproject?.(group.project!)}
                title="Edit project instructions"
              >
                <span class="group-slash">/</span>{group.project.name}
              </button>
            {:else}
              <span class="group-ungrouped">Shadows</span>
            {/if}
          </div>
          <div class="pills">
            {#each group.agents as agent (agent.id)}
              {@const isArchived = !!agent.archivedAt}
              {@const subtitle = agent.shadow?.shadowTitle || agent.shadow?.shadowGrade || agent.model || ""}
              {@const cwdLabel = agent.cwd ? shortenPath(agent.cwd) : ""}
              <div
                class="pill"
                class:active={agent.id === agentStore.activeTabId}
                class:standby={agent.status === "stopped"}
                class:archived={isArchived}
                onclick={() => agentStore.selectAgent(agent.id)}
                onkeydown={(e: KeyboardEvent) => { if (e.key === 'Enter') agentStore.selectAgent(agent.id); }}
                oncontextmenu={(e: MouseEvent) => openAgentMenu(e, agent)}
                role="button"
                tabindex="0"
              >
                <div class="pill-main">
                  <span class="pill-name" title={agent.name}>{agent.name}</span>
                  {#if subtitle}
                    <span class="pill-subtitle" title={subtitle}>{subtitle}</span>
                  {/if}
                </div>
                {#if cwdLabel || formatCost(agent.lifetimeCost)}
                  <div class="pill-meta">
                    {#if cwdLabel}
                      <span class="pill-cwd" title={agent.cwd}>{cwdLabel}</span>
                    {/if}
                    {#if formatCost(agent.lifetimeCost)}
                      <span class="pill-cost">{formatCost(agent.lifetimeCost)}</span>
                    {/if}
                  </div>
                {/if}
                {#if isArchived}
                  <button
                    class="pill-btn pill-summon"
                    onclick={(e: MouseEvent) => { e.stopPropagation(); agentStore.summonAgent(agent.id); }}
                    title="Summon back"
                    aria-label="Summon {agent.name} back"
                  >
                    &#x21BA;
                  </button>
                {:else}
                  <button
                    class="pill-btn pill-dismiss"
                    onclick={(e: MouseEvent) => { e.stopPropagation(); ondismiss(agent.id); }}
                    title="Dismiss"
                    aria-label="Dismiss {agent.name}"
                  >
                    &times;
                  </button>
                {/if}
              </div>
            {/each}
          </div>
        </div>
      {/each}
    </div>
  {/if}

  {#if contextMenu}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      class="context-menu-backdrop"
      role="presentation"
      onclick={closeContextMenu}
      oncontextmenu={(e: MouseEvent) => { e.preventDefault(); closeContextMenu(); }}
    ></div>
    <div
      class="context-menu"
      style:left="{contextMenu.x}px"
      style:top="{contextMenu.y}px"
      role="menu"
    >
      {#if contextMenu.archived}
        <button class="context-menu-item" onclick={handleSummon} role="menuitem">
          Summon back
        </button>
      {/if}
      <button class="context-menu-item" onclick={handleEditAgent} role="menuitem">
        Edit agent
      </button>
      <button class="context-menu-item" onclick={handleSaveTemplate} role="menuitem">
        Save as template
      </button>
      <div class="context-menu-divider" role="separator"></div>
      <button class="context-menu-item danger" onclick={handleDeletePermanent} role="menuitem">
        Delete permanently
      </button>
    </div>
  {/if}
</div>

<style>
  .roster {
    display: flex;
    align-items: stretch;
    gap: 12px;
    padding: 8px 12px;
    background: var(--bg-sidebar);
    border-bottom: 1px solid var(--border-subtle);
    overflow: hidden;
    flex-shrink: 0;
    max-height: 160px;
    user-select: none;
  }

  .roster-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-right: 12px;
    border-right: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }

  .brand {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    letter-spacing: 0.5px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .view-toggle {
    display: inline-flex;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    overflow: hidden;
    background: var(--bg-panel-2);
  }

  .view-toggle-btn {
    border: none;
    background: transparent;
    color: var(--text-muted);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    font-size: 10px;
    letter-spacing: 0.4px;
    text-transform: uppercase;
    padding: 3px 8px;
    cursor: pointer;
    transition: background 0.12s, color 0.12s;
  }
  .view-toggle-btn:hover {
    color: var(--text-secondary);
  }
  .view-toggle-btn.selected {
    background: var(--bg-panel-3);
    color: var(--accent);
  }

  .btn-new {
    width: 28px;
    height: 28px;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    background: var(--bg-panel-2);
    color: var(--accent);
    font-size: 18px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background 0.15s, color 0.15s;
  }
  .btn-new:hover {
    background: var(--bg-panel-3);
    color: var(--accent-light);
  }

  .empty {
    display: flex;
    align-items: center;
    padding: 0 16px;
    color: var(--text-muted);
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .groups {
    display: flex;
    align-items: stretch;
    gap: 16px;
    flex: 1;
    min-width: 0;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: thin;
  }

  .group {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  .group-label {
    font-size: 9px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.6px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    padding: 0 2px;
    flex-shrink: 0;
  }

  .group-project-btn {
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    color: inherit;
    cursor: pointer;
    transition: color 0.15s;
  }
  .group-project-btn:hover {
    color: var(--accent);
  }
  .group-slash {
    color: var(--accent);
    font-weight: 700;
    margin-right: 2px;
  }

  .group-ungrouped {
    font: inherit;
  }

  .pills {
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: minmax(160px, 200px);
    grid-template-rows: repeat(2, auto);
    gap: 6px;
    min-width: 0;
    align-content: start;
  }

  .pill {
    display: grid;
    grid-template-columns: 1fr auto;
    grid-template-areas:
      "main btn"
      "meta btn";
    align-items: center;
    column-gap: 6px;
    row-gap: 1px;
    padding: 5px 8px;
    border-radius: 6px;
    border: 1px solid transparent;
    background: var(--bg-panel-2);
    color: var(--text-secondary);
    cursor: pointer;
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    transition: background 0.12s, border-color 0.12s, color 0.12s;
    min-width: 0;
  }
  .pill:hover {
    background: var(--bg-panel-3);
    border-color: var(--border-subtle);
  }
  .pill.active {
    background: var(--bg-panel-3);
    border-color: var(--accent);
    color: var(--text-primary);
  }
  .pill.standby {
    opacity: 0.65;
  }
  .pill.archived {
    opacity: 0.45;
    font-style: italic;
  }
  .pill.archived .pill-name {
    color: var(--text-muted);
  }
  .pill.archived:hover {
    opacity: 0.85;
  }

  .pill-main {
    grid-area: main;
    display: flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
  }

  .pill-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pill-subtitle {
    font-size: 9px;
    color: var(--accent);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 50%;
  }

  .pill-meta {
    grid-area: meta;
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 6px;
    min-width: 0;
  }

  .pill-cwd {
    font-size: 9px;
    color: var(--text-muted);
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }

  .pill-cost {
    font-size: 9px;
    color: var(--text-muted);
    flex-shrink: 0;
    white-space: nowrap;
  }

  .pill-btn {
    grid-area: btn;
    align-self: stretch;
    border: none;
    background: none;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 14px;
    padding: 0 4px;
    line-height: 1;
    border-radius: 4px;
    transition: color 0.15s, background 0.12s;
  }
  .pill-dismiss:hover {
    color: var(--error);
    background: var(--bg-panel-2);
  }
  .pill-summon:hover {
    color: var(--accent);
    background: var(--bg-panel-2);
  }

  .context-menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 500;
  }

  .context-menu {
    position: fixed;
    z-index: 501;
    min-width: 160px;
    padding: 4px;
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    box-shadow: 0 12px 32px var(--shadow-dark);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .context-menu-item {
    padding: 6px 10px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    text-align: left;
    cursor: pointer;
    transition: background 0.12s, color 0.12s;
  }

  .context-menu-item:hover {
    background: var(--bg-panel-2);
    color: var(--accent);
  }

  .context-menu-item.danger {
    color: var(--error, #eb5757);
  }
  .context-menu-item.danger:hover {
    background: var(--error, #eb5757);
    color: var(--bg-panel);
  }

  .context-menu-divider {
    height: 1px;
    background: var(--border-subtle);
    margin: 2px 4px;
  }

  .roster.collapsed {
    max-height: 44px;
  }
  .roster.collapsed .groups {
    display: none;
  }
</style>
