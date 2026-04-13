<script lang="ts">
  import { onMount } from "svelte";
  import { invoke, listen } from "$lib/api";
  import { commands } from "./lib/bindings";
  import Sidebar from "./lib/Sidebar.svelte";
  import AgentView from "./lib/AgentView.svelte";
  import SpawnDialog from "./lib/SpawnDialog.svelte";
  import ConfirmDialog from "./lib/ConfirmDialog.svelte";
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
  import { loadKeybindings, matchBinding } from "$lib/keybindings.svelte";

  let agents: Agent[] = $state([]);
  let projects: Project[] = $state([]);
  let openTabs: string[] = $state([]);
  let activeTabId: string | null = $state(null);
  let sidebarCollapsed = $state(false);
  /** MON-66: Active / All toggle for the sidebar. Active hides archived shadows. */
  let sidebarShowAll = $state(false);
  let showSpawnDialog = $state(false);
  let showSettings = $state(false);

  // MON-66: pending confirmation dialogs. Only one is active at a time.
  type PendingConfirm =
    | { kind: "dismiss"; agent: Agent }
    | { kind: "delete"; agent: Agent };
  let pendingConfirm: PendingConfirm | null = $state(null);
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
    archivedAt?: string | null;
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

  async function loadSavedAgents(includeArchived: boolean = false) {
    try {
      const dbAgents = await invoke<AgentDbRow[]>("db_get_agents", {
        includeArchived,
      });
      const loaded: Agent[] = [];
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
          archivedAt: row.archivedAt || undefined,
        };
        loaded.push(agent);
      }
      agents = loaded;
      // Filter open tabs to the currently loaded set. On initial launch this
      // is a no-op (loadTabState hasn't run yet, openTabs is []); on a toggle
      // flip from All→Active it drops tabs whose agents are now hidden.
      const agentIdSet = new Set(loaded.map((a) => a.id));
      openTabs = openTabs.filter((id) => agentIdSet.has(id));
      // If the active agent got filtered out (e.g. flipped to Active mode
      // while focused on an archived shadow), fall back to the first visible.
      if (activeTabId && !agentIdSet.has(activeTabId)) {
        activeTabId = loaded.find((a) => !a.archivedAt)?.id ?? null;
      }
      // Default-select the first non-archived agent when nothing else is active,
      // so a fresh start lands on a usable shadow instead of a dismissed one.
      if (!activeTabId) {
        activeTabId = loaded.find((a) => !a.archivedAt)?.id ?? null;
      }
    } catch {
      // No saved state
    }
  }

  /**
   * Restore cheap UI preferences that don't depend on the agent list. Must
   * run before `loadSavedAgents` so the archive filter reflects the user's
   * last toggle state.
   */
  async function loadUiPrefs() {
    try {
      const collapsedJson = await invoke<string | null>("db_get_ui_state", { key: "sidebarCollapsed" });
      const themeJson = await invoke<string | null>("db_get_ui_state", { key: "theme" });
      const showAllJson = await invoke<string | null>("db_get_ui_state", { key: "sidebarShowAll" });
      if (collapsedJson) sidebarCollapsed = JSON.parse(collapsedJson);
      if (showAllJson) sidebarShowAll = JSON.parse(showAllJson);
      if (themeJson) applyTheme(JSON.parse(themeJson));
    } catch {}
  }

  /**
   * Restore tab-related UI state. Runs after agents are loaded so we can
   * validate saved tab ids against the currently visible agent roster.
   */
  async function loadTabState() {
    try {
      const tabsJson = await invoke<string | null>("db_get_ui_state", { key: "openTabs" });
      const activeJson = await invoke<string | null>("db_get_ui_state", { key: "activeTabId" });
      if (tabsJson) {
        const savedTabs: string[] = JSON.parse(tabsJson);
        const agentIds = new Set(agents.map((a) => a.id));
        openTabs = savedTabs.filter((id) => agentIds.has(id));
      }
      // If no saved tabs (first launch or cleared state), don't open any
      if (activeJson) {
        const savedActive = JSON.parse(activeJson);
        if (openTabs.includes(savedActive)) activeTabId = savedActive;
        else if (openTabs.length > 0) activeTabId = openTabs[0];
        else activeTabId = null;
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

  // --- Tab history for recent-agent switching (MON-44) ---
  // Plain array (not $state) — only read imperatively in switchToRecentAgent().
  // Using $state here would create an infinite $effect loop since the effect
  // both reads and writes the array.
  let tabHistory: string[] = [];

  $effect(() => {
    if (activeTabId && openTabs.includes(activeTabId)) {
      tabHistory = [activeTabId, ...tabHistory.filter((id) => id !== activeTabId)].slice(0, 20);
    }
  });

  function switchToRecentAgent() {
    const recent = tabHistory.find((id) => id !== activeTabId && openTabs.includes(id));
    if (recent) activeTabId = recent;
  }

  function switchToNextAgent() {
    if (!activeTabId || openTabs.length <= 1) return;
    const idx = openTabs.indexOf(activeTabId);
    const nextIdx = (idx + 1) % openTabs.length;
    activeTabId = openTabs[nextIdx];
  }

  let uiStateInitialized = false;
  function saveUiState() {
    if (!uiStateInitialized) return;
    invoke("db_set_ui_state", { key: "openTabs", value: JSON.stringify(openTabs) }).catch(() => {});
    invoke("db_set_ui_state", { key: "activeTabId", value: JSON.stringify(activeTabId) }).catch(() => {});
    invoke("db_set_ui_state", { key: "sidebarCollapsed", value: JSON.stringify(sidebarCollapsed) }).catch(() => {});
    invoke("db_set_ui_state", { key: "sidebarShowAll", value: JSON.stringify(sidebarShowAll) }).catch(() => {});
  }

  $effect(() => { openTabs; activeTabId; sidebarCollapsed; sidebarShowAll; saveUiState(); });

  // MON-66: flipped imperatively by the sidebar toggle. Updates state + reloads
  // so archived rows appear/disappear. Not driven by an $effect — loadSavedAgents
  // writes to activeTabId/openTabs and would create a reactive feedback loop.
  async function setSidebarShowAll(next: boolean) {
    if (next === sidebarShowAll) return;
    sidebarShowAll = next;
    await loadSavedAgents(next);
  }

  onMount(async () => {
    await loadProjects();
    await loadUiPrefs();                  // restore sidebarShowAll first
    await loadSavedAgents(sidebarShowAll); // respect the restored filter
    await loadTabState();                 // then validate tabs against agents
    await loadKeybindings();
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

  // MON-66: the sidebar X button flows through here. Opens the confirm dialog
  // and waits for the user; the actual dismiss (kill + archive) runs in
  // `confirmPending` once the user accepts.
  function requestDismiss(id: string) {
    const agent = agents.find((a) => a.id === id);
    if (!agent) return;
    pendingConfirm = { kind: "dismiss", agent };
  }

  // MON-66: right-click → "Delete permanently" flows through here. Separate
  // dialog, irreversible wording, calls db_delete_agent on confirm.
  function requestDelete(id: string) {
    const agent = agents.find((a) => a.id === id);
    if (!agent) return;
    pendingConfirm = { kind: "delete", agent };
  }

  // MON-66: right-click → "Summon back" on an archived shadow. No confirm —
  // unarchive is trivially reversible (user can dismiss again), unlike delete.
  async function summonAgent(id: string) {
    try {
      await invoke("db_unarchive_agent", { agentId: id });
      agents = agents.map((a) => (a.id === id ? { ...a, archivedAt: undefined } : a));
    } catch (e) {
      console.error("Failed to summon agent:", e);
    }
  }

  async function confirmPending() {
    const p = pendingConfirm;
    if (!p) return;
    pendingConfirm = null;
    const id = p.agent.id;
    if (p.kind === "dismiss") {
      killAgent(id);
      try {
        await invoke("db_archive_agent", { agentId: id });
      } catch (e) {
        console.error("Failed to archive agent:", e);
      }
    } else if (p.kind === "delete") {
      killAgent(id);
      try {
        await invoke("db_delete_agent", { agentId: id });
      } catch (e) {
        console.error("Failed to delete agent:", e);
      }
    }
  }

  function cancelPending() {
    pendingConfirm = null;
  }

  function handleKeydown(e: KeyboardEvent) {
    const target = e.target as HTMLElement;
    const inInput = target.tagName === "TEXTAREA" || target.tagName === "INPUT" || target.tagName === "SELECT";
    const inDialog = target.closest("[role=dialog]") !== null;

    // --- Always active (even in inputs/dialogs) ---

    if (matchBinding(e, "global.spawn-agent")) {
      e.preventDefault();
      showSpawnDialog = true;
      return;
    }

    if (matchBinding(e, "global.settings")) {
      e.preventDefault();
      showSettings = !showSettings;
      return;
    }

    if (matchBinding(e, "global.toggle-sidebar")) {
      e.preventDefault();
      sidebarCollapsed = !sidebarCollapsed;
      return;
    }

    // Zoom (non-editable, kept as direct checks for = / + ambiguity)
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

    // Tab switching (Ctrl+1-9)
    for (let i = 1; i <= 9; i++) {
      if (matchBinding(e, `nav.tab-${i}`)) {
        e.preventDefault();
        if (i - 1 < openTabs.length) {
          activeTabId = openTabs[i - 1];
        }
        return;
      }
    }

    // Recent agent (Ctrl+Tab)
    if (matchBinding(e, "nav.recent-agent")) {
      e.preventDefault();
      switchToRecentAgent();
      return;
    }

    // Next agent (Ctrl+PageDown)
    if (matchBinding(e, "nav.next-agent")) {
      e.preventDefault();
      switchToNextAgent();
      return;
    }

    // --- Only when not in input/dialog ---
    if (inInput || inDialog) return;

    if (matchBinding(e, "global.focus-chat") || matchBinding(e, "global.focus-chat-alt")) {
      e.preventDefault();
      agentViewRef?.focusInput();
      return;
    }

    // Escape — unfocus (not in registry, universal behavior)
    if (e.key === "Escape") {
      (document.activeElement as HTMLElement)?.blur();
      return;
    }

    // Abort agent (Ctrl+C when no text selected)
    if (matchBinding(e, "global.abort-agent")) {
      const selection = window.getSelection();
      if (selection && selection.toString().length > 0) return;
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
    showAll={sidebarShowAll}
    onselect={selectAgent}
    oncreate={() => (showSpawnDialog = true)}
    ondismiss={requestDismiss}
    ondelete={requestDelete}
    onsummon={summonAgent}
    ontoggleshowall={setSidebarShowAll}
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

<ConfirmDialog
  open={pendingConfirm?.kind === "dismiss"}
  title="Dismiss {pendingConfirm?.agent.name}?"
  message="The shadow will be removed from the active roster. Conversation history, sessions, and shadow identity are preserved — you can summon them back later from the All view."
  confirmLabel="Dismiss"
  cancelLabel="Cancel"
  onconfirm={confirmPending}
  oncancel={cancelPending}
/>

<ConfirmDialog
  open={pendingConfirm?.kind === "delete"}
  title="Permanently delete {pendingConfirm?.agent.name}?"
  message="This is irreversible. All conversation history, sessions, and stats for this shadow will be deleted. This cannot be undone."
  confirmLabel="Delete permanently"
  cancelLabel="Cancel"
  danger
  onconfirm={confirmPending}
  oncancel={cancelPending}
/>

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
