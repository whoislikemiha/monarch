<script lang="ts">
  import type { ToolProps } from "../types";
  import type { QuestEventRow, QuestRow } from "../../bindings";
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
  let markingDoneId = $state<string | null>(null);

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

  function nowIsoSeconds(): string {
    return new Date().toISOString().replace(/\.\d{3}Z$/, "Z");
  }

  async function markDone(quest: QuestRow) {
    if (!agentContext || !questState || quest.status === "done") return;
    markingDoneId = quest.id;
    try {
      await questStore.updateQuest(agentContext.agentId, {
        id: quest.id,
        title: null,
        description: null,
        status: "done",
        grade: null,
        execHint: null,
        assigneeShadowId: null,
        summary: null,
        startedAt: null,
        completedAt: quest.completedAt ?? nowIsoSeconds(),
        abandonedAt: null,
      });
    } catch (e) {
      questState.error = String(e);
    } finally {
      markingDoneId = null;
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
    trigger: string;
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
      const trigger =
        typeof obj.trigger === "string" ? obj.trigger : "continuous";
      const claimsCount =
        typeof obj.claims_count === "number" ? obj.claims_count : 0;
      const summary = typeof obj.summary === "string" ? obj.summary : "";
      return { keeperRunId, trigger, claimsCount, summary };
    } catch {
      return null;
    }
  }

  interface MemorySuggestionPayload {
    title: string;
    summary: string;
    content: string;
  }
  function parseMemorySuggestionPayload(raw: string | null): MemorySuggestionPayload | null {
    if (!raw) return null;
    try {
      const parsed = JSON.parse(raw);
      if (parsed == null || typeof parsed !== "object") return null;
      const obj = parsed as Record<string, unknown>;
      const title = typeof obj.title === "string" ? obj.title : "";
      const summary = typeof obj.summary === "string" ? obj.summary : "";
      const content = typeof obj.content === "string" ? obj.content : "";
      if (!title && !summary && !content) return null;
      return { title, summary, content };
    } catch {
      return null;
    }
  }

  function formatTrigger(trigger: string): string {
    return trigger === "quest_close" ? "quest close" : "continuous";
  }

  interface ActionPayload {
    intent: string;
    status: string;
    outcome: string;
  }
  interface ToolCallPayload {
    toolName: string;
    status: string;
    argsPreview: string;
    resultPreview: string;
    durationMs: number | null;
    isError: boolean;
  }
  interface OutcomePayload {
    outcome: string;
    autoClosed: boolean;
  }
  interface DecisionPayload {
    decision: string;
    rationale: string;
  }
  interface EventNode {
    event: QuestEventRow;
    children: QuestEventRow[];
  }

  function parsePayload(raw: string | null): Record<string, unknown> {
    if (!raw) return {};
    try {
      const parsed = JSON.parse(raw);
      return parsed && typeof parsed === "object" ? parsed as Record<string, unknown> : {};
    } catch {
      return {};
    }
  }

  function eventTree(events: QuestEventRow[]): EventNode[] {
    const childrenByParent = new Map<string, QuestEventRow[]>();
    const roots: QuestEventRow[] = [];
    for (const ev of events) {
      if (ev.parentEventId) {
        const list = childrenByParent.get(ev.parentEventId) ?? [];
        list.push(ev);
        childrenByParent.set(ev.parentEventId, list);
      } else {
        roots.push(ev);
      }
    }
    return roots.map((event) => ({
      event,
      children: childrenByParent.get(event.id) ?? [],
    }));
  }

  function actionPayload(raw: string | null): ActionPayload {
    const obj = parsePayload(raw);
    return {
      intent: typeof obj.intent === "string" ? obj.intent : "",
      status: typeof obj.status === "string" ? obj.status : "",
      outcome: typeof obj.outcome === "string" ? obj.outcome : "",
    };
  }

  function toolCallPayload(raw: string | null): ToolCallPayload {
    const obj = parsePayload(raw);
    return {
      toolName: typeof obj.tool_name === "string" ? obj.tool_name : "tool",
      status: typeof obj.status === "string" ? obj.status : "",
      argsPreview: typeof obj.args_preview === "string" ? obj.args_preview : "",
      resultPreview: typeof obj.result_preview === "string" ? obj.result_preview : "",
      durationMs: typeof obj.duration_ms === "number" ? obj.duration_ms : null,
      isError: obj.is_error === true,
    };
  }

  function outcomePayload(raw: string | null): OutcomePayload {
    const obj = parsePayload(raw);
    return {
      outcome: typeof obj.outcome === "string" ? obj.outcome : "",
      autoClosed: obj.auto_closed === true,
    };
  }

  function decisionPayload(raw: string | null): DecisionPayload {
    const obj = parsePayload(raw);
    return {
      decision: typeof obj.decision === "string" ? obj.decision : "",
      rationale: typeof obj.rationale === "string" ? obj.rationale : "",
    };
  }

  function durationLabel(ms: number | null): string {
    if (ms == null) return "";
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(ms < 10000 ? 1 : 0)}s`;
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
                        {#each eventTree(events) as node (node.event.id)}
                          {@const ev = node.event}
                          {#if ev.eventType === "coherent_action"}
                            {@const action = actionPayload(ev.payloadJson)}
                            <div class="event-row action-row">
                              <span class="action-icon" title="Coherent action">◆</span>
                              <span class="event-type action-type">{action.intent || "Current action"}</span>
                              {#if action.status}
                                <span class="status-chip status-{action.status}">{action.status}</span>
                              {/if}
                              <span class="muted small">{formatRelative(ev.createdAt)}</span>
                            </div>
                            {#if action.outcome}
                              <div class="action-outcome-inline">{action.outcome}</div>
                            {/if}
                            {#if node.children.length}
                              <div class="event-children">
                                {#each node.children as child (child.id)}
                                  {#if child.eventType === "tool_call"}
                                    {@const tool = toolCallPayload(child.payloadJson)}
                                    <div class="event-row child-row tool-row" class:error={tool.isError}>
                                      <span class="child-marker"></span>
                                      <span class="event-type">{tool.toolName}</span>
                                      {#if tool.status}
                                        <span class="status-chip status-{tool.status}">{tool.status}</span>
                                      {/if}
                                      {#if tool.durationMs != null}
                                        <span class="muted small">{durationLabel(tool.durationMs)}</span>
                                      {/if}
                                    </div>
                                    {#if tool.argsPreview || tool.resultPreview}
                                      <div class="child-summary">
                                        {#if tool.argsPreview}<span>{tool.argsPreview}</span>{/if}
                                        {#if tool.resultPreview}<span>{tool.resultPreview}</span>{/if}
                                      </div>
                                    {/if}
                                  {:else if child.eventType === "action_outcome"}
                                    {@const outcome = outcomePayload(child.payloadJson)}
                                    <div class="event-row child-row outcome-row">
                                      <span class="child-marker"></span>
                                      <span class="event-type">outcome</span>
                                      {#if outcome.autoClosed}
                                        <span class="status-chip status-auto_closed">auto</span>
                                      {/if}
                                      <span class="muted small">{formatRelative(child.createdAt)}</span>
                                    </div>
                                    <div class="child-summary">{outcome.outcome || "(no outcome)"}</div>
                                  {:else if child.eventType === "executor_decision"}
                                    {@const decision = decisionPayload(child.payloadJson)}
                                    <div class="event-row child-row decision-row">
                                      <span class="child-marker"></span>
                                      <span class="event-type">decision</span>
                                      <span class="muted small">{formatRelative(child.createdAt)}</span>
                                    </div>
                                    <div class="child-summary">
                                      <span>{decision.decision || "(decision)"}</span>
                                      {#if decision.rationale}<span>{decision.rationale}</span>{/if}
                                    </div>
                                  {:else}
                                    <div class="event-row child-row">
                                      <span class="child-marker"></span>
                                      <span class="event-type">{child.eventType}</span>
                                      <span class="muted small">{formatRelative(child.createdAt)}</span>
                                    </div>
                                    {#if child.payloadJson}
                                      <pre class="event-payload child-payload">{child.payloadJson}</pre>
                                    {/if}
                                  {/if}
                                {/each}
                              </div>
                            {/if}
                          {:else if ev.eventType === "compaction_tick"}
                            {@const cp = parseCompactionPayload(ev.payloadJson)}
                            <div class="event-row compaction-row">
                              <span class="compaction-icon" title="Keeper compaction tick">◈</span>
                              <span class="event-type compaction-type">
                                {cp ? formatTrigger(cp.trigger) : "compaction"}
                              </span>
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
                          {:else if ev.eventType === "memory_suggestion"}
                            {@const suggestion = parseMemorySuggestionPayload(ev.payloadJson)}
                            <div class="event-row memory-suggestion-row">
                              <span class="memory-suggestion-icon" title="Executor memory suggestion">◇</span>
                              <span class="event-type memory-suggestion-type">memory suggestion</span>
                              <span class="muted small">{ev.actor ?? "—"}</span>
                              <span class="muted small">{formatRelative(ev.createdAt)}</span>
                            </div>
                            {#if suggestion}
                              <div class="memory-suggestion-card">
                                <div class="memory-suggestion-title">{suggestion.title || "(untitled)"}</div>
                                {#if suggestion.summary}
                                  <div class="memory-suggestion-summary">{suggestion.summary}</div>
                                {/if}
                                {#if suggestion.content}
                                  <details class="memory-suggestion-details">
                                    <summary>Details</summary>
                                    <div>{suggestion.content}</div>
                                  </details>
                                {/if}
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
                    <div class="detail-actions">
                      {#if quest.status !== "done"}
                        <button
                          type="button"
                          class="done-btn"
                          onclick={(e) => {
                            e.stopPropagation();
                            markDone(quest);
                          }}
                          disabled={markingDoneId === quest.id}
                        >
                          {markingDoneId === quest.id ? "Closing..." : "Mark done"}
                        </button>
                      {/if}
                      <button
                        type="button"
                        class="ghost-btn"
                        onclick={(e) => {
                          e.stopPropagation();
                          questStore.startCreate(agentContext.agentId, quest.id);
                        }}
                      >
                        + Sub-quest
                      </button>
                    </div>
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
  .detail-actions {
    display: flex;
    gap: 6px;
    margin-top: 6px;
  }
  .done-btn {
    padding: 4px 10px;
    border: 1px solid #4da36b;
    border-radius: 4px;
    background: rgba(77, 163, 107, 0.12);
    color: #4da36b;
    font-size: 10px;
    font-family: inherit;
    cursor: pointer;
  }
  .done-btn:hover:not(:disabled) {
    background: rgba(77, 163, 107, 0.18);
  }
  .done-btn:disabled {
    opacity: 0.55;
    cursor: default;
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

  .action-row {
    align-items: flex-start;
    padding: 4px 0;
  }
  .action-icon {
    color: var(--accent);
    font-size: 11px;
    line-height: 1.4;
  }
  .action-type {
    flex: 1;
    min-width: 0;
    color: var(--text-primary);
    font-family: inherit;
    font-weight: 600;
    white-space: normal;
    overflow-wrap: anywhere;
  }
  .status-chip {
    flex-shrink: 0;
    padding: 1px 5px;
    border: 1px solid var(--border-subtle);
    border-radius: 3px;
    color: var(--text-muted);
    font-size: 9px;
    line-height: 1.2;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }
  .status-active,
  .status-running {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-bg-subtle);
  }
  .status-completed,
  .status-done {
    border-color: #4da36b;
    color: #4da36b;
    background: rgba(77, 163, 107, 0.1);
  }
  .status-error,
  .status-auto_closed {
    border-color: #c45a5a;
    color: #c45a5a;
    background: rgba(196, 90, 90, 0.1);
  }
  .action-outcome-inline {
    margin: 0 0 2px 18px;
    color: var(--text-secondary);
    font-size: 10px;
    line-height: 1.4;
    overflow-wrap: anywhere;
  }
  .event-children {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0 0 4px 18px;
    padding-left: 10px;
    border-left: 1px solid var(--border-subtle);
  }
  .child-row {
    min-height: 18px;
  }
  .child-marker {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--text-muted);
    flex-shrink: 0;
  }
  .tool-row.error .child-marker {
    background: #c45a5a;
  }
  .outcome-row .child-marker {
    background: #4da36b;
  }
  .decision-row .child-marker {
    background: #d6a84d;
  }
  .child-summary {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-left: 11px;
    padding: 3px 6px;
    border-radius: 3px;
    background: var(--bg-sidebar);
    color: var(--text-secondary);
    font-size: 9px;
    line-height: 1.4;
    overflow-wrap: anywhere;
  }
  .child-payload {
    margin-left: 11px;
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

  .memory-suggestion-row {
    padding-left: 0;
  }
  .memory-suggestion-icon {
    color: #d6a84d;
    font-size: 11px;
  }
  .memory-suggestion-type {
    color: #d6a84d;
    font-weight: 600;
  }
  .memory-suggestion-card {
    margin: 2px 0 4px 18px;
    padding: 6px 8px;
    border-left: 2px solid #d6a84d;
    background: var(--bg-sidebar);
    color: var(--text-primary);
    font-size: 11px;
    line-height: 1.4;
  }
  .memory-suggestion-title {
    font-weight: 600;
  }
  .memory-suggestion-summary {
    color: var(--text-secondary);
  }
  .memory-suggestion-details {
    margin-top: 4px;
    color: var(--text-secondary);
  }
  .memory-suggestion-details summary {
    cursor: pointer;
    color: var(--text-muted);
    font-size: 10px;
  }
  .memory-suggestion-details div {
    margin-top: 4px;
    white-space: pre-wrap;
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
