<script lang="ts">
  import type { Agent, SavedAgent } from "./types";

  let {
    agents,
    savedAgents = [],
    activeId,
    councilMode = false,
    onselect,
    oncreate,
    onkill,
    oncouncil,
    onviewhistory,
  }: {
    agents: Agent[];
    savedAgents?: SavedAgent[];
    activeId: string | null;
    councilMode?: boolean;
    onselect: (id: string) => void;
    oncreate: () => void;
    onkill: (id: string) => void;
    oncouncil?: () => void;
    onviewhistory?: (saved: SavedAgent) => void;
  } = $props();

  let runningCount = $derived(agents.filter((a) => a.status === "running").length);

  // Saved agents that aren't currently active
  let inactiveSaved = $derived(
    savedAgents.filter((s) => !agents.some((a) => a.id === s.id || a.name === s.name))
  );
</script>

<aside class="sidebar">
  <div class="sidebar-header">
    <h1>Monarch</h1>
    <button class="btn-new" onclick={oncreate} title="Extract Shadow (Ctrl+N)">+</button>
  </div>

  <div class="agent-list">
    {#if agents.length > 0}
      <div class="section-label">Active</div>
    {/if}
    {#each agents as agent (agent.id)}
      <div
        class="agent-item"
        class:active={agent.id === activeId}
        onclick={() => onselect(agent.id)}
        onkeydown={(e: KeyboardEvent) => { if (e.key === 'Enter') onselect(agent.id); }}
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

    {#if inactiveSaved.length > 0}
      <div class="section-label">History</div>
      {#each inactiveSaved as saved (saved.id)}
        <button
          class="agent-item saved"
          onclick={() => onviewhistory?.(saved)}
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
    {/if}
  </div>

  {#if agents.length === 0 && inactiveSaved.length === 0}
    <div class="empty-state">
      No shadows extracted.<br />Click + to extract one.
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
</style>
