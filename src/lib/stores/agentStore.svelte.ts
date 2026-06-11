/**
 * Shared frontend state for Monarch's agent roster, tabs, and projects.
 *
 * Before MON-47 this lived as local `$state` inside `App.svelte` (~950 lines)
 * and propagated to children via prop-drilling 4 layers deep. This module
 * extracts the shared state + lifecycle functions so children can import
 * what they need directly. `App.svelte` shrinks to a thin shell owning only
 * dialog state, zoom, keybinding routing, and derived per-active-agent
 * context (see `agentContext` consumers in the toolbox).
 *
 * ## Why a class
 *
 * Svelte 5 forbids exporting reassigned `$state` variables from modules —
 * reassignment breaks the reactive proxy reference. Class fields dodge this
 * because all mutations are property writes on a stable instance.
 *
 * ## Why `setupEffects` is separate from the constructor
 *
 * `$effect` requires a component owner. Calling it in the class constructor
 * (which runs at module load, outside any component) silently no-ops.
 * `App.svelte` calls `agentStore.setupEffects()` during its own setup so the
 * persistence and tab-history effects get a real owner.
 *
 * ## Feedback-loop traps preserved from App.svelte
 *
 *   1. `uiStateInitialized` gates the persistence effect so the initial
 *      mount doesn't clobber saved state with defaults before `init()`
 *      finishes.
 *   2. `tabHistory` is a plain array, not `$state`. The effect that
 *      maintains it both reads and writes it; making it reactive causes
 *      an infinite loop.
 *   3. `setSidebarShowAll` is imperative (not an `$effect` on the boolean)
 *      because it calls `loadSavedAgents`, which writes to `openTabs` /
 *      `activeTabId` — a reactive path would cycle.
 */

import { invoke, listen, type UnlistenFn } from "$lib/api";
import { commands } from "../bindings";
import { removeLiveState } from "../toolbox/liveAgentStore.svelte";
import { notificationsStore } from "./notificationsStore.svelte";
import type { Agent, AgentConfig, Project } from "../types";

// --- DB row types mirroring Rust ---------------------------------------

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
  /** MON-73 */
  avatarType?: string | null;
  avatarPath?: string | null;
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

// --- Helpers ------------------------------------------------------------

function createViewKey(agentId: string): string {
  return `${agentId}:${Date.now()}:${Math.random().toString(36).slice(2, 8)}`;
}

