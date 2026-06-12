/**
 * Per-workspace live-state binding. Ports the seed + subscribe machinery from
 * the legacy AgentView so the new workspace doesn't reinvent the contract:
 *
 *   - seed: pull `get_agent_state`, rebuild from SQLite when needed (continued
 *     session ancestry / reopened session / brand-new agent).
 *   - subscribe: `agent-state-{id}` snapshots → liveAgentStore; `agent-event`
 *     for session_ready / sidecar_error / extension UI; exit + stderr.
 *   - send / abort: thin wrappers over `send_command`, waking a stopped agent.
 *
 * One instance per mounted SoloWorkspace (keyed by viewKey), so binding follows
 * the active agent/session cleanly.
 */
import { invoke, listen, type UnlistenFn } from "$lib/api";
import { commands, type LiveAgentState as WireLiveAgentState } from "$lib/bindings";
import {
  seedFromSnapshot,
  applyUpdate,
  seedChatFromSnapshot,
  applyChatUpdate,
  chatLiveStore,
} from "$lib/toolbox/liveAgentStore.svelte";
import { agentStore } from "$lib/stores/agentStore.svelte";
import type { Agent, ExtensionUIRequest } from "$lib/types";

export interface PendingImage {
  data: string;
  mimeType: string;
}

type NarrowEvent =
  | { type: "session_ready"; agentId: string; contextWindow?: number }
  | { type: "sidecar_error"; error: string }
  | ({ type: "extension_ui_request" } & ExtensionUIRequest);

export class LiveBinding {
  /** Interactive extension request awaiting a response (rendered by the chat). */
  pendingExtension: ExtensionUIRequest | null = $state(null);

  agentId = "";
  private unlisteners: UnlistenFn[] = [];
  private pendingSourceSessionId: string | undefined;
  private sessionReadyResolve: (() => void) | null = null;
  private version = 0;

  async bind(target: Agent): Promise<void> {
    const version = ++this.version;
    this.agentId = target.id;
    this.clear();
    this.pendingSourceSessionId = undefined;

    await this.seed(target);
    if (version !== this.version) return;

    this.unlisteners.push(
      await listen<WireLiveAgentState>(`agent-state-${target.id}`, (e) => {
        if (version !== this.version) return;
        applyUpdate(target.id, e.payload);
      }),
    );
    // MON-128 (P3): the chat organ's own snapshot channel. Seeded lazily —
    // a missing chat state just means the organ hasn't spoken yet.
    try {
      const chatSnapshot = await invoke<WireLiveAgentState | null>("get_agent_chat_state", {
        agentId: target.id,
      });
      if (chatSnapshot) seedChatFromSnapshot(target.id, chatSnapshot);
    } catch (e) {
      console.error("Failed to fetch chat state:", e);
    }
    if (version !== this.version) return;
    this.unlisteners.push(
      await listen<WireLiveAgentState>(`agent-chat-state-${target.id}`, (e) => {
        if (version !== this.version) return;
        applyChatUpdate(target.id, e.payload);
      }),
    );
    this.unlisteners.push(
      await listen<string>(`agent-event-${target.id}`, (e) => {
        if (version !== this.version) return;
        this.handleEvent(e.payload, target.id);
      }),
    );
    this.unlisteners.push(
      await listen<number | null>(`agent-exit-${target.id}`, (e) => {
        if (version !== this.version) return;
        agentStore.updateAgent(target.id, (a) => ({ ...a, status: "stopped", exitCode: e.payload }));
      }),
    );
    this.unlisteners.push(
      await listen<string>(`agent-stderr-${target.id}`, (e) => {
        if (version !== this.version) return;
        agentStore.updateAgent(target.id, (a) => ({ ...a, stderrLines: [...(a.stderrLines || []), e.payload] }));
      }),
    );
  }

  destroy(): void {
    this.version++;
    this.clear();
  }

  private clear(): void {
    this.unlisteners.forEach((u) => u());
    this.unlisteners = [];
  }

  private async seed(target: Agent): Promise<void> {
    let snapshot: WireLiveAgentState | null = null;
    try {
      snapshot = await invoke<WireLiveAgentState | null>("get_agent_state", { agentId: target.id });
    } catch (e) {
      console.error("Failed to fetch agent state:", e);
    }
    const hasLiveItems = !!snapshot && snapshot.items.length > 0;

    if (target.sourceSessionId) {
      this.pendingSourceSessionId = target.sourceSessionId;
      try {
        const rebuilt = await invoke<WireLiveAgentState>("rebuild_agent_state_from_session", {
          agentId: target.id,
          sessionId: target.sourceSessionId,
          statusText: "Restored previous session",
        });
        seedFromSnapshot(target.id, rebuilt);
      } catch (e) {
        console.error("Failed to rebuild from source session:", e);
      }
      agentStore.updateAgent(target.id, (a) => ({ ...a, sourceSessionId: undefined }));
      return;
    }

    if (!hasLiveItems && target.sessionId) {
      try {
        const rebuilt = await invoke<WireLiveAgentState>("rebuild_agent_state_from_session", {
          agentId: target.id,
          sessionId: target.sessionId,
          statusText: "Reopened current session",
        });
        seedFromSnapshot(target.id, rebuilt);
      } catch (e) {
        console.error("Failed to rebuild agent state:", e);
      }
      return;
    }

    if (snapshot) {
      seedFromSnapshot(target.id, snapshot);
      return;
    }

    try {
      const rebuilt = await invoke<WireLiveAgentState>("rebuild_agent_state_from_session", {
        agentId: target.id,
        sessionId: null,
        statusText: `Viewing ${target.shadow?.shadowName || target.name}`,
      });
      seedFromSnapshot(target.id, rebuilt);
    } catch (e) {
      console.error("Failed to seed empty agent state:", e);
    }
  }

