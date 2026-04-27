<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { invoke, listen, type UnlistenFn } from "$lib/api";
  import { commands } from "$lib/bindings";
  import MessageList from "./MessageList.svelte";
  import { classifierStore } from "./classifierStore.svelte";
  import ChatInput, { type PendingImage } from "./ChatInput.svelte";
  import AgentPortrait, { type PortraitCorner } from "./AgentPortrait.svelte";
  import ExtensionDialog from "./ExtensionDialog.svelte";
  import PromptEditor from "./PromptEditor.svelte";
  import HistoryPanel from "./HistoryPanel.svelte";
  import ImageLightbox from "./ImageLightbox.svelte";
  import type { Agent, DisplayItem, ExtensionUIRequest } from "./types";
  import type { LiveAgentState as WireLiveAgentState } from "./bindings";
  import {
    liveAgentStore,
    seedFromSnapshot,
    applyUpdate,
    detachedLiveState,
  } from "./toolbox/liveAgentStore.svelte";
  import type { LiveAgentState } from "./toolbox/types";
  import { agentStore } from "./stores/agentStore.svelte";

  // Dev-only desync indicator. Opt-in via VITE_MONARCH_DEBUG_DESYNC=true.
  // Rationale: MON-14 Phase 2 is the first time the frontend can observe
  // Rust-side state divergence; we want to catch it during dev without any
  // UX cost in prod builds.
  const DEBUG_DESYNC = import.meta.env.VITE_MONARCH_DEBUG_DESYNC === "true";

  let {
    agent,
    projectName,
    customPrompt = $bindable(null),
    onprojectedit,
  }: {
    agent: Agent;
    projectName?: string;
    customPrompt?: string | null;
    onprojectedit?: () => void;
  } = $props();

  let pendingExtensionRequest: ExtensionUIRequest | null = $state(null);
  let showStderr = $state(false);
  // Captured on mount — used when session_ready fires to replay session ancestry
  let pendingSourceSessionId: string | undefined = $state(undefined);
  let showPromptEditor = $state(false);
  let showHistory = $state(false);

  // MON-77: portrait placement + minified preference (persisted via ui_state).
  let portraitCorner = $state<PortraitCorner>("bottom-right");
  let portraitMinified = $state<boolean>(false);

  onMount(async () => {
    try {
      const savedCorner = await invoke<string | null>("db_get_ui_state", { key: "portraitCorner" });
      if (savedCorner === "top-left" || savedCorner === "top-right" || savedCorner === "bottom-left" || savedCorner === "bottom-right") {
        portraitCorner = savedCorner;
      }
      const savedMinified = await invoke<string | null>("db_get_ui_state", { key: "portraitMinified" });
      if (savedMinified === "true") portraitMinified = true;
      else if (savedMinified === "false") portraitMinified = false;
    } catch {}
  });

  async function setPortraitCorner(c: PortraitCorner) {
    portraitCorner = c;
    try {
      await invoke("db_set_ui_state", { key: "portraitCorner", value: c });
    } catch {}
  }

  async function setPortraitMinified(next: boolean) {
    portraitMinified = next;
    try {
      await invoke("db_set_ui_state", { key: "portraitMinified", value: next ? "true" : "false" });
    } catch {}
  }

  let unlistenState: UnlistenFn | undefined;
  let unlistenEvent: UnlistenFn | undefined;
  let unlistenExit: UnlistenFn | undefined;
  let unlistenStderr: UnlistenFn | undefined;
  let scrollContainer: HTMLDivElement | undefined = $state(undefined);
  let chatInputRef: { focus: () => void; addImageFile: (file: File) => void } | undefined = $state(undefined);
  let isDragging = $state(false);
  let boundAgentId = $state("");
  let sessionReadyResolve: (() => void) | null = null;
  let boundSessionId: string | undefined = $state(undefined);
  let activationVersion = 0;
  let lightboxSrc = $state<string | null>(null);

  // Ephemeral map of sent-with-message images, keyed by the user message's
  // 0-based index among user messages in `items`. Cleared whenever we bind a
  // different agent/session or start a new one — image data is not persisted
  // alongside history, so reloaded sessions will show text only.
  let sentImages = $state(new Map<number, PendingImage[]>());

  // Live state — read from liveAgentStore keyed by the bound agent id.
  // The store is a passive receiver of Rust-assembled snapshots from
  // `agent-state-{id}`; all turn assembly happens in Rust (MON-14 Phase 2).
  const DETACHED_LIVE: LiveAgentState = detachedLiveState();
  let live: LiveAgentState = $derived(
    (boundAgentId && liveAgentStore.byAgent.get(boundAgentId)) || DETACHED_LIVE,
  );
  let classifications = $derived(classifierStore.byAgent.get(agent.id)?.ordinalMap);

  $effect(() => {
    classifierStore.ensure(agent.id);
  });

  export function focusInput() {
    chatInputRef?.focus();
  }

  function updateAgent(updater: (agent: Agent) => Agent, agentId: string = agent.id) {
    agentStore.updateAgent(agentId, updater);
  }

  function countPersistedMessages(list: DisplayItem[]): number {
    return list.filter((item) => item.kind === "user" || item.kind === "assistant").length;
  }

  // Stick-to-bottom threshold: treat "within 20px of the bottom" as still
  // at the bottom so tiny scroll drifts from layout changes don't flip the
  // flag. Tracked by the `onscroll` handler on the scroll container.
  const STICK_BOTTOM_PX = 20;
  let isAtBottom = $state(true);

  function updateIsAtBottom() {
    const container = scrollContainer;
    if (!container) return;
    const distance =
      container.scrollHeight - container.scrollTop - container.clientHeight;
    isAtBottom = distance <= STICK_BOTTOM_PX;
  }

  /**
   * Auto-scroll to the bottom of the chat. When `force` is false (the default
   * used by streaming paths), respects the user's scroll position — if they
   * scrolled up to read history, the view stays put. When `force` is true
   * (initial session load, restore paths), jumps to the bottom regardless.
   */
  function scrollToBottom(force = false) {
    const container = scrollContainer;
    if (!container) return;
    if (!force && !isAtBottom) return;

    requestAnimationFrame(() => {
      container.scrollTop = container.scrollHeight;
      isAtBottom = true;
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
        totalCost: s.totalCost,
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
        scrollToBottom(true);
        break;

      case "sidecar_error":
        // MON-51: agentStore holds a parallel listener on `agent-event-{id}`
        // that surfaces sidecar_error as a toast for every agent (including
        // background ones the user isn't currently viewing). This console.error
        // remains as a dev-time diagnostic for the active agent only.
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
    commands
      .respondExtensionUi({
        agentId: agent.id,
        requestId: pendingExtensionRequest.requestId,
        value,
      } as any)
      .catch((e) => console.error("Failed to respond to extension UI:", e));
    pendingExtensionRequest = null;
  }

  function cancelExtensionRequest() {
    if (!pendingExtensionRequest) return;
    commands
      .respondExtensionUi({
        agentId: agent.id,
        requestId: pendingExtensionRequest.requestId,
        value: { cancelled: true },
      } as any)
      .catch(() => {});
    pendingExtensionRequest = null;
  }

  async function sendPiCommand(cmd: Record<string, any>) {
    await invoke("send_command", { id: agent.id, commandJson: JSON.stringify(cmd) });
  }

  async function sendPrompt(message: string, images: PendingImage[] = []) {
    if (agent.status === "stopped") {
      const sessionReady = new Promise<void>((resolve) => { sessionReadyResolve = resolve; });
      await agentStore.spawnStoppedAgent(agent.id);
      await sessionReady;
    }
    // Record images against the upcoming user message *before* dispatching
    // so the MessageList lookup works the moment Rust emits the new state.
    if (images.length > 0) {
      const userIndex = live.items.filter((i) => i.kind === "user").length;
      const next = new Map(sentImages);
      next.set(userIndex, images);
      sentImages = next;
    }
    if (images.length === 0) {
      await sendPiCommand({ type: "prompt", message });
    } else {
      const parts = [
        ...(message ? [{ type: "text", text: message }] : []),
        ...images.map((img) => ({ type: "image", data: img.data, mimeType: img.mimeType })),
      ];
      await sendPiCommand({ type: "prompt", message: parts });
    }
  }

  function handleDragOver(e: DragEvent) {
    if (e.dataTransfer?.types.includes("Files")) {
      e.preventDefault();
      isDragging = true;
    }
  }

  function handleDragLeave(e: DragEvent) {
    // Only clear when leaving the wrapper itself, not a child element.
    const wrapper = (e.currentTarget as HTMLElement);
    if (!wrapper.contains(e.relatedTarget as Node)) {
      isDragging = false;
    }
  }

  function handleDrop(e: DragEvent) {
    e.preventDefault();
    isDragging = false;
    const files = e.dataTransfer?.files;
    if (!files) return;
    for (const file of files) {
      if (file.type.startsWith("image/")) {
        chatInputRef?.addImageFile(file);
      }
    }
  }

  // Convenience accessors used by the template — thin wrappers around `live`.
  let items = $derived(live.items);
  let streamingMessage = $derived(live.streamingMessage);
  let lastUsage = $derived(live.lastUsage);
  let activityStatus = $derived(live.activityStatus);
  let eventCount = $derived(live.eventCount);
  let isStreaming = $derived(live.isStreaming);

  // MON-71: single per-view 1Hz ticker driving all live duration counters
  // (turn header, running tool call, active thinking block). Only ticks
  // while the agent is streaming so idle views don't trigger re-renders.
  let nowMs: number = $state(Date.now());
  $effect(() => {
    if (!isStreaming) return;
    nowMs = Date.now();
    const id = setInterval(() => { nowMs = Date.now(); }, 1000);
    return () => clearInterval(id);
  });

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
    sentImages = new Map();

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
    sentImages = new Map();
    lightboxSrc = null;

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
      updateAgent((current) => ({
        ...current,
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

  // MON-50: refresh the sidebar lifetime-cost counter whenever the active
  // session's total cost ticks. sessionStats.totalCost only changes at turn
  // end (when Rust persists the assistant message), so this coalesces the
  // 16ms snapshot bursts into one refresh per turn — one IPC roundtrip per
  // message_end, no frontend accumulation or divergence risk.
  let lastSeenSessionCost = $state<number | undefined>(undefined);
  $effect(() => {
    const cost = agent.sessionStats?.totalCost;
    if (cost == null) return;
    if (lastSeenSessionCost != null && cost === lastSeenSessionCost) return;
    lastSeenSessionCost = cost;
    void agentStore.refreshLifetimeCost(agent.id);
  });

  onDestroy(() => {
    activationVersion++;
    clearListeners();
  });
</script>

<div
  class="agent-view-wrapper"
  class:dragging={isDragging}
  role="region"
  aria-label="Agent view"
  ondragover={handleDragOver}
  ondragleave={handleDragLeave}
  ondrop={handleDrop}
>
{#if isDragging}
  <div class="drop-overlay">Drop image to attach</div>
{/if}
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
        <button class="restore-btn" onclick={() => agentStore.restartAgent(agent.id)}>
          {agent.status === "error" ? "Retry" : "Restart"}
        </button>
      </div>
    </div>
  {:else}
    <!-- Normal chat view (commands live on the portrait) -->

    <div class="messages-area">
      <div class="messages-scroll" bind:this={scrollContainer} onscroll={updateIsAtBottom}>
        <MessageList
          {items}
          {streamingMessage}
          {nowMs}
          agentName={agent.name}
          {sentImages}
          {classifications}
          onimageclick={(src) => (lightboxSrc = src)}
        />

        {#if agent.status === "stopped" && !isStandby}
          <div class="exit-banner">
            <span>Agent stopped</span>
            <button class="restart-btn" onclick={() => agentStore.restartAgent(agent.id)}>Restart</button>
          </div>
        {/if}
        {#if isStandby}
          <div class="standby-banner">
            <span>Session paused — send a message to wake</span>
          </div>
        {/if}
      </div>

      <div class="portrait-anchor pos-{portraitCorner}">
        <AgentPortrait
          {agent}
          {projectName}
          {isStreaming}
          {items}
          {lastUsage}
          contextWindow={agent.contextWindow}
          thinkingLevel={agent.thinkingLevel}
          provider={agent.provider}
          model={agent.model}
          sessionStats={agent.sessionStats}
          onabort={abort}
          onthinking={setThinkingLevel}
          onprompt={() => (showPromptEditor = true)}
          onhistory={() => (showHistory = true)}
          oncompact={compact}
          onnewsession={newSession}
          {onprojectedit}
          onmove={setPortraitCorner}
          corner={portraitCorner}
          minified={portraitMinified}
          onminify={setPortraitMinified}
          {streamingMessage}
          {nowMs}
        />
      </div>
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
      <ChatInput
        onsend={sendPrompt}
        onabort={abort}
        onthumbclick={(src) => (lightboxSrc = src)}
        streaming={isStreaming}
        cwd={agent.cwd}
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
      sentImages = new Map();
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

{#if lightboxSrc}
  <ImageLightbox src={lightboxSrc} onclose={() => (lightboxSrc = null)} />
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
    background: var(--bg-panel);
    min-width: 0;
    height: 100%;
  }

  .messages-area {
    flex: 1;
    position: relative;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .messages-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 16px 20px;
    scroll-behavior: smooth;
  }

  .portrait-anchor {
    position: absolute;
    z-index: 5;
    pointer-events: none;
  }

  .portrait-anchor.pos-bottom-right { bottom: 12px; right: 12px; }
  .portrait-anchor.pos-bottom-left { bottom: 12px; left: 12px; }
  .portrait-anchor.pos-top-right { top: 12px; right: 12px; }
  .portrait-anchor.pos-top-left { top: 12px; left: 12px; }

  .portrait-anchor > :global(.portrait) {
    pointer-events: auto;
  }

  @media (max-width: 720px) {
    .portrait-anchor {
      display: none;
    }
  }

  .input-area {
    border-top: 1px solid var(--border-subtle);
    padding: 12px 20px;
    background: var(--bg-sidebar);
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
    color: var(--accent);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s;
  }

  .restart-btn:hover {
    background: var(--bg-panel-2);
    border-color: var(--accent);
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
    background: var(--error-bg-faint);
    color: var(--error);
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
    color: var(--text-primary);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .compact-stderr {
    width: 100%;
    max-width: 600px;
    border: 1px solid var(--warning-border-subtle);
    border-radius: 8px;
    overflow: hidden;
  }

  .compact-stderr-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 12px;
    background: var(--warning-bg-subtle);
    border-bottom: 1px solid var(--warning-border-faint);
    font-size: 11px;
    color: var(--warning);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .compact-stderr-content {
    padding: 10px 12px;
    margin: 0;
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    color: var(--warning);
    background: var(--warning-bg-faint);
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
    background: var(--error-bg-subtle);
    border: 1px solid var(--error-border-subtle);
    color: var(--error);
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
    background: var(--accent);
    color: var(--text-on-accent);
    font-size: 12px;
    font-weight: 600;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s;
  }

  .restore-btn:hover {
    background: var(--accent-hover);
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
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    color: var(--text-muted);
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
    background: var(--warning-bg-faint);
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
    background: var(--accent-blue-bg-subtle);
    border-top: 1px solid var(--accent-blue-border-subtle);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    color: var(--accent-blue);
  }

  .activity-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent-blue);
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
    background: var(--error-bg-faint);
    border: 1px solid var(--error-border-faint);
    color: var(--error);
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
    background: var(--accent-bg-subtle);
    border: 1px dashed var(--accent);
    color: var(--text-muted);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    opacity: 0.7;
  }

  @keyframes pulse {
    0%, 100% { opacity: 0.4; }
    50% { opacity: 1; }
  }

  .agent-view-wrapper {
    position: relative;
  }

  .drop-overlay {
    position: absolute;
    inset: 0;
    z-index: 100;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    border: 2px dashed var(--accent);
    border-radius: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--accent);
    font-size: 14px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    font-weight: 600;
    pointer-events: none;
  }
</style>
