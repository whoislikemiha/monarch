<script lang="ts">
  import type { ToolProps } from "../types";
  import type { QuestRow } from "../../bindings";
  import { questStore } from "../questStore.svelte";
  import ShadowAvatar from "../../avatar/ShadowAvatar.svelte";

  let { agentContext }: ToolProps = $props();

  /**
   * Per-agent reactive slice. `ensure` creates an empty entry on first
   * access; `refresh` fills it. We key by agentId because toolbox tools
   * stay mounted across agent switches — a fresh agent needs its own
   * slice, not someone else's stale one.
   */
  let questState = $derived(
    agentContext ? (questStore.byAgent.get(agentContext.agentId) ?? null) : null,
  );

  $effect(() => {
    if (agentContext) {
      questStore.ensure(agentContext.agentId);
      questStore.refresh(agentContext.agentId);
    }
  });

  // --- Tree shaping --------------------------------------------------------
  // The backend returns a flat list per root ordered by created_at ASC.
  // Turn it into a parent→children adjacency map so the template can
  // render it depth-first without recursion.

  interface TreeNode {
    quest: QuestRow;
    depth: number;
  }

  function flattenTree(tree: QuestRow[]): TreeNode[] {
    const byParent = new Map<string | null, QuestRow[]>();
    for (const q of tree) {
      const key = q.parentId ?? null;
      const list = byParent.get(key) ?? [];
      list.push(q);
      byParent.set(key, list);
    }
    const out: TreeNode[] = [];
    function walk(parentId: string | null, depth: number) {
      const children = byParent.get(parentId) ?? [];
      for (const q of children) {
        out.push({ quest: q, depth });
        walk(q.id, depth + 1);
      }
    }
    walk(null, 0);
    return out;
  }

  // --- Node visuals --------------------------------------------------------

  const STATUS_COLOR: Record<string, string> = {
    pending: "var(--text-muted)",
    in_progress: "var(--accent)",
    claimed_done: "#d6a84d",
    verified: "#4da36b",
    disputed: "#c45a5a",
    ambiguous: "#a07ecc",
    done: "#4da36b",
    abandoned: "var(--text-muted)",
    superseded: "var(--text-muted)",
  };

  function formatRelative(iso: string): string {
    const then = new Date(iso).getTime();
    const now = Date.now();
    const delta = Math.max(0, now - then);
    const s = Math.floor(delta / 1000);
    if (s < 60) return `${s}s ago`;
    const m = Math.floor(s / 60);
    if (m < 60) return `${m}m ago`;
    const h = Math.floor(m / 60);
    if (h < 24) return `${h}h ago`;
    const d = Math.floor(h / 24);
    return `${d}d ago`;
  }

  // --- New-quest form ------------------------------------------------------

  let formTitle = $state("");
  let formDescription = $state("");
  let formGrade = $state<"E" | "D" | "C" | "B" | "A" | "S">("C");
  let formExecHint = $state<"in_context" | "delegate" | "explore">("in_context");
  let formParentId = $state<string>("");
  let formSubmitting = $state(false);

  $effect(() => {
    // When the create form opens, reset fields and preselect parent.
    if (questState?.creating) {
      formTitle = "";
      formDescription = "";
      formGrade = "C";
      formExecHint = "in_context";
      formParentId = questState.creatingParentId ?? "";
    }
  });

  async function submitCreate() {
    if (!agentContext || !questState || !formTitle.trim()) return;
    formSubmitting = true;
    try {
      await questStore.createQuest(agentContext.agentId, {
        id: null,
        parentId: formParentId || null,
        title: formTitle.trim(),
        description: formDescription.trim() || null,
        status: "pending",
        grade: formGrade,
        execHint: formExecHint,
        assigneeShadowId: agentContext.agentId,
        createdBy: "monarch",
      });
      questStore.cancelCreate(agentContext.agentId);
    } catch (e) {
      questState.error = String(e);
    } finally {
      formSubmitting = false;
    }
  }

  // Pool of parent options: every quest already loaded for this agent
  // (across all roots). Keeps the form simple without a second fetch.
  let parentOptions = $derived.by(() => {
    if (!questState) return [] as QuestRow[];
    const all: QuestRow[] = [];
    for (const tree of questState.treesByRoot.values()) {
      for (const q of tree) all.push(q);
    }
    return all;
  });

  // MON-100: parsed compaction_tick payload. Returns null when the row
  // isn't a Keeper tick or the payload doesn't decode — both fall back to
  // the generic event renderer.
  interface CompactionPayload {
    keeperRunId: number | null;
    claimsCount: number;
    summary: string;
  }
  function parseCompactionPayload(raw: string | null): CompactionPayload | null {
    if (!raw) return null;
    try {
      const parsed = JSON.parse(raw);
      if (parsed == null || typeof parsed !== "object") return null;
      const obj = parsed as Record<string, unknown>;
      const keeperRunId =
        typeof obj.keeper_run_id === "number" ? obj.keeper_run_id : null;
      const claimsCount =
        typeof obj.claims_count === "number" ? obj.claims_count : 0;
      const summary = typeof obj.summary === "string" ? obj.summary : "";
      return { keeperRunId, claimsCount, summary };
    } catch {
      return null;
    }
  }