  private handleEvent(raw: string, targetAgentId: string): void {
    let event: NarrowEvent;
    try {
      event = JSON.parse(raw);
    } catch {
      return;
    }
    switch (event.type) {
      case "session_ready":
        agentStore.updateAgent(targetAgentId, (a) => ({
          ...a,
          contextWindow: event.contextWindow ?? a.contextWindow,
        }));
        if (this.sessionReadyResolve) {
          this.sessionReadyResolve();
          this.sessionReadyResolve = null;
        }
        {
          // The bind-time capture misses the wake-after-restart path:
          // spawnStoppedAgent sets sourceSessionId on the store AFTER this
          // binding mounted, so read the live store too — otherwise the new
          // session never replays history and the LLM starts blank while the
          // UI shows the full conversation.
          const fromStore = agentStore.getAgent(targetAgentId)?.sourceSessionId;
          const sourceSessionId = this.pendingSourceSessionId ?? fromStore;
          this.pendingSourceSessionId = undefined;
          if (sourceSessionId) {
            invoke("load_session_context", { agentId: targetAgentId, sourceSessionId }).catch((e) =>
              console.error("Failed to load session context:", e),
            );
            agentStore.updateAgent(targetAgentId, (a) => ({ ...a, sourceSessionId: undefined }));
          }
        }
        break;
      case "sidecar_error":
        console.error("[sidecar] error:", event.error);
        break;
      case "extension_ui_request":
        this.handleExtension(event as unknown as ExtensionUIRequest);
        break;
    }
  }

  private handleExtension(request: ExtensionUIRequest): void {
    switch (request.method) {
      case "notify":
      case "setStatus":
        return;
      case "setTitle":
        if (request.title) agentStore.updateAgent(this.agentId, (a) => ({ ...a, name: request.title! }));
        return;
      case "setWidget":
      case "set_editor_text":
        return;
    }
    this.pendingExtension = request;
  }

  respondExtension(value: unknown): void {
    if (!this.pendingExtension) return;
    commands
      .respondExtensionUi({ agentId: this.agentId, requestId: this.pendingExtension.requestId, value } as any)
      .catch((e) => console.error("Failed to respond to extension UI:", e));
    this.pendingExtension = null;
  }

  cancelExtension(): void {
    if (!this.pendingExtension) return;
    commands
      .respondExtensionUi({ agentId: this.agentId, requestId: this.pendingExtension.requestId, value: { cancelled: true } } as any)
      .catch(() => {});
    this.pendingExtension = null;
  }

  private async sendCommand(target: Agent, cmd: Record<string, unknown>): Promise<void> {
    await invoke("send_command", { id: target.id, commandJson: JSON.stringify(cmd) });
  }

  async sendPrompt(target: Agent, message: string, images: PendingImage[] = []): Promise<void> {
    if (target.status === "stopped") {
      const ready = new Promise<void>((resolve) => { this.sessionReadyResolve = resolve; });
      await agentStore.spawnStoppedAgent(target.id);
      await ready;
    }
    if (images.length === 0) {
      await this.sendCommand(target, { type: "prompt", message });
    } else {
      const parts = [
        ...(message ? [{ type: "text", text: message }] : []),
        ...images.map((img) => ({ type: "image", data: img.data, mimeType: img.mimeType })),
      ];
      await this.sendCommand(target, { type: "prompt", message: parts });
    }
  }

  async abort(target: Agent): Promise<void> {
    await this.sendCommand(target, { type: "abort" });
  }

  /**
   * MON-128 (P3): send a captain message to the chat-shadow organ. Wakes a
   * stopped agent first — the chat organ borrows the executor's create
   * config, so the agent must be live. The reply streams on
   * `agent-chat-state-{id}` into `chatLiveStore`.
   */
  async sendChatPrompt(target: Agent, message: string): Promise<void> {
    if (target.status === "stopped") {
      const ready = new Promise<void>((resolve) => { this.sessionReadyResolve = resolve; });
      await agentStore.spawnStoppedAgent(target.id);
      await ready;
    }
    await invoke("chat_prompt", { agentId: target.id, message });
  }

  /** MON-128: abort the chat organ's in-flight turn (not the executor's). */
  async abortChat(target: Agent): Promise<void> {
    await this.sendCommand(target, { type: "abort", sessionRole: "chat" });
  }

  /** MON-128: drop the cached chat state (e.g. when switching agents). */
  clearChatState(agentId: string): void {
    chatLiveStore.byAgent.delete(agentId);
  }

  async setThinkingLevel(target: Agent, level: string): Promise<void> {
    await this.sendCommand(target, { type: "set_thinking_level", level });
  }

  async setModel(target: Agent, provider: string, modelId: string): Promise<void> {
    await this.sendCommand(target, { type: "set_model", provider, modelId });
  }

  async compact(target: Agent): Promise<void> {
    await this.sendCommand(target, { type: "compact" });
  }
}
