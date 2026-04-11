/**
 * Module-level store for agent, tab, project, and sidebar state.
 *
 * Replaces the prop-drilled state that used to live in App.svelte.
 * Uses a class so $state properties can be exported and mutated.
 *
 *   import { agentStore } from "$lib/stores/agentStore.svelte";
 *   // Read:  agentStore.agents, agentStore.activeTabId
 *   // Write: agentStore.createAgent(...), agentStore.openTab(...)
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
// Store class
// ---------------------------------------------------------------------------

class AgentStore {
  // --- Core state ---
  agents: Agent[] = $state([]);
  projects: Project[] = $state([]);
  openTabs: string[] = $state([]);
  activeTabId: string | null = $state(null);
  sidebarCollapsed: boolean = $state(false);

  // --- Internal ---
  private counter = 0;
  private exitListeners: Map<string, UnlistenFn> = new Map();
  private uiStateInitialized = false;

  // Tab history (MON-44) — plain array, not $state.
  // Only read imperatively in switchToRecentAgent(). Using $state would
  // create an infinite $effect loop since the effect both reads and writes.
  private tabHistory: string[] = [];

  // --- Derived ---
  readonly activeAgent: Agent | undefined = $derived(
    this.agents.find((a) => a.id === this.activeTabId),
  );

  readonly activeProject: Project | undefined = $derived(
    this.activeAgent?.projectId
      ? this.projects.find((p) => p.id === this.activeAgent!.projectId)
      : undefined,
  );

  constructor() {
    // Track tab history
    $effect(() => {
      if (this.activeTabId && this.openTabs.includes(this.activeTabId)) {
        this.tabHistory = [this.activeTabId, ...this.tabHistory.filter((id) => id !== this.activeTabId)].slice(0, 20);
      }
    });

    // Persist UI state on change
    $effect(() => {
      // Touch reactive deps
      this.openTabs;
      this.activeTabId;
      this.sidebarCollapsed;
      this.saveUiState();
    });
  }

  // -------------------------------------------------------------------------
  // UI state persistence
  // -------------------------------------------------------------------------

  private saveUiState() {
    if (!this.uiStateInitialized) return;
    invoke("db_set_ui_state", { key: "openTabs", value: JSON.stringify(this.openTabs) }).catch(() => {});
    invoke("db_set_ui_state", { key: "activeTabId", value: JSON.stringify(this.activeTabId) }).catch(() => {});
    invoke("db_set_ui_state", { key: "sidebarCollapsed", value: JSON.stringify(this.sidebarCollapsed) }).catch(() => {});
  }

  // -------------------------------------------------------------------------
  // Data loading
  // -------------------------------------------------------------------------

  async loadProjects() {
    try {
      const rows = await invoke<ProjectDbRow[]>("db_get_projects");
      this.projects = rows;
    } catch {
      // No projects yet
    }
  }

  private async loadSavedAgents() {
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
        this.agents = [...this.agents, agent];
      }
      if (this.agents.length > 0) {
        this.openTabs = this.agents.map((a) => a.id);
        if (!this.activeTabId) this.activeTabId = this.agents[0].id;
      }
    } catch {
      // No saved state
    }
  }

  private async loadUiState() {
    try {
      const tabsJson = await invoke<string | null>("db_get_ui_state", { key: "openTabs" });
      const activeJson = await invoke<string | null>("db_get_ui_state", { key: "activeTabId" });
      const collapsedJson = await invoke<string | null>("db_get_ui_state", { key: "sidebarCollapsed" });
      const themeJson = await invoke<string | null>("db_get_ui_state", { key: "theme" });
      if (tabsJson) {
        const savedTabs: string[] = JSON.parse(tabsJson);
        const agentIds = new Set(this.agents.map((a) => a.id));
        this.openTabs = savedTabs.filter((id) => agentIds.has(id));
      }
      if (activeJson) {
        const savedActive = JSON.parse(activeJson);
        if (this.openTabs.includes(savedActive)) this.activeTabId = savedActive;
        else if (this.openTabs.length > 0) this.activeTabId = this.openTabs[0];
      }
      if (collapsedJson) this.sidebarCollapsed = JSON.parse(collapsedJson);
      if (themeJson) {
        applyTheme(JSON.parse(themeJson));
      }
    } catch {}
  }

  // -------------------------------------------------------------------------
  // Initialization (called from App.svelte onMount)
  // -------------------------------------------------------------------------

  async initialize() {
    await this.loadProjects();
    await this.loadSavedAgents();
    await this.loadUiState();
    this.uiStateInitialized = true;
  }

  // -------------------------------------------------------------------------
  // Tab management
  // -------------------------------------------------------------------------

  openTab(id: string) {
    if (!this.openTabs.includes(id)) {
      this.openTabs = [...this.openTabs, id];
    }
    this.activeTabId = id;
  }

  closeTab(id: string) {
    const idx = this.openTabs.indexOf(id);
    if (idx === -1) return;
    this.openTabs = this.openTabs.filter((t) => t !== id);
    if (this.activeTabId === id) {
      const nextIdx = Math.min(idx, this.openTabs.length - 1);
      this.activeTabId = this.openTabs[nextIdx] ?? null;
    }
  }

  selectAgent(id: string) {
    this.openTab(id);
  }

  switchToRecentAgent() {
    const recent = this.tabHistory.find((id) => id !== this.activeTabId && this.openTabs.includes(id));
    if (recent) this.activeTabId = recent;
  }

  switchToNextAgent() {
    if (!this.activeTabId || this.openTabs.length <= 1) return;
    const idx = this.openTabs.indexOf(this.activeTabId);
    const nextIdx = (idx + 1) % this.openTabs.length;
    this.activeTabId = this.openTabs[nextIdx];
  }

  toggleSidebar() {
    this.sidebarCollapsed = !this.sidebarCollapsed;
  }

  // -------------------------------------------------------------------------
  // Agent lifecycle
  // -------------------------------------------------------------------------

  async createAgent(
    config?: AgentConfig,
    options?: {
      agentId?: string;
      sessionId?: string;
      sourceSessionId?: string;
      parentSessionId?: string;
      reuseExistingSession?: boolean;
    },
  ): Promise<string> {
    this.counter++;
    const id = options?.agentId || `agent-${Date.now()}-${this.counter}`;
    const name = config?.shadow?.shadowName || `Agent ${this.counter}`;
    const cwd = config?.cwd || "/home/miha";
    const sessionId = options?.sessionId || `session-${Date.now()}-${this.counter}`;
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
    this.agents = [...this.agents, agent];
    this.openTab(id);

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
        await this.loadProjects();
        try {
          const dbAgents = await invoke<AgentDbRow[]>("db_get_agents");
          const dbAgent = dbAgents.find(a => a.id === id);
          if (dbAgent?.projectId) {
            this.agents = this.agents.map((a) =>
              a.id === id ? { ...a, projectId: dbAgent.projectId || undefined } : a,
            );
          }
        } catch { /* ignore */ }
        this.agents = this.agents.map((a) =>
          a.id === id ? { ...a, status: "running" as const } : a,
        );
      })
      .catch((err) => {
        console.error("Failed to spawn agent:", err);
        const line = formatSpawnError(err);
        this.agents = this.agents.map((a) =>
          a.id === id
            ? { ...a, status: "error" as const, stderrLines: [...a.stderrLines, line] }
            : a,
        );
      });

    const unlisten = await listen(`agent-exit-${id}`, () => {
      this.agents = this.agents.map((a) =>
        a.id === id ? { ...a, status: "stopped" as const } : a,
      );
    });
    this.exitListeners.set(id, unlisten);

    return id;
  }

  restartAgent(id: string) {
    const agent = this.agents.find((a) => a.id === id);
    if (!agent) return;
    this.killAgent(id);
    this.createAgent({
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

  killAgent(id: string) {
    invoke("kill_agent", { id, graceful: true });
    const unlisten = this.exitListeners.get(id);
    if (unlisten) { unlisten(); this.exitListeners.delete(id); }
    this.closeTab(id);
    this.agents = this.agents.filter((a) => a.id !== id);
    removeLiveState(id);
  }

  updateAgent(id: string, updater: (agent: Agent) => Agent) {
    this.agents = this.agents.map((agent) => (agent.id === id ? updater(agent) : agent));
  }

  async newConversation(agentId: string) {
    const agent = this.agents.find((a) => a.id === agentId);
    if (!agent) return;
    if (agent.status === "stopped") {
      this.openTab(agentId);
      return;
    }
    const previousSessionId = agent.sessionId;
    this.counter++;
    const newSessionId = `session-${Date.now()}-${this.counter}`;
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
    this.agents = this.agents.map((a) =>
      a.id === agentId ? {
        ...a, viewKey: createViewKey(agentId), sessionId: newSessionId,
        sessions: [{ sessionId: newSessionId, model: a.model, provider: a.provider, startedAt: new Date().toISOString(), messageCount: 0 }, ...a.sessions],
      } : a,
    );
    this.openTab(agentId);
  }

  async spawnStoppedAgent(id: string): Promise<void> {
    const agent = this.agents.find((a) => a.id === id);
    if (!agent || agent.status !== "stopped") return;
    const previousSessionId = agent.sessionId;
    this.counter++;
    const newSessionId = `session-${Date.now()}-${this.counter}`;
    try {
      const session: SessionDbRow = {
        id: newSessionId, agentId: id, model: agent.model || null, provider: agent.provider || null,
        startedAt: new Date().toISOString(), endedAt: null, messageCount: 0,
        totalTokens: 0, totalCost: 0, parentSessionId: previousSessionId || null,
      };
      await invoke("db_create_session", { session });
    } catch (e) { console.error("Failed to create session for lazy spawn:", e); }
    this.agents = this.agents.map((a) =>
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
      await this.loadProjects();
    } catch (err) {
      console.error("Failed to spawn stopped agent:", err);
      this.agents = this.agents.map((a) => a.id === id ? { ...a, status: "error" as const, stderrLines: [...a.stderrLines, String(err)] } : a);
      throw err;
    }
    const unlisten = await listen(`agent-exit-${id}`, () => {
      this.agents = this.agents.map((a) => a.id === id ? { ...a, status: "stopped" as const } : a);
    });
    this.exitListeners.set(id, unlisten);
  }

  updateProjects(updater: (projects: Project[]) => Project[]) {
    this.projects = updater(this.projects);
  }
}

export const agentStore = new AgentStore();
