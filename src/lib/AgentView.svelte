<script lang="ts">
  import { onDestroy } from "svelte";
  import { invoke, listen, type UnlistenFn } from "$lib/api";
  import MessageList from "./MessageList.svelte";
  import ChatInput from "./ChatInput.svelte";
  import AgentControls from "./AgentControls.svelte";
  import AgentHeader from "./AgentHeader.svelte";
  import ContextInspector from "./ContextInspector.svelte";
  import ExtensionDialog from "./ExtensionDialog.svelte";
  import PromptEditor from "./PromptEditor.svelte";
  import HistoryPanel from "./HistoryPanel.svelte";
  import type {
    Agent,
    AgentViewState,
    PiEvent,
    DisplayItem,
    ToolExecution,
    AssistantMessage,
    ExtensionUIRequest,
    ContentBlock,
  } from "./types";

  let {
    agent,
    projectName,
    projectInstructions,
    onrestart,
    onagentchange,
    getcachedstate,
    onviewstatechange,
    onprojectedit,
  }: {
    agent: Agent;
    projectName?: string;
    projectInstructions?: string | null;
    onrestart?: (id: string) => void;
    onagentchange?: (agentId: string, updater: (agent: Agent) => Agent) => void;
    getcachedstate?: (agentId: string) => AgentViewState | undefined;
    onviewstatechange?: (agentId: string, state: AgentViewState | null) => void;
    onprojectedit?: () => void;
  } = $props();

  let items: DisplayItem[] = $state([]);
  let toolExecutions: Map<string, ToolExecution> = $state(new Map());
  let streamingMessage: AssistantMessage | null = $state(null);
  let isStreaming = $state(false);
  let lastUsage: import("./types").Usage | undefined = $state(undefined);
  let pendingExtensionRequest: ExtensionUIRequest | null = $state(null);
  let customPrompt: string | null = $state(null);
  let showStderr = $state(false);
  // Captured on mount — used when session_ready fires to replay session ancestry
  let pendingSourceSessionId: string | undefined = $state(undefined);
  let showPromptEditor = $state(false);
  let showHistory = $state(false);
  let showContextInspector = $state(false);
  let currentToolGroup: { kind: "tool-group"; executions: ToolExecution[]; turnComplete: boolean } | null = $state(null);

  let unlistenEvent: UnlistenFn | undefined;
  let unlistenExit: UnlistenFn | undefined;
  let unlistenStderr: UnlistenFn | undefined;
  let scrollContainer: HTMLDivElement | undefined = $state(undefined);
  let chatInputRef: { focus: () => void } | undefined = $state(undefined);
  let boundAgentId = $state("");
  let boundSessionId: string | undefined = $state(undefined);
  let activationVersion = 0;

  export function focusInput() {
    chatInputRef?.focus();
  }

  function updateAgent(updater: (agent: Agent) => Agent, agentId: string = agent.id) {
    onagentchange?.(agentId, updater);
  }

  function countPersistedMessages(list: DisplayItem[]): number {
    return list.filter((item) => item.kind === "user" || item.kind === "assistant").length;
  }

  function snapshotViewState(sessionId: string | undefined = boundSessionId): AgentViewState {
    return {
      sessionId,
      messageCount: countPersistedMessages(items),
      items: [...items],
      toolExecutions: [...toolExecutions.values()],
      streamingMessage,
      wasStreaming: isStreaming,
      lastUsage,
      showStderr,
      activityStatus,
      eventCount,
      currentToolGroup: currentToolGroup
        ? {
            kind: "tool-group",
            executions: [...currentToolGroup.executions],
            turnComplete: currentToolGroup.turnComplete,
          }
        : null,
    };
  }

  function persistCurrentViewState(
    agentId: string | undefined = boundAgentId,
    sessionId: string | undefined = boundSessionId,
  ) {
    if (!agentId) return;
    onviewstatechange?.(agentId, snapshotViewState(sessionId));
  }

  // Restores items + tool state from a cached snapshot. Intentionally does NOT restore
  // `isStreaming` — that is authoritative from the live agent (target.isStreaming), not
  // the cache, which could be stale if the agent finished streaming in the background.
  function restoreCachedViewState(state: AgentViewState) {
    items = [...state.items];
    toolExecutions = new Map(state.toolExecutions.map((execution) => [execution.toolCallId, execution]));
    streamingMessage = state.streamingMessage;
    lastUsage = state.lastUsage;
    showStderr = state.showStderr;
    activityStatus = state.activityStatus;
    eventCount = state.eventCount;
    currentToolGroup = state.currentToolGroup
      ? {
          kind: "tool-group",
          executions: [...state.currentToolGroup.executions],
          turnComplete: state.currentToolGroup.turnComplete,
        }
      : null;
  }

  function scrollToBottom() {
    const container = scrollContainer;
    if (!container) return;

    requestAnimationFrame(() => {
      container.scrollTop = container.scrollHeight;
    });
  }

  // Live activity status
  let activityStatus = $state("");
  let eventCount = $state(0);

  function setActivity(text: string) {
    activityStatus = text;
  }

  function tryParseStoredContent(content: unknown): unknown {
    if (typeof content !== "string") return content;

    const trimmed = content.trim();
    if (!trimmed) return content;

    const looksSerialized = trimmed.startsWith("[") || trimmed.startsWith("{") || trimmed.startsWith("\"");
    if (!looksSerialized) return content;

    try {
      return JSON.parse(content);
    } catch {
      return content;
    }
  }

  function normalizeUserContent(content: unknown): string {
    const resolved = tryParseStoredContent(content);

    if (typeof resolved === "string") return resolved;
    if (Array.isArray(resolved)) {
      return resolved
        .filter((block): block is { type: "text"; text: string } => !!block && typeof block === "object" && "type" in block && (block as any).type === "text")
        .map((block) => block.text)
        .join("");
    }
    return "";
  }

  function normalizeAssistantContent(content: unknown): ContentBlock[] {
    const resolved = tryParseStoredContent(content);

    if (Array.isArray(resolved)) return resolved as ContentBlock[];
    if (typeof resolved === "string") {
      return [{ type: "text", text: resolved }];
    }
    return [];
  }

  function parseStoredToolResult(content: unknown, fallbackId: string): ToolExecution | null {
    const resolved = tryParseStoredContent(content);

    if (!resolved || Array.isArray(resolved) || typeof resolved !== "object") return null;

    const payload = resolved as {
      toolCallId?: string;
      toolName?: string;
      result?: unknown;
      isError?: boolean;
    };

    if (!payload.toolCallId && !payload.toolName && payload.result == null) return null;

    return {
      toolCallId: payload.toolCallId || fallbackId,
      toolName: payload.toolName || "tool",
      args: undefined,
      result: tryParseStoredContent(payload.result),
      isError: !!payload.isError,
      status: payload.isError ? "error" : "done",
    };
  }

  function hasVisibleAssistantContent(blocks: ContentBlock[]): boolean {
    return blocks.some((block) => {
      if (block.type === "text") return block.text.trim().length > 0;
      if (block.type === "thinking") return block.thinking.trim().length > 0;
      if (block.type === "image") return true;
      return false;
    });
  }

  function restoreDisplayItemsFromMessages(messages: any[], statusText: string): DisplayItem[] {
    const restored: DisplayItem[] = [];
    let pendingToolResults: ToolExecution[] = [];

    function flushPendingToolResults() {
      if (pendingToolResults.length === 0) return;
      restored.push({
        kind: "tool-group",
        executions: pendingToolResults,
        turnComplete: true,
      });
      pendingToolResults = [];
    }

    for (const [index, msg] of messages.entries()) {
      if (msg.role === "user") {
        flushPendingToolResults();
        restored.push({
          kind: "user",
          content: normalizeUserContent(msg.content ?? msg),
          timestamp: msg.timestamp,
        });
      } else if (msg.role === "toolResult") {
        const parsed = parseStoredToolResult(msg.content ?? msg, `restored-tool-${index}`);
        if (parsed) {
          pendingToolResults = [...pendingToolResults, parsed];
        }
      } else if (msg.role === "assistant") {
        const content = normalizeAssistantContent(msg.content ?? msg);
        const toolCalls = content.filter((block): block is Extract<ContentBlock, { type: "toolCall" }> => block.type === "toolCall");

        if (toolCalls.length > 0 || pendingToolResults.length > 0) {
          const pendingById = new Map(
            pendingToolResults.map((execution) => [execution.toolCallId, execution]),
          );
          const mergedExecutions: ToolExecution[] = toolCalls.map((block) => {
            const result = pendingById.get(block.id);
            return {
              toolCallId: block.id,
              toolName: block.name,
              args: block.arguments,
              result: result?.result,
              isError: result?.isError,
              status: result?.status ?? "done",
            };
          });

          const handledIds = new Set(toolCalls.map((block) => block.id));
          for (const execution of pendingToolResults) {
            if (!handledIds.has(execution.toolCallId)) {
              mergedExecutions.push(execution);
            }
          }

          if (mergedExecutions.length > 0) {
            restored.push({
              kind: "tool-group",
              executions: mergedExecutions,
              turnComplete: true,
            });
          }

          pendingToolResults = [];
        }

        if (!hasVisibleAssistantContent(content)) {
          continue;
        }

        restored.push({
          kind: "assistant",
          content,
          model: msg.model,
          timestamp: msg.timestamp,
        });
      }
    }

    flushPendingToolResults();

    if (restored.length === 0) {
      return [{ kind: "status", text: `${statusText} (no stored messages)` }];
    }

    return [{ kind: "status", text: statusText }, ...restored];
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

  async function fetchStoredMessages(sessionId: string) {
    let lastError: unknown;

    try {
      const ancestry = await invoke<any[]>("db_get_messages_with_ancestry", { sessionId });
      if (ancestry.length > 0) return ancestry;
    } catch (e) {
      lastError = e;
    }

    try {
      const direct = await invoke<any[]>("db_get_messages", { sessionId });
      if (direct.length > 0) return direct;
    } catch (e) {
      lastError = e;
    }

    if (lastError) throw lastError;
    return [];
  }

  async function loadSessionMessages(sessionId: string, statusText: string) {
    const dbMessages = await fetchStoredMessages(sessionId);

    if (dbMessages.length > 0) {
      items = restoreDisplayItemsFromMessages(
        dbMessages.map((msg) => ({
          role: msg.role,
          content: msg.content,
          model: msg.model,
        })),
        statusText,
      );
    } else {
      const knownSession = agent.sessions.find((s) => s.sessionId === sessionId);
      if ((knownSession?.messageCount || 0) > 0) {
        items = [...items, { kind: "notification", text: `Session ${sessionId} has persisted messages but none could be loaded`, level: "error" }];
      }
      items = [{ kind: "status", text: `${statusText} (no stored messages)` }];
    }

    toolExecutions = new Map();
    streamingMessage = null;
    lastUsage = undefined;
    currentToolGroup = null;
    pendingExtensionRequest = null;
    customPrompt = null;
    showStderr = false;
    activityStatus = "";
    eventCount = 0;
    scrollToBottom();
  }

  function handleEvent(raw: string, targetAgentId: string) {
    let event: PiEvent;
    try {
      event = JSON.parse(raw);
    } catch {
      return;
    }

    eventCount++;

    // DB writes are now handled by Rust on the event handler thread

    switch (event.type) {
      case "session_ready":
        updateAgent(
          (current) => ({
            ...current,
            contextWindow: event.contextWindow ?? current.contextWindow,
          }),
          targetAgentId,
        );
        setActivity("Shadow ready");
        items = [...items, { kind: "status", text: "Session ready" }];
        // If restoring, replay past messages into the sidecar's LLM context
        if (pendingSourceSessionId) {
          const sourceSessionId = pendingSourceSessionId;
          pendingSourceSessionId = undefined;
          invoke("load_session_context", {
            agentId: targetAgentId,
            sourceSessionId,
          }).then(() => {
            items = [...items, { kind: "status", text: "Context restored from previous session" }];
            scrollToBottom();
          }).catch((e) => {
            console.error("Failed to load session context:", e);
          });
        }
        scrollToBottom();
        break;
      case "agent_start":
        isStreaming = true;
        updateAgent((current) => ({ ...current, isStreaming: true }), targetAgentId);
        setActivity("Agent processing...");
        items = [...items, { kind: "status", text: "Agent started" }];
        scrollToBottom();
        break;

      case "agent_end":
        isStreaming = false;
        updateAgent((current) => ({ ...current, isStreaming: false }), targetAgentId);
        setActivity("");
        if (streamingMessage) {
          items = [
            ...items,
            {
              kind: "assistant",
              content: streamingMessage.content,
              usage: streamingMessage.usage,
              model: streamingMessage.model,
              timestamp: streamingMessage.timestamp,
            },
          ];
          streamingMessage = null;
        }
        items = [...items, { kind: "status", text: "Agent finished" }];
        refreshSessionsFromDb(targetAgentId);
        scrollToBottom();
        break;

      case "turn_start":
        setActivity("LLM call in progress...");
        // Start a new tool group for this turn
        currentToolGroup = null;
        break;

      case "turn_end":
        setActivity("Processing response...");
        // Mark current tool group as complete
        if (currentToolGroup) {
          currentToolGroup.turnComplete = true;
          items = [...items];
        }
        currentToolGroup = null;
        break;

      case "message_start":
        if (event.message.role === "user") {
          const content =
            typeof event.message.content === "string"
              ? event.message.content
              : event.message.content
                  .filter((b): b is { type: "text"; text: string } => b.type === "text")
                  .map((b) => b.text)
                  .join("");
          items = [
            ...items,
            {
              kind: "user",
              content,
              timestamp: event.message.timestamp,
            },
          ];
        } else if (event.message.role === "assistant") {
          streamingMessage = event.message as AssistantMessage;
          setActivity("Receiving response...");
        }
        scrollToBottom();
        break;

      case "message_update":
        if (event.message.role === "assistant") {
          streamingMessage = event.message as AssistantMessage;
          if (streamingMessage.usage) {
            lastUsage = streamingMessage.usage;
          }
        }
        scrollToBottom();
        break;

      case "message_end":
        if (event.message.role === "assistant" && streamingMessage) {
          const msg = event.message as AssistantMessage;
          items = [
            ...items,
            {
              kind: "assistant",
              content: msg.content,
              usage: msg.usage,
              model: msg.model,
              timestamp: msg.timestamp,
            },
          ];
          if (msg.usage) lastUsage = msg.usage;
          streamingMessage = null;
        }
        scrollToBottom();
        break;

      case "tool_execution_start": {
        setActivity(`Running tool: ${event.toolName}`);
        const exec: ToolExecution = {
          toolCallId: event.toolCallId,
          toolName: event.toolName,
          args: event.args,
          status: "running",
        };
        toolExecutions.set(event.toolCallId, exec);

        // Add to current tool group, or create one
        if (!currentToolGroup) {
          currentToolGroup = { kind: "tool-group", executions: [exec], turnComplete: false };
          items = [...items, currentToolGroup];
        } else {
          currentToolGroup.executions = [...currentToolGroup.executions, exec];
          items = [...items]; // trigger reactivity
        }
        scrollToBottom();
        break;
      }

      case "tool_execution_end": {
        setActivity("");
        const existing = toolExecutions.get(event.toolCallId);
        if (existing) {
          const updated = {
            ...existing,
            result: event.result,
            isError: event.isError,
            status: (event.isError ? "error" : "done") as ToolExecution["status"],
          };
          toolExecutions.set(event.toolCallId, updated);

          // Update within the tool group
          if (currentToolGroup) {
            currentToolGroup.executions = currentToolGroup.executions.map((e) =>
              e.toolCallId === event.toolCallId ? updated : e,
            );
            items = [...items];
          }

        }
        scrollToBottom();
        break;
      }

      case "compaction_start":
        setActivity("Compacting context...");
        items = [...items, { kind: "status", text: `Context compaction started (${event.reason})` }];
        scrollToBottom();
        break;

      case "compaction_end":
        setActivity("");
        items = [...items, { kind: "status", text: event.aborted ? "Compaction aborted" : "Context compacted" }];
        scrollToBottom();
        break;

      case "auto_retry_start":
        setActivity(`Auto-retry attempt ${event.attempt}...`);
        items = [...items, { kind: "status", text: `Auto-retry attempt ${event.attempt}` }];
        scrollToBottom();
        break;

      case "extension_ui_request":
        handleExtensionUIRequest(event as unknown as ExtensionUIRequest);
        break;

      case "sidecar_error": {
        const errorMsg = (event as any).error || "Unknown sidecar error";
        items = [...items, { kind: "notification", text: errorMsg, level: "error" }];
        setActivity("");
        scrollToBottom();
        break;
      }
    }
  }

  function handleExtensionUIRequest(request: ExtensionUIRequest) {
    // Fire-and-forget methods — no response needed
    switch (request.method) {
      case "notify":
        items = [
          ...items,
          {
            kind: "notification",
            text: request.message || "",
            level: request.notifyType || "info",
          },
        ];
        scrollToBottom();
        return;
      case "setStatus":
        // Could show in controls area — for now, log it
        if (request.statusText) {
          items = [...items, { kind: "status", text: `[${request.statusKey}] ${request.statusText}` }];
          scrollToBottom();
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
    setActivity("Sending prompt...");
    await sendPiCommand({ type: "prompt", message });
  }

  async function abort() {
    await sendPiCommand({ type: "abort" });
  }

  function copyStderr() {
    const text = agent.stderrLines.join("\n");
    navigator.clipboard.writeText(text);
  }

  // Show compact error view when agent failed and has no useful messages
  let hasMessages = $derived(items.some((i) => i.kind === "user" || i.kind === "assistant"));
  let showCompactError = $derived(
    (agent.status === "error" || agent.status === "stopped") && !hasMessages
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
      items = [...items, { kind: "notification", text: `Failed to create new session: ${e}`, level: "error" }];
      return;
    }

    // Update the current session's message count in history, then add the new session
    let nextSessions = agent.sessions;
    if (agent.sessionId) {
      const msgCount = countPersistedMessages(items);
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
    items = [];
    toolExecutions = new Map();
    streamingMessage = null;
    lastUsage = undefined;
    items = [...items, { kind: "status", text: "New session started" }];
    refreshSessionsFromDb();
  }

  function resetViewState() {
    items = [];
    toolExecutions = new Map();
    streamingMessage = null;
    isStreaming = false;
    lastUsage = undefined;
    pendingExtensionRequest = null;
    showStderr = false;
    pendingSourceSessionId = undefined;
    showPromptEditor = false;
    showHistory = false;
    showContextInspector = false;
    currentToolGroup = null;
    activityStatus = "";
    eventCount = 0;
  }

  function clearListeners() {
    unlistenEvent?.();
    unlistenExit?.();
    unlistenStderr?.();
    unlistenEvent = undefined;
    unlistenExit = undefined;
    unlistenStderr = undefined;
  }

  async function bindAgent(target: Agent) {
    const version = ++activationVersion;
    if (boundAgentId && boundAgentId !== target.id) {
      persistCurrentViewState(boundAgentId, boundSessionId);
    }
    boundAgentId = target.id;
    boundSessionId = target.sessionId;
    clearListeners();
    resetViewState();
    isStreaming = target.isStreaming;

    // Optimistically restore the cache for instant render IFF it's plausibly fresh.
    // We always reconcile against the DB below — the cache is strictly a UX shortcut,
    // never authoritative.
    const cachedState = getcachedstate?.(target.id);
    const sessionMatches = !!cachedState && cachedState.sessionId === target.sessionId;
    const mayBeStreamingStale = !!cachedState && (cachedState.wasStreaming || target.isStreaming);
    const optimisticRestore = !!cachedState && sessionMatches && !mayBeStreamingStale;
    if (cachedState && optimisticRestore) {
      restoreCachedViewState(cachedState);
    }

    const sessions = await refreshSessionsFromDb(target.id);
    const promptPromise = invoke<string | null>("get_agent_prompt", { agentId: target.id })
      .catch(() => null);
    if (version !== activationVersion) return;

    customPrompt = await promptPromise;
    if (version !== activationVersion) return;

    const currentSession = target.sessionId
      ? sessions.find((session) => session.sessionId === target.sessionId)
      : undefined;

    // Cache is fresh iff: session matches, wasn't mid-stream at capture, agent isn't
    // streaming now, and messageCount hasn't drifted from what the DB reports.
    const dbMessageCount = currentSession?.messageCount || 0;
    const cacheIsFresh =
      !!cachedState &&
      sessionMatches &&
      !mayBeStreamingStale &&
      (cachedState.messageCount ?? 0) === dbMessageCount;

    if (target.sourceSessionId) {
      pendingSourceSessionId = target.sourceSessionId;
      try {
        await loadSessionMessages(target.sourceSessionId, "Restored previous session");
      } catch (e) {
        console.error("Failed to restore messages from DB:", e);
        items = [...items, { kind: "notification", text: `Failed to restore stored messages: ${e}`, level: "error" }];
      }
      updateAgent((current) => ({ ...current, sourceSessionId: undefined }), target.id);
    } else if (!cacheIsFresh && target.sessionId && dbMessageCount > 0) {
      // Cache was missing, stale, or captured mid-stream — reload from SQLite so the
      // UI reflects everything that happened while this agent was in the background.
      try {
        await loadSessionMessages(target.sessionId, "Reopened current session");
      } catch (e) {
        console.error("Failed to load current session messages from DB:", e);
        items = [...items, { kind: "notification", text: `Failed to load current session messages: ${e}`, level: "error" }];
      }
    } else if (!cacheIsFresh && !optimisticRestore) {
      items = [{ kind: "status", text: `Viewing ${target.shadow?.shadowName || target.name}` }];
    }

    if (version !== activationVersion) return;

    setActivity("Shadow connected — waiting for Pi process...");

    unlistenEvent = await listen<string>(
      `agent-event-${target.id}`,
      (event) => {
        if (version !== activationVersion) return;
        if (eventCount === 0) {
          items = [...items, { kind: "status", text: "Pi process started — receiving events" }];
          scrollToBottom();
        }
        handleEvent(event.payload, target.id);
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
      setActivity("");
      const code = event.payload;
      const msg = code != null && code !== 0
        ? `Agent process exited with code ${code}`
        : "Agent process exited";
      items = [...items, { kind: "status", text: msg }];
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
    persistCurrentViewState();
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

      {#if agent.status === "stopped"}
        <div class="exit-banner">
          <span>Agent stopped</span>
          {#if onrestart}
            <button class="restart-btn" onclick={() => onrestart?.(agent.id)}>Restart</button>
          {/if}
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
        oncontextinspect={() => (showContextInspector = !showContextInspector)}
      />
      <ChatInput
        onsend={sendPrompt}
        disabled={isStreaming}
        bind:this={chatInputRef}
      />
    </div>
  {/if}
</div>

{#if showContextInspector}
  <ContextInspector
    {items}
    {lastUsage}
    contextWindow={agent.contextWindow}
    sessionStats={agent.sessionStats}
    customPrompt={customPrompt}
    {projectInstructions}
    shadow={agent.shadow}
    onclose={() => (showContextInspector = false)}
  />
{/if}
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
        items = [...items, { kind: "notification", text: `Failed to switch session: ${e}`, level: "error" }];
        return;
      }

      // Update current session message count before switching
      let nextSessions = agent.sessions;
      if (agent.sessionId) {
        const msgCount = countPersistedMessages(items);
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

      // Load messages for display
      try {
        await loadSessionMessages(session.sessionId, `Continuing from previous session`);
      } catch (e) {
        items = [...items, { kind: "notification", text: `Failed to load stored session messages: ${e}`, level: "error" }];
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

  @keyframes pulse {
    0%, 100% { opacity: 0.4; }
    50% { opacity: 1; }
  }
</style>
