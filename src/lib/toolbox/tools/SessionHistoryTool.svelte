<script lang="ts">
  /**
   * Session history browser (MON-127). Lists an agent's sessions newest-first
   * with title/preview metadata, renders a selected session's conversation
   * read-only (per-session messages, no ancestry), and offers the two session
   * actions: start a fresh session, or continue a past one as the active
   * session. Replaces the legacy HistoryPanel modal.
   */
  import { invoke } from "$lib/api";
  import type { ToolProps } from "../types";
  import type { DisplayItem } from "$lib/types";
  import { agentStore } from "$lib/stores/agentStore.svelte";
  import { formatCost } from "$lib/format";
  import MessageStream from "$lib/workspace/message/MessageStream.svelte";

  let { agentContext }: ToolProps = $props();

  interface SessionSummary {
    id: string;
    model: string | null;
    provider: string | null;
    startedAt: string;
    endedAt: string | null;
    messageCount: number;
    totalTokens: number;
    totalCost: number;
    parentSessionId: string | null;
    title: string | null;
    preview: string | null;
  }

  let agentId = $derived(agentContext?.agentId ?? null);
  let activeSessionId = $derived(agentContext?.agent.sessionId ?? null);

  let summaries: SessionSummary[] = $state([]);
  let loading = $state(false);
  let error = $state("");

  let selectedId: string | null = $state(null);
  let selectedItems: DisplayItem[] = $state([]);
  let loadingItems = $state(false);

  let renamingId: string | null = $state(null);
  let renameDraft = $state("");

  async function refresh(id: string) {
    loading = true;
    error = "";
    try {
      summaries = await invoke<SessionSummary[]>("db_list_session_summaries", { agentId: id });
    } catch (e) {
      error = String(e);
      summaries = [];
    }
    loading = false;
  }

  // Reload on agent switch (the panel stays mounted) and whenever the active
  // session changes (new session / continue both create or reorder rows).
  $effect(() => {
    activeSessionId;
    if (!agentId) {
      summaries = [];
      selectedId = null;
      return;
    }
    selectedId = null;
    selectedItems = [];
    refresh(agentId);
  });

  async function select(id: string) {
    if (selectedId === id) {
      selectedId = null;
      selectedItems = [];
      return;
    }
    selectedId = id;
    loadingItems = true;
    selectedItems = [];
    try {
      selectedItems = await invoke<DisplayItem[]>("get_session_display_items", { sessionId: id });
    } catch (e) {
      error = String(e);
    }
    loadingItems = false;
  }

  function label(s: SessionSummary): string {
    return s.title || s.preview || "Untitled session";
  }

  function formatDate(iso: string): string {
    try {
      const d = new Date(iso);
      return d.toLocaleDateString([], { month: "short", day: "numeric" }) +
        " " + d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    } catch {
      return iso;
    }
  }

  function startRename(s: SessionSummary, ev: Event) {
    ev.stopPropagation();
    renamingId = s.id;
    renameDraft = s.title ?? "";
  }

  async function commitRename() {
    if (!renamingId || !agentId) return;
    const id = renamingId;
    const title = renameDraft.trim() || null;
    renamingId = null;
    try {
      await invoke("db_set_session_title", { sessionId: id, title });
      await refresh(agentId);
    } catch (e) {
      error = String(e);
    }
  }

  async function continueSession(id: string, ev: Event) {
    ev.stopPropagation();
    if (!agentId || id === activeSessionId) return;
    await agentStore.switchSession(agentId, id);
  }

  async function newSession() {
    if (!agentId) return;
    await agentStore.newConversation(agentId);
    await refresh(agentId);
  }
</script>

