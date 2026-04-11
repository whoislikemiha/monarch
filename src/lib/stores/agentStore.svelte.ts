/**
 * Module-level store for agent, tab, project, and sidebar state.
 *
 * Replaces the prop-drilled state that used to live in App.svelte.
 * Components import what they need directly:
 *
 *   import { agents, activeTabId, createAgent } from "$lib/stores/agentStore.svelte";
 */

import { invoke, listen, type UnlistenFn } from "$lib/api";
import { commands } from "$lib/bindings";
import { removeLiveState } from "$lib/toolbox/liveAgentStore.svelte";
import { applyTheme } from "$lib/themes";
import type { Agent, AgentConfig, Project } from "$lib/types";

// ---------------------------------------------------------------------------
// DB row types (matching Rust)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

export let agents: Agent[] = $state([]);
export let projects: Project[] = $state([]);
export let openTabs: string[] = $state([]);
export let activeTabId: string | null = $state(null);
export let sidebarCollapsed: boolean = $state(false);

let counter = 0;
let exitListeners: Map<string, UnlistenFn> = new Map();
let uiStateInitialized = false;

// ---------------------------------------------------------------------------
// Derived
// ---------------------------------------------------------------------------

export let activeAgent: Agent | undefined = $derived(
  agents.find((a) => a.id === activeTabId),
);

export let activeProject: Project | undefined = $derived(
  activeAgent?.projectId
    ? projects.find((p) => p.id === activeAgent!.projectId)
    : undefined,
);

// ---------------------------------------------------------------------------
// Tab history (MON-44)
// ---------------------------------------------------------------------------
// Plain array — not $state. Only read imperatively in switchToRecentAgent().
// Using $state would create an infinite $effect loop since the effect
// both reads and writes the array.
let tabHistory: string[] = [];

$effect(() => {
  if (activeTabId && openTabs.includes(activeTabId)) {
    tabHistory = [activeTabId, ...tabHistory.filter((id) => id !== activeTabId)].slice(0, 20);
  }
});

// ---------------------------------------------------------------------------
// UI state persistence
// ---------------------------------------------------------------------------

function saveUiState() {
  if (!uiStateInitialized) return;
  invoke("db_set_ui_state", { key: "openTabs", value: JSON.stringify(openTabs) }).catch(() => {});
  invoke("db_set_ui_state", { key: "activeTabId", value: JSON.stringify(activeTabId) }).catch(() => {});
  invoke("db_set_ui_state", { key: "sidebarCollapsed", value: JSON.stringify(sidebarCollapsed) }).catch(() => {});
}

$effect(() => {
  openTabs;
  activeTabId;
  sidebarCollapsed;
  saveUiState();
});

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function createViewKey(agentId: string): string {
  return `${agentId}:${Date.now()}:${Math.random().toString(36).slice(2, 8)}`;
}

/**
 * Build a user-facing stderr line from a spawn_agent rejection (MON-31).
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

// ---------------------------------------------------------------------------
// Data loading
// ---------------------------------------------------------------------------

export async function loadProjects() {
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

// ---------------------------------------------------------------------------
// Initialization (called from App.svelte onMount)
// ---------------------------------------------------------------------------

export async function initialize() {
  await loadProjects();
  await loadSavedAgents();
  await loadUiState();
  uiStateInitialized = true;
}

// ---------------------------------------------------------------------------
// Tab management
// ---------------------------------------------------------------------------

export function setActiveTab(id: string | null) {
  activeTabId = id;
}

export function openTab(id: string) {
  if (!openTabs.includes(id)) {
    openTabs = [...openTabs, id];
  }
  activeTabId = id;
}

export function closeTab(id: string) {
  const idx = openTabs.indexOf(id);
  if (idx === -1) return;
  openTabs = openTabs.filter((t) => t !== id);
  if (activeTabId === id) {
    const nextIdx = Math.min(idx, openTabs.length - 1);
    activeTabId = openTabs[nextIdx] ?? null;
  }
}

export function selectAgent(id: string) {
  openTab(id);
}

export function switchToRecentAgent() {
  const recent = tabHistory.find((id) => id !== activeTabId && openTabs.includes(id));
  if (recent) activeTabId = recent;
}

export function switchToNextAgent() {
  if (!activeTabId || openTabs.length <= 1) return;
  const idx = openTabs.indexOf(activeTabId);
  const nextIdx = (idx + 1) % openTabs.length;
  activeTabId = openTabs[nextIdx];
}

// ---------------------------------------------------------------------------
// Agent lifecycle
// ---------------------------------------------------------------------------

export async function createAgent(
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
      await loadProjects();
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

  const unlisten = await listen(`agent-exit-${id}`, () => {
    agents = agents.map((a) =>
      a.id === id ? { ...a, status: "stopped" as const } : a,
    );
  });
  exitListeners.set(id, unlisten);

  return id;
}

export function restartAgent(id: string) {
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

export function killAgent(id: string) {
  invoke("kill_agent", { id, graceful: true });
  const unlisten = exitListeners.get(id);
  if (unlisten) { unlisten(); exitListeners.delete(id); }
  closeTab(id);
  agents = agents.filter((a) => a.id !== id);
  removeLiveState(id);
}

export function updateAgent(id: string, updater: (agent: Agent) => Agent) {
  agents = agents.map((agent) => (agent.id === id ? updater(agent) : agent));
}

export async function newConversation(agentId: string) {
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

export async function spawnStoppedAgent(id: string): Promise<void> {
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

export function updateProjects(updater: (projects: Project[]) => Project[]) {
  projects = updater(projects);
}

export function toggleSidebar() {
  sidebarCollapsed = !sidebarCollapsed;
}
