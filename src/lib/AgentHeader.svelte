<script lang="ts">
  import type { Agent } from "./types";
  import { ShadowAvatar } from "./avatar";

  let {
    agent,
    projectName,
    onprompt,
    onhistory,
    oncompact,
    onnewsession,
    onprojectedit,
  }: {
    agent: Agent;
    projectName?: string;
    onprompt: () => void;
    onhistory: () => void;
    oncompact: () => void;
    onnewsession: () => void;
    onprojectedit?: () => void;
  } = $props();

  let showMenu = $state(false);

  function shortenPath(path: string): string {
    return path.replace(/^\/home\/[^/]+/, "~");
  }

  function handleAction(fn: () => void) {
    showMenu = false;
    fn();
  }
</script>

<svelte:window onclick={() => (showMenu = false)} />

<div class="agent-header">
  <div class="header-left">
    <div class="header-avatar">
      <ShadowAvatar agentId={agent.id} size={40} />
    </div>
    <span class="agent-name">{agent.shadow?.shadowName || agent.name}</span>
    {#if agent.shadow?.shadowTitle}
      <span class="agent-title">{agent.shadow.shadowTitle}</span>
    {/if}
    {#if projectName}
      <button
        class="project-badge"
        onclick={() => onprojectedit?.()}
        title="Edit project instructions"
      >
        <span class="project-slash">/</span>{projectName}
      </button>
    {:else if agent.cwd}
      <span class="agent-cwd" title={agent.cwd}>{shortenPath(agent.cwd)}</span>
    {/if}
  </div>

  <div class="header-right">
    <button class="new-session-btn" onclick={onnewsession} title="New conversation">
      + new chat
    </button>
    <div class="menu-wrap">
      <button
        class="menu-btn"
        onclick={(e: MouseEvent) => { e.stopPropagation(); showMenu = !showMenu; }}
        title="Agent actions"
      >
        ...
      </button>
      {#if showMenu}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div class="menu-dropdown" onclick={(e: MouseEvent) => e.stopPropagation()} role="menu" tabindex="-1">
          <button class="menu-item" onclick={() => handleAction(onprompt)} role="menuitem">
            System Prompt
          </button>
          {#if onprojectedit}
            <button class="menu-item" onclick={() => handleAction(onprojectedit)} role="menuitem">
              Project Instructions
            </button>
          {/if}
          <button class="menu-item" onclick={() => handleAction(onhistory)} role="menuitem">
            Session History
          </button>
          <div class="menu-divider"></div>
          <button class="menu-item" onclick={() => handleAction(oncompact)} role="menuitem">
            Compact Context
          </button>
          <button class="menu-item" onclick={() => handleAction(onnewsession)} role="menuitem">
            New Session
          </button>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .agent-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 20px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-sidebar);
    flex-shrink: 0;
  }

  .header-left {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .header-avatar {
    width: 40px;
    height: 40px;
    flex-shrink: 0;
    border-radius: 50%;
    overflow: hidden;
  }

  .agent-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .agent-title {
    font-size: 11px;
    color: var(--accent);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-badge {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 2px 8px;
    border: 1px solid var(--accent-border-subtle);
    border-radius: 4px;
    background: var(--accent-bg-subtle);
    color: var(--text-muted);
    font-size: 10px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s, color 0.15s, border-color 0.15s;
    white-space: nowrap;
  }

  .project-badge:hover {
    background: var(--accent-bg-hover);
    color: var(--accent);
    border-color: var(--accent-border-hover);
  }

  .project-slash {
    color: var(--accent);
    font-weight: 700;
  }

  .agent-cwd {
    font-size: 10px;
    color: var(--text-muted);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .header-right {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .new-session-btn {
    padding: 4px 10px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: transparent;
    color: var(--text-muted);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s, color 0.15s, border-color 0.15s;
  }

  .new-session-btn:hover {
    background: var(--bg-panel-2);
    color: var(--accent);
    border-color: var(--accent);
  }

  .menu-wrap {
    position: relative;
  }

  .menu-btn {
    width: 28px;
    height: 28px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: transparent;
    color: var(--text-muted);
    font-size: 14px;
    font-weight: 700;
    letter-spacing: 1px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background 0.15s, color 0.15s;
  }

  .menu-btn:hover {
    background: var(--bg-panel-2);
    color: var(--text-primary);
  }

  .menu-dropdown {
    position: absolute;
    top: 100%;
    right: 0;
    margin-top: 4px;
    min-width: 180px;
    background: var(--bg-panel-2);
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    padding: 4px;
    z-index: 50;
    display: flex;
    flex-direction: column;
  }

  .menu-item {
    padding: 8px 12px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    text-align: left;
    transition: background 0.1s, color 0.1s;
  }

  .menu-item:hover {
    background: var(--bg-panel-3);
    color: var(--text-primary);
  }

  .menu-divider {
    height: 1px;
    background: var(--border-subtle);
    margin: 4px 8px;
  }
</style>
