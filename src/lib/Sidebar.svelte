<script lang="ts">
  import type { Agent, Project, ShadowIdentity } from "./types";
  import { ShadowAvatar } from "./avatar";
  import { agentStore } from "./stores/agentStore.svelte";

  interface ProjectGroup {
    project?: Project;
    agents: Agent[];
  }

  /** Snapshot of the clicked row passed to onsavetemplate. */
  export interface TemplateSource {
    name: string;
    provider?: string;
    model?: string;
    thinkingLevel?: string;
    cwd?: string;
    shadow?: ShadowIdentity;
  }

  // Props kept:
  //   - `oncreate` triggers the spawn dialog owned by App.
  //   - `ondismiss` / `ondelete` go through App's confirm dialogs before
  //     any lifecycle call.
  //   - `oneditproject` opens the project editor modal in App.
  //   - `onsavetemplate` is a local handler with DB invoke logic in App.
  // Everything else is read from / dispatched into `agentStore` directly.
  let {
    oncreate,
    ondismiss,
    ondelete,
    oneditproject,
    onsavetemplate,
  }: {
    oncreate: () => void;
    /** X button on a row — opens the dismiss confirm in App. */
    ondismiss: (id: string) => void;
    /** Context-menu "Delete permanently" — opens the delete confirm in App. */
    ondelete: (id: string) => void;
    oneditproject?: (project: Project) => void;
    onsavetemplate?: (source: TemplateSource) => void;
  } = $props();

  // Custom themed context menu — native right-click menu is suppressed so
  // we can render one that matches the rest of the app.
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

  // Group agents by project
  let projectGroups = $derived.by(() => {
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

<aside class="sidebar" class:collapsed={agentStore.sidebarCollapsed}>
  {#if agentStore.sidebarCollapsed}
    <!-- Collapsed rail view -->
    <div class="rail">
      <div class="rail-icon" title="Monarch">M</div>
      <button class="rail-btn" onclick={oncreate} title="Extract Shadow (Ctrl+N)">+</button>
    </div>
  {:else}
    <!-- Full sidebar -->
    <div class="sidebar-header">
      <h1>Monarch</h1>
      <div class="header-controls">
        <div
          class="view-toggle"
          role="group"
          aria-label="Shadow roster view"
        >
          <button
            class="view-toggle-btn"
            class:selected={!agentStore.sidebarShowAll}
            onclick={() => agentStore.setSidebarShowAll(false)}
            title="Show only active shadows"
            aria-pressed={!agentStore.sidebarShowAll}
          >
            Active
          </button>
          <button
            class="view-toggle-btn"
            class:selected={agentStore.sidebarShowAll}
            onclick={() => agentStore.setSidebarShowAll(true)}
            title="Include archived shadows"
            aria-pressed={agentStore.sidebarShowAll}
          >
            All
          </button>
        </div>
        <button class="btn-new" onclick={oncreate} title="Extract Shadow (Ctrl+N)">+</button>
      </div>
    </div>

    <div class="agent-list">
      {#each projectGroups as group}
        {#if group.project}
          <div class="section-label project-label">
            <button
              class="project-name-btn"
              onclick={() => oneditproject?.(group.project!)}
              title="Edit project instructions"
            >
              <span class="project-icon">/</span>{group.project.name}
            </button>
          </div>
        {:else if group.agents.length > 0}
          <div class="section-label">Shadows</div>
        {/if}
        {#each group.agents as agent (agent.id)}
          {@const isArchived = !!agent.archivedAt}
          <div
            class="agent-item"
            class:active={agent.id === agentStore.activeTabId}
            class:standby={agent.status === "stopped"}
            class:archived={isArchived}
            onclick={() => agentStore.selectAgent(agent.id)}
            onkeydown={(e: KeyboardEvent) => { if (e.key === 'Enter') agentStore.selectAgent(agent.id); }}
            oncontextmenu={(e: MouseEvent) => openAgentMenu(e, agent)}
            role="button"
            tabindex="0"
          >
            <div class="avatar-wrap">
              <ShadowAvatar agentId={agent.id} size={200} />
            </div>
            <div class="agent-info">
              <span class="agent-name">{agent.name}</span>
              {#if agent.shadow}
                <span class="agent-grade">{agent.shadow.shadowGrade}</span>
              {:else if agent.model}
                <span class="agent-model">{agent.model}</span>
              {/if}
            </div>
            {#if isArchived}
              <button
                class="btn-icon btn-summon"
                onclick={(e: MouseEvent) => { e.stopPropagation(); agentStore.summonAgent(agent.id); }}
                title="Summon back"
                aria-label="Summon {agent.name} back"
              >
                &#x21BA;
              </button>
            {:else}
              <button
                class="btn-icon btn-dismiss"
                onclick={(e: MouseEvent) => { e.stopPropagation(); ondismiss(agent.id); }}
                title="Dismiss"
                aria-label="Dismiss {agent.name}"
              >
                &times;
              </button>
            {/if}
          </div>
        {/each}
      {/each}
    </div>

    {#if agentStore.agents.length === 0}
      <div class="empty-state">
        No shadows extracted.<br />Click + to extract one.
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
          <button
            class="context-menu-item"
            onclick={handleSummon}
            role="menuitem"
          >
            Summon back
          </button>
        {/if}
        <button
          class="context-menu-item"
          onclick={handleSaveTemplate}
          role="menuitem"
        >
          Save as template
        </button>
        <div class="context-menu-divider" role="separator"></div>
        <button
          class="context-menu-item danger"
          onclick={handleDeletePermanent}
          role="menuitem"
        >
          Delete permanently
        </button>
      </div>
    {/if}

  {/if}
</aside>

<style>
  .sidebar {
    width: 220px;
    min-width: 220px;
    background: var(--bg-sidebar);
    border-right: 1px solid var(--border-subtle);
    display: flex;
    flex-direction: column;
    user-select: none;
    transition: width 0.15s ease, min-width 0.15s ease;
  }

  .sidebar.collapsed {
    width: 42px;
    min-width: 42px;
  }

  .rail {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 12px 0;
  }

  .rail-icon {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 14px;
    font-weight: 700;
    color: var(--accent);
    font-family: 'JetBrainsMono Nerd Font', 'JetBrains Mono', monospace;
    border-bottom: 1px solid var(--border-subtle);
    padding-bottom: 8px;
    margin-bottom: 4px;
  }

  .rail-btn {
    width: 28px;
    height: 28px;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    background: var(--bg-panel-2);
    color: var(--accent);
    font-size: 16px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background 0.15s, color 0.15s;
    font-family: 'JetBrainsMono Nerd Font', 'JetBrains Mono', monospace;
  }

  .rail-btn:hover {
    background: var(--bg-panel-3);
    color: var(--accent-light);
  }

  .sidebar-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 14px 16px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .sidebar-header h1 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    letter-spacing: 0.5px;
    font-family: 'JetBrainsMono Nerd Font', 'JetBrains Mono', monospace;
  }

  .header-controls {
    display: flex;
    align-items: center;
    gap: 6px;
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
    font-family: 'JetBrainsMono Nerd Font', 'JetBrains Mono', monospace;
    font-size: 10px;
    letter-spacing: 0.4px;
    text-transform: uppercase;
    padding: 4px 8px;
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

  .agent-list {
    flex: 1;
    overflow-y: auto;
    padding: 8px;
  }

  .section-label {
    font-size: 10px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    padding: 8px 10px 4px;
    font-family: 'JetBrainsMono Nerd Font', 'JetBrains Mono', monospace;
  }

  .project-label {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .project-name-btn {
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    color: inherit;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 4px;
    transition: color 0.15s;
  }

  .project-name-btn:hover {
    color: var(--accent);
  }

  .project-icon {
    color: var(--accent);
    font-weight: 700;
  }

  .agent-item {
    width: 100%;
    display: grid;
    grid-template-columns: 1fr auto;
    grid-template-rows: auto auto;
    grid-template-areas:
      "avatar avatar"
      "info kill";
    align-items: center;
    column-gap: 8px;
    row-gap: 6px;
    padding: 10px;
    margin-bottom: 8px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    font-size: 12px;
    font-family: 'JetBrainsMono Nerd Font', 'JetBrains Mono', monospace;
    text-align: left;
    transition: background 0.15s, color 0.15s;
  }
  .agent-item:hover {
    background: var(--bg-panel-2);
  }
  .agent-item.active {
    background: var(--bg-panel-2);
    color: var(--text-primary);
  }
  .agent-item.standby {
    opacity: 0.5;
  }
  .agent-item.standby:hover {
    opacity: 1;
  }
  /* MON-66: visually distinct from standby — stopped-but-active vs. dismissed.
     Italic name + lower contrast signals "not currently on the roster". */
  .agent-item.archived {
    opacity: 0.45;
    font-style: italic;
  }
  .agent-item.archived .agent-name {
    color: var(--text-muted);
  }
  .agent-item.archived:hover {
    opacity: 0.8;
  }

  .avatar-wrap {
    grid-area: avatar;
    width: 100%;
    aspect-ratio: 1 / 1;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 8px;
    overflow: hidden;
  }

  .avatar-wrap :global(canvas) {
    width: 100% !important;
    height: 100% !important;
  }

  .agent-info {
    grid-area: info;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .agent-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .agent-model {
    font-size: 10px;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .agent-grade {
    font-size: 10px;
    color: var(--accent);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .btn-icon {
    grid-area: kill;
    border: none;
    background: none;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 16px;
    padding: 0 2px;
    line-height: 1;
    transition: color 0.15s;
  }
  .btn-dismiss:hover {
    color: var(--error);
  }
  .btn-summon:hover {
    color: var(--accent);
  }

  .empty-state {
    padding: 24px 16px;
    color: var(--text-muted);
    font-size: 12px;
    font-family: 'JetBrainsMono Nerd Font', 'JetBrains Mono', monospace;
    text-align: center;
    line-height: 1.6;
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
    font-family: 'JetBrainsMono Nerd Font', 'JetBrains Mono', monospace;
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
</style>
