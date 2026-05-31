<script lang="ts">
  import type { ToolProps } from "../types";
  import type { PlanItemRow, QuestEventRow, QuestRefRow, QuestRow } from "../../bindings";
  import { questStore } from "../questStore.svelte";
  import ShadowAvatar from "../../avatar/ShadowAvatar.svelte";
  import { agentStore } from "../../stores/agentStore.svelte";

  let { agentContext }: ToolProps = $props();
  let agentId = $derived(agentContext?.agentId ?? "");

  /**
   * Per-agent reactive slice. `ensure` creates an empty entry on first
   * access; `refresh` fills it. We key by agentId because toolbox tools
   * stay mounted across agent switches — a fresh agent needs its own
   * slice, not someone else's stale one.
   */
  let questState = $derived(
    agentContext ? (questStore.byAgent.get(agentId) ?? null) : null,
  );
  let activeQuestId = $derived(questState?.workingMemory?.currentQuestId ?? null);
  let activeQuest = $derived.by(() => {
    if (!activeQuestId || !questState) return null;
    for (const tree of questState.treesByRoot.values()) {
      const found = tree.find((quest) => quest.id === activeQuestId);
      if (found) return found;
    }
    return questState.roots.find((quest) => quest.id === activeQuestId) ?? null;
  });

  $effect(() => {
    if (agentContext) {
      questStore.ensure(agentId);
      questStore.refresh(agentId);
    }
  });

  $effect(() => {
    if (!agentContext || !activeQuestId || questState?.planItemsByQuest.has(activeQuestId)) return;
    questStore.loadPlanItems(agentId, activeQuestId).catch((e) => {
      if (questState) questState.error = String(e);
    });
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

  function formatDateTime(iso: string | null): string {
    if (!iso) return "";
    const date = new Date(iso);
    if (Number.isNaN(date.getTime())) return iso;
    return new Intl.DateTimeFormat(undefined, {
      month: "short",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
    }).format(date);
  }

  function assigneeLabel(agentId: string | null): string {
    if (!agentId) return "Unassigned";
    const agent = agentStore.getAgent(agentId);
    if (!agent) return agentId;
    const name = agent.shadow?.shadowName || agent.name || agentId;
    return agent.shadow?.shadowTitle ? `${name}, ${agent.shadow.shadowTitle}` : name;
  }

  // --- New-quest form ------------------------------------------------------

  let formTitle = $state("");
  let formDescription = $state("");
  let formGrade = $state<"E" | "D" | "C" | "B" | "A" | "S">("C");
  let formExecHint = $state<"in_context" | "delegate" | "explore">("in_context");
  let formParentId = $state<string>("");
  let formSubmitting = $state(false);
  let markingDoneId = $state<string | null>(null);
  let savingQuestId = $state<string | null>(null);
  let savingEventId = $state<string | null>(null);
  let savingRefId = $state<string | null>(null);
  let addingPlanQuestId = $state<string | null>(null);
  let planBusyKey = $state<string | null>(null);
  let planDraftTitles = $state(new Map<string, string>());
  let newPlanTitles = $state(new Map<string, string>());

  type QuestEditDraft = {
    status: string;
    grade: "E" | "D" | "C" | "B" | "A" | "S";
    scope: string;
    currentDirection: string;
    rationale: string;
    summary: string;
    changeRationale: string;
  };
  type QuestEventDraft = {
    eventType: "note" | "blocker" | "blocker_resolved" | "question" | "answer";
    title: string;
    text: string;
  };
  type QuestRefDraft = {
    refType: string;
    label: string;
    target: string;
  };

  let questDrafts = $state<Record<string, QuestEditDraft>>({});
  let eventDrafts = $state<Record<string, QuestEventDraft>>({});
  let refDrafts = $state<Record<string, QuestRefDraft>>({});

  function ensureQuestDraft(quest: QuestRow): QuestEditDraft {
    const existing = questDrafts[quest.id];
    if (existing) return existing;
    questDrafts[quest.id] = {
      status: quest.status,
      grade: (quest.grade ?? "C") as QuestEditDraft["grade"],
      scope: quest.scope ?? "",
      currentDirection: quest.currentDirection ?? "",
      rationale: quest.rationale ?? "",
      summary: quest.summary ?? "",
      changeRationale: "",
    };
    return questDrafts[quest.id];
  }

  function resetQuestDraft(quest: QuestRow) {
    questDrafts[quest.id] = {
      status: quest.status,
      grade: (quest.grade ?? "C") as QuestEditDraft["grade"],
      scope: quest.scope ?? "",
      currentDirection: quest.currentDirection ?? "",
      rationale: quest.rationale ?? "",
      summary: quest.summary ?? "",
      changeRationale: "",
    };
  }

  function ensureEventDraft(questId: string): QuestEventDraft {
    const existing = eventDrafts[questId];
    if (existing) return existing;
    eventDrafts[questId] = { eventType: "note", title: "", text: "" };
    return eventDrafts[questId];
  }

  function ensureRefDraft(questId: string): QuestRefDraft {
    const existing = refDrafts[questId];
    if (existing) return existing;
    refDrafts[questId] = { refType: "url", label: "", target: "" };
    return refDrafts[questId];
  }

  async function saveQuestDraft(quest: QuestRow) {
    if (!agentContext || !questState) return;
    const draft = ensureQuestDraft(quest);
    savingQuestId = quest.id;
    try {
      await questStore.updateQuestManual(agentContext.agentId, {
        id: quest.id,
        status: draft.status,
        scope: draft.scope.trim() || null,
        currentDirection: draft.currentDirection.trim() || null,
        rationale: draft.rationale.trim() || null,
        grade: draft.grade,
        summary: draft.summary.trim() || null,
        changeRationale: draft.changeRationale.trim() || null,
        actor: "monarch",
        author: "captain",
      });
      draft.changeRationale = "";
    } catch (e) {
      questState.error = String(e);
    } finally {
      savingQuestId = null;
    }
  }

  async function submitManualEvent(questId: string) {
    if (!agentContext || !questState) return;
    const draft = ensureEventDraft(questId);
    if (!draft.text.trim()) return;
    savingEventId = questId;
    try {
      await questStore.recordManualQuestEvent(agentContext.agentId, {
        questId,
        eventType: draft.eventType,
        title: draft.title.trim() || null,
        text: draft.text.trim(),
        metadataJson: null,
        actor: "monarch",
        author: "captain",
        surfaceOverride: null,
      });
      eventDrafts[questId] = { eventType: draft.eventType, title: "", text: "" };
    } catch (e) {
      questState.error = String(e);
    } finally {
      savingEventId = null;
    }
  }

  async function submitQuestRef(questId: string) {
    if (!agentContext || !questState) return;
    const draft = ensureRefDraft(questId);
    if (!draft.target.trim()) return;
    savingRefId = questId;
    try {
      await questStore.createQuestRef(agentContext.agentId, {
        id: null,
        questId,
        refType: draft.refType.trim() || "url",
        label: draft.label.trim() || null,
        target: draft.target.trim(),
        metadataJson: null,
        createdBy: "captain",
      });
      refDrafts[questId] = { refType: draft.refType, label: "", target: "" };
    } catch (e) {
      questState.error = String(e);
    } finally {
      savingRefId = null;
    }
  }

  async function deleteQuestRef(questId: string, ref: QuestRefRow) {
    if (!agentContext || !questState) return;
    savingRefId = ref.id;
    try {
      await questStore.deleteQuestRef(agentContext.agentId, questId, ref.id);
    } catch (e) {
      questState.error = String(e);
    } finally {
      savingRefId = null;
    }
  }

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
      await questStore.createQuest(agentId, {
        id: null,
        parentId: formParentId || null,
        title: formTitle.trim(),
        description: formDescription.trim() || null,
        status: "pending",
        grade: formGrade,
        execHint: formExecHint,
        assigneeShadowId: agentId,
        createdBy: "monarch",
      });
      questStore.cancelCreate(agentId);
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
      await questStore.updateQuest(agentId, {
        id: quest.id,
        title: null,
        description: null,
        scope: null,
        currentDirection: null,
        rationale: null,
        forkParentId: null,
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

  function keeperLabel(trigger: string): string {
    if (trigger === "quest_close") return "Keeper quest close";
    if (trigger === "continuous") return "Keeper checkpoint";
    return "Keeper note";
  }

  function keeperHint(trigger: string): string {
    if (trigger === "quest_close") return "Summary produced when the quest was marked done.";
    if (trigger === "continuous") return "Background memory checkpoint from context compaction.";
    return "Keeper memory summary.";
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
      // Status is already visible in the quest metadata; the auto-created
      // and mark-done rows add noise to the execution narrative.
      if (ev.eventType === "status_change") continue;
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

  function manualEventLabel(kind: string): string {
    if (kind === "scope_change") return "scope changed";
    if (kind === "direction_change") return "direction changed";
    if (kind === "quest_rationale_change") return "rationale changed";
    if (kind === "quest_summary_change") return "summary changed";
    if (kind === "grade_change") return "grade changed";
    if (kind === "blocker_resolved") return "blocker resolved";
    return kind.replaceAll("_", " ");
  }

  function eventText(raw: string | null): string {
    const obj = parsePayload(raw);
    const text = obj.text;
    if (typeof text === "string") return text;
    const to = obj.to;
    if (typeof to === "string") return to;
    return raw ?? "";
  }

  function eventRationale(raw: string | null): string {
    const obj = parsePayload(raw);
    return typeof obj.rationale === "string" ? obj.rationale : "";
  }

  function durationLabel(ms: number | null): string {
    if (ms == null) return "";
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(ms < 10000 ? 1 : 0)}s`;
  }

  function planItemsFor(questId: string): PlanItemRow[] {
    return questState?.planItemsByQuest.get(questId) ?? [];
  }

  function planItemById(questId: string, itemId: string | null | undefined): PlanItemRow | null {
    if (!itemId) return null;
    return planItemsFor(questId).find((item) => item.id === itemId) ?? null;
  }

  function planTitle(questId: string, itemId: string | null | undefined): string {
    return planItemById(questId, itemId)?.title ?? "plan item";
  }

  function draftTitle(item: PlanItemRow): string {
    return planDraftTitles.get(item.id) ?? item.title;
  }

  function setDraftTitle(itemId: string, value: string) {
    const next = new Map(planDraftTitles);
    next.set(itemId, value);
    planDraftTitles = next;
  }

  function newPlanTitle(questId: string): string {
    return newPlanTitles.get(questId) ?? "";
  }

  function setNewPlanTitle(questId: string, value: string) {
    const next = new Map(newPlanTitles);
    next.set(questId, value);
    newPlanTitles = next;
  }

  async function runPlanMutation(key: string, fn: () => Promise<void>) {
    if (!agentContext || !questState) return;
    planBusyKey = key;
    try {
      await fn();
    } catch (e) {
      questState.error = String(e);
    } finally {
      planBusyKey = null;
    }
  }

  async function submitAddPlanItem(questId: string) {
    if (!agentContext) return;
    const title = newPlanTitle(questId).trim();
    if (!title) return;
    const items = planItemsFor(questId);
    const afterItemId = items.at(-1)?.id ?? null;
    await runPlanMutation(`add:${questId}`, async () => {
      await questStore.addPlanItem(agentId, {
        questId,
        title,
        afterItemId,
        createdBy: "captain",
      });
      setNewPlanTitle(questId, "");
      addingPlanQuestId = null;
    });
  }

  async function commitPlanTitle(questId: string, item: PlanItemRow) {
    if (!agentContext) return;
    const title = draftTitle(item).trim();
    if (!title || title === item.title) return;
    await runPlanMutation(`edit:${item.id}`, () =>
      questStore.updatePlanItem(agentId, questId, {
        id: item.id,
        title,
        rationale: null,
        orderIndex: null,
      }),
    );
    const next = new Map(planDraftTitles);
    next.delete(item.id);
    planDraftTitles = next;
  }

  function promptText(message: string): string | null {
    const value = window.prompt(message);
    if (value == null) return null;
    const trimmed = value.trim();
    return trimmed || null;
  }

  const PLAN_EVENT_TYPES = new Set([
    "plan_created",
    "plan_changed",
    "plan_item_started",
    "plan_item_completed",
    "plan_item_skipped",
    "plan_item_blocked",
  ]);

  interface PlanEventPayload {
    itemId: string | null;
    deletedItemId: string | null;
    itemIds: string[];
    title: string;
    outcome: string;
    reason: string;
    rationale: string;
    createdBy: string;
  }

  function planEventPayload(raw: string | null): PlanEventPayload {
    const obj = parsePayload(raw);
    const itemIds = Array.isArray(obj.item_ids)
      ? obj.item_ids.filter((id): id is string => typeof id === "string")
      : [];
    return {
      itemId: typeof obj.item_id === "string" ? obj.item_id : null,
      deletedItemId: typeof obj.deleted_item_id === "string" ? obj.deleted_item_id : null,
      itemIds,
      title: typeof obj.title === "string" ? obj.title : "",
      outcome: typeof obj.outcome === "string" ? obj.outcome : "",
      reason: typeof obj.reason === "string" ? obj.reason : "",
      rationale: typeof obj.rationale === "string" ? obj.rationale : "",
      createdBy: typeof obj.created_by === "string" ? obj.created_by : "",
    };
  }

  function planEventLabel(type: string): string {
    if (type === "plan_created") return "plan created";
    if (type === "plan_changed") return "plan changed";
    if (type === "plan_item_started") return "started";
    if (type === "plan_item_completed") return "completed";
    if (type === "plan_item_skipped") return "skipped";
    if (type === "plan_item_blocked") return "blocked";
    return type;
  }
</script>

{#snippet planPanel(panelQuest: QuestRow, planItems: PlanItemRow[], agentId: string)}
  <div class="plan-panel">
    <div class="plan-header">
      <div>
        <div class="log-title">Active plan</div>
        <div class="plan-quest-title">{panelQuest.title}</div>
      </div>
      {#if addingPlanQuestId !== panelQuest.id}
        <button
          type="button"
          class="mini-btn"
          onclick={() => {
            addingPlanQuestId = panelQuest.id;
            setNewPlanTitle(panelQuest.id, "");
          }}
        >
          + Item
        </button>
      {/if}
    </div>
    {#if addingPlanQuestId === panelQuest.id}
      <form
        class="plan-add-form"
        onsubmit={(e) => {
          e.preventDefault();
          submitAddPlanItem(panelQuest.id);
        }}
      >
        <input
          class="input plan-title-input"
          type="text"
          value={newPlanTitle(panelQuest.id)}
          oninput={(e) => setNewPlanTitle(panelQuest.id, e.currentTarget.value)}
          placeholder="Next step"
          disabled={planBusyKey === `add:${panelQuest.id}`}
        />
        <button
          type="submit"
          class="primary-btn compact"
          disabled={!newPlanTitle(panelQuest.id).trim() || planBusyKey === `add:${panelQuest.id}`}
        >
          Add
        </button>
        <button
          type="button"
          class="ghost-btn compact"
          onclick={() => (addingPlanQuestId = null)}
          disabled={planBusyKey === `add:${panelQuest.id}`}
        >
          Cancel
        </button>
      </form>
    {/if}
    {#if planItems.length === 0}
      <div class="muted small">No plan items.</div>
    {:else}
      <div class="plan-list">
        {#each planItems as item, index (item.id)}
          <div class="plan-item status-{item.status}">
            <div class="plan-item-main">
              <span class="plan-index">{index + 1}</span>
              <input
                class="plan-title-input"
                value={draftTitle(item)}
                oninput={(e) => setDraftTitle(item.id, e.currentTarget.value)}
                onchange={() => commitPlanTitle(panelQuest.id, item)}
                onkeydown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    commitPlanTitle(panelQuest.id, item);
                  }
                }}
                disabled={planBusyKey === `edit:${item.id}`}
              />
              <span class="status-chip status-{item.status}">{item.status}</span>
            </div>
            {#if item.rationale}
              <div class="plan-rationale">{item.rationale}</div>
            {/if}
            <div class="plan-actions">
              <button
                type="button"
                class="icon-btn"
                title="Move up"
                disabled={index === 0 || planBusyKey !== null}
                onclick={() =>
                  runPlanMutation(`move:${item.id}`, () =>
                    questStore.movePlanItem(agentId, panelQuest.id, item.id, -1),
                  )}
              >
                ↑
              </button>
              <button
                type="button"
                class="icon-btn"
                title="Move down"
                disabled={index === planItems.length - 1 || planBusyKey !== null}
                onclick={() =>
                  runPlanMutation(`move:${item.id}`, () =>
                    questStore.movePlanItem(agentId, panelQuest.id, item.id, 1),
                  )}
              >
                ↓
              </button>
              <button
                type="button"
                class="mini-btn"
                disabled={item.status === "active" || planBusyKey !== null}
                onclick={() =>
                  runPlanMutation(`start:${item.id}`, () =>
                    questStore.startPlanItem(agentId, panelQuest.id, item.id),
                  )}
              >
                Start
              </button>
              <button
                type="button"
                class="mini-btn"
                disabled={item.status === "completed" || planBusyKey !== null}
                onclick={() => {
                  const outcome = promptText("Outcome");
                  runPlanMutation(`complete:${item.id}`, () =>
                    questStore.completePlanItem(agentId, panelQuest.id, item.id, outcome),
                  );
                }}
              >
                Done
              </button>
              <button
                type="button"
                class="mini-btn"
                disabled={item.status === "skipped" || planBusyKey !== null}
                onclick={() => {
                  const reason = promptText("Skip reason");
                  runPlanMutation(`skip:${item.id}`, () =>
                    questStore.skipPlanItem(agentId, panelQuest.id, item.id, reason),
                  );
                }}
              >
                Skip
              </button>
              <button
                type="button"
                class="mini-btn"
                disabled={item.status === "blocked" || planBusyKey !== null}
                onclick={() => {
                  const reason = promptText("Block reason");
                  if (!reason) return;
                  runPlanMutation(`block:${item.id}`, () =>
                    questStore.blockPlanItem(agentId, panelQuest.id, item.id, reason),
                  );
                }}
              >
                Block
              </button>
              <button
                type="button"
                class="icon-btn danger"
                title="Delete"
                disabled={planBusyKey !== null}
                onclick={() =>
                  runPlanMutation(`delete:${item.id}`, () =>
                    questStore.deletePlanItem(agentId, panelQuest.id, item.id),
                  )}
              >
                ×
              </button>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/snippet}

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
          onclick={() => questStore.startCreate(agentId)}
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
            onclick={() => questStore.cancelCreate(agentId)}
            disabled={formSubmitting}
          >
            Cancel
          </button>
        </div>
      </form>
    {/if}

    {#if activeQuest}
      {@const activePlanItems = planItemsFor(activeQuest.id)}
      {@render planPanel(activeQuest, activePlanItems, agentId)}
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
              {@const refs = questState.refsByQuest.get(quest.id) ?? []}
              {@const report = questState.reportsByQuest.get(quest.id) ?? null}
              <div
                class="node"
                class:expanded
                style="margin-left: {depth * 14}px"
              >
                <button
                  type="button"
                  class="node-row"
                  onclick={() => questStore.toggleExpand(agentId, quest.id)}
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
                  {@const draft = ensureQuestDraft(quest)}
                  {@const eventDraft = ensureEventDraft(quest.id)}
                  {@const refDraft = ensureRefDraft(quest.id)}
                  <div class="detail">
                    <div class="quest-info">
                      <div class="quest-title-full">{quest.title}</div>
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
                        {#if quest.assigneeShadowId}
                          <span class="meta-row">
                            <span class="meta-label">Assignee</span>
                            <span class="meta-value" title={quest.assigneeShadowId}>
                              {assigneeLabel(quest.assigneeShadowId)}
                            </span>
                          </span>
                        {/if}
                        <span class="meta-row">
                          <span class="meta-label">Created</span>
                          <span class="meta-value">{formatDateTime(quest.createdAt)}</span>
                        </span>
                        {#if quest.startedAt}
                          <span class="meta-row">
                            <span class="meta-label">Started</span>
                            <span class="meta-value">{formatDateTime(quest.startedAt)}</span>
                          </span>
                        {/if}
                        {#if quest.completedAt}
                          <span class="meta-row">
                            <span class="meta-label">Completed</span>
                            <span class="meta-value">{formatDateTime(quest.completedAt)}</span>
                          </span>
                        {/if}
                      </div>
                      {#if quest.description}
                        <p class="description">{quest.description}</p>
                      {/if}
                    </div>

                    <form class="quest-editor" onsubmit={(e) => { e.preventDefault(); saveQuestDraft(quest); }}>
                      <div class="section-title">Brief</div>
                      <div class="field-row">
                        <label class="field">
                          <span class="label">Status</span>
                          <select class="input" bind:value={draft.status}>
                            <option value="pending">pending</option>
                            <option value="in_progress">in_progress</option>
                            <option value="claimed_done">claimed_done</option>
                            <option value="verified">verified</option>
                            <option value="disputed">disputed</option>
                            <option value="ambiguous">ambiguous</option>
                            <option value="done">done</option>
                            <option value="abandoned">abandoned</option>
                            <option value="superseded">superseded</option>
                          </select>
                        </label>
                        <label class="field">
                          <span class="label">Grade</span>
                          <select class="input" bind:value={draft.grade}>
                            <option value="E">E</option>
                            <option value="D">D</option>
                            <option value="C">C</option>
                            <option value="B">B</option>
                            <option value="A">A</option>
                            <option value="S">S</option>
                          </select>
                        </label>
                      </div>
                      <label class="field">
                        <span class="label">Scope</span>
                        <textarea class="input textarea" rows="2" bind:value={draft.scope}></textarea>
                      </label>
                      <label class="field">
                        <span class="label">Current direction</span>
                        <textarea class="input textarea" rows="2" bind:value={draft.currentDirection}></textarea>
                      </label>
                      <label class="field">
                        <span class="label">Rationale</span>
                        <textarea class="input textarea" rows="2" bind:value={draft.rationale}></textarea>
                      </label>
                      <label class="field">
                        <span class="label">Summary</span>
                        <textarea class="input textarea" rows="2" bind:value={draft.summary}></textarea>
                      </label>
                      <label class="field">
                        <span class="label">Change rationale</span>
                        <input class="input" type="text" bind:value={draft.changeRationale} />
                      </label>
                      <div class="form-actions">
                        <button type="submit" class="primary-btn" disabled={savingQuestId === quest.id}>
                          {savingQuestId === quest.id ? "Saving..." : "Save brief"}
                        </button>
                        <button type="button" class="ghost-btn" onclick={() => resetQuestDraft(quest)}>
                          Reset
                        </button>
                      </div>
                    </form>

                    <div class="refs-panel">
                      <div class="section-title">References</div>
                      {#if refs.length === 0}
                        <div class="muted small">No references.</div>
                      {:else}
                        <div class="refs-list">
                          {#each refs as ref (ref.id)}
                            <div class="ref-row">
                              <span class="ref-type">{ref.refType}</span>
                              <span class="ref-target" title={ref.target}>{ref.label || ref.target}</span>
                              <button
                                type="button"
                                class="icon-btn"
                                onclick={() => deleteQuestRef(quest.id, ref)}
                                disabled={savingRefId === ref.id}
                                title="Delete reference"
                              >
                                ×
                              </button>
                            </div>
                          {/each}
                        </div>
                      {/if}
                      <form class="inline-form" onsubmit={(e) => { e.preventDefault(); submitQuestRef(quest.id); }}>
                        <select class="input compact-input" bind:value={refDraft.refType}>
                          <option value="url">url</option>
                          <option value="linear">linear</option>
                          <option value="github_issue">github_issue</option>
                          <option value="github_pr">github_pr</option>
                          <option value="file">file</option>
                          <option value="artifact">artifact</option>
                        </select>
                        <input class="input compact-input" type="text" bind:value={refDraft.label} placeholder="Label" />
                        <input class="input ref-input" type="text" bind:value={refDraft.target} placeholder="Target" />
                        <button
                          type="submit"
                          class="ghost-btn"
                          disabled={savingRefId === quest.id || !refDraft.target.trim()}
                        >
                          Add
                        </button>
                      </form>
                    </div>

                    {#if report}
                      <div class="report-panel">
                        <div class="section-title">
                          Quest Report
                          {#if report.outcome}
                            <span class="report-outcome outcome-{report.outcome}">{report.outcome}</span>
                          {/if}
                          {#if report.grade}
                            <span class="report-grade">{report.grade}</span>
                          {/if}
                        </div>
                        {#if report.raw}
                          <div class="muted small">Report payload could not be parsed.</div>
                          <pre class="report-raw">{report.raw}</pre>
                        {:else}
                          {#if report.summary}
                            <p class="report-summary">{report.summary}</p>
                          {/if}
                          {#if report.decisions.length > 0}
                            <div class="report-block">
                              <div class="report-label">Decisions</div>
                              <ul class="report-list">
                                {#each report.decisions as d, i (i)}
                                  <li>
                                    <span class="report-decision">{d.decision}</span>
                                    {#if d.rationale}
                                      <span class="report-rationale"> — {d.rationale}</span>
                                    {/if}
                                  </li>
                                {/each}
                              </ul>
                            </div>
                          {/if}
                          {#if report.learned.length > 0}
                            <div class="report-block">
                              <div class="report-label">Learned</div>
                              <ul class="report-list">
                                {#each report.learned as item, i (i)}
                                  <li>{item}</li>
                                {/each}
                              </ul>
                            </div>
                          {/if}
                          {#if report.artifacts.length > 0}
                            <div class="report-block">
                              <div class="report-label">Artifacts</div>
                              <ul class="report-list">
                                {#each report.artifacts as a, i (i)}
                                  <li>
                                    <span class="report-artifact-role">{a.role}</span>
                                    <span class="report-artifact-file">{a.file}</span>
                                  </li>
                                {/each}
                              </ul>
                            </div>
                          {/if}
                          {#if report.open_threads.length > 0}
                            <div class="report-block">
                              <div class="report-label">Open threads</div>
                              <ul class="report-list">
                                {#each report.open_threads as item, i (i)}
                                  <li>{item}</li>
                                {/each}
                              </ul>
                            </div>
                          {/if}
                          {#if report.reflection}
                            <div class="report-block">
                              <div class="report-label">Reflection</div>
                              <p class="report-reflection">{report.reflection}</p>
                            </div>
                          {/if}
                        {/if}
                      </div>
                    {/if}

                    <form class="manual-event-form" onsubmit={(e) => { e.preventDefault(); submitManualEvent(quest.id); }}>
                      <div class="section-title">Add event</div>
                      <div class="inline-form">
                        <select class="input compact-input" bind:value={eventDraft.eventType}>
                          <option value="note">note</option>
                          <option value="blocker">blocker</option>
                          <option value="blocker_resolved">blocker_resolved</option>
                          <option value="question">question</option>
                          <option value="answer">answer</option>
                        </select>
                        <input class="input compact-input" type="text" bind:value={eventDraft.title} placeholder="Title" />
                      </div>
                      <textarea class="input textarea" rows="2" bind:value={eventDraft.text}></textarea>
                      <div class="form-actions">
                        <button
                          type="submit"
                          class="ghost-btn"
                          disabled={savingEventId === quest.id || !eventDraft.text.trim()}
                        >
                          {savingEventId === quest.id ? "Adding..." : "Add event"}
                        </button>
                      </div>
                    </form>
                    <div class="event-log">
                      <div class="log-title">Event log</div>
                      {#if events.length === 0}
                        <div class="muted small">No events.</div>
                      {:else}
                        {#each eventTree(events) as node (node.event.id)}
                          {@const ev = node.event}
                          {#if ev.eventType === "coherent_action"}
                            {@const action = actionPayload(ev.payloadJson)}
                            {@const actionOpen = questState.expandedEventIds.has(ev.id)}
                            <button
                              type="button"
                              class="event-row event-toggle action-row"
                              onclick={() => questStore.toggleEventExpand(agentId, ev.id)}
                              aria-expanded={actionOpen}
                            >
                              <span class="event-disclosure">{actionOpen ? "▾" : "▸"}</span>
                              <span class="action-icon" title="Coherent action">◆</span>
                              <span class="event-type action-type">{action.intent || "Current action"}</span>
                              {#if action.status}
                                <span class="status-chip status-{action.status}">{action.status}</span>
                              {/if}
                              {#if ev.planItemId}
                                <span
                                  class="plan-chip"
                                  title={planTitle(ev.questId, ev.planItemId)}
                                >
                                  {planTitle(ev.questId, ev.planItemId)}
                                </span>
                              {/if}
                              {#if node.children.length}
                                <span class="child-count">{node.children.length}</span>
                              {/if}
                              <span class="muted small">{formatRelative(ev.createdAt)}</span>
                            </button>
                            {#if action.outcome}
                              <div class="action-outcome-inline">{action.outcome}</div>
                            {/if}
                            {#if actionOpen && node.children.length}
                              <div class="event-children">
                                {#each node.children as child (child.id)}
                                  {#if child.eventType === "tool_call"}
                                    {@const tool = toolCallPayload(child.payloadJson)}
                                    {@const toolOpen = questState.expandedEventIds.has(child.id)}
                                    <button
                                      type="button"
                                      class="event-row event-toggle child-row tool-row"
                                      class:error={tool.isError}
                                      onclick={() => questStore.toggleEventExpand(agentId, child.id)}
                                      aria-expanded={toolOpen}
                                    >
                                      <span class="event-disclosure">{toolOpen ? "▾" : "▸"}</span>
                                      <span class="child-marker"></span>
                                      <span class="event-type">{tool.toolName}</span>
                                      {#if tool.status}
                                        <span class="status-chip status-{tool.status}">{tool.status}</span>
                                      {/if}
                                      {#if tool.durationMs != null}
                                        <span class="muted small">{durationLabel(tool.durationMs)}</span>
                                      {/if}
                                    </button>
                                    {#if toolOpen && (tool.argsPreview || tool.resultPreview)}
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
                          {:else if PLAN_EVENT_TYPES.has(ev.eventType)}
                            {@const planEvent = planEventPayload(ev.payloadJson)}
                            <div class="event-row plan-event-row">
                              <span class="plan-event-icon" title="Plan lifecycle">◇</span>
                              <span class="event-type plan-event-type">{planEventLabel(ev.eventType)}</span>
                              {#if planEvent.itemId}
                                <span class="plan-chip">{planTitle(ev.questId, planEvent.itemId)}</span>
                              {:else if planEvent.deletedItemId}
                                <span class="plan-chip muted">deleted item</span>
                              {:else if planEvent.itemIds.length}
                                <span class="child-count">{planEvent.itemIds.length}</span>
                              {/if}
                              {#if planEvent.outcome}
                                <span class="muted small">{planEvent.outcome}</span>
                              {:else if planEvent.reason}
                                <span class="muted small">{planEvent.reason}</span>
                              {:else if planEvent.rationale}
                                <span class="muted small">{planEvent.rationale}</span>
                              {/if}
                              <span class="muted small">{formatRelative(ev.createdAt)}</span>
                            </div>
                          {:else if ev.eventType === "compaction_tick"}
                            {@const cp = parseCompactionPayload(ev.payloadJson)}
                            {@const keeperOpen = questState.expandedEventIds.has(ev.id)}
                            <button
                              type="button"
                              class="event-row event-toggle compaction-row"
                              onclick={() => questStore.toggleEventExpand(agentId, ev.id)}
                              aria-expanded={keeperOpen}
                              title={cp ? keeperHint(cp.trigger) : "Keeper memory summary"}
                            >
                              <span class="event-disclosure">{keeperOpen ? "▾" : "▸"}</span>
                              <span class="compaction-icon" title="Keeper compaction tick">◈</span>
                              <span class="event-type compaction-type">
                                {cp ? keeperLabel(cp.trigger) : "Keeper summary"}
                              </span>
                              <span class="muted small">{ev.actor ?? "—"}</span>
                              <span class="muted small">{formatRelative(ev.createdAt)}</span>
                              {#if cp}
                                <span class="claims-pill" title="{cp.claimsCount} atomic claims persisted">
                                  +{cp.claimsCount} {cp.claimsCount === 1 ? "claim" : "claims"}
                                </span>
                              {/if}
                            </button>
                            {#if keeperOpen && cp}
                              <div class="compaction-summary">{cp.summary || "(no summary returned)"}</div>
                              <div class="compaction-meta muted small">
                                run #{cp.keeperRunId ?? "?"}
                              </div>
                            {:else if keeperOpen && ev.payloadJson}
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
                          {:else if ["scope_change", "direction_change", "quest_rationale_change", "quest_summary_change", "grade_change", "note", "blocker", "blocker_resolved", "question", "answer"].includes(ev.eventType)}
                            {@const text = eventText(ev.payloadJson)}
                            {@const rationale = eventRationale(ev.payloadJson)}
                            <div class="event-row quest-change-row">
                              <span class="quest-change-icon">●</span>
                              <span class="event-type quest-change-type">{manualEventLabel(ev.eventType)}</span>
                              <span class="muted small">{ev.actor ?? "—"}</span>
                              <span class="muted small">{formatRelative(ev.createdAt)}</span>
                            </div>
                            {#if text}
                              <div class="quest-change-summary">{text}</div>
                            {/if}
                            {#if rationale}
                              <div class="quest-change-rationale">{rationale}</div>
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
                          questStore.startCreate(agentId, quest.id);
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
    align-items: flex-start;
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
    min-width: 0;
    font-size: 11px;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .node.expanded > .node-row .title {
    overflow: visible;
    text-overflow: clip;
    white-space: normal;
    overflow-wrap: anywhere;
    line-height: 1.35;
  }

  .ts {
    flex-shrink: 0;
    font-size: 9px;
  }

  /* Detail — inline expansion */
  .detail {
    margin: 2px 0 6px 20px;
    padding: 10px;
    border-left: 2px solid var(--border-subtle);
    background: var(--bg-panel);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .quest-info {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .quest-title-full {
    color: var(--text-primary);
    font-size: 12px;
    font-weight: 600;
    line-height: 1.35;
    overflow-wrap: anywhere;
  }
  .detail-meta {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(112px, 1fr));
    gap: 8px 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }
  .meta-row {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    font-size: 10px;
  }
  .meta-label {
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .meta-value {
    color: var(--text-primary);
    overflow-wrap: anywhere;
  }
  .description {
    margin: 0;
    font-size: 11px;
    color: var(--text-secondary);
    white-space: pre-wrap;
  }
  .plan-panel {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: var(--bg-panel);
  }
  .plan-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .plan-quest-title {
    margin-top: 2px;
    color: var(--text-primary);
    font-size: 11px;
    font-weight: 600;
    line-height: 1.3;
    overflow-wrap: anywhere;
  }
  .plan-add-form {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    gap: 6px;
  }
  .plan-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .plan-item {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 6px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: var(--bg-panel-2);
  }
  .plan-item.status-active {
    border-color: color-mix(in srgb, var(--accent) 55%, var(--border-subtle));
    background: var(--accent-bg-subtle);
  }
  .plan-item.status-completed {
    border-color: rgba(77, 163, 107, 0.45);
  }
  .plan-item.status-blocked {
    border-color: rgba(196, 90, 90, 0.45);
  }
  .plan-item-main {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .plan-index {
    flex-shrink: 0;
    width: 18px;
    height: 18px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--border-subtle);
    border-radius: 50%;
    color: var(--text-muted);
    font-size: 9px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }
  .plan-title-input {
    min-width: 0;
    width: 100%;
    padding: 3px 5px;
    border: 1px solid transparent;
    border-radius: 4px;
    background: var(--bg-panel);
    color: var(--text-primary);
    font: inherit;
    font-size: 11px;
  }
  .plan-title-input:focus {
    border-color: var(--accent);
    outline: none;
  }
  .plan-rationale {
    margin-left: 24px;
    color: var(--text-secondary);
    font-size: 10px;
    line-height: 1.35;
    overflow-wrap: anywhere;
  }
  .plan-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-left: 24px;
  }
  .mini-btn,
  .icon-btn {
    min-height: 22px;
    padding: 2px 7px;
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    background: var(--bg-panel);
    color: var(--text-primary);
    font-size: 10px;
    font-family: inherit;
    cursor: pointer;
  }
  .icon-btn {
    width: 24px;
    padding: 2px 0;
    text-align: center;
  }
  .mini-btn:hover:not(:disabled),
  .icon-btn:hover:not(:disabled) {
    background: var(--bg-sidebar);
  }
  .mini-btn:disabled,
  .icon-btn:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .icon-btn.danger {
    color: #c45a5a;
  }
  .primary-btn.compact,
  .ghost-btn.compact {
    padding: 3px 8px;
  }
  .event-log {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-top: 10px;
    border-top: 1px solid var(--border-subtle);
  }
  .quest-editor,
  .refs-panel,
  .manual-event-form,
  .report-panel {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-top: 10px;
    border-top: 1px solid var(--border-subtle);
  }
  .report-outcome,
  .report-grade {
    margin-left: 6px;
    padding: 1px 5px;
    border-radius: 3px;
    font-size: 9px;
    letter-spacing: normal;
    text-transform: none;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }
  .report-outcome {
    border: 1px solid var(--border-subtle);
    color: var(--text-muted);
  }
  .outcome-done {
    border-color: color-mix(in srgb, var(--accent-positive, #3fb950) 50%, transparent);
    color: var(--accent-positive, #3fb950);
  }
  .outcome-abandoned,
  .outcome-blocked {
    border-color: color-mix(in srgb, var(--accent-negative, #f85149) 50%, transparent);
    color: var(--accent-negative, #f85149);
  }
  .report-grade {
    background: var(--bg-sidebar);
    color: var(--text-primary);
  }
  .report-summary {
    margin: 0;
    font-size: 11px;
    color: var(--text-primary);
  }
  .report-block {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .report-label {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
  }
  .report-list {
    margin: 0;
    padding-left: 16px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 10px;
    color: var(--text-primary);
  }
  .report-rationale {
    color: var(--text-muted);
  }
  .report-artifact-role {
    margin-right: 6px;
    padding: 0 4px;
    border: 1px solid var(--border-subtle);
    border-radius: 3px;
    color: var(--text-muted);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }
  .report-artifact-file {
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }
  .report-reflection {
    margin: 0;
    font-size: 10px;
    font-style: italic;
    color: var(--text-secondary, var(--text-muted));
  }
  .report-raw {
    margin: 0;
    padding: 6px;
    border-radius: 4px;
    background: var(--bg-sidebar);
    font-size: 10px;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .section-title,
  .log-title {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
  }
  .inline-form {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .compact-input {
    width: 120px;
  }
  .ref-input {
    flex: 1;
    min-width: 160px;
  }
  .refs-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .ref-row {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 22px;
    padding: 2px 4px;
    border-radius: 4px;
    background: var(--bg-sidebar);
    font-size: 10px;
  }
  .ref-type {
    flex-shrink: 0;
    padding: 1px 5px;
    border: 1px solid var(--border-subtle);
    border-radius: 3px;
    color: var(--text-muted);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }
  .ref-target {
    flex: 1;
    min-width: 0;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .icon-btn {
    width: 20px;
    height: 20px;
    padding: 0;
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    cursor: pointer;
  }
  .icon-btn:hover:not(:disabled) {
    color: var(--text-primary);
    background: var(--bg-panel-2);
  }
  .event-row {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 10px;
  }
  .event-toggle {
    width: 100%;
    border: none;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .event-toggle:hover {
    background: var(--bg-panel-2);
  }
  .event-disclosure {
    width: 10px;
    flex-shrink: 0;
    color: var(--text-muted);
    font-size: 9px;
    text-align: center;
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
    border-radius: 4px;
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
  .child-count {
    flex-shrink: 0;
    min-width: 16px;
    padding: 1px 5px;
    border: 1px solid var(--border-subtle);
    border-radius: 3px;
    color: var(--text-muted);
    font-size: 9px;
    line-height: 1.2;
    text-align: center;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }
  .plan-chip {
    flex-shrink: 1;
    max-width: 180px;
    min-width: 0;
    padding: 1px 5px;
    border: 1px solid rgba(214, 168, 77, 0.55);
    border-radius: 3px;
    color: #d6a84d;
    background: rgba(214, 168, 77, 0.1);
    font-size: 9px;
    line-height: 1.2;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .plan-event-row {
    min-height: 20px;
    padding: 2px 0;
  }
  .plan-event-icon,
  .plan-event-type {
    color: #d6a84d;
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
    padding: 2px 0;
    border-radius: 4px;
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
    margin-left: 26px;
    padding: 3px 6px;
    border-radius: 3px;
    background: var(--bg-sidebar);
    color: var(--text-secondary);
    font-size: 9px;
    line-height: 1.4;
    overflow-wrap: anywhere;
  }
  .child-payload {
    margin-left: 26px;
  }

  /* MON-100: compaction_tick visual treatment. Subtle accent border +
     dedicated icon set this kind of event apart from quest-status events
     without making it loud. */
  .compaction-row {
    padding: 3px 0;
    border-radius: 4px;
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

  .quest-change-row {
    padding: 3px 0;
  }
  .quest-change-icon {
    color: var(--accent);
    font-size: 8px;
  }
  .quest-change-type {
    color: var(--text-primary);
    font-weight: 600;
  }
  .quest-change-summary,
  .quest-change-rationale {
    margin-left: 18px;
    padding: 3px 6px;
    border-radius: 3px;
    background: var(--bg-sidebar);
    color: var(--text-secondary);
    font-size: 10px;
    line-height: 1.4;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .quest-change-rationale {
    color: var(--text-muted);
    font-style: italic;
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
