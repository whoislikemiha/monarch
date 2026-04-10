<script lang="ts">
  import type { Agent, Project, SessionRecord, ShadowIdentity } from "./types";

  interface SavedAgentInfo {
    id: string;
    name: string;
    projectId?: string;
    provider?: string;
    model?: string;
    thinkingLevel?: string;
    cwd?: string;
    shadow?: ShadowIdentity;
    sessions: SessionRecord[];
  }

  interface ProjectGroup {
    project?: Project;
    agents: Agent[];
    savedAgents: SavedAgentInfo[];
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

  let {
    agents,
    projects = [],
    savedAgents = [],
    activeId,
    councilMode = false,
    onselect,
    oncreate,
    onkill,
    oncouncil,
    onviewhistory,
    oneditproject,
    onsavetemplate,
  }: {
    agents: Agent[];
    projects?: Project[];
    savedAgents?: SavedAgentInfo[];
    activeId: string | null;
    councilMode?: boolean;
    onselect: (id: string) => void;
    oncreate: () => void;
    onkill: (id: string) => void;
    oncouncil?: () => void;
    onviewhistory?: (saved: SavedAgentInfo) => void;
    oneditproject?: (project: Project) => void;
    onsavetemplate?: (source: TemplateSource) => void;
  } = $props();

  // Custom themed context menu — native right-click menu is suppressed so
  // we can render one that matches the rest of the app.
  let contextMenu: { x: number; y: number; source: TemplateSource } | null = $state(null);

  function openAgentMenu(e: MouseEvent, agent: Agent) {
    e.preventDefault();
    contextMenu = {
      x: e.clientX,
      y: e.clientY,
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

  function openSavedMenu(e: MouseEvent, saved: SavedAgentInfo) {
    e.preventDefault();
    contextMenu = {
      x: e.clientX,
      y: e.clientY,
      source: {
        name: saved.shadow?.shadowName || saved.name,
        provider: saved.provider,
        model: saved.model,
        thinkingLevel: saved.thinkingLevel,
        cwd: saved.cwd,
        shadow: saved.shadow,
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

  let runningCount = $derived(agents.filter((a) => a.status === "running").length);

  // Saved agents that aren't currently active
  let inactiveSaved = $derived(
    savedAgents.filter((s) => !agents.some((a) => a.id === s.id || a.name === s.name))
  );

  // Group both active agents and saved agents by project
  let projectGroups = $derived.by(() => {
    const projectMap = new Map<string, Project>();
    for (const p of projects) projectMap.set(p.id, p);

    const groups = new Map<string, ProjectGroup>();
    const ungrouped: ProjectGroup = { project: undefined, agents: [], savedAgents: [] };

    function ensureGroup(projectId: string): ProjectGroup {
      if (!groups.has(projectId)) {
        groups.set(projectId, { project: projectMap.get(projectId), agents: [], savedAgents: [] });
      }
      return groups.get(projectId)!;
    }

    for (const agent of agents) {
      if (agent.projectId && projectMap.has(agent.projectId)) {
        ensureGroup(agent.projectId).agents.push(agent);
      } else {
        ungrouped.agents.push(agent);
      }
    }

    for (const saved of inactiveSaved) {
      if (saved.projectId && projectMap.has(saved.projectId)) {
        ensureGroup(saved.projectId).savedAgents.push(saved);
      } else {
        ungrouped.savedAgents.push(saved);
      }
    }

    const result: ProjectGroup[] = [...groups.values()];
    if (ungrouped.agents.length > 0 || ungrouped.savedAgents.length > 0) {
      result.push(ungrouped);
    }
    return result;
  });
</script>

<aside class="sidebar">
  <div class="sidebar-header">
    <h1>Monarch</h1>
    <button class="btn-new" onclick={oncreate} title="Extract Shadow (Ctrl+N)">+</button>
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
      {:else if group.agents.length > 0 || group.savedAgents.length > 0}
        <div class="section-label">Shadows</div>
      {/if}
      {#each group.agents as agent (agent.id)}
        <div
          class="agent-item"
          class:active={agent.id === activeId}
          onclick={() => onselect(agent.id)}
          onkeydown={(e: KeyboardEvent) => { if (e.key === 'Enter') onselect(agent.id); }}
          oncontextmenu={(e: MouseEvent) => openAgentMenu(e, agent)}
          role="button"
          tabindex="0"
        >
          <span class="status-dot {agent.isStreaming ? 'streaming' : agent.status}"></span>
          <div class="agent-info">
            <span class="agent-name">{agent.name}</span>
            {#if agent.shadow}
              <span class="agent-grade">{agent.shadow.shadowGrade}</span>
            {:else if agent.model}
              <span class="agent-model">{agent.model}</span>
            {/if}
          </div>
          <button
            class="btn-kill"
            onclick={(e: MouseEvent) => { e.stopPropagation(); onkill(agent.id); }}
            title="Kill"
          >
            &times;
          </button>
        </div>
      {/each}
      {#each group.savedAgents as saved (saved.id)}
        <button
          class="agent-item saved"
          onclick={() => onviewhistory?.(saved)}
          oncontextmenu={(e: MouseEvent) => openSavedMenu(e, saved)}
        >
          <span class="status-dot stopped"></span>
          <div class="agent-info">
            <span class="agent-name">{saved.name}</span>
            <span class="agent-meta">
              {saved.sessions.length} session{saved.sessions.length !== 1 ? 's' : ''}
              {#if saved.model}
                &middot; {saved.model}
              {/if}
            </span>
          </div>
        </button>
      {/each}
    {/each}
  </div>

  {#if agents.length === 0 && inactiveSaved.length === 0}
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
      <button
        class="context-menu-item"
        onclick={handleSaveTemplate}
        role="menuitem"
      >
        Save as template
      </button>
    </div>
  {/if}

  {#if runningCount >= 2 && oncouncil}
    <div class="council-section">
      <button
        class="council-btn"
        class:active={councilMode}
        onclick={oncouncil}
      >
        Council Mode
        <span class="council-shortcut">Ctrl+L</span>
      </button>
    </div>
  {/if}
</aside>

<style>
  .sidebar {
    width: 220px;
    min-width: 220px;
    background: var(--bg-sidebar, #0c0816);
    border-right: 1px solid var(--border-subtle);
    display: flex;
    flex-direction: column;
    user-select: none;
  }

  .sidebar-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px;
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

  .btn-new {
    width: 28px;
    height: 28px;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    background: var(--bg-panel-2);
    color: var(--accent-purple);
    font-size: 18px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background 0.15s, color 0.15s;
  }
  .btn-new:hover {
    background: var(--bg-panel-3);
    color: #e2d4ff;
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
    color: var(--accent-purple, #be95ff);
  }

  .project-icon {
    color: var(--accent-purple, #be95ff);
    font-weight: 700;
  }

  .agent-item {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border: none;
    border-radius: 6px;
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
  .agent-item.saved {
    opacity: 0.7;
  }
  .agent-item.saved:hover {
    opacity: 1;
  }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .status-dot.running {
    background: var(--success);
    box-shadow: 0 0 4px rgba(66, 190, 101, 0.55);
  }
  .status-dot.stopped {
    background: var(--text-muted);
  }
  .status-dot.starting {
    background: var(--warning);
  }
  .status-dot.streaming {
    background: var(--accent-blue);
    box-shadow: 0 0 4px rgba(51, 177, 255, 0.55);
    animation: pulse 1s ease-in-out infinite;
  }
  .status-dot.error {
    background: var(--error);
  }

  @keyframes pulse {
    0%, 100% { opacity: 0.6; }
    50% { opacity: 1; }
  }

  .agent-info {
    flex: 1;
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
    color: var(--accent-purple, #be95ff);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .agent-meta {
    font-size: 10px;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .btn-kill {
    border: none;
    background: none;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 16px;
    padding: 0 2px;
    line-height: 1;
    transition: color 0.15s;
  }
  .btn-kill:hover {
    color: var(--error);
  }

  .empty-state {
    padding: 24px 16px;
    color: var(--text-muted);
    font-size: 12px;
    font-family: 'JetBrainsMono Nerd Font', 'JetBrains Mono', monospace;
    text-align: center;
    line-height: 1.6;
  }

  .council-section {
    padding: 8px;
    border-top: 1px solid var(--border-subtle);
  }

  .council-btn {
    width: 100%;
    padding: 8px 12px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: var(--bg-panel-2);
    color: var(--text-secondary);
    font-size: 11px;
    font-family: 'JetBrainsMono Nerd Font', 'JetBrains Mono', monospace;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: space-between;
    transition: background 0.15s, border-color 0.15s, color 0.15s;
  }

  .council-btn:hover {
    background: var(--bg-panel-3);
    color: var(--accent-purple, #be95ff);
    border-color: var(--accent-purple, #be95ff);
  }

  .council-btn.active {
    background: rgba(190, 149, 255, 0.12);
    border-color: var(--accent-purple, #be95ff);
    color: var(--accent-purple, #be95ff);
  }

  .council-shortcut {
    font-size: 9px;
    opacity: 0.5;
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
    background: var(--bg-panel, #171126);
    border: 1px solid var(--border-subtle, #35274f);
    border-radius: 6px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .context-menu-item {
    padding: 6px 10px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-secondary, #dde1e6);
    font-size: 11px;
    font-family: 'JetBrainsMono Nerd Font', 'JetBrains Mono', monospace;
    text-align: left;
    cursor: pointer;
    transition: background 0.12s, color 0.12s;
  }

  .context-menu-item:hover {
    background: var(--bg-panel-2, #201734);
    color: var(--accent-purple, #be95ff);
  }
</style>
