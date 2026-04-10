<script lang="ts">
  import { onDestroy } from "svelte";
  import { invoke, listen, type UnlistenFn } from "$lib/api";
  import MessageList from "./MessageList.svelte";
  import ChatInput from "./ChatInput.svelte";
  import AgentControls from "./AgentControls.svelte";
  import AgentHeader from "./AgentHeader.svelte";
  import ExtensionDialog from "./ExtensionDialog.svelte";
  import PromptEditor from "./PromptEditor.svelte";
  import HistoryPanel from "./HistoryPanel.svelte";
  import type { Agent, DisplayItem, ExtensionUIRequest } from "./types";
  import type { LiveAgentState as WireLiveAgentState } from "./bindings";
  import {
    liveAgentStore,
    seedFromSnapshot,
    applyUpdate,
    detachedLiveState,
  } from "./toolbox/liveAgentStore.svelte";
  import type { LiveAgentState } from "./toolbox/types";

  // Dev-only desync indicator. Opt-in via VITE_MONARCH_DEBUG_DESYNC=true.
  // Rationale: MON-14 Phase 2 is the first time the frontend can observe
  // Rust-side state divergence; we want to catch it during dev without any
  // UX cost in prod builds.
  const DEBUG_DESYNC = import.meta.env.VITE_MONARCH_DEBUG_DESYNC === "true";

  let {
    agent,
    projectName,
    customPrompt = $bindable(null),
    onrestart,
    onspawn,
    onagentchange,
    onprojectedit,
  }: {
    agent: Agent;
    projectName?: string;
    customPrompt?: string | null;
    onrestart?: (id: string) => void;
    onspawn?: (agentId: string) => Promise<void>;
    onagentchange?: (agentId: string, updater: (agent: Agent) => Agent) => void;
    onprojectedit?: () => void;
  } = $props();

  let isStreaming = $state(false);
  let pendingExtensionRequest: ExtensionUIRequest | null = $state(null);
  let showStderr = $state(false);
  // Captured on mount — used when session_ready fires to replay session ancestry
  let pendingSourceSessionId: string | undefined = $state(undefined);
  let showPromptEditor = $state(false);
  let showHistory = $state(false);

  let unlistenState: UnlistenFn | undefined;
  let unlistenEvent: UnlistenFn | undefined;
  let unlistenExit: UnlistenFn | undefined;
  let unlistenStderr: UnlistenFn | undefined;
  let scrollContainer: HTMLDivElement | undefined = $state(undefined);
  let chatInputRef: { focus: () => void } | undefined = $state(undefined);
  let boundAgentId = $state("");
  let sessionReadyResolve: (() => void) | null = null;
  let boundSessionId: string | undefined = $state(undefined);
  let activationVersion = 0;

  // Live state — read from liveAgentStore keyed by the bound agent id.
  // The store is a passive receiver of Rust-assembled snapshots from
  // `agent-state-{id}`; all turn assembly happens in Rust (MON-14 Phase 2).
  const DETACHED_LIVE: LiveAgentState = detachedLiveState();
  let live: LiveAgentState = $derived(
    (boundAgentId && liveAgentStore.byAgent.get(boundAgentId)) || DETACHED_LIVE,
  );

  export function focusInput() {
    chatInputRef?.focus();
  }

  function updateAgent(updater: (agent: Agent) => Agent, agentId: string = agent.id) {
    onagentchange?.(agentId, updater);
  }

  function countPersistedMessages(list: DisplayItem[]): number {
    return list.filter((item) => item.kind === "user" || item.kind === "assistant").length;
  }

  function scrollToBottom() {
    const container = scrollContainer;
    if (!container) return;

    requestAnimationFrame(() => {
      container.scrollTop = container.scrollHeight;
    });
  }

  async function refreshSessionsFromDb(agentId: string = agent.id) {
    try {
      const dbSessions = await invoke<any[]>("db_get_sessions", { agentId });
      const sessions = dbSessions.map((s: any) => ({
        sessionId: s.id,
        model: s.model || undefined,
        provider: s.provider || undefined,
        startedAt: s.startedAt,
        messageCount: s.messageCount,
      }));
      updateAgent((current) => {
        const activeSessionId = current.sessionId;
        const activeSession = activeSessionId
          ? dbSessions.find((session: any) => session.id === activeSessionId)
          : undefined;

        return {
          ...current,
          sessions,
          sessionStats: activeSession
            ? {
                totalTokens: activeSession.totalTokens ?? 0,
                totalCost: activeSession.totalCost ?? 0,
                messageCount: activeSession.messageCount ?? 0,
                turnCount: Math.ceil((activeSession.messageCount ?? 0) / 2),
              }
            : current.sessionStats,
        };
      }, agentId);
      return sessions;
    } catch (e) {
      console.error("Failed to refresh sessions from DB:", e);
      return [];
    }
  }

  type NarrowEvent =
    | { type: "session_ready"; agentId: string; contextWindow?: number }
    | { type: "sidecar_error"; error: string }
    | ({ type: "extension_ui_request" } & ExtensionUIRequest);

  /**
   * `agent-event-{id}` is reduced in Phase 2 to out-of-band channels only:
   * session_ready, sidecar_error, and extension_ui_request. Message/tool
   * events are assembled by Rust and arrive on `agent-state-{id}` instead.
   */
  function handleNarrowEvent(raw: string, targetAgentId: string) {
    let event: NarrowEvent;
    try {
      event = JSON.parse(raw);
    } catch {
      return;
    }

    switch (event.type) {
      case "session_ready":
        updateAgent(
          (current) => ({
            ...current,
            contextWindow: event.contextWindow ?? current.contextWindow,
          }),
          targetAgentId,
        );
        if (sessionReadyResolve) {
          sessionReadyResolve();
          sessionReadyResolve = null;
        }
        // If restoring, replay past messages into the sidecar's LLM context.
        // Rust will emit an updated `agent-state-{id}` snapshot once the
        // load_session command lands; no frontend assembly needed.
        if (pendingSourceSessionId) {
          const sourceSessionId = pendingSourceSessionId;
          pendingSourceSessionId = undefined;
          invoke("load_session_context", {
            agentId: targetAgentId,
            sourceSessionId,
          }).catch((e) => {
            console.error("Failed to load session context:", e);
          });
        }
        scrollToBottom();
        break;

      case "sidecar_error":
        // Error pings are still logged to the console; the Rust side does
        // not fold them into LiveAgentState (see agent.rs).
        console.error("[sidecar] error:", event.error);
        break;

      case "extension_ui_request":
        handleExtensionUIRequest(event as unknown as ExtensionUIRequest);
        break;
    }
  }

  function handleExtensionUIRequest(request: ExtensionUIRequest) {
    // Fire-and-forget methods — no response needed
    switch (request.method) {
      case "notify":
      case "setStatus":
        // Informational pings are no longer rendered as notifications in
        // the message list (Rust does not assemble these). Log them so dev
        // surfaces can still observe them.
        if (request.message || request.statusText) {
          console.info("[extension-ui]", request.method, request.message ?? request.statusText);
        }
        return;
      case "setTitle":
        if (request.title) {
          updateAgent((current) => ({ ...current, name: request.title! }));
        }
        return;
      case "setWidget":
      case "set_editor_text":
        // Not yet implemented
        return;
    }

    // Interactive methods — show dialog
    pendingExtensionRequest = request;
  }

  function respondToExtension(value: any) {
    if (!pendingExtensionRequest) return;
    invoke("respond_extension_ui", {
      agentId: agent.id,
      requestId: pendingExtensionRequest.requestId,
      value,
    }).catch((e) => console.error("Failed to respond to extension UI:", e));
    pendingExtensionRequest = null;
  }

  function cancelExtensionRequest() {
    if (!pendingExtensionRequest) return;
    invoke("respond_extension_ui", {
      agentId: agent.id,
      requestId: pendingExtensionRequest.requestId,
      value: { cancelled: true },
    }).catch(() => {});
    pendingExtensionRequest = null;
  }

  async function sendPiCommand(cmd: Record<string, any>) {
    await invoke("send_command", { id: agent.id, commandJson: JSON.stringify(cmd) });
  }

  async function sendPrompt(message: string) {
    if (agent.status === "stopped" && onspawn) {
      const sessionReady = new Promise<void>((resolve) => { sessionReadyResolve = resolve; });
      await onspawn(agent.id);
      await sessionReady;
    }
    await sendPiCommand({ type: "prompt", message });
  }

  // Convenience accessors used by the template — thin wrappers around `live`.
  let items = $derived(live.items);
  let streamingMessage = $derived(live.streamingMessage);
  let lastUsage = $derived(live.lastUsage);
  let activityStatus = $derived(live.activityStatus);
  let eventCount = $derived(live.eventCount);

  async function abort() {
    await sendPiCommand({ type: "abort" });
  }

  function copyStderr() {
    const text = agent.stderrLines.join("\n");
    navigator.clipboard.writeText(text);
  }

  // Show compact error view when agent failed and has no useful messages
  let hasMessages = $derived(items.some((i) => i.kind === "user" || i.kind === "assistant"));
  let isStandby = $derived(agent.status === "stopped" && !!agent.sessionId);
  let showCompactError = $derived(
    (agent.status === "error" || (agent.status === "stopped" && !isStandby)) && !hasMessages
  );

  async function setThinkingLevel(level: string) {
    await sendPiCommand({ type: "set_thinking_level", level });
    updateAgent((current) => ({ ...current, thinkingLevel: level }));
  }

  async function setModel(provider: string, modelId: string) {
    await sendPiCommand({ type: "set_model", provider, modelId });
  }

  async function compact() {
    await sendPiCommand({ type: "compact" });
  }

  async function newSession() {
    const newSessionId = `session-${Date.now()}`;
    try {
      await invoke("new_agent_session", {
        agentId: agent.id,
        newSessionId,
      });
    } catch (e) {
      console.error("Failed to create new session:", e);
      return;
    }

    // Update the current session's message count in history, then add the new session
    let nextSessions = agent.sessions;
    if (agent.sessionId) {
      const msgCount = countPersistedMessages(live.items);
      nextSessions = nextSessions.map(s =>
        s.sessionId === agent.sessionId ? { ...s, messageCount: msgCount } : s
      );
    }

    // Add new session to the front of history
    nextSessions = [{
      sessionId: newSessionId,
      model: agent.model,
      provider: agent.provider,
      startedAt: new Date().toISOString(),
      messageCount: 0,
    }, ...nextSessions];
    updateAgent((current) => ({
      ...current,
      sessionId: newSessionId,
      sessions: nextSessions,
    }));
    boundSessionId = newSessionId;

    // Clear Rust-owned live state and emit a fresh snapshot. Passing
    // sessionId=null resets to an empty state with a single status item.
    try {
      const snapshot = await invoke<WireLiveAgentState>("rebuild_agent_state_from_session", {
        agentId: agent.id,
        sessionId: null,
        statusText: "New session started",
      });
      seedFromSnapshot(agent.id, snapshot);
    } catch (e) {
      console.error("Failed to reset live state:", e);
    }
    refreshSessionsFromDb();
  }

  function resetUiLocalState() {
    isStreaming = false;
    pendingExtensionRequest = null;
    showStderr = false;
    pendingSourceSessionId = undefined;
    showPromptEditor = false;
    showHistory = false;
  }

  function clearListeners() {
    unlistenState?.();
    unlistenEvent?.();
    unlistenExit?.();
    unlistenStderr?.();
    unlistenState = undefined;
    unlistenEvent = undefined;
    unlistenExit = undefined;
    unlistenStderr = undefined;
  }

  async function seedAgentState(target: Agent) {
    // Pull-then-subscribe: ask Rust for the current assembled state and
    // decide whether a DB rebuild is needed.
    let snapshot: WireLiveAgentState | null = null;
    try {
      snapshot = await invoke<WireLiveAgentState | null>("get_agent_state", {
        agentId: target.id,
      });
    } catch (e) {
      console.error("Failed to fetch agent state:", e);
    }

    const hasLiveItems = !!snapshot && snapshot.items.length > 0;

    // Decide whether we need to rebuild from SQLite. Cases:
    //   1. target.sourceSessionId set → restore ancestry for a continued session.
    //   2. existing session with stored messages but no live state → reopen.
    //   3. nothing → seed with an empty "viewing" status snapshot.
    if (target.sourceSessionId) {
      pendingSourceSessionId = target.sourceSessionId;
      try {
        const rebuilt = await invoke<WireLiveAgentState>("rebuild_agent_state_from_session", {
          agentId: target.id,
          sessionId: target.sourceSessionId,
          statusText: "Restored previous session",
        });
        seedFromSnapshot(target.id, rebuilt);
      } catch (e) {
        console.error("Failed to rebuild agent state from source session:", e);
      }
      updateAgent((current) => ({ ...current, sourceSessionId: undefined }), target.id);
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

    // Brand-new agent with no session yet — seed an empty state with a
    // "viewing" status item via the rebuild command (sessionId=null).
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

  async function bindAgent(target: Agent) {
    const version = ++activationVersion;
    boundAgentId = target.id;
    boundSessionId = target.sessionId;
    clearListeners();
    resetUiLocalState();
    isStreaming = target.isStreaming;

    const sessionsPromise = refreshSessionsFromDb(target.id);
    const promptPromise = invoke<string | null>("get_agent_prompt", { agentId: target.id })
      .catch(() => null);

    await seedAgentState(target);
    if (version !== activationVersion) return;

    await sessionsPromise;
    if (version !== activationVersion) return;
    customPrompt = await promptPromise;
    if (version !== activationVersion) return;

    // Pull-then-subscribe: after the initial seed, follow incremental
    // snapshots on `agent-state-{id}`. Rust passes the `LiveAgentState`
    // straight to `Emitter::emit`, so the payload arrives as the already-
    // deserialized object — no inner `JSON.parse` step.
    unlistenState = await listen<WireLiveAgentState>(
      `agent-state-${target.id}`,
      (event) => {
        if (version !== activationVersion) return;
        applyUpdate(target.id, event.payload);
        scrollToBottom();
      },
    );

    // Narrowed agent-event-{id} listener for out-of-band signals only.
    // Message/tool events are fully owned by Rust on `agent-state-{id}`.
    unlistenEvent = await listen<string>(
      `agent-event-${target.id}`,
      (event) => {
        if (version !== activationVersion) return;
        handleNarrowEvent(event.payload, target.id);
      },
    );

    unlistenExit = await listen<number | null>(`agent-exit-${target.id}`, (event) => {
      if (version !== activationVersion) return;
      isStreaming = false;
      updateAgent((current) => ({
        ...current,
        isStreaming: false,
        status: "stopped",
        exitCode: event.payload,
      }), target.id);
      const code = event.payload;
      if (code != null && code !== 0) {
        showStderr = true;
      }
      scrollToBottom();
    });

    unlistenStderr = await listen<string>(`agent-stderr-${target.id}`, (event) => {
      if (version !== activationVersion) return;
      updateAgent((current) => ({
        ...current,
        stderrLines: [...(current.stderrLines || []), event.payload],
      }), target.id);
    });
  }

  $effect(() => {
    if (agent.id === boundAgentId) return;
    void bindAgent(agent);
  });

  onDestroy(() => {
    activationVersion++;
    clearListeners();
  });
</script>

<div class="agent-view-wrapper">
<div class="agent-view">
  {#if showCompactError}
    <!-- Compact error view — no chat, just the error and actions -->
    <div class="compact-error">
      <div class="compact-error-icon">
        {#if agent.status === "error"}!{:else}x{/if}
      </div>
      <h3 class="compact-error-title">
        {#if agent.status === "error"}
          Failed to extract {agent.shadow?.shadowName || agent.name}
        {:else}
          {agent.shadow?.shadowName || agent.name} stopped
        {/if}
      </h3>

      {#if agent.stderrLines?.length}
        <div class="compact-stderr">
          <div class="compact-stderr-header">
            <span>stderr</span>
            <button class="copy-btn" onclick={copyStderr}>copy</button>
          </div>
          <pre class="compact-stderr-content">{agent.stderrLines.join("\n")}</pre>
        </div>
      {/if}

      {#if items.some((i) => i.kind === "notification")}
        <div class="compact-errors">
          {#each items.filter((i) => i.kind === "notification") as item}
            {#if item.kind === "notification"}
              <div class="compact-error-msg">{item.text}</div>
            {/if}
          {/each}
        </div>
      {/if}

      <div class="compact-actions">
        {#if onrestart}
          <button class="restore-btn" onclick={() => onrestart?.(agent.id)}>
            {agent.status === "error" ? "Retry" : "Restart"}
          </button>
        {/if}
      </div>
    </div>
  {:else}
    <!-- Normal chat view -->
    <AgentHeader
      {agent}
      {projectName}
      onprompt={() => (showPromptEditor = true)}
      onhistory={() => (showHistory = true)}
      oncompact={compact}
      onnewsession={newSession}
      {onprojectedit}
    />

    <div class="messages-scroll" bind:this={scrollContainer}>
      <MessageList {items} {streamingMessage} />

      {#if agent.status === "stopped" && !isStandby}
        <div class="exit-banner">
          <span>Agent stopped</span>
          {#if onrestart}
            <button class="restart-btn" onclick={() => onrestart?.(agent.id)}>Restart</button>
          {/if}
        </div>
      {/if}
      {#if isStandby}
        <div class="standby-banner">
          <span>Session paused — send a message to wake</span>
        </div>
      {/if}
    </div>

    {#if agent.stderrLines?.length}
      <div class="stderr-section">
        <div class="stderr-header">
          <button class="stderr-toggle" onclick={() => (showStderr = !showStderr)}>
            {showStderr ? "▾" : "▸"} stderr ({agent.stderrLines.length})
          </button>
          {#if showStderr}
            <button class="copy-btn" onclick={copyStderr}>copy</button>
          {/if}
        </div>
        {#if showStderr}
          <pre class="stderr-content">{agent.stderrLines.join("\n")}</pre>
        {/if}
      </div>
    {/if}

    {#if activityStatus}
      <div class="activity-bar">
        <span class="activity-dot"></span>
        <span class="activity-text">{activityStatus}</span>
        <span class="event-count">{eventCount} events</span>
      </div>
    {/if}

    {#if DEBUG_DESYNC && live.desynced}
      <div class="desync-badge" title="Rust-side state desync detected. See VITE_MONARCH_DEBUG_DESYNC.">
        desynced (v{live.stateVersion})
      </div>
    {/if}

    <div class="input-area">
      <AgentControls
        {isStreaming}
        {items}
        {lastUsage}
        contextWindow={agent.contextWindow}
        thinkingLevel={agent.thinkingLevel}
        model={agent.model}
        sessionStats={agent.sessionStats}
        onabort={abort}
        onthinking={setThinkingLevel}
      />
      <ChatInput
        onsend={sendPrompt}
        disabled={isStreaming}
        bind:this={chatInputRef}
      />
    </div>
  {/if}
</div>

</div>

{#if pendingExtensionRequest}
  <ExtensionDialog
    request={pendingExtensionRequest}
    onrespond={respondToExtension}
    oncancel={cancelExtensionRequest}
  />
{/if}

{#if showHistory}
  <HistoryPanel
    agentId={agent.id}
    sessions={agent.sessions || []}
    currentSessionId={agent.sessionId}
    onload={async (session) => {
      showHistory = false;

      // Switch the live agent to the selected persisted session instead of
      // creating a new continuation row every time.
      try {
        await invoke("switch_agent_session", {
          agentId: agent.id,
          sessionId: session.sessionId,
        });
      } catch (e) {
        console.error("Failed to switch session:", e);
        return;
      }

      // Update current session message count before switching
      let nextSessions = agent.sessions;
      if (agent.sessionId) {
        const msgCount = countPersistedMessages(live.items);
        nextSessions = nextSessions.map(s =>
          s.sessionId === agent.sessionId ? { ...s, messageCount: msgCount } : s
        );
      }

      // Re-focus the selected session in history instead of adding a new row.
      nextSessions = [
        session,
        ...nextSessions.filter((s) => s.sessionId !== session.sessionId),
      ];
      updateAgent((current) => ({
        ...current,
        sessionId: session.sessionId,
        sessions: nextSessions,
      }));
      boundSessionId = session.sessionId;
      await refreshSessionsFromDb();

      // Replay old messages into the sidecar's LLM context
      try {
        await invoke("load_session_context", {
          agentId: agent.id,
          sourceSessionId: session.sessionId,
        });
      } catch (e) {
        console.error("Failed to load session context:", e);
      }

      // Rebuild Rust-owned LiveAgentState from the target session and seed
      // the store. Rust emits a snapshot on `agent-state-{id}` too, but we
      // take the direct return value to avoid an event round-trip.
      try {
        const snapshot = await invoke<WireLiveAgentState>("rebuild_agent_state_from_session", {
          agentId: agent.id,
          sessionId: session.sessionId,
          statusText: "Continuing from previous session",
        });
        seedFromSnapshot(agent.id, snapshot);
      } catch (e) {
        console.error("Failed to rebuild agent state for switched session:", e);
      }
    }}
    onclose={() => (showHistory = false)}
  />
{/if}

{#if showPromptEditor}
  <PromptEditor
    agentId={agent.id}
    shadowName={agent.shadow?.shadowName || agent.name}
    shadowTitle={agent.shadow?.shadowTitle}
    shadowGrade={agent.shadow?.shadowGrade}
    onclose={() => (showPromptEditor = false)}
  />
{/if}

<style>
  .agent-view-wrapper {
    flex: 1;
    display: flex;
    min-width: 0;
    height: 100%;
    overflow: hidden;
  }

  .agent-view {
    flex: 1;
    display: flex;
    flex-direction: column;
    background: var(--bg-panel, #171126);
    min-width: 0;
    height: 100%;
  }

  .messages-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 16px 20px;
    scroll-behavior: smooth;
  }

  .input-area {
    border-top: 1px solid var(--border-subtle);
    padding: 12px 20px;
    background: var(--bg-sidebar, #0c0816);
  }

  .exit-banner {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 12px;
    margin-top: 12px;
    border-radius: 8px;
    background: var(--bg-panel-2);
    border: 1px solid var(--border-subtle);
    color: var(--text-muted);
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }
  .restart-btn {
    padding: 4px 12px;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    background: var(--bg-panel-3);
    color: var(--accent-purple);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s;
  }

  .restart-btn:hover {
    background: var(--bg-panel-2);
    border-color: var(--accent-purple);
  }

  /* --- Compact error view --- */
  .compact-error {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
    padding: 32px;
  }

  .compact-error-icon {
    width: 48px;
    height: 48px;
    border-radius: 50%;
    background: rgba(238, 83, 150, 0.12);
    color: var(--error, #ee5396);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 24px;
    font-weight: 700;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .compact-error-title {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary, #f2f4f8);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .compact-stderr {
    width: 100%;
    max-width: 600px;
    border: 1px solid rgba(255, 233, 123, 0.2);
    border-radius: 8px;
    overflow: hidden;
  }

  .compact-stderr-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 12px;
    background: rgba(255, 233, 123, 0.06);
    border-bottom: 1px solid rgba(255, 233, 123, 0.1);
    font-size: 11px;
    color: var(--warning, #ffe97b);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .compact-stderr-content {
    padding: 10px 12px;
    margin: 0;
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    color: var(--warning, #ffe97b);
    background: rgba(255, 233, 123, 0.03);
    max-height: 200px;
    overflow-y: auto;
    white-space: pre-wrap;
    word-break: break-word;
    user-select: text;
    cursor: text;
  }

  .compact-errors {
    width: 100%;
    max-width: 600px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .compact-error-msg {
    padding: 8px 12px;
    border-radius: 6px;
    background: rgba(238, 83, 150, 0.06);
    border: 1px solid rgba(238, 83, 150, 0.15);
    color: var(--error, #ee5396);
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    user-select: text;
    cursor: text;
  }

  .compact-actions {
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

  /* --- Stderr section (in chat view) --- */
  .stderr-section {
    border-top: 1px solid var(--border-subtle);
    background: var(--bg-panel);
  }

  .stderr-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .stderr-toggle {
    padding: 6px 20px;
    background: none;
    border: none;
    color: var(--warning);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    text-align: left;
  }

  .stderr-toggle:hover {
    background: var(--bg-panel-2);
  }

  .copy-btn {
    padding: 2px 10px;
    background: none;
    border: 1px solid var(--border-subtle, #35274f);
    border-radius: 4px;
    color: var(--text-muted, #8f7aa8);
    font-size: 10px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    margin-right: 12px;
    transition: background 0.15s, color 0.15s;
  }

  .copy-btn:hover {
    background: var(--bg-panel-2);
    color: var(--text-primary);
  }

  .stderr-content {
    padding: 8px 20px;
    margin: 0;
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    color: var(--warning);
    background: rgba(255, 233, 123, 0.04);
    max-height: 200px;
    overflow-y: auto;
    white-space: pre-wrap;
    word-break: break-word;
    user-select: text;
    cursor: text;
  }

  .activity-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 20px;
    background: rgba(51, 177, 255, 0.06);
    border-top: 1px solid rgba(51, 177, 255, 0.15);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    color: var(--accent-blue, #33b1ff);
  }

  .activity-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent-blue, #33b1ff);
    animation: pulse 1s ease-in-out infinite;
    flex-shrink: 0;
  }

  .activity-text {
    flex: 1;
  }

  .event-count {
    color: var(--text-muted);
    font-size: 10px;
  }

  .desync-badge {
    align-self: flex-end;
    margin: 4px 20px;
    padding: 3px 10px;
    border-radius: 999px;
    background: rgba(238, 83, 150, 0.12);
    border: 1px solid rgba(238, 83, 150, 0.4);
    color: var(--error, #ee5396);
    font-size: 10px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    cursor: help;
  }

  .standby-banner {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 10px 12px;
    margin-top: 12px;
    border-radius: 8px;
    background: rgba(190, 149, 255, 0.06);
    border: 1px dashed var(--accent-purple, #be95ff);
    color: var(--text-muted);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    opacity: 0.7;
  }

  @keyframes pulse {
    0%, 100% { opacity: 0.4; }
    50% { opacity: 1; }
  }
</style>