</script>

<div class="quest-tool">
  {#if !agentContext || !questState}
    <p class="empty">No agent selected.</p>
  {:else}
    <!-- Header: create button + status -->
    <div class="header">
      {#if !questState.creating}
        <button
          class="new-btn"
          type="button"
          onclick={() => questStore.startCreate(agentContext.agentId)}
        >
          + New quest
        </button>
      {:else}
        <span class="header-title">New quest</span>
      {/if}
      {#if questState.loading}<span class="muted">Loading…</span>{/if}
    </div>

    {#if questState.error}
      <p class="error-msg">{questState.error}</p>
    {/if}

    <!-- Create form -->
    {#if questState.creating}
      <form class="create-form" onsubmit={(e) => { e.preventDefault(); submitCreate(); }}>
        <label class="field">
          <span class="label">Title</span>
          <input
            class="input"
            type="text"
            bind:value={formTitle}
            placeholder="Short, imperative"
            required
          />
        </label>
        <label class="field">
          <span class="label">Description</span>
          <textarea
            class="input textarea"
            bind:value={formDescription}
            rows="2"
            placeholder="Optional — what does done look like?"
          ></textarea>
        </label>
        <div class="field-row">
          <label class="field">
            <span class="label">Grade</span>
            <select class="input" bind:value={formGrade}>
              <option value="E">E — trivial</option>
              <option value="D">D — small</option>
              <option value="C">C — routine</option>
              <option value="B">B — module</option>
              <option value="A">A — architectural</option>
              <option value="S">S — project-scale</option>
            </select>
          </label>
          <label class="field">
            <span class="label">Exec hint</span>
            <select class="input" bind:value={formExecHint}>
              <option value="in_context">in_context</option>
              <option value="delegate">delegate</option>
              <option value="explore">explore</option>
            </select>
          </label>
        </div>
        <label class="field">
          <span class="label">Parent</span>
          <select class="input" bind:value={formParentId}>
            <option value="">— none (root quest)</option>
            {#each parentOptions as opt (opt.id)}
              <option value={opt.id}>{opt.title}</option>
            {/each}
          </select>
        </label>
        <div class="form-actions">
          <button type="submit" class="primary-btn" disabled={formSubmitting || !formTitle.trim()}>
            {formSubmitting ? "Saving…" : "Create"}
          </button>
          <button
            type="button"
            class="ghost-btn"
            onclick={() => questStore.cancelCreate(agentContext.agentId)}
            disabled={formSubmitting}
          >
            Cancel
          </button>
        </div>
      </form>
    {/if}

    <!-- Timeline -->
    {#if questState.roots.length === 0 && !questState.loading}
      <p class="empty">No quests yet for this shadow.</p>
    {:else}
      <div class="timeline">
        {#each questState.roots as root (root.id)}
          {@const tree = questState.treesByRoot.get(root.id) ?? [root]}
          {@const flat = flattenTree(tree)}
          <div class="root">
            {#each flat as { quest, depth } (quest.id)}
              {@const expanded = questState.expandedQuestIds.has(quest.id)}
              {@const events = questState.eventsByQuest.get(quest.id) ?? []}
              <div
                class="node"
                class:expanded
                style="margin-left: {depth * 14}px"
              >
                <button
                  type="button"
                  class="node-row"
                  onclick={() => questStore.toggleExpand(agentContext.agentId, quest.id)}
                  aria-expanded={expanded}
                >
                  <span class="disclosure">{expanded ? "▾" : "▸"}</span>
                  {#if quest.assigneeShadowId}
                    <span class="avatar">
                      <ShadowAvatar
                        agentId={quest.assigneeShadowId}
                        size={18}
                      />
                    </span>
                  {/if}
                  <span
                    class="status-dot"
                    style="background:{STATUS_COLOR[quest.status] ?? 'var(--text-muted)'}"
                    title={quest.status}
                  ></span>
                  {#if quest.grade}
                    <span class="grade">{quest.grade}</span>
                  {/if}
                  <span class="title">{quest.title}</span>
                  <span class="ts muted">{formatRelative(quest.createdAt)}</span>
                </button>
                {#if expanded}
                  <div class="detail">
                    <div class="detail-meta">
                      <span class="meta-row">
                        <span class="meta-label">Status</span>
                        <span class="meta-value">{quest.status}</span>
                      </span>
                      {#if quest.grade}
                        <span class="meta-row">
                          <span class="meta-label">Grade</span>
                          <span class="meta-value">{quest.grade}</span>
                        </span>
                      {/if}
                      {#if quest.execHint}
                        <span class="meta-row">
                          <span class="meta-label">Exec</span>
                          <span class="meta-value">{quest.execHint}</span>
                        </span>
                      {/if}
                      <span class="meta-row">
                        <span class="meta-label">Created by</span>
                        <span class="meta-value">{quest.createdBy}</span>
                      </span>
                      <span class="meta-row">
                        <span class="meta-label">Created</span>
                        <span class="meta-value">{quest.createdAt}</span>
                      </span>
                      {#if quest.startedAt}
                        <span class="meta-row">
                          <span class="meta-label">Started</span>
                          <span class="meta-value">{quest.startedAt}</span>
                        </span>
                      {/if}
                      {#if quest.completedAt}
                        <span class="meta-row">
                          <span class="meta-label">Completed</span>
                          <span class="meta-value">{quest.completedAt}</span>
                        </span>
                      {/if}
                    </div>
                    {#if quest.description}
                      <p class="description">{quest.description}</p>
                    {/if}
                    <div class="event-log">
                      <div class="log-title">Event log</div>
                      {#if events.length === 0}
                        <div class="muted small">No events.</div>
                      {:else}
                        {#each events as ev (ev.id)}
                          {#if ev.eventType === "compaction_tick"}
                            {@const cp = parseCompactionPayload(ev.payloadJson)}
                            <div class="event-row compaction-row">
                              <span class="compaction-icon" title="Keeper compaction tick">◈</span>
                              <span class="event-type compaction-type">compaction</span>
                              <span class="muted small">{ev.actor ?? "—"}</span>
                              <span class="muted small">{formatRelative(ev.createdAt)}</span>
                              {#if cp}
                                <span class="claims-pill" title="{cp.claimsCount} atomic claims persisted">
                                  +{cp.claimsCount} {cp.claimsCount === 1 ? "claim" : "claims"}
                                </span>
                              {/if}
                            </div>
                            {#if cp}
                              <div class="compaction-summary">{cp.summary || "(no summary returned)"}</div>
                              <div class="compaction-meta muted small">
                                run #{cp.keeperRunId ?? "?"}
                              </div>
                            {:else if ev.payloadJson}
                              <pre class="event-payload">{ev.payloadJson}</pre>
                            {/if}
                          {:else}
                            <div class="event-row">
                              <span class="event-type">{ev.eventType}</span>
                              <span class="muted small">{ev.actor ?? "—"}</span>
                              <span class="muted small">{formatRelative(ev.createdAt)}</span>
                            </div>
                            {#if ev.payloadJson}
                              <pre class="event-payload">{ev.payloadJson}</pre>
                            {/if}
                          {/if}
                        {/each}
                      {/if}
                    </div>
                    <button
                      type="button"
                      class="ghost-btn sub-btn"
                      onclick={(e) => {
                        e.stopPropagation();
                        questStore.startCreate(agentContext.agentId, quest.id);
                      }}
                    >
                      + Sub-quest
                    </button>
                  </div>
                {/if}
              </div>
            {/each}
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</div>

<style>
  .quest-tool {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .header-title {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-secondary);
  }

  .new-btn {
    padding: 4px 10px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: var(--bg-panel-2);
    color: var(--text-primary);
    font-family: inherit;
    font-size: 10px;
    cursor: pointer;
  }
  .new-btn:hover {
    background: var(--accent-bg-hover);
  }

  /* Create form */
  .create-form {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: var(--bg-panel-2);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .field-row {
    display: flex;
    gap: 8px;
  }
  .field-row .field {
    flex: 1;
  }
  .label {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
  }
  .input {
    padding: 4px 6px;
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    background: var(--bg-panel);
    color: var(--text-primary);
    font-family: inherit;
    font-size: 11px;
  }
  .textarea {
    resize: vertical;
    min-height: 28px;
  }
  .form-actions {
    display: flex;
    gap: 6px;
    margin-top: 4px;
  }
  .primary-btn {
    padding: 4px 10px;
    border: 1px solid var(--accent);
    border-radius: 4px;
    background: var(--accent);
    color: var(--bg-panel);
    font-size: 10px;
    font-family: inherit;
    cursor: pointer;
  }
  .primary-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .ghost-btn {
    padding: 4px 10px;
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    background: transparent;
    color: var(--text-primary);
    font-size: 10px;
    font-family: inherit;
    cursor: pointer;
  }
  .ghost-btn:hover:not(:disabled) {
    background: var(--bg-panel-2);
  }
  .sub-btn {
    margin-top: 6px;
  }

  /* Timeline */
  .timeline {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .root {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 4px 0;
    border-bottom: 1px solid var(--border-subtle);
  }
  .root:last-child {
    border-bottom: none;
  }

  .node {
    display: flex;
    flex-direction: column;
  }

  .node-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 4px;
    background: transparent;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    text-align: left;
    font: inherit;
    color: inherit;
    width: 100%;
  }
  .node-row:hover {
    background: var(--bg-panel-2);
  }
  .node.expanded > .node-row {
    background: var(--bg-panel-2);
  }

  .disclosure {
    width: 10px;
    text-align: center;
    color: var(--text-muted);
    font-size: 9px;
    flex-shrink: 0;
  }

  .avatar {
    width: 18px;
    height: 18px;
    flex-shrink: 0;
  }

  .status-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .grade {
    flex-shrink: 0;
    padding: 0 4px;
    border: 1px solid var(--border-subtle);
    border-radius: 3px;
    font-size: 9px;
    font-weight: 600;
    color: var(--text-secondary);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    background: var(--bg-panel);
  }

  .title {
    flex: 1;
    font-size: 11px;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ts {
    flex-shrink: 0;
    font-size: 9px;
  }

  /* Detail — inline expansion */
  .detail {
    margin: 2px 0 6px 20px;
    padding: 8px;
    border-left: 2px solid var(--border-subtle);
    background: var(--bg-panel);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .detail-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }
  .meta-row {
    display: flex;
    gap: 4px;
    font-size: 10px;
  }
  .meta-label {
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .meta-value {
    color: var(--text-primary);
  }
  .description {
    margin: 0;
    font-size: 11px;
    color: var(--text-secondary);
    white-space: pre-wrap;
  }
  .event-log {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .log-title {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
  }
  .event-row {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 10px;
  }
  .event-type {
    color: var(--text-primary);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }
  .event-payload {
    margin: 0;
    padding: 4px 6px;
    font-size: 9px;
    color: var(--text-secondary);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    background: var(--bg-sidebar);
    border-radius: 3px;
    white-space: pre-wrap;
    word-break: break-all;
  }

  /* MON-100: compaction_tick visual treatment. Subtle accent border +
     dedicated icon set this kind of event apart from quest-status events
     without making it loud. */
  .compaction-row {
    padding-left: 0;
  }
  .compaction-icon {
    color: var(--accent);
    font-size: 11px;
  }
  .compaction-type {
    color: var(--accent);
    font-weight: 600;
  }
  .claims-pill {
    margin-left: auto;
    padding: 0 6px;
    border: 1px solid var(--accent);
    border-radius: 3px;
    background: var(--accent-bg-hover);
    color: var(--accent);
    font-size: 9px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }
  .compaction-summary {
    margin: 2px 0 4px 18px;
    padding: 4px 8px;
    border-left: 2px solid var(--accent);
    background: var(--bg-sidebar);
    color: var(--text-primary);
    font-size: 11px;
    line-height: 1.4;
    white-space: pre-wrap;
  }
  .compaction-meta {
    margin-left: 18px;
  }

  .empty {
    margin: 0;
    color: var(--text-muted);
    font-size: 11px;
    font-style: italic;
  }
  .error-msg {
    margin: 0;
    color: var(--error);
    font-size: 11px;
  }
  .muted {
    color: var(--text-muted);
  }
  .small {
    font-size: 10px;
  }
</style>