/**
 * Build a user-facing stderr line from a spawn_agent rejection. MON-31
 * hands back a `{ kind, message, details }` DTO rather than an opaque
 * string; branch on `kind` so the surface text is recognizable.
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

// --- Store --------------------------------------------------------------

class AgentStore {
  agents: Agent[] = $state([]);
  projects: Project[] = $state([]);
  openTabs: string[] = $state([]);
  activeTabId: string | null = $state(null);
  sidebarCollapsed = $state(false);
  /** MON-66: Active / All toggle for the sidebar. Active hides archived shadows. */
  sidebarShowAll = $state(false);

  // --- Private helpers / counters --------------------------------------

  private counter = 0;
  private exitListeners = new Map<string, UnlistenFn>();
  // Plain array (not $state) — the maintainer effect both reads and writes
  // it; reactivity would cause an infinite loop.
  private tabHistory: string[] = [];
  private uiStateInitialized = false;

  getAgent(id: string): Agent | undefined {
    return this.agents.find((a) => a.id === id);
  }

  // --- Initialization --------------------------------------------------

  /**
   * Hydrate the store from SQLite. Order matters: UI prefs first (so the
   * restored `sidebarShowAll` controls which agents load), then agents,
   * then tab state (validates saved tabs against the visible roster).
   */
  async init(): Promise<void> {
    await this.loadProjects();
    await this.loadUiPrefs();
    await this.loadSavedAgents(this.sidebarShowAll);
    await this.loadTabState();
    this.uiStateInitialized = true;
  }

  /**
   * Register reactive effects that need a component owner. Must be called
   * from inside a component setup (e.g. App.svelte's `<script>`) — calling
   * it in the class constructor at module scope silently no-ops because
   * `$effect` can't attach without an owner.
   */
  setupEffects(): void {
    // Persist UI state after any change — but only after `init()` has
    // finished, to avoid overwriting saved values with defaults.
    $effect(() => {
      // Read dependencies so the effect re-runs on any of them.
      this.openTabs;
      this.activeTabId;
      this.sidebarCollapsed;
      this.sidebarShowAll;
      if (!this.uiStateInitialized) return;
      invoke("db_set_ui_state", { key: "openTabs", value: JSON.stringify(this.openTabs) }).catch(() => {});
      invoke("db_set_ui_state", { key: "activeTabId", value: JSON.stringify(this.activeTabId) }).catch(() => {});
      invoke("db_set_ui_state", { key: "sidebarCollapsed", value: JSON.stringify(this.sidebarCollapsed) }).catch(() => {});
      invoke("db_set_ui_state", { key: "sidebarShowAll", value: JSON.stringify(this.sidebarShowAll) }).catch(() => {});
    });

    // Tab history for recent-agent switching (MON-44). Prepend the
    // currently-active tab, keep the last 20. `tabHistory` is intentionally
    // non-reactive — see the field declaration.
    $effect(() => {
      if (this.activeTabId && this.openTabs.includes(this.activeTabId)) {
        this.tabHistory = [
          this.activeTabId,
          ...this.tabHistory.filter((id) => id !== this.activeTabId),
        ].slice(0, 20);
      }
    });
  }

  private async loadProjects(): Promise<void> {
    try {
      const rows = await invoke<ProjectDbRow[]>("db_get_projects");
      this.projects = rows;
    } catch {
      // No projects yet
    }
  }

  private async loadSavedAgents(includeArchived: boolean = false): Promise<void> {
    try {
      const dbAgents = await invoke<AgentDbRow[]>("db_get_agents", {
        includeArchived,
      });
      const loaded: Agent[] = [];
      for (const row of dbAgents) {
        const sessions = await invoke<SessionDbRow[]>("db_get_sessions", { agentId: row.id });
        const latestSession = sessions[0];
        // MON-50: pull lifetime cost from agent_stats (the authoritative
        // aggregate incremented atomically by Rust on every message_end).
        // Swallow errors so a missing stats row can't block agent hydration.
        let lifetimeCost: number | undefined;
        try {
          const stats = await invoke<{ totalCost: number } | null>(
            "db_get_agent_stats",
            { agentId: row.id },
          );
          lifetimeCost = stats?.totalCost;
        } catch {
          lifetimeCost = undefined;
        }
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
            totalCost: s.totalCost,
          })),
          sourceSessionId: latestSession?.id,
          archivedAt: row.archivedAt || undefined,
          lifetimeCost,
          avatarType: (row.avatarType as "rive" | "image") || undefined,
          avatarPath: row.avatarPath || undefined,
        };
        loaded.push(agent);
      }
      this.agents = loaded;
      // Filter open tabs to the currently loaded set. On initial launch this
      // is a no-op (loadTabState hasn't run yet, openTabs is []); on a toggle
      // flip from All→Active it drops tabs whose agents are now hidden.
      const agentIdSet = new Set(loaded.map((a) => a.id));
      this.openTabs = this.openTabs.filter((id) => agentIdSet.has(id));
      // If the active agent got filtered out (e.g. flipped to Active mode
      // while focused on an archived shadow), fall back to the first visible.
      if (this.activeTabId && !agentIdSet.has(this.activeTabId)) {
        this.activeTabId = loaded.find((a) => !a.archivedAt)?.id ?? null;
      }
      // Default-select the first non-archived agent when nothing else is active,
      // so a fresh start lands on a usable shadow instead of a dismissed one.
      if (!this.activeTabId) {
        this.activeTabId = loaded.find((a) => !a.archivedAt)?.id ?? null;
      }
    } catch {
      // No saved state
    }
  }

  /**
   * Restore cheap UI preferences that don't depend on the agent list. Must
   * run before `loadSavedAgents` so the archive filter reflects the user's
   * last toggle state. Theme is applied here as a side effect because it
   * needs to happen before the first paint.
   */
  private async loadUiPrefs(): Promise<void> {
    try {
      const collapsedJson = await invoke<string | null>("db_get_ui_state", { key: "sidebarCollapsed" });
      const showAllJson = await invoke<string | null>("db_get_ui_state", { key: "sidebarShowAll" });
      if (collapsedJson) this.sidebarCollapsed = JSON.parse(collapsedJson);
      if (showAllJson) this.sidebarShowAll = JSON.parse(showAllJson);
    } catch {}
  }

  /**
   * Restore tab-related UI state. Runs after agents are loaded so we can
   * validate saved tab ids against the currently visible agent roster.
   */
  private async loadTabState(): Promise<void> {
    try {
      const tabsJson = await invoke<string | null>("db_get_ui_state", { key: "openTabs" });
      const activeJson = await invoke<string | null>("db_get_ui_state", { key: "activeTabId" });
      if (tabsJson) {
        const savedTabs: string[] = JSON.parse(tabsJson);
        const agentIds = new Set(this.agents.map((a) => a.id));
        this.openTabs = savedTabs.filter((id) => agentIds.has(id));
      }
      // If no saved tabs (first launch or cleared state), don't open any
      if (activeJson) {
        const savedActive = JSON.parse(activeJson);
        if (this.openTabs.includes(savedActive)) this.activeTabId = savedActive;
        else if (this.openTabs.length > 0) this.activeTabId = this.openTabs[0];
        else this.activeTabId = null;
      }
    } catch {}
  }

  // --- Tab management -------------------------------------------------

  openTab(id: string): void {
    if (!this.openTabs.includes(id)) {
      this.openTabs = [...this.openTabs, id];
    }
    this.activeTabId = id;
  }

  closeTab(id: string): void {
    const idx = this.openTabs.indexOf(id);
    if (idx === -1) return;
    this.openTabs = this.openTabs.filter((t) => t !== id);
    if (this.activeTabId === id) {
      const nextIdx = Math.min(idx, this.openTabs.length - 1);
      this.activeTabId = this.openTabs[nextIdx] ?? null;
    }
  }

  selectAgent(id: string): void {
    this.openTab(id);
  }

  switchToRecentAgent(): void {
    const recent = this.tabHistory.find(
      (id) => id !== this.activeTabId && this.openTabs.includes(id),
    );
    if (recent) this.activeTabId = recent;
  }

  switchToNextAgent(): void {
    if (!this.activeTabId || this.openTabs.length <= 1) return;
    const idx = this.openTabs.indexOf(this.activeTabId);
    const nextIdx = (idx + 1) % this.openTabs.length;
    this.activeTabId = this.openTabs[nextIdx];
  }

  switchToTabIndex(index: number): void {
    if (index >= 0 && index < this.openTabs.length) {
      this.activeTabId = this.openTabs[index];
    }
  }

  // --- Sidebar toggles -------------------------------------------------

  /**
   * Flipped imperatively by the sidebar toggle (MON-66). Updates state +
   * reloads so archived rows appear/disappear. Not driven by an `$effect` —
   * `loadSavedAgents` writes to `activeTabId` / `openTabs` and would create
   * a reactive feedback loop.
   */
  async setSidebarShowAll(next: boolean): Promise<void> {
    if (next === this.sidebarShowAll) return;
    this.sidebarShowAll = next;
    await this.loadSavedAgents(next);
  }

  toggleSidebarCollapsed(): void {
    this.sidebarCollapsed = !this.sidebarCollapsed;
  }

  // --- Agent lifecycle ------------------------------------------------

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
        // Link to parent if restoring from a previous session
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
      sessionId,
      provider: config?.provider || null,
      model: config?.model || null,
      thinkingLevel: config?.thinkingLevel || null,
      cwd: cwd ?? null,
      shadow: config?.shadow ?? null,
      contextWindow: config?.contextWindow ?? null,
    })
      .then(async () => {
        // Refresh projects — spawn_agent may have auto-created one
        await this.loadProjects();
        // Get the agent's project_id from DB (set by Rust during spawn)
        try {
          const dbAgents = await invoke<AgentDbRow[]>("db_get_agents");
          const dbAgent = dbAgents.find((a) => a.id === id);
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
        notificationsStore.add({
          level: "error",
          message: line,
          agentId: id,
          agentName: this.getAgent(id)?.name ?? name,
        });
      });

    await this.registerAgentListeners(id);

    return id;
  }

  restartAgent(id: string): void {
    const agent = this.getAgent(id);
    if (!agent) return;
    this.killAgent(id);
    this.createAgent(
      {
        provider: agent.provider,
        model: agent.model,
        thinkingLevel: agent.thinkingLevel,
        cwd: agent.cwd,
        shadow: agent.shadow,
      },
      {
        agentId: agent.id,
        sessionId: agent.sessionId,
        sourceSessionId: agent.sessionId,
        reuseExistingSession: true,
      },
    );
  }

  async newConversation(agentId: string): Promise<void> {
    const agent = this.getAgent(agentId);
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
        id: newSessionId,
        agentId,
        model: agent.model || null,
        provider: agent.provider || null,
        startedAt: new Date().toISOString(),
        endedAt: null,
        messageCount: 0,
        totalTokens: 0,
        totalCost: 0,
        parentSessionId: previousSessionId || null,
      };
      await invoke("db_create_session", { session });
    } catch (e) {
      console.error("Failed to create new conversation session:", e);
      return;
    }
    try {
      await invoke("switch_agent_session", { agentId, sessionId: newSessionId });
    } catch (e) {
      console.error("Failed to switch session:", e);
      return;
    }
    this.agents = this.agents.map((a) =>
      a.id === agentId
        ? {
            ...a,
            viewKey: createViewKey(agentId),
            sessionId: newSessionId,
            sessions: [
              { sessionId: newSessionId, model: a.model, provider: a.provider, startedAt: new Date().toISOString(), messageCount: 0 },
              ...a.sessions,
            ],
          }
        : a,
    );
    this.openTab(agentId);
  }

  /**
   * Switch the live agent to an existing persisted session (from history).
   * Bumps viewKey so the workspace remounts and re-binds/seeds from that
   * session, and replays its messages into the sidecar's LLM context.
   */
  async switchSession(agentId: string, sessionId: string): Promise<void> {
    const agent = this.getAgent(agentId);
    if (!agent || agent.sessionId === sessionId) return;
    try {
      await invoke("switch_agent_session", { agentId, sessionId });
    } catch (e) {
      console.error("Failed to switch session:", e);
      return;
    }
    this.agents = this.agents.map((a) =>
      a.id === agentId
        ? {
            ...a,
            sessionId,
            sourceSessionId: undefined,
            viewKey: createViewKey(agentId),
            sessions: [
              ...(a.sessions.find((s) => s.sessionId === sessionId)
                ? a.sessions.filter((s) => s.sessionId === sessionId)
                : []),
              ...a.sessions.filter((s) => s.sessionId !== sessionId),
            ],
          }
        : a,
    );
    invoke("load_session_context", { agentId, sourceSessionId: sessionId }).catch((e) =>
      console.error("Failed to load session context:", e),
    );
  }

  async spawnStoppedAgent(id: string): Promise<void> {
    const agent = this.getAgent(id);
    if (!agent || agent.status !== "stopped") return;
    const previousSessionId = agent.sessionId;
    this.counter++;
    const newSessionId = `session-${Date.now()}-${this.counter}`;
    try {
      const session: SessionDbRow = {
        id: newSessionId,
        agentId: id,
        model: agent.model || null,
        provider: agent.provider || null,
        startedAt: new Date().toISOString(),
        endedAt: null,
        messageCount: 0,
        totalTokens: 0,
        totalCost: 0,
        parentSessionId: previousSessionId || null,
      };
      await invoke("db_create_session", { session });
    } catch (e) {
      console.error("Failed to create session for lazy spawn:", e);
    }
    this.agents = this.agents.map((a) =>
      a.id === id
        ? {
            ...a,
            status: "running" as const,
            sessionId: newSessionId,
            sourceSessionId: previousSessionId,
            sessions: [
              { sessionId: newSessionId, model: a.model, provider: a.provider, startedAt: new Date().toISOString(), messageCount: 0 },
              ...a.sessions,
            ],
          }
        : a,
    );
    try {
      await commands.spawnAgent({
        id,
        sessionId: newSessionId,
        provider: agent.provider || null,
        model: agent.model || null,
        thinkingLevel: agent.thinkingLevel || null,
        cwd: agent.cwd || "/home/miha",
        shadow: agent.shadow ?? null,
        contextWindow: agent.contextWindow ?? null,
      });
      await this.loadProjects();
    } catch (err) {
      console.error("Failed to spawn stopped agent:", err);
      const line = formatSpawnError(err);
      this.agents = this.agents.map((a) =>
        a.id === id ? { ...a, status: "error" as const, stderrLines: [...a.stderrLines, line] } : a,
      );
      notificationsStore.add({
        level: "error",
        message: line,
        agentId: id,
        agentName: this.getAgent(id)?.name ?? agent.name,
      });
      throw err;
    }
    await this.registerAgentListeners(id);
  }

  /**
   * MON-51: register the per-agent listeners that feed the notifications
   * surface. Lives here (not in AgentView) so background agents — ones the
   * user isn't currently viewing — still surface errors. Covers:
   *   - `agent-exit-{id}`: flips status and toasts on non-zero exit.
   *   - `agent-event-{id}`: toasts on `sidecar_error`. The event payload is
   *     JSON-encoded per the MON-14 narrowed protocol; malformed payloads
   *     are ignored (AgentView keeps its own listener for the active agent
   *     and will log if it cares).
   * Both teardown closures are folded into a single `exitListeners` entry
   * so `killAgent` stays one-line.
   */
  private async registerAgentListeners(id: string): Promise<void> {
    const unlistenExit = await listen<number | null>(`agent-exit-${id}`, (event) => {
      this.agents = this.agents.map((a) =>
        a.id === id ? { ...a, status: "stopped" as const } : a,
      );
      const code = event.payload;
      if (code != null && code !== 0) {
        notificationsStore.add({
          level: "error",
          message: `Sidecar exited (code ${code})`,
          agentId: id,
          agentName: this.getAgent(id)?.name,
        });
      }
    });

    const unlistenEvent = await listen<string>(`agent-event-${id}`, (event) => {
      let parsed: { type?: string; error?: string };
      try {
        parsed = JSON.parse(event.payload);
      } catch {
        return;
      }
      if (parsed.type === "sidecar_error" && parsed.error) {
        notificationsStore.add({
          level: "error",
          message: parsed.error,
          agentId: id,
          agentName: this.getAgent(id)?.name,
        });
      }
    });

    this.exitListeners.set(id, () => {
      unlistenExit();
      unlistenEvent();
    });
  }

  updateAgent(id: string, updater: (agent: Agent) => Agent): void {
    this.agents = this.agents.map((agent) => (agent.id === id ? updater(agent) : agent));
  }

  /**
   * MON-73: Persist user edits to an agent (name, shadow identity, model,
   * provider, thinking level, cwd, avatar) and reflect them in the in-memory
   * roster immediately.
   */
  async saveAgentEdits(payload: {
    id: string;
    name: string;
    shadowName?: string;
    shadowTitle?: string;
    shadowGrade?: string;
    provider?: string;
    model?: string;
    thinkingLevel?: string;
    cwd?: string;
    avatarType?: "rive" | "image";
    avatarPath?: string;
  }): Promise<void> {
    await invoke("db_update_agent", { payload: {
      id: payload.id,
      name: payload.name,
      shadowName: payload.shadowName ?? null,
      shadowTitle: payload.shadowTitle ?? null,
      shadowGrade: payload.shadowGrade ?? null,
      provider: payload.provider ?? null,
      model: payload.model ?? null,
      thinkingLevel: payload.thinkingLevel ?? null,
      cwd: payload.cwd ?? null,
      avatarType: payload.avatarType ?? null,
      avatarPath: payload.avatarPath ?? null,
    }});
    this.agents = this.agents.map((a) => {
      if (a.id !== payload.id) return a;
      return {
        ...a,
        name: payload.shadowName || payload.name,
        provider: payload.provider || undefined,
        model: payload.model || undefined,
        thinkingLevel: payload.thinkingLevel || undefined,
        cwd: payload.cwd || undefined,
        shadow: payload.shadowName
          ? {
              shadowName: payload.shadowName,
              shadowTitle: payload.shadowTitle || payload.shadowName,
              shadowGrade: (payload.shadowGrade as any) || "Knight",
            }
          : undefined,
        avatarType: payload.avatarType,
        avatarPath: payload.avatarPath,
      };
    });
  }

  /**
   * MON-50: refresh `lifetimeCost` for a single agent from `agent_stats`.
   * Called by `AgentView` when it observes `sessionStats.totalCost` tick,
   * which happens at turn end after Rust persists the message. Errors
   * swallowed — a stale counter is better than a noisy console.
   */
  async refreshLifetimeCost(agentId: string): Promise<void> {
    try {
      const stats = await invoke<{ totalCost: number } | null>(
        "db_get_agent_stats",
        { agentId },
      );
      if (stats) {
        this.updateAgent(agentId, (a) => ({ ...a, lifetimeCost: stats.totalCost }));
      }
    } catch {
      /* ignore */
    }
  }

  /**
   * Kill sidecar runtime + close tab + drop from roster + tear down live
   * state. Used directly for restarts and as the first step of archive/delete.
   * Note: does NOT touch the DB — the agent row persists. Callers that want
   * to remove from DB must follow up with `db_archive_agent` / `db_delete_agent`.
   */
  killAgent(id: string): void {
    invoke("kill_agent", { id, graceful: true });
    const unlisten = this.exitListeners.get(id);
    if (unlisten) {
      unlisten();
      this.exitListeners.delete(id);
    }
    this.closeTab(id);
    this.agents = this.agents.filter((a) => a.id !== id);
    removeLiveState(id);
  }

  /**
   * MON-66 dismiss primitive. Kills the runtime, removes from the active
   * roster, and archives the DB row (preserving history for `Summon back`).
   * The confirm dialog lives in App.svelte; this assumes the user already
   * said yes.
   */
  async archiveAgent(id: string): Promise<void> {
    this.killAgent(id);
    try {
      await invoke("db_archive_agent", { agentId: id });
    } catch (e) {
      console.error("Failed to archive agent:", e);
    }
  }

  /**
   * MON-66 permanent-delete primitive. Kills and purges from the DB — all
   * conversation history, sessions, and stats are gone. Confirm dialog lives
   * in App.svelte.
   */
  async deleteAgent(id: string): Promise<void> {
    this.killAgent(id);
    try {
      await invoke("db_delete_agent", { agentId: id });
    } catch (e) {
      console.error("Failed to delete agent:", e);
    }
  }

  /**
   * MON-66 summon-back. Unarchive without confirm — the operation is
   * trivially reversible (user can dismiss again), unlike delete. Only
   * updates the current in-memory row; the shadow is not respawned (it
   * stays `stopped` until the user explicitly re-engages it).
   */
  async summonAgent(id: string): Promise<void> {
    try {
      await invoke("db_unarchive_agent", { agentId: id });
      this.agents = this.agents.map((a) => (a.id === id ? { ...a, archivedAt: undefined } : a));
    } catch (e) {
      console.error("Failed to summon agent:", e);
    }
  }

  /**
   * Used by ProjectEditor when it updates a project row in-place. The editor
   * knows the updated shape; we just replace it in the list.
   */
  replaceProject(updated: Project): void {
    this.projects = this.projects.map((p) => (p.id === updated.id ? updated : p));
  }
}

export const agentStore = new AgentStore();
