<script lang="ts">
  import { invoke } from "$lib/api";
  import { formatCost } from "./format";
  import type { SessionRecord } from "./types";

  let {
    sessions,
    agentId,
    currentSessionId,
    onload,
    onclose,
  }: {
    sessions: SessionRecord[];
    agentId?: string;
    currentSessionId?: string;
    onload: (session: SessionRecord) => void;
    onclose: () => void;
  } = $props();

  let localSessions: SessionRecord[] = $state([]);
  let previewSession: SessionRecord | null = $state(null);
  let previewMessages: any[] = $state([]);
  let loadingPreview = $state(false);
  let loadingSessions = $state(false);
  let previewError = $state("");

  async function refreshSessions() {
    if (!agentId) {
      localSessions = sessions;
      return;
    }

    loadingSessions = true;
    try {
      const dbSessions = await invoke<any[]>("db_get_sessions", { agentId });
      localSessions = dbSessions.map((s: any) => ({
        sessionId: s.id,
        model: s.model || undefined,
        provider: s.provider || undefined,
        startedAt: s.startedAt,
        messageCount: s.messageCount,
        totalCost: s.totalCost,
      }));
    } catch {
      localSessions = sessions;
    }
    loadingSessions = false;
  }

  $effect(() => {
    localSessions = sessions;
    refreshSessions();
  });

  async function loadPreview(session: SessionRecord) {
    previewSession = session;
    loadingPreview = true;
    previewError = "";
    try {
      let dbMessages: any[] = [];

      try {
        dbMessages = await invoke<any[]>("db_get_messages_with_ancestry", { sessionId: session.sessionId });
      } catch {
        dbMessages = await invoke<any[]>("db_get_messages", { sessionId: session.sessionId });
      }

      previewMessages = dbMessages.map((m: any) => ({
        role: m.role,
        content: tryParseJson(m.content),
      }));
    } catch (e) {
      previewMessages = [];
      previewError = String(e);
    }
    loadingPreview = false;
  }

  function tryParseJson(s: string): any {
    try { return JSON.parse(s); } catch { return s; }
  }

  function formatDate(iso: string): string {
    try {
      const d = new Date(iso);
      return d.toLocaleDateString() + " " + d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    } catch {
      return iso;
    }
  }

  function extractText(msg: any): string {
    if (!msg?.content) return "";
    if (typeof msg.content === "string") return msg.content;
    if (Array.isArray(msg.content)) {
      return msg.content
        .filter((b: any) => b.type === "text")
        .map((b: any) => b.text)
        .join(" ");
    }
    return "";
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="overlay" onclick={onclose} role="presentation">
  <div
    class="history-panel"
    onclick={(e: MouseEvent) => e.stopPropagation()}
    role="dialog"
    tabindex="-1"
  >
    <div class="history-header">
      <h2>Session History</h2>
      <span class="session-count">{localSessions.length} session{localSessions.length !== 1 ? "s" : ""}</span>
      <button class="btn-close" onclick={onclose}>Close</button>
    </div>

    <div class="history-body">
      <div class="session-list">
        {#if loadingSessions}
          <div class="empty">Loading sessions...</div>
        {:else if localSessions.length === 0}
          <div class="empty">No past sessions</div>
        {/if}
        {#each localSessions as session (session.sessionId || session.startedAt)}
          <button
            class="session-item"
            class:active={previewSession?.sessionId === session.sessionId}
            class:current={session.sessionId === currentSessionId}
            onclick={() => loadPreview(session)}
          >
            <div class="session-meta">
              {#if session.model}
                <span class="session-model">{session.model}</span>
              {/if}
              <span class="session-date">{formatDate(session.startedAt)}</span>
            </div>
            {#if session.messageCount}
              <span class="session-msgs">{session.messageCount} msgs</span>
            {/if}
            {#if formatCost(session.totalCost)}
              <span class="session-cost">{formatCost(session.totalCost)}</span>
            {/if}
            {#if session.sessionId === currentSessionId}
              <span class="current-tag">active</span>
            {/if}
          </button>
        {/each}
      </div>

      <div class="preview-pane">
        {#if previewSession}
          <div class="preview-header">
            <span>
              {previewSession.model || "unknown model"}
              {#if formatCost(previewSession.totalCost)}
                <span class="preview-cost">· {formatCost(previewSession.totalCost)}</span>
              {/if}
            </span>
            <button
              class="load-btn"
              onclick={() => previewSession && onload(previewSession)}
              disabled={previewSession.sessionId === currentSessionId}
            >
              {previewSession.sessionId === currentSessionId
                ? "Current"
                : "Continue Session"}
            </button>
          </div>
          {#if loadingPreview}
            <div class="preview-loading">Loading messages...</div>
          {:else if previewError}
            <div class="preview-loading">Failed to load messages: {previewError}</div>
          {:else if previewMessages.length === 0}
            <div class="preview-loading">No messages in this session</div>
          {:else}
            <div class="preview-messages">
              {#each previewMessages as msg}
                <div class="preview-msg" class:user={msg.role === "user"} class:assistant={msg.role === "assistant"}>
                  <span class="msg-role">{msg.role}</span>
                  <span class="msg-text">{extractText(msg).slice(0, 200)}{extractText(msg).length > 200 ? "..." : ""}</span>
                </div>
              {/each}
            </div>
          {/if}
        {:else}
          <div class="preview-loading">Select a session to preview</div>
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: var(--overlay-backdrop);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }

  .history-panel {
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: 12px;
    width: 900px;
    max-width: 90vw;
    height: 70vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .history-header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 16px 20px;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }

  .history-header h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .session-count {
    font-size: 11px;
    color: var(--text-muted);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .btn-close {
    margin-left: auto;
    padding: 6px 12px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
  }

  .btn-close:hover {
    background: var(--bg-panel-2);
  }

  .history-body {
    flex: 1;
    display: flex;
    overflow: hidden;
  }

  .session-list {
    width: 280px;
    min-width: 280px;
    border-right: 1px solid var(--border-subtle);
    overflow-y: auto;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .session-item {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    border: 1px solid transparent;
    border-radius: 6px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    text-align: left;
    transition: background 0.1s;
  }

  .session-item:hover {
    background: var(--bg-panel-2);
  }

  .session-item.active {
    background: var(--bg-panel-2);
    border-color: var(--accent);
  }

  .session-item.current {
    border-left: 2px solid var(--success);
  }

  .session-meta {
    display: flex;
    justify-content: space-between;
    gap: 8px;
  }

  .session-model {
    color: var(--accent);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 160px;
  }

  .session-date {
    color: var(--text-muted);
    font-size: 10px;
    white-space: nowrap;
  }

  .session-msgs,
  .session-cost {
    color: var(--text-muted);
    font-size: 10px;
  }

  .preview-cost {
    color: var(--text-muted);
    font-weight: 400;
  }

  .current-tag {
    font-size: 9px;
    color: var(--success);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .preview-pane {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .preview-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 16px;
    border-bottom: 1px solid var(--border-subtle);
    font-size: 12px;
    color: var(--text-secondary);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    flex-shrink: 0;
  }

  .load-btn {
    padding: 4px 14px;
    border: 1px solid var(--accent);
    border-radius: 6px;
    background: transparent;
    color: var(--accent);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s;
  }

  .load-btn:hover:not(:disabled) {
    background: var(--accent-bg-hover);
  }

  .load-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .preview-loading {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .preview-messages {
    flex: 1;
    overflow-y: auto;
    padding: 12px 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .preview-msg {
    display: flex;
    gap: 8px;
    font-size: 12px;
    line-height: 1.5;
  }

  .msg-role {
    flex-shrink: 0;
    width: 60px;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .preview-msg.user .msg-role {
    color: var(--accent-blue);
  }

  .preview-msg.assistant .msg-role {
    color: var(--accent);
  }

  .msg-text {
    color: var(--text-secondary);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    word-break: break-word;
  }

  .empty {
    padding: 24px;
    text-align: center;
    color: var(--text-muted);
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }
</style>