<div class="sessions">
  {#if !agentContext}
    <div class="empty">No agent selected</div>
  {:else}
    <div class="bar">
      <span class="count mono">{summaries.length} session{summaries.length === 1 ? "" : "s"}</span>
      <button class="newbtn" onclick={newSession}>+ New session</button>
    </div>

    {#if error}<div class="err">{error}</div>{/if}

    {#if loading && summaries.length === 0}
      <div class="empty">Loading…</div>
    {:else if summaries.length === 0}
      <div class="empty">No sessions yet</div>
    {/if}

    <div class="list" role="list">
      {#each summaries as s (s.id)}
        <div class="row" role="listitem" class:open={selectedId === s.id} class:active={s.id === activeSessionId}>
          <button class="head" onclick={() => select(s.id)}>
            <span class="dot" class:on={s.id === activeSessionId} aria-hidden="true"></span>
            <span class="title-wrap">
              {#if renamingId === s.id}
                <!-- svelte-ignore a11y_autofocus -->
                <input
                  class="rename mono"
                  autofocus
                  bind:value={renameDraft}
                  onclick={(e) => e.stopPropagation()}
                  onkeydown={(e) => {
                    if (e.key === "Enter") commitRename();
                    if (e.key === "Escape") renamingId = null;
                  }}
                  onblur={commitRename}
                />
              {:else}
                <span class="title" title={label(s)}>{label(s)}</span>
              {/if}
              <span class="meta mono">
                {formatDate(s.startedAt)}
                · {s.messageCount} msg{s.messageCount === 1 ? "" : "s"}
                {#if s.parentSessionId}· continued{/if}
                {#if formatCost(s.totalCost)}· {formatCost(s.totalCost)}{/if}
                {#if s.id === activeSessionId}· active{/if}
              </span>
            </span>
          </button>
          <span class="acts">
            <button class="act" title="Rename session" aria-label="Rename session" onclick={(e) => startRename(s, e)}>
              <svg viewBox="0 0 16 16" width="11" height="11" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M11.5 2.5l2 2L6 12l-2.8.8L4 10z"/></svg>
            </button>
            {#if s.id !== activeSessionId}
              <button class="act go" title="Continue this session" aria-label="Continue this session" onclick={(e) => continueSession(s.id, e)}>
                <svg viewBox="0 0 16 16" width="11" height="11" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M4 8h8M9 4.5L12.5 8 9 11.5"/></svg>
              </button>
            {/if}
          </span>
        </div>

        {#if selectedId === s.id}
          <div class="viewer">
            {#if loadingItems}
              <div class="empty">Loading conversation…</div>
            {:else if selectedItems.length === 0}
              <div class="empty">No messages in this session</div>
            {:else}
              <MessageStream agent={agentContext.agent} items={selectedItems} streamingMessage={null} />
            {/if}
          </div>
        {/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  .sessions { display: flex; flex-direction: column; min-height: 0; height: 100%; }

  .bar {
    display: flex; align-items: center; justify-content: space-between;
    padding: var(--s2) var(--s3); flex: none;
    border-bottom: 1px solid var(--border-subtle);
  }
  .count { font-size: 10px; color: var(--text-muted); }
  .newbtn {
    font: inherit; font-size: 11px; color: var(--accent-ink, var(--text-primary));
    background: var(--accent); border: none; border-radius: var(--r-sm);
    padding: 3px var(--s2); cursor: pointer;
  }

  .err { padding: var(--s2) var(--s3); font-size: 11px; color: var(--status-error); }

  .list { overflow-y: auto; min-height: 0; flex: 1; padding: var(--s1); display: flex; flex-direction: column; gap: 1px; }

  .row {
    display: flex; align-items: stretch; border-radius: var(--r-sm);
    border: 1px solid transparent;
  }
  .row:hover { background: var(--bg-raised); }
  .row.open { background: var(--bg-raised); border-color: var(--border); }

  .head {
    flex: 1; min-width: 0; display: flex; align-items: center; gap: var(--s2);
    background: none; border: none; padding: var(--s2); cursor: pointer; text-align: left;
    font: inherit; color: var(--text-secondary);
  }
  .dot { width: 6px; height: 6px; border-radius: 50%; flex: none; background: var(--border-strong); }
  .dot.on { background: var(--status-success); }

  .title-wrap { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
  .title {
    font-size: 12px; color: var(--text-primary);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .row.active .title { font-weight: 600; }
  .meta { font-size: 9.5px; color: var(--text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  .rename {
    font-size: 11px; color: var(--text-primary);
    background: var(--bg-sink); border: 1px solid var(--focus); border-radius: var(--r-sm);
    padding: 2px var(--s1); outline: none; width: 100%;
  }

  .acts { display: flex; align-items: center; gap: 2px; padding-right: var(--s1); opacity: 0; }
  .row:hover .acts, .row.open .acts { opacity: 1; }
  .act {
    width: 22px; height: 22px; display: inline-flex; align-items: center; justify-content: center;
    background: none; border: 1px solid var(--border-subtle); border-radius: var(--r-sm);
    color: var(--text-secondary); cursor: pointer; flex: none;
  }
  .act:hover { background: var(--bg-overlay); color: var(--text-primary); }
  .act.go { color: var(--accent); border-color: color-mix(in srgb, var(--accent) 40%, transparent); }

  .viewer {
    min-height: 80px; max-height: 320px; display: flex; flex-direction: column;
    margin: 0 var(--s1) var(--s1); border: 1px solid var(--border-subtle);
    border-radius: var(--r-sm); background: var(--bg-sink); overflow: hidden;
  }

  .empty {
    padding: var(--s4); text-align: center; font-size: 11px; color: var(--text-muted);
  }
</style>
