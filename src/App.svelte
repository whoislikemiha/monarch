<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import Sidebar from "./lib/Sidebar.svelte";
  import AgentView from "./lib/AgentView.svelte";
  import CouncilView from "./lib/CouncilView.svelte";
  import SpawnDialog from "./lib/SpawnDialog.svelte";
  import HistoryPanel from "./lib/HistoryPanel.svelte";
  import type { Agent, AgentConfig, SavedAgent, SessionRecord } from "./lib/types";

  let agents: Agent[] = $state([]);
  let activeId: string | null = $state(null);
  let showSpawnDialog = $state(false);
  let showSidebar = $state(true);
  let councilMode = $state(false);
  let counter = 0;
  let agentViewRef: AgentView | undefined = $state(undefined);
  let savedAgents: SavedAgent[] = $state([]);
  let showRestoreBar = $state(false);
  let viewingSavedAgent: SavedAgent | null = $state(null);

  // Council needs at least 2 running agents
  let councilAgents = $derived(agents.filter((a) => a.status === "running"));

  // --- Persistence ---

  function agentToSaved(agent: Agent): SavedAgent {
    const activeSession: SessionRecord | undefined = agent.sessionFile
      ? {
          sessionFile: agent.sessionFile,
          sessionId: agent.sessionId,
          model: agent.model,
          provider: agent.provider,
          startedAt: new Date().toISOString(),
          messageCount: agent.sessionStats?.messageCount,
        }
      : undefined;

    return {
      id: agent.id,
      name: agent.name,
      provider: agent.provider,
      model: agent.model,
      thinkingLevel: agent.thinkingLevel,
      cwd: agent.cwd,
      shadow: agent.shadow,
      activeSession,
      sessions: agent.sessions || [],
    };
  }

  async function persistAgents() {
    try {
      const toSave = agents
        .filter((a) => a.status !== "error")
        .map(agentToSaved);
      await invoke("save_agents", { agents: toSave });
    } catch (e) {
      console.error("Failed to persist agents:", e);
    }
  }

  // Auto-save: on agent list changes + periodic + on close
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    void agents.length;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(persistAgents, 1000);
  });

  // Periodic save every 10s to catch deep mutations (sessionFile, etc.)
  let periodicSave: ReturnType<typeof setInterval> | null = null;
  $effect(() => {
    if (agents.length > 0 && !periodicSave) {
      periodicSave = setInterval(persistAgents, 10000);
    }
    if (agents.length === 0 && periodicSave) {
      clearInterval(periodicSave);
      periodicSave = null;
    }
  });

  // Save on window close
  if (typeof window !== "undefined") {
    window.addEventListener("beforeunload", () => {
      persistAgents();
    });
  }

  async function loadSavedAgents() {
    try {
      const loaded = await invoke<SavedAgent[]>("load_agents");
      // Ensure sessions array exists on all
      savedAgents = loaded.map((a) => ({ ...a, sessions: a.sessions || [] }));
      if (savedAgents.length > 0) {
        showRestoreBar = true;
      }
    } catch {
      // No saved state or not in Tauri
    }
  }

  async function restoreAllAgents() {
    showRestoreBar = false;
    for (const saved of savedAgents) {
      await restoreAgent(saved);
    }
    savedAgents = [];
  }

  async function restoreAgent(saved: SavedAgent) {
    const config: AgentConfig = {
      provider: saved.provider,
      model: saved.model,
      thinkingLevel: saved.thinkingLevel,
      cwd: saved.cwd,
      shadow: saved.shadow,
    };
    const sessionFile = saved.activeSession?.sessionFile;
    await createAgent(config, sessionFile);

    // After creating, carry over the session history
    const newId = agents[agents.length - 1]?.id;
    if (newId) {
      // Merge: active session becomes part of history, plus all past sessions
      const allSessions = [...saved.sessions];
      if (saved.activeSession) {
        // Add active session to history if not already there
        const exists = allSessions.some(
          (s) => s.sessionFile === saved.activeSession!.sessionFile,
        );
        if (!exists) {
          allSessions.unshift(saved.activeSession);
        }
      }
      agents = agents.map((a) =>
        a.id === newId ? { ...a, sessions: allSessions } : a,
      );
    }
  }

  function dismissRestore() {
    showRestoreBar = false;
    savedAgents = [];
    // Clear saved state
    invoke("save_agents", { agents: [] }).catch(() => {});
  }

  onMount(() => {
    loadSavedAgents();
  });

  // --- Agent lifecycle ---

  async function createAgent(config?: AgentConfig, restoreSessionFile?: string) {
    counter++;
    const id = `agent-${Date.now()}`;
    const name = config?.shadow?.shadowName || `Agent ${counter}`;
    const cwd = config?.cwd || "/home/miha";
    const agent: Agent = {
      id,
      name,
      status: "running",
      provider: config?.provider,
      model: config?.model,
      thinkingLevel: config?.thinkingLevel || "off",
      cwd,
      isStreaming: false,
      stderrLines: [],
      shadow: config?.shadow,
      sessions: [],
    };
    agents = [...agents, agent];
    activeId = id;

    invoke("spawn_agent", {
      id,
      provider: config?.provider || null,
      model: config?.model || null,
      thinkingLevel: config?.thinkingLevel || null,
      cwd,
      extensions: config?.extensions || null,
      shadowName: config?.shadow?.shadowName || null,
      shadowTitle: config?.shadow?.shadowTitle || null,
      shadowGrade: config?.shadow?.shadowGrade || null,
      sessionFile: restoreSessionFile || null,
    })
      .then(() => {
        agents = agents.map((a) =>
          a.id === id ? { ...a, status: "running" as const } : a,
        );
        // If restoring, fetch message history from the session
        if (restoreSessionFile) {
          invoke("send_command", {
            id,
            commandJson: JSON.stringify({ type: "get_messages", id: "restore-messages" }),
          });
        }
      })
      .catch((err) => {
        console.error("Failed to spawn agent:", err);
        agents = agents.map((a) =>
          a.id === id
            ? { ...a, status: "error" as const, stderrLines: [...a.stderrLines, String(err)] }
            : a,
        );
      });

    listen(`agent-exit-${id}`, () => {
      agents = agents.map((a) =>
        a.id === id ? { ...a, status: "stopped" as const, isStreaming: false } : a,
      );
    });
  }

  function restartAgent(id: string) {
    const agent = agents.find((a) => a.id === id);
    if (!agent) return;
    const sessionFile = agent.sessionFile;
    killAgent(id);
    createAgent({
      provider: agent.provider,
      model: agent.model,
      thinkingLevel: agent.thinkingLevel,
      cwd: agent.cwd,
      shadow: agent.shadow,
    }, sessionFile);
  }

  function selectAgent(id: string) {
    activeId = id;
  }

  function killAgent(id: string) {
    invoke("kill_agent", { id, graceful: true });
    agents = agents.filter((a) => a.id !== id);
    if (activeId === id) {
      activeId = agents.length > 0 ? agents[agents.length - 1].id : null;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    // Don't capture when in input/textarea/dialog
    const target = e.target as HTMLElement;
    const inInput = target.tagName === "TEXTAREA" || target.tagName === "INPUT" || target.tagName === "SELECT";
    const inDialog = target.closest("[role=dialog]") !== null;

    // Ctrl+N — spawn new agent (always)
    if (e.ctrlKey && e.key === "n") {
      e.preventDefault();
      showSpawnDialog = true;
      return;
    }

    // Ctrl+B — toggle sidebar (always)
    if (e.ctrlKey && e.key === "b") {
      e.preventDefault();
      showSidebar = !showSidebar;
      return;
    }

    // Ctrl+L — toggle council mode
    if (e.ctrlKey && e.key === "l") {
      e.preventDefault();
      if (councilAgents.length >= 2) {
        councilMode = !councilMode;
      }
      return;
    }

    // Ctrl+1-9 — switch agents (always)
    if (e.ctrlKey && e.key >= "1" && e.key <= "9") {
      e.preventDefault();
      const idx = parseInt(e.key) - 1;
      if (idx < agents.length) {
        activeId = agents[idx].id;
      }
      return;
    }

    // Below only when not in an input
    if (inInput || inDialog) return;

    // / or i — focus chat input
    if (e.key === "/" || e.key === "i") {
      e.preventDefault();
      agentViewRef?.focusInput();
      return;
    }

    // Escape — unfocus (blur active element)
    if (e.key === "Escape") {
      (document.activeElement as HTMLElement)?.blur();
      return;
    }

    // Ctrl+C — copy if text selected, otherwise abort active agent
    if (e.ctrlKey && e.key === "c") {
      const selection = window.getSelection();
      if (selection && selection.toString().length > 0) {
        // Let the browser handle copy
        return;
      }
      e.preventDefault();
      if (activeAgent) {
        invoke("send_command", {
          id: activeAgent.id,
          commandJson: JSON.stringify({ type: "abort" }),
        });
      }
      return;
    }
  }

  let activeAgent = $derived(agents.find((a) => a.id === activeId));
</script>

<svelte:window onkeydown={handleKeydown} />

<main class="app">
  {#if showSidebar}
    <Sidebar
      {agents}
      {savedAgents}
      {activeId}
      {councilMode}
      onselect={selectAgent}
      oncreate={() => (showSpawnDialog = true)}
      onkill={killAgent}
      oncouncil={() => {
        if (councilAgents.length >= 2) councilMode = !councilMode;
      }}
      onviewhistory={(saved) => {
        viewingSavedAgent = saved;
      }}
    />
  {/if}
  <div class="main-panel">
    {#if councilMode && councilAgents.length >= 2}
      <CouncilView
        agents={councilAgents}
        onback={() => (councilMode = false)}
      />
    {:else if activeAgent}
      {#key activeAgent.id}
        <AgentView
          agent={activeAgent}
          onrestart={restartAgent}
          bind:this={agentViewRef}
        />
      {/key}
    {:else}
      <div class="empty-state">
        {#if showRestoreBar && savedAgents.length > 0}
          <div class="restore-bar">
            <span class="restore-text">
              {savedAgents.length} shadow{savedAgents.length > 1 ? "s" : ""} from last session
            </span>
            <div class="restore-names">
              {#each savedAgents as sa}
                <span class="restore-name">{sa.name}</span>
              {/each}
            </div>
            <div class="restore-actions">
              <button class="restore-btn" onclick={restoreAllAgents}>Restore All</button>
              <button class="dismiss-btn" onclick={dismissRestore}>Dismiss</button>
            </div>
          </div>
        {:else}
          <span class="empty-icon">&gt;_</span>
          <p>Extract a shadow to begin</p>
          <p class="hint">Ctrl+N extract &middot; Ctrl+B sidebar &middot; Ctrl+L council &middot; Ctrl+1-9 switch</p>
        {/if}
      </div>
    {/if}
  </div>
</main>

{#if showSpawnDialog}
  <SpawnDialog
    onspawn={(config) => {
      showSpawnDialog = false;
      createAgent(config);
    }}
    oncancel={() => (showSpawnDialog = false)}
  />
{/if}

{#if viewingSavedAgent}
  <HistoryPanel
    sessions={viewingSavedAgent.sessions}
    onload={(sessionFile) => {
      // Restore this agent with the selected session
      const saved = viewingSavedAgent;
      viewingSavedAgent = null;
      if (saved) restoreAgent({ ...saved, activeSession: { sessionFile, startedAt: new Date().toISOString() }, sessions: saved.sessions });
    }}
    onclose={() => (viewingSavedAgent = null)}
  />
{/if}

<style>
  .app {
    display: flex;
    width: 100vw;
    height: 100vh;
  }

  .main-panel {
    flex: 1;
    display: flex;
    min-width: 0;
  }

  .empty-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
  }

  .empty-icon {
    font-size: 48px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    margin-bottom: 12px;
    color: var(--accent-purple);
  }

  .empty-state p {
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    margin: 0;
  }

  .hint {
    margin-top: 8px !important;
    font-size: 11px !important;
    opacity: 0.6;
  }

  .restore-bar {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 24px 32px;
    background: var(--bg-panel-2, #201734);
    border: 1px solid var(--accent-purple, #be95ff);
    border-radius: 12px;
    max-width: 400px;
  }

  .restore-text {
    font-size: 13px;
    color: var(--text-primary, #f2f4f8);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .restore-names {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    justify-content: center;
  }

  .restore-name {
    font-size: 11px;
    padding: 3px 10px;
    border-radius: 4px;
    background: rgba(190, 149, 255, 0.12);
    color: var(--accent-purple, #be95ff);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .restore-actions {
    display: flex;
    gap: 8px;
  }

  .restore-btn {
    padding: 8px 20px;
    border: none;
    border-radius: 8px;
    background: var(--accent-purple, #be95ff);
    color: #140d22;
    font-size: 12px;
    font-weight: 600;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s;
  }

  .restore-btn:hover {
    background: #d5bbff;
  }

  .dismiss-btn {
    padding: 8px 16px;
    border: 1px solid var(--border-subtle, #35274f);
    border-radius: 8px;
    background: transparent;
    color: var(--text-secondary, #dde1e6);
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s;
  }

  .dismiss-btn:hover {
    background: var(--bg-panel-2, #201734);
  }
</style>
