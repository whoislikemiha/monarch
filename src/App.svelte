<script lang="ts">
  import { onMount } from "svelte";
  import { invoke, listen } from "$lib/api";
  import { commands } from "./lib/bindings";
  import Sidebar from "./lib/Sidebar.svelte";
  import AgentView from "./lib/AgentView.svelte";
  import SpawnDialog from "./lib/SpawnDialog.svelte";
  import TabBar from "./lib/TabBar.svelte";
  import ProjectEditor from "./lib/ProjectEditor.svelte";
  import ToolRail from "./lib/toolbox/ToolRail.svelte";
  import SettingsDialog from "./lib/SettingsDialog.svelte";
  import ToolPanelStack from "./lib/toolbox/ToolPanelStack.svelte";
  import {
    liveAgentStore,
    removeLiveState,
  } from "./lib/toolbox/liveAgentStore.svelte";
  import {
    persistOpenIds,
    persistWidth,
    restoreOpenIds,
    restoreWidth,
  } from "./lib/toolbox/persistence";
  import type { AgentContext } from "./lib/toolbox/types";
  import type { Agent, AgentConfig, AgentViewState, Project, SessionRecord } from "./lib/types";
  import { applyTheme, DEFAULT_THEME } from "./lib/themes";

  let agents: Agent[] = $state([]);
  let projects: Project[] = $state([]);
  let openTabs: string[] = $state([]);
  let activeTabId: string | null = $state(null);
  let sidebarCollapsed = $state(false);
  let showSpawnDialog = $state(false);
  let showSettings = $state(false);
  let counter = 0;
  let agentViewRef: AgentView | undefined = $state(undefined);
  let exitListeners: Map<string, import("$lib/api").UnlistenFn> = new Map();
  let agentViewStates: Map<string, AgentViewState> = $state(new Map());

  // --- Toolbox state ---
  let openToolIds: string[] = $state(restoreOpenIds());
  let toolboxWidth = $state(restoreWidth());

  $effect(() => {
    persistOpenIds(openToolIds);
  });
  $effect(() => {
    persistWidth(toolboxWidth);
  });

  /**
   * Build a user-facing stderr line from a spawn_agent rejection. MON-31
   * hands back a `{ kind, message, details }` DTO rather than an opaque
   * string; branch on `kind` so the surface text is recognizable to the
   * user. Falls back to `String(err)` for non-DTO shapes (marshalling
   * errors, unexpected throws).
   */
  function formatSpawnError(err: unknown): string {
    if (err && typeof err === "object" && "kind" in err) {
      const dto = err as { kind: string; message?: string; details?: string | null };
      const msg = dto.message ?? "";
      if (dto.kind.startsWith("sidecar")) {
        return `Sidecar unreachable — ${msg}`;
      }
      switch (dto.kind) {
        case "db":
          return `Database error: ${msg}`;
        case "invalidInput":
          return msg;
        case "notFound":
          return `Not found: ${msg}`;
        default:
          return msg || String(err);
      }
    }
    return String(err);
  }

  function toggleTool(id: string) {
    openToolIds = openToolIds.includes(id)
      ? openToolIds.filter((t) => t !== id)
      : [...openToolIds, id];
  }

  function closeTool(id: string) {
    openToolIds = openToolIds.filter((t) => t !== id);
  }

  function createViewKey(agentId: string): string {
    return `${agentId}:${Date.now()}:${Math.random().toString(36).slice(2, 8)}`;
  }

  // Editing project instructions
  let editingProject: Project | null = $state(null);

  // --- DB row types matching Rust ---

  interface ProjectDbRow {
    id: string;
    name: string;
    rootPath: string;
    instructions?: string | null;
    createdAt: string;
    updatedAt: string;
  }

  interface AgentDbRow {
    id: string;
    name: string;
    projectId?: string | null;
    shadowName?: string | null;
    shadowTitle?: string | null;
    shadowGrade?: string | null;
    provider?: string | null;
    model?: string | null;
    thinkingLevel?: string | null;
    cwd?: string | null;
    customPrompt?: string | null;
    contextWindow?: number | null;
    createdAt: string;
    updatedAt: string;
  }

  interface SessionDbRow {
    id: string;
    agentId: string;
    model?: string | null;
    provider?: string | null;
    startedAt: string;
    endedAt?: string | null;
    messageCount: number;
    totalTokens: number;
    totalCost: number;
    parentSessionId?: string | null;
  }

  // --- Load saved agents from SQLite ---

  async function loadProjects() {
    try {
      const rows = await invoke<ProjectDbRow[]>("db_get_projects");
      projects = rows;
    } catch {
      // No projects yet
    }
  }

  async function loadSavedAgents() {
    try {
      const dbAgents = await invoke<AgentDbRow[]>("db_get_agents");
      for (const row of dbAgents) {
        const sessions = await invoke<SessionDbRow[]>("db_get_sessions", { agentId: row.id });
        const latestSession = sessions[0];
        const agent: Agent = {
          id: row.id,
          viewKey: createViewKey(row.id),
          name: row.shadowName || row.name,
          status: "stopped",
          projectId: row.projectId || undefined,
          provider: row.provider || undefined,
          model: row.model || undefined,
          thinkingLevel: row.thinkingLevel || undefined,
          cwd: row.cwd || undefined,
          stderrLines: [],
          contextWindow: row.contextWindow || undefined,
          shadow: row.shadowName
            ? { shadowName: row.shadowName, shadowTitle: row.shadowTitle || "", shadowGrade: (row.shadowGrade as any) || "Knight" }
            : undefined,
          sessionId: latestSession?.id,
          sessions: sessions.map((s) => ({
            sessionId: s.id,
            model: s.model || undefined,
            provider: s.provider || undefined,
            startedAt: s.startedAt,
            messageCount: s.messageCount,
          })),
          sourceSessionId: latestSession?.id,
        };
        agents = [...agents, agent];
      }
      if (agents.length > 0) {
        openTabs = agents.map((a) => a.id);
        if (!activeTabId) activeTabId = agents[0].id;
      }
    } catch {
      // No saved state
    }
  }

  async function loadUiState() {
    try {
      const tabsJson = await invoke<string | null>("db_get_ui_state", { key: "openTabs" });
      const activeJson = await invoke<string | null>("db_get_ui_state", { key: "activeTabId" });
      const collapsedJson = await invoke<string | null>("db_get_ui_state", { key: "sidebarCollapsed" });
      const themeJson = await invoke<string | null>("db_get_ui_state", { key: "theme" });
      if (tabsJson) {
        const savedTabs: string[] = JSON.parse(tabsJson);
        const agentIds = new Set(agents.map((a) => a.id));
        openTabs = savedTabs.filter((id) => agentIds.has(id));
      }
      if (activeJson) {
        const savedActive = JSON.parse(activeJson);
        if (openTabs.includes(savedActive)) activeTabId = savedActive;
        else if (openTabs.length > 0) activeTabId = openTabs[0];
      }
      if (collapsedJson) sidebarCollapsed = JSON.parse(collapsedJson);
      if (themeJson) {
        applyTheme(JSON.parse(themeJson));
      }
    } catch {}
  }

  // --- Zoom state ---
  const ZOOM_STEP = 0.05;
  const ZOOM_DEFAULT = 1.0;
  let zoomLevel = $state(ZOOM_DEFAULT);

  async function applyZoom(level: number) {
    try {
      const clamped = await invoke<number>("set_zoom", { level });
      zoomLevel = clamped;
      invoke("db_set_ui_state", { key: "zoomLevel", value: String(clamped) }).catch(() => {});
    } catch {
      // Not in Tauri (browser mode) — skip
    }
  }

  function handleWheel(e: WheelEvent) {
    if (!e.ctrlKey) return;
    e.preventDefault();
    const direction = e.deltaY < 0 ? 1 : -1;
    applyZoom(zoomLevel + direction * ZOOM_STEP);
  }

  let uiStateInitialized = false;
  function saveUiState() {
    if (!uiStateInitialized) return;
    invoke("db_set_ui_state", { key: "openTabs", value: JSON.stringify(openTabs) }).catch(() => {});
    invoke("db_set_ui_state", { key: "activeTabId", value: JSON.stringify(activeTabId) }).catch(() => {});
    invoke("db_set_ui_state", { key: "sidebarCollapsed", value: JSON.stringify(sidebarCollapsed) }).catch(() => {});
  }

  $effect(() => { openTabs; activeTabId; sidebarCollapsed; saveUiState(); });

  onMount(async () => {
    await loadProjects();
    await loadSavedAgents();
    await loadUiState();
    uiStateInitialized = true;

    // Restore zoom level
    try {
      const saved = await invoke<string | null>("db_get_ui_state", { key: "zoomLevel" });
      if (saved) {
        const level = parseFloat(saved);
        if (!isNaN(level)) applyZoom(level);
      }
    } catch {}
  });

  // --- Agent lifecycle ---

  async function createAgent(
    config?: AgentConfig,
    options?: {
      agentId?: string;
      sessionId?: string;
      sourceSessionId?: string;
      parentSessionId?: string;
      reuseExistingSession?: boolean;
    },
  ): Promise<string> {
    counter++;
    const id = options?.agentId || `agent-${Date.now()}-${counter}`;
    const name = config?.shadow?.shadowName || `Agent ${counter}`;
    const cwd = config?.cwd || "/home/miha";
    const sessionId = options?.sessionId || `session-${Date.now()}-${counter}`;
    const agent: Agent = {
      id,
      viewKey: createViewKey(id),
      name,
      status: "running",
      provider: config?.provider,
      model: config?.model,
      thinkingLevel: config?.thinkingLevel || "off",
      cwd,
      stderrLines: [],
      shadow: config?.shadow,
      contextWindow: config?.contextWindow,
      sessionId,
      sessions: [{
        sessionId,
        model: config?.model,
        provider: config?.provider,
        startedAt: new Date().toISOString(),
        messageCount: 0,
      }],
      sourceSessionId: options?.sourceSessionId || undefined,
    };
    agents = [...agents, agent];
    openTab(id);

    // Save agent and session to DB immediately
    try {
      const row: AgentDbRow = {
        id: agent.id,
        name: agent.name,
        shadowName: agent.shadow?.shadowName || null,
        shadowTitle: agent.shadow?.shadowTitle || null,
        shadowGrade: agent.shadow?.shadowGrade || null,
        provider: agent.provider || null,
        model: agent.model || null,
        thinkingLevel: agent.thinkingLevel || null,
        cwd: agent.cwd || null,
        customPrompt: null,
        contextWindow: agent.contextWindow || null,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      };
      await invoke("db_upsert_agent", { agent: row });

      if (!options?.reuseExistingSession) {
        // Create session row — link to parent if restoring from a previous session
        const session: SessionDbRow = {
          id: sessionId,
          agentId: agent.id,
          model: agent.model || null,
          provider: agent.provider || null,
          startedAt: new Date().toISOString(),
          endedAt: null,
          messageCount: 0,
          totalTokens: 0,
          totalCost: 0,
          parentSessionId: options?.parentSessionId || null,
        };
        await invoke("db_create_session", { session });
      }
    } catch (e) {
      console.error("Failed to persist agent:", e);
    }

    commands.spawnAgent({
      id,
      sessionId: sessionId,
      provider: config?.provider || null,
      model: config?.model || null,
      thinkingLevel: config?.thinkingLevel || null,
      cwd: cwd ?? null,
      shadow: config?.shadow ?? null,
      contextWindow: config?.contextWindow ?? null,
    })
      .then(async () => {
        // Refresh projects — spawn_agent may have auto-created one
        await loadProjects();
        // Get the agent's project_id from DB (set by Rust during spawn)
        try {
          const dbAgents = await invoke<AgentDbRow[]>("db_get_agents");
          const dbAgent = dbAgents.find(a => a.id === id);
          if (dbAgent?.projectId) {
            agents = agents.map((a) =>
              a.id === id ? { ...a, projectId: dbAgent.projectId || undefined } : a,
            );
          }
        } catch { /* ignore */ }
        agents = agents.map((a) =>
          a.id === id ? { ...a, status: "running" as const } : a,
        );
      })
      .catch((err) => {
        console.error("Failed to spawn agent:", err);
        const line = formatSpawnError(err);
        agents = agents.map((a) =>
          a.id === id
            ? { ...a, status: "error" as const, stderrLines: [...a.stderrLines, line] }
            : a,
        );
      });

    // Track exit listener for cleanup
    const unlisten = await listen(`agent-exit-${id}`, () => {
      agents = agents.map((a) =>
        a.id === id ? { ...a, status: "stopped" as const } : a,
      );
    });
    exitListeners.set(id, unlisten);

    return id;
  }

  function restartAgent(id: string) {
    const agent = agents.find((a) => a.id === id);
    if (!agent) return;
    killAgent(id);
    createAgent({
      provider: agent.provider,
      model: agent.model,
      thinkingLevel: agent.thinkingLevel,
      cwd: agent.cwd,
      shadow: agent.shadow,
    }, {
      agentId: agent.id,
      sessionId: agent.sessionId,
      sourceSessionId: agent.sessionId,
      reuseExistingSession: true,
    });
  }

  // --- Tab management ---

  function openTab(id: string) {
    if (!openTabs.includes(id)) {
      openTabs = [...openTabs, id];
    }
    activeTabId = id;
  }

  function closeTab(id: string) {
    const idx = openTabs.indexOf(id);
    if (idx === -1) return;
    openTabs = openTabs.filter((t) => t !== id);
    if (activeTabId === id) {
      const nextIdx = Math.min(idx, openTabs.length - 1);
      activeTabId = openTabs[nextIdx] ?? null;
    }
  }

  async function newConversation(agentId: string) {
    const agent = agents.find((a) => a.id === agentId);
    if (!agent) return;
    if (agent.status === "stopped") {
      openTab(agentId);
      return;
    }
    const previousSessionId = agent.sessionId;
    counter++;
    const newSessionId = `session-${Date.now()}-${counter}`;
    try {
      const session: SessionDbRow = {
        id: newSessionId, agentId, model: agent.model || null, provider: agent.provider || null,
        startedAt: new Date().toISOString(), endedAt: null, messageCount: 0,
        totalTokens: 0, totalCost: 0, parentSessionId: previousSessionId || null,
      };
      await invoke("db_create_session", { session });
    } catch (e) { console.error("Failed to create new conversation session:", e); return; }
    try {
      await invoke("switch_agent_session", { agentId, sessionId: newSessionId });
    } catch (e) { console.error("Failed to switch session:", e); return; }
    agents = agents.map((a) =>
      a.id === agentId ? {
        ...a, viewKey: createViewKey(agentId), sessionId: newSessionId,
        sessions: [{ sessionId: newSessionId, model: a.model, provider: a.provider, startedAt: new Date().toISOString(), messageCount: 0 }, ...a.sessions],
      } : a,
    );
    openTab(agentId);
  }

  async function spawnStoppedAgent(id: string): Promise<void> {
    const agent = agents.find((a) => a.id === id);
    if (!agent || agent.status !== "stopped") return;
    const previousSessionId = agent.sessionId;
    counter++;
    const newSessionId = `session-${Date.now()}-${counter}`;
    try {
      const session: SessionDbRow = {
        id: newSessionId, agentId: id, model: agent.model || null, provider: agent.provider || null,
        startedAt: new Date().toISOString(), endedAt: null, messageCount: 0,
        totalTokens: 0, totalCost: 0, parentSessionId: previousSessionId || null,
      };
      await invoke("db_create_session", { session });
    } catch (e) { console.error("Failed to create session for lazy spawn:", e); }
    agents = agents.map((a) =>
      a.id === id ? {
        ...a, status: "running" as const, sessionId: newSessionId, sourceSessionId: previousSessionId,
        sessions: [{ sessionId: newSessionId, model: a.model, provider: a.provider, startedAt: new Date().toISOString(), messageCount: 0 }, ...a.sessions],
      } : a,
    );
    try {
      await commands.spawnAgent({
        id, sessionId: newSessionId, provider: agent.provider || null, model: agent.model || null,
        thinkingLevel: agent.thinkingLevel || null, cwd: agent.cwd || "/home/miha",
        shadow: agent.shadow ?? null,
        contextWindow: agent.contextWindow ?? null,
      });
      await loadProjects();
    } catch (err) {
      console.error("Failed to spawn stopped agent:", err);
      agents = agents.map((a) => a.id === id ? { ...a, status: "error" as const, stderrLines: [...a.stderrLines, String(err)] } : a);
      throw err;
    }
    const unlisten = await listen(`agent-exit-${id}`, () => {
      agents = agents.map((a) => a.id === id ? { ...a, status: "stopped" as const } : a);
    });
    exitListeners.set(id, unlisten);
  }

  function selectAgent(id: string) {
    openTab(id);
  }

  function updateAgent(id: string, updater: (agent: Agent) => Agent) {
    agents = agents.map((agent) => (agent.id === id ? updater(agent) : agent));
  }

  function killAgent(id: string) {
    invoke("kill_agent", { id, graceful: true });
    // Clean up exit listener
    const unlisten = exitListeners.get(id);
    if (unlisten) { unlisten(); exitListeners.delete(id); }
    closeTab(id);
    agents = agents.filter((a) => a.id !== id);
    removeLiveState(id);
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

    // Ctrl+, — toggle settings (always)
    if (e.ctrlKey && e.key === ",") {
      e.preventDefault();
      showSettings = !showSettings;
      return;
    }

    // Ctrl+B — toggle sidebar (always)
    if (e.ctrlKey && e.key === "b") {
      e.preventDefault();
      sidebarCollapsed = !sidebarCollapsed;
      return;
    }

    // Ctrl+= / Ctrl+- / Ctrl+0 — zoom (always)
    if (e.ctrlKey && (e.key === "=" || e.key === "+")) {
      e.preventDefault();
      applyZoom(zoomLevel + ZOOM_STEP);
      return;
    }
    if (e.ctrlKey && e.key === "-") {
      e.preventDefault();
      applyZoom(zoomLevel - ZOOM_STEP);
      return;
    }
    if (e.ctrlKey && e.key === "0") {
      e.preventDefault();
      applyZoom(ZOOM_DEFAULT);
      return;
    }

    // Ctrl+1-9 — switch tabs (always)
    if (e.ctrlKey && e.key >= "1" && e.key <= "9") {
      e.preventDefault();
      const idx = parseInt(e.key) - 1;
      if (idx < openTabs.length) {
        activeTabId = openTabs[idx];
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

  let activeAgent = $derived(agents.find((a) => a.id === activeTabId));
  let activeProject = $derived(
    activeAgent?.projectId
      ? projects.find((p) => p.id === activeAgent!.projectId)
      : undefined
  );

  let currentLive = $derived(
    activeTabId ? liveAgentStore.byAgent.get(activeTabId) ?? null : null,
  );
  let activeCustomPrompt: string | null = $state(null);
  let agentContext: AgentContext = $derived(
    activeAgent && currentLive
      ? {
          agentId: activeAgent.id,
          agent: activeAgent,
          live: currentLive,
          setup: {
            customPrompt: activeCustomPrompt,
            projectInstructions: activeProject?.instructions ?? null,
          },
        }
      : null,
  );
</script>

<svelte:window onkeydown={handleKeydown} onwheel={handleWheel} />

<main class="app">
  <Sidebar
    {agents}
    {projects}
    collapsed={sidebarCollapsed}
    activeId={activeTabId}
    onselect={selectAgent}
    oncreate={() => (showSpawnDialog = true)}
    onkill={killAgent}
    oneditproject={(project) => { editingProject = project; }}
    onsavetemplate={async (source) => {
      const now = new Date().toISOString();
      const template = {
        id: `tpl-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        name: source.name,
        provider: source.provider ?? null,
        model: source.model ?? null,
        thinkingLevel: source.thinkingLevel ?? null,
        cwd: source.cwd ?? null,
        shadowName: source.shadow?.shadowName ?? source.name,
        shadowTitle: source.shadow?.shadowTitle ?? null,
        shadowGrade: source.shadow?.shadowGrade ?? null,
        createdAt: now,
        updatedAt: now,
      };
      try {
        await invoke("db_save_agent_template", { template });
      } catch {}
    }}
  />
  <div class="main-panel">
    <TabBar
      {agents}
      {openTabs}
      {activeTabId}
      onselect={selectAgent}
      onclose={closeTab}
      onnewconversation={newConversation}
    />
    <div class="main-content">
      {#if activeAgent}
        {#key activeAgent.viewKey}
          <AgentView
            agent={activeAgent}
            projectName={activeProject?.name}
            onrestart={restartAgent}
            onspawn={spawnStoppedAgent}
            onagentchange={(agentId, updater) => updateAgent(agentId, updater)}
            onprojectedit={() => { if (activeProject) editingProject = activeProject; }}
            bind:customPrompt={activeCustomPrompt}
            bind:this={agentViewRef}
          />
        {/key}
      {:else}
        <div class="empty-state">
          <span class="empty-icon">&gt;_</span>
          <p>Extract a shadow to begin</p>
          <p class="hint">Ctrl+N extract &middot; Ctrl+B sidebar &middot; Ctrl+1-9 switch</p>
        </div>
      {/if}
    </div>
  </div>
  <ToolPanelStack
    {openToolIds}
    {agentContext}
    width={toolboxWidth}
    onclose={closeTool}
    onresize={(w) => (toolboxWidth = w)}
  />
  <ToolRail {openToolIds} ontoggle={toggleTool} onsettings={() => (showSettings = true)} />
</main>

{#if showSpawnDialog}
  <SpawnDialog
    {projects}
    onspawn={(config) => {
      showSpawnDialog = false;
      createAgent(config);
    }}
    oncancel={() => (showSpawnDialog = false)}
  />
{/if}

{#if editingProject}
  <ProjectEditor
    project={editingProject}
    agents={agents}
    onclose={() => (editingProject = null)}
    onupdate={(updated) => {
      projects = projects.map(p => p.id === updated.id ? updated : p);
      editingProject = updated;
    }}
  />
{/if}

{#if showSettings}
  <SettingsDialog
    onclose={() => (showSettings = false)}
    {zoomLevel}
    onzoom={applyZoom}
  />
{/if}

<style>
  .app {
    display: flex;
    width: 100vw;
    height: 100vh;
    min-width: 0;
    min-height: 100vh;
    overflow: hidden;
  }

  .main-panel {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    position: relative;
    overflow: hidden;
  }

  .main-content {
    flex: 1;
    display: flex;
    min-width: 0;
    min-height: 0;
    position: relative;
    overflow: hidden;
  }

  .empty-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 32px;
    text-align: center;
    color: var(--text-muted);
  }

  .empty-icon {
    font-size: 48px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    line-height: 1;
    color: var(--accent);
  }

  .empty-state p {
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    margin: 0;
    max-width: 32rem;
  }

  .hint {
    margin-top: 8px !important;
    font-size: 11px !important;
    opacity: 0.6;
  }
</style>
