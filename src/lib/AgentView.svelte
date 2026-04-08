<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import MessageList from "./MessageList.svelte";
  import ChatInput from "./ChatInput.svelte";
  import AgentControls from "./AgentControls.svelte";
  import AgentHeader from "./AgentHeader.svelte";
  import ExtensionDialog from "./ExtensionDialog.svelte";
  import PromptEditor from "./PromptEditor.svelte";
  import HistoryPanel from "./HistoryPanel.svelte";
  import type {
    Agent,
    PiEvent,
    DisplayItem,
    ToolExecution,
    AssistantMessage,
    ExtensionUIRequest,
  } from "./types";

  let { agent, onrestart }: { agent: Agent; onrestart?: (id: string) => void } = $props();

  let items: DisplayItem[] = $state([]);
  let toolExecutions: Map<string, ToolExecution> = $state(new Map());
  let streamingMessage: AssistantMessage | null = $state(null);
  let isStreaming = $state(false);
  let lastUsage: import("./types").Usage | undefined = $state(undefined);
  let pendingExtensionRequest: ExtensionUIRequest | null = $state(null);
  let showStderr = $state(false);
  let showPromptEditor = $state(false);
  let showHistory = $state(false);

  let unlistenEvent: UnlistenFn;
  let unlistenExit: UnlistenFn;
  let unlistenStderr: UnlistenFn;
  let scrollContainer: HTMLDivElement;
  let chatInputRef: { focus: () => void } | undefined = $state(undefined);

  export function focusInput() {
    chatInputRef?.focus();
  }

  function scrollToBottom() {
    if (scrollContainer) {
      requestAnimationFrame(() => {
        scrollContainer.scrollTop = scrollContainer.scrollHeight;
      });
    }
  }

  // Live activity status
  let activityStatus = $state("");
  let eventCount = $state(0);

  function setActivity(text: string) {
    activityStatus = text;
  }

  function handleEvent(raw: string) {
    let event: PiEvent;
    try {
      event = JSON.parse(raw);
    } catch {
      return;
    }

    eventCount++;

    switch (event.type) {
      case "agent_start":
        isStreaming = true;
        agent.isStreaming = true;
        setActivity("Agent processing...");
        items = [...items, { kind: "status", text: "Agent started" }];
        scrollToBottom();
        break;

      case "agent_end":
        isStreaming = false;
        agent.isStreaming = false;
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
        // Refresh session stats after each run
        sendPiCommand({ type: "get_session_stats", id: "stats" });
        scrollToBottom();
        break;

      case "turn_start":
        setActivity("LLM call in progress...");
        items = [...items, { kind: "status", text: "Calling LLM..." }];
        scrollToBottom();
        break;

      case "turn_end":
        setActivity("Processing response...");
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
        items = [...items, { kind: "tool", execution: exec }];
        scrollToBottom();
        break;
      }

      case "tool_execution_end": {
        setActivity("");
        const existing = toolExecutions.get(event.toolCallId);
        if (existing) {
          existing.result = event.result;
          existing.isError = event.isError;
          existing.status = event.isError ? "error" : "done";
          items = [...items]; // trigger reactivity
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

      case "response":
        handleResponse(event);
        break;
    }
  }

  function handleResponse(event: PiEvent & { type: "response" }) {
    // Surface errors visibly
    if (!event.success) {
      const errMsg = event.error || `Command "${event.command}" failed`;
      items = [...items, { kind: "notification", text: errMsg, level: "error" }];
      setActivity("");
      scrollToBottom();
      return;
    }

    switch (event.command) {
      case "get_state": {
        const state = event.data as any;
        if (state?.model) {
          const modelId = state.model.id || state.model.modelId || (typeof state.model === "string" ? state.model : "");
          if (modelId) agent.model = modelId;
          if (state.model.provider) agent.provider = state.model.provider;
        }
        if (state?.thinkingLevel) {
          agent.thinkingLevel = state.thinkingLevel;
        }
        if (state?.sessionFile) agent.sessionFile = state.sessionFile;
        if (state?.sessionId) agent.sessionId = state.sessionId;
        setActivity("");
        break;
      }
      case "get_session_stats": {
        const data = event.data as any;
        if (data) {
          agent.sessionStats = {
            totalTokens: data.totalTokens || 0,
            totalCost: data.totalCost || data.cost?.total || 0,
            messageCount: data.messageCount || 0,
            turnCount: data.turnCount || 0,
          };
        }
        break;
      }
      case "get_messages": {
        // Restore messages from a previous session
        const data = event.data as any;
        const messages = data?.messages || data;
        if (Array.isArray(messages) && messages.length > 0) {
          // Clear existing items and rebuild from history
          const restored: DisplayItem[] = [];
          for (const msg of messages) {
            if (msg.role === "user") {
              const content =
                typeof msg.content === "string"
                  ? msg.content
                  : (msg.content || [])
                      .filter((b: any) => b.type === "text")
                      .map((b: any) => b.text)
                      .join("");
              if (content) {
                restored.push({ kind: "user", content, timestamp: msg.timestamp });
              }
            } else if (msg.role === "assistant") {
              restored.push({
                kind: "assistant",
                content: msg.content || [],
                usage: msg.usage,
                model: msg.model,
                timestamp: msg.timestamp,
              });
            }
          }
          if (restored.length > 0) {
            items = [
              { kind: "status", text: `Restored ${restored.length} messages from previous session` },
              ...restored,
            ];
            scrollToBottom();
          }
        }
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
        if (request.title) agent.name = request.title;
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
    const response = {
      type: "extension_ui_response",
      id: pendingExtensionRequest.id,
      ...value,
    };
    sendPiCommand(response);
    pendingExtensionRequest = null;
  }

  function cancelExtensionRequest() {
    if (!pendingExtensionRequest) return;
    sendPiCommand({
      type: "extension_ui_response",
      id: pendingExtensionRequest.id,
      cancelled: true,
    });
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
    agent.thinkingLevel = level;
  }

  async function setModel(provider: string, modelId: string) {
    await sendPiCommand({ type: "set_model", provider, modelId });
  }

  async function compact() {
    await sendPiCommand({ type: "compact" });
  }

  async function newSession() {
    await sendPiCommand({ type: "new_session" });
    items = [];
    toolExecutions = new Map();
    streamingMessage = null;
    lastUsage = undefined;
    items = [...items, { kind: "status", text: "New session started" }];
  }

  onMount(async () => {
    // Show that we're connected
    setActivity("Shadow connected — waiting for Pi process...");
    items = [...items, { kind: "status", text: `Spawning ${agent.shadow?.shadowName || agent.name}...` }];

    unlistenEvent = await listen<string>(
      `agent-event-${agent.id}`,
      (event) => {
        // First event means Pi is alive
        if (eventCount === 0) {
          items = [...items, { kind: "status", text: "Pi process started — receiving events" }];
          scrollToBottom();
        }
        handleEvent(event.payload);
      },
    );

    unlistenExit = await listen<number | null>(`agent-exit-${agent.id}`, (event) => {
      isStreaming = false;
      agent.isStreaming = false;
      agent.status = "stopped";
      agent.exitCode = event.payload;
      setActivity("");
      const code = event.payload;
      const msg = code != null && code !== 0
        ? `Agent process exited with code ${code}`
        : "Agent process exited";
      items = [...items, { kind: "status", text: msg }];
      // Auto-show stderr if process died with error
      if (code != null && code !== 0 && agent.stderrLines.length > 0) {
        showStderr = true;
      }
      scrollToBottom();
    });

    unlistenStderr = await listen<string>(`agent-stderr-${agent.id}`, (event) => {
      agent.stderrLines = [...(agent.stderrLines || []), event.payload];
    });

    // Initial state sync
    sendPiCommand({ type: "get_state", id: "init-state" });
  });

  onDestroy(() => {
    unlistenEvent?.();
    unlistenExit?.();
    unlistenStderr?.();
  });
</script>

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
      onprompt={() => (showPromptEditor = true)}
      onhistory={() => (showHistory = true)}
      oncompact={compact}
      onnewsession={newSession}
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
        {lastUsage}
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

{#if pendingExtensionRequest}
  <ExtensionDialog
    request={pendingExtensionRequest}
    onrespond={respondToExtension}
    oncancel={cancelExtensionRequest}
  />
{/if}

{#if showHistory}
  <HistoryPanel
    sessions={agent.sessions || []}
    currentSessionFile={agent.sessionFile}
    onload={async (sessionFile) => {
      showHistory = false;
      // Switch to the selected session
      await sendPiCommand({ type: "switch_session", sessionPath: sessionFile });
      await sendPiCommand({ type: "get_messages", id: "load-history" });
      await sendPiCommand({ type: "get_state", id: "post-switch" });
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

  .exit-banner.error {
    border-color: rgba(238, 83, 150, 0.35);
    color: var(--error);
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
