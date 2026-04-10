<script lang="ts">
  import type { Agent } from "./types";

  let {
    agents,
    openTabs,
    activeTabId,
    onselect,
    onclose,
    onnewconversation,
  }: {
    agents: Agent[];
    openTabs: string[];
    activeTabId: string | null;
    onselect: (id: string) => void;
    onclose: (id: string) => void;
    onnewconversation: (agentId: string) => void;
  } = $props();

  let showDropdown = $state(false);
  let availableAgents = $derived(agents.filter((a) => !openTabs.includes(a.id)));
  let tabAgents = $derived(openTabs.map((id) => agents.find((a) => a.id === id)).filter((a): a is Agent => !!a));

  function handleDropdownSelect(agentId: string) {
    showDropdown = false;
    onnewconversation(agentId);
  }

  function closeDropdown() {
    showDropdown = false;
  }
</script>

<div class="tab-bar">
  <div class="tabs">
    {#each tabAgents as agent, i (agent.id)}
      <div
        class="tab"
        class:active={agent.id === activeTabId}
        class:standby={agent.status === "stopped"}
        onclick={() => onselect(agent.id)}
        onkeydown={(e) => { if (e.key === 'Enter') onselect(agent.id); }}
        title="{agent.name}{agent.model ? ` · ${agent.model}` : ''}"
        role="tab"
        tabindex="0"
      >
        <span class="tab-dot {agent.isStreaming ? 'streaming' : agent.status}"></span>
        <span class="tab-name">{agent.name}</span>
        <button
          class="tab-close"
          onclick={(e) => { e.stopPropagation(); onclose(agent.id); }}
          title="Close tab"
        >&times;</button>
      </div>
    {/each}
  </div>

  <div class="tab-actions">
    <button
      class="tab-add"
      onclick={() => (showDropdown = !showDropdown)}
      title="Open agent in new tab"
    >+</button>

    {#if showDropdown}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div class="dropdown-backdrop" role="presentation" onclick={closeDropdown}></div>
      <div class="dropdown" role="menu">
        {#if availableAgents.length === 0 && agents.length === 0}
          <div class="dropdown-empty">No agents — Ctrl+N to create</div>
        {:else if availableAgents.length === 0}
          <div class="dropdown-empty">All agents are open</div>
        {:else}
          {#each availableAgents as agent (agent.id)}
            <button
              class="dropdown-item"
              role="menuitem"
              onclick={() => handleDropdownSelect(agent.id)}
            >
              <span class="tab-dot {agent.isStreaming ? 'streaming' : agent.status}"></span>
              <span class="dropdown-name">{agent.name}</span>
              {#if agent.model}<span class="dropdown-model">{agent.model}</span>{/if}
            </button>
          {/each}
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .tab-bar {
    display: flex;
    align-items: stretch;
    height: 34px;
    min-height: 34px;
    background: var(--bg-sidebar, #0c0816);
    border-bottom: 1px solid var(--border-subtle, #35274f);
    user-select: none;
    overflow: hidden;
  }

  .tabs {
    display: flex;
    flex: 1;
    min-width: 0;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .tabs::-webkit-scrollbar {
    display: none;
  }

  .tab {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 12px;
    min-width: 0;
    max-width: 180px;
    cursor: pointer;
    border-right: 1px solid var(--border-subtle, #35274f);
    color: var(--text-muted, #8f7aa8);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    transition: background 0.12s, color 0.12s;
    flex-shrink: 0;
  }

  .tab:hover {
    background: var(--bg-panel-2, #201734);
    color: var(--text-secondary, #dde1e6);
  }

  .tab.active {
    background: var(--bg-panel, #171126);
    color: var(--text-primary, #f2f4f8);
    border-bottom: 2px solid var(--accent-purple, #be95ff);
  }

  .tab.standby {
    opacity: 0.6;
  }

  .tab-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .tab-dot.running {
    background: var(--success, #42be65);
    box-shadow: 0 0 3px rgba(66, 190, 101, 0.5);
  }
  .tab-dot.stopped {
    background: var(--text-muted, #8f7aa8);
  }
  .tab-dot.starting {
    background: var(--warning, #ffe97b);
  }
  .tab-dot.streaming {
    background: var(--accent-blue, #33b1ff);
    box-shadow: 0 0 3px rgba(51, 177, 255, 0.5);
    animation: pulse 1s ease-in-out infinite;
  }
  .tab-dot.error {
    background: var(--error, #ee5396);
  }

  .tab-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tab-close {
    border: none;
    background: none;
    color: var(--text-muted, #8f7aa8);
    cursor: pointer;
    font-size: 14px;
    line-height: 1;
    padding: 0 2px;
    opacity: 0;
    transition: opacity 0.12s, color 0.12s;
  }
  .tab:hover .tab-close {
    opacity: 1;
  }
  .tab-close:hover {
    color: var(--error, #ee5396);
  }

  .tab-actions {
    display: flex;
    align-items: center;
    padding: 0 6px;
    position: relative;
  }

  .tab-add {
    width: 24px;
    height: 24px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-muted, #8f7aa8);
    font-size: 16px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background 0.12s, color 0.12s;
  }
  .tab-add:hover {
    background: var(--bg-panel-2, #201734);
    color: var(--accent-purple, #be95ff);
  }

  .dropdown-backdrop {
    position: fixed;
    inset: 0;
    z-index: 500;
  }

  .dropdown {
    position: absolute;
    top: 100%;
    right: 0;
    z-index: 501;
    min-width: 200px;
    max-height: 300px;
    overflow-y: auto;
    padding: 4px;
    margin-top: 4px;
    background: var(--bg-panel, #171126);
    border: 1px solid var(--border-subtle, #35274f);
    border-radius: 6px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .dropdown-empty {
    padding: 12px 10px;
    color: var(--text-muted, #8f7aa8);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    text-align: center;
  }

  .dropdown-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-secondary, #dde1e6);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    text-align: left;
    cursor: pointer;
    transition: background 0.12s, color 0.12s;
  }
  .dropdown-item:hover {
    background: var(--bg-panel-2, #201734);
    color: var(--accent-purple, #be95ff);
  }

  .dropdown-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dropdown-model {
    font-size: 10px;
    color: var(--text-muted, #8f7aa8);
    flex-shrink: 0;
  }

  @keyframes pulse {
    0%, 100% { opacity: 0.4; }
    50% { opacity: 1; }
  }
</style>
