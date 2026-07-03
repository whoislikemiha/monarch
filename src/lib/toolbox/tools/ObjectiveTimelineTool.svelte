<script lang="ts">
  import type { ToolProps } from "../types";
  import type { PlanItemRow, ObjectiveEventRow, ObjectiveRefRow, ObjectiveRow } from "../../bindings";
  import { objectiveStore } from "../objectiveStore.svelte";
  import Avatar from "$lib/ui/Avatar.svelte";
  import { agentStore } from "../../stores/agentStore.svelte";

  let { agentContext }: ToolProps = $props();
  let agentId = $derived(agentContext?.agentId ?? "");

  /**
   * Per-agent reactive slice. `ensure` creates an empty entry on first
   * access; `refresh` fills it. We key by agentId because toolbox tools
   * stay mounted across agent switches — a fresh agent needs its own
   * slice, not someone else's stale one.
   */
  let objectiveState = $derived(
    agentContext ? (objectiveStore.byAgent.get(agentId) ?? null) : null,
  );
  let activeObjectiveId = $derived(objectiveState?.workingMemory?.currentObjectiveId ?? null);
  let activeObjective = $derived.by(() => {
    if (!activeObjectiveId || !objectiveState) return null;
    for (const tree of objectiveState.treesByRoot.values()) {
      const found = tree.find((objective) => objective.id === activeObjectiveId);
      if (found) return found;
    }
    return objectiveState.roots.find((objective) => objective.id === activeObjectiveId) ?? null;
  });

  $effect(() => {
    if (agentContext) {
      objectiveStore.ensure(agentId);
      objectiveStore.refresh(agentId);
    }
  });

  $effect(() => {
    if (!agentContext || !activeObjectiveId || objectiveState?.planItemsByObjective.has(activeObjectiveId)) return;
    objectiveStore.loadPlanItems(agentId, activeObjectiveId).catch((e) => {
      if (objectiveState) objectiveState.error = String(e);
    });
  });

  // --- Tree shaping --------------------------------------------------------
  // The backend returns a flat list per root ordered by created_at ASC.
  // Turn it into a parent→children adjacency map so the template can
  // render it depth-first without recursion.

  interface TreeNode {
    objective: ObjectiveRow;
    depth: number;
  }

  function flattenTree(tree: ObjectiveRow[]): TreeNode[] {
    const byParent = new Map<string | null, ObjectiveRow[]>();
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
        out.push({ objective: q, depth });
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

  // --- New-objective form ------------------------------------------------------

  let formTitle = $state("");
  let formDescription = $state("");
  let formGrade = $state<"E" | "D" | "C" | "B" | "A" | "S">("C");
  let formExecHint = $state<"in_context" | "delegate" | "explore">("in_context");
  let formParentId = $state<string>("");
  let formSubmitting = $state(false);
  let markingDoneId = $state<string | null>(null);
  let savingObjectiveId = $state<string | null>(null);
  let savingEventId = $state<string | null>(null);
  let savingRefId = $state<string | null>(null);
  let addingPlanObjectiveId = $state<string | null>(null);
  let planBusyKey = $state<string | null>(null);
  let planDraftTitles = $state(new Map<string, string>());
  let newPlanTitles = $state(new Map<string, string>());

  type ObjectiveEditDraft = {
    status: string;
    grade: "E" | "D" | "C" | "B" | "A" | "S";
    scope: string;
    currentDirection: string;
    rationale: string;
    summary: string;
    changeRationale: string;
  };
  type ObjectiveEventDraft = {
    eventType: "note" | "blocker" | "blocker_resolved" | "question" | "answer";
    title: string;
    text: string;
  };
  type ObjectiveRefDraft = {
    refType: string;
    label: string;
    target: string;
  };

  let objectiveDrafts = $state<Record<string, ObjectiveEditDraft>>({});
  let eventDrafts = $state<Record<string, ObjectiveEventDraft>>({});
  let refDrafts = $state<Record<string, ObjectiveRefDraft>>({});

  function ensureObjectiveDraft(objective: ObjectiveRow): ObjectiveEditDraft {
    const existing = objectiveDrafts[objective.id];
    if (existing) return existing;
    objectiveDrafts[objective.id] = {
      status: objective.status,
      grade: (objective.grade ?? "C") as ObjectiveEditDraft["grade"],
      scope: objective.scope ?? "",
      currentDirection: objective.currentDirection ?? "",
      rationale: objective.rationale ?? "",
      summary: objective.summary ?? "",
      changeRationale: "",
    };
    return objectiveDrafts[objective.id];
  }

  function resetObjectiveDraft(objective: ObjectiveRow) {
    objectiveDrafts[objective.id] = {
      status: objective.status,
      grade: (objective.grade ?? "C") as ObjectiveEditDraft["grade"],
      scope: objective.scope ?? "",
      currentDirection: objective.currentDirection ?? "",
      rationale: objective.rationale ?? "",
      summary: objective.summary ?? "",
      changeRationale: "",
    };
  }

  function ensureEventDraft(objectiveId: string): ObjectiveEventDraft {
    const existing = eventDrafts[objectiveId];
    if (existing) return existing;
    eventDrafts[objectiveId] = { eventType: "note", title: "", text: "" };
    return eventDrafts[objectiveId];
  }

  function ensureRefDraft(objectiveId: string): ObjectiveRefDraft {
    const existing = refDrafts[objectiveId];
    if (existing) return existing;
    refDrafts[objectiveId] = { refType: "url", label: "", target: "" };
    return refDrafts[objectiveId];
  }

  async function saveObjectiveDraft(objective: ObjectiveRow) {
    if (!agentContext || !objectiveState) return;
    const draft = ensureObjectiveDraft(objective);
    savingObjectiveId = objective.id;
    try {
      await objectiveStore.updateObjectiveManual(agentContext.agentId, {
        id: objective.id,
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
      objectiveState.error = String(e);
    } finally {
      savingObjectiveId = null;
    }
  }

  async function submitManualEvent(objectiveId: string) {
    if (!agentContext || !objectiveState) return;
    const draft = ensureEventDraft(objectiveId);
    if (!draft.text.trim()) return;
    savingEventId = objectiveId;
    try {
      await objectiveStore.recordManualObjectiveEvent(agentContext.agentId, {
        objectiveId,
        eventType: draft.eventType,
        title: draft.title.trim() || null,
        text: draft.text.trim(),
        metadataJson: null,
        actor: "monarch",
        author: "captain",
        surfaceOverride: null,
      });
      eventDrafts[objectiveId] = { eventType: draft.eventType, title: "", text: "" };
    } catch (e) {
      objectiveState.error = String(e);
    } finally {
      savingEventId = null;
    }
  }

  async function submitObjectiveRef(objectiveId: string) {
    if (!agentContext || !objectiveState) return;
    const draft = ensureRefDraft(objectiveId);
    if (!draft.target.trim()) return;
    savingRefId = objectiveId;
    try {
      await objectiveStore.createObjectiveRef(agentContext.agentId, {
        id: null,
        objectiveId,
        refType: draft.refType.trim() || "url",
        label: draft.label.trim() || null,
        target: draft.target.trim(),
        metadataJson: null,
        createdBy: "captain",
      });
      refDrafts[objectiveId] = { refType: draft.refType, label: "", target: "" };
    } catch (e) {
      objectiveState.error = String(e);
    } finally {
      savingRefId = null;
    }
  }

  async function deleteObjectiveRef(objectiveId: string, ref: ObjectiveRefRow) {
    if (!agentContext || !objectiveState) return;
    savingRefId = ref.id;
    try {
      await objectiveStore.deleteObjectiveRef(agentContext.agentId, objectiveId, ref.id);
    } catch (e) {
      objectiveState.error = String(e);
    } finally {
      savingRefId = null;
    }
  }

  $effect(() => {
    // When the create form opens, reset fields and preselect parent.
    if (objectiveState?.creating) {
      formTitle = "";
      formDescription = "";
      formGrade = "C";
      formExecHint = "in_context";
      // P1: default new objectives under the campaign root so captured work
      // lands as a pending branch in the tree, not a detached root.
      formParentId =
        objectiveState.creatingParentId ?? objectiveState.campaignRootId ?? "";
    }
  });

  async function submitCreate() {
    if (!agentContext || !objectiveState || !formTitle.trim()) return;
    formSubmitting = true;
    try {
      await objectiveStore.createObjective(agentId, {
        id: null,
        parentId: formParentId || null,
        title: formTitle.trim(),
        description: formDescription.trim() || null,
        status: "pending",
        grade: formGrade,
        execHint: formExecHint,
        assigneeShadowId: agentId,
        createdBy: "monarch",
        kind: null,
      });
      objectiveStore.cancelCreate(agentId);
    } catch (e) {
      objectiveState.error = String(e);
    } finally {
      formSubmitting = false;
    }
  }

  function nowIsoSeconds(): string {
    return new Date().toISOString().replace(/\.\d{3}Z$/, "Z");
  }

  async function markDone(objective: ObjectiveRow) {
    if (!agentContext || !objectiveState || objective.status === "done") return;
    markingDoneId = objective.id;
    try {
      await objectiveStore.updateObjective(agentId, {
        id: objective.id,
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
        completedAt: objective.completedAt ?? nowIsoSeconds(),
        abandonedAt: null,
      });
    } catch (e) {
      objectiveState.error = String(e);
    } finally {
      markingDoneId = null;
    }
  }

  // Pool of parent options: every objective already loaded for this agent
  // (across all roots). Keeps the form simple without a second fetch.
  let parentOptions = $derived.by(() => {
    if (!objectiveState) return [] as ObjectiveRow[];
    const all: ObjectiveRow[] = [];
    for (const tree of objectiveState.treesByRoot.values()) {
      for (const q of tree) all.push(q);
    }
    return all;
  });

  // MON-100: parsed compaction_tick payload. Returns null when the row
  // isn't a Curator tick or the payload doesn't decode — both fall back to
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
    if (trigger === "objective_close") return "Curator objective close";
    if (trigger === "continuous") return "Curator checkpoint";
    return "Curator note";
  }

  function keeperHint(trigger: string): string {
    if (trigger === "objective_close") return "Summary produced when the objective was marked done.";
    if (trigger === "continuous") return "Background memory checkpoint from context compaction.";
    return "Curator memory summary.";
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
    event: ObjectiveEventRow;
    children: ObjectiveEventRow[];
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

  function eventTree(events: ObjectiveEventRow[]): EventNode[] {
    const childrenByParent = new Map<string, ObjectiveEventRow[]>();
    const roots: ObjectiveEventRow[] = [];
    for (const ev of events) {
      // Status is already visible in the objective metadata; the auto-created
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

  // Display labels for persisted actor/role values (the stored values stay
  // canonical for the backend; these are cosmetic only).
  const ACTOR_LABELS: Record<string, string> = {
    monarch: "You",
    captain: "Supervisor",
    keeper: "Curator",
    chat_shadow: "Agent",
  };
  const actorLabel = (actor: string | null | undefined): string =>
    actor ? (ACTOR_LABELS[actor] ?? actor) : "—";

  function manualEventLabel(kind: string): string {
    if (kind === "scope_change") return "scope changed";
    if (kind === "direction_change") return "direction changed";
    if (kind === "objective_rationale_change") return "rationale changed";
    if (kind === "objective_summary_change") return "summary changed";
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

  function planItemsFor(objectiveId: string): PlanItemRow[] {
    return objectiveState?.planItemsByObjective.get(objectiveId) ?? [];
  }

  function planItemById(objectiveId: string, itemId: string | null | undefined): PlanItemRow | null {
    if (!itemId) return null;
    return planItemsFor(objectiveId).find((item) => item.id === itemId) ?? null;
  }

  function planTitle(objectiveId: string, itemId: string | null | undefined): string {
    return planItemById(objectiveId, itemId)?.title ?? "plan item";
  }

  function draftTitle(item: PlanItemRow): string {
    return planDraftTitles.get(item.id) ?? item.title;
  }

  function setDraftTitle(itemId: string, value: string) {
    const next = new Map(planDraftTitles);
    next.set(itemId, value);
    planDraftTitles = next;
  }

  function newPlanTitle(objectiveId: string): string {
    return newPlanTitles.get(objectiveId) ?? "";
  }

  function setNewPlanTitle(objectiveId: string, value: string) {
    const next = new Map(newPlanTitles);
    next.set(objectiveId, value);
    newPlanTitles = next;
  }

  async function runPlanMutation(key: string, fn: () => Promise<void>) {
    if (!agentContext || !objectiveState) return;
    planBusyKey = key;
    try {
      await fn();
    } catch (e) {
      objectiveState.error = String(e);
    } finally {
      planBusyKey = null;
    }
  }

  async function submitAddPlanItem(objectiveId: string) {
    if (!agentContext) return;
    const title = newPlanTitle(objectiveId).trim();
    if (!title) return;
    const items = planItemsFor(objectiveId);
    const afterItemId = items.at(-1)?.id ?? null;
    await runPlanMutation(`add:${objectiveId}`, async () => {
      await objectiveStore.addPlanItem(agentId, {
        objectiveId,
        title,
        afterItemId,
        createdBy: "captain",
      });
      setNewPlanTitle(objectiveId, "");
      addingPlanObjectiveId = null;
    });
  }

  async function commitPlanTitle(objectiveId: string, item: PlanItemRow) {
    if (!agentContext) return;
    const title = draftTitle(item).trim();
    if (!title || title === item.title) return;
    await runPlanMutation(`edit:${item.id}`, () =>
      objectiveStore.updatePlanItem(agentId, objectiveId, {
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

{#snippet planPanel(panelObjective: ObjectiveRow, planItems: PlanItemRow[], agentId: string)}
  <div class="plan-panel">
    <div class="plan-header">
      <div>
        <div class="log-title">Active plan</div>
        <div class="plan-objective-title">{panelObjective.title}</div>
      </div>
      {#if addingPlanObjectiveId !== panelObjective.id}
        <button
          type="button"
          class="mini-btn"
          onclick={() => {
            addingPlanObjectiveId = panelObjective.id;
            setNewPlanTitle(panelObjective.id, "");
          }}
        >
          + Item
        </button>
      {/if}
    </div>
    {#if addingPlanObjectiveId === panelObjective.id}
      <form
        class="plan-add-form"
        onsubmit={(e) => {
          e.preventDefault();
          submitAddPlanItem(panelObjective.id);
        }}
      >
        <input
          class="input plan-title-input"
          type="text"
          value={newPlanTitle(panelObjective.id)}
          oninput={(e) => setNewPlanTitle(panelObjective.id, e.currentTarget.value)}
          placeholder="Next step"
          disabled={planBusyKey === `add:${panelObjective.id}`}
        />
        <button
          type="submit"
          class="primary-btn compact"
          disabled={!newPlanTitle(panelObjective.id).trim() || planBusyKey === `add:${panelObjective.id}`}
        >
          Add
        </button>
        <button
          type="button"
          class="ghost-btn compact"
          onclick={() => (addingPlanObjectiveId = null)}
          disabled={planBusyKey === `add:${panelObjective.id}`}
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
                onchange={() => commitPlanTitle(panelObjective.id, item)}
                onkeydown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    commitPlanTitle(panelObjective.id, item);
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
                    objectiveStore.movePlanItem(agentId, panelObjective.id, item.id, -1),
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
                    objectiveStore.movePlanItem(agentId, panelObjective.id, item.id, 1),
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
                    objectiveStore.startPlanItem(agentId, panelObjective.id, item.id),
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
                    objectiveStore.completePlanItem(agentId, panelObjective.id, item.id, outcome),
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
                    objectiveStore.skipPlanItem(agentId, panelObjective.id, item.id, reason),
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
                    objectiveStore.blockPlanItem(agentId, panelObjective.id, item.id, reason),
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
                    objectiveStore.deletePlanItem(agentId, panelObjective.id, item.id),
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

<div class="objective-tool">
  {#if !agentContext || !objectiveState}
    <p class="empty">No agent selected.</p>
  {:else}
    <!-- Header: create button + status -->
    <div class="header">
      {#if !objectiveState.creating}
        <button
          class="new-btn"
          type="button"
          onclick={() => objectiveStore.startCreate(agentId)}
        >
          + New objective
        </button>
      {:else}
        <span class="header-title">New objective</span>
      {/if}
      {#if objectiveState.loading}<span class="muted">Loading…</span>{/if}
    </div>

    {#if objectiveState.error}
      <p class="error-msg">{objectiveState.error}</p>
    {/if}

    <!-- Create form -->
    {#if objectiveState.creating}
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
            <option value="">— none (root objective)</option>
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
            onclick={() => objectiveStore.cancelCreate(agentId)}
            disabled={formSubmitting}
          >
            Cancel
          </button>
        </div>
      </form>
    {/if}

    {#if activeObjective}
      {@const activePlanItems = planItemsFor(activeObjective.id)}
      {@render planPanel(activeObjective, activePlanItems, agentId)}
    {/if}

    <!-- Timeline -->
    {#if objectiveState.roots.length === 0 && !objectiveState.loading}
      <p class="empty">No objectives yet for this agent.</p>
    {:else}
      <div class="timeline">
        {#each objectiveState.roots as root (root.id)}
          {@const tree = objectiveState.treesByRoot.get(root.id) ?? [root]}
          {@const flat = flattenTree(tree)}
          <div class="root">
            {#each flat as { objective, depth } (objective.id)}
              {@const expanded = objectiveState.expandedObjectiveIds.has(objective.id)}
              {@const events = objectiveState.eventsByObjective.get(objective.id) ?? []}
              {@const refs = objectiveState.refsByObjective.get(objective.id) ?? []}
              {@const report = objectiveState.reportsByObjective.get(objective.id) ?? null}
              <div
                class="node"
                class:expanded
                style="margin-left: {depth * 14}px"
              >
                <button
                  type="button"
                  class="node-row"
                  onclick={() => objectiveStore.toggleExpand(agentId, objective.id)}
                  aria-expanded={expanded}
                >
                  <span class="disclosure">{expanded ? "▾" : "▸"}</span>
                  {#if objective.assigneeShadowId}
                    {@const assignee = agentStore.getAgent(objective.assigneeShadowId)}
                    <span class="avatar">
                      <Avatar
                        name={assignee?.name ?? objective.assigneeShadowId}
                        size={18}
                        avatarType={assignee?.avatarType}
                        avatarPath={assignee?.avatarPath}
                      />
                    </span>
                  {/if}
                  <span
                    class="status-dot"
                    style="background:{STATUS_COLOR[objective.status] ?? 'var(--text-muted)'}"
                    title={objective.status}
                  ></span>
                  {#if objective.grade}
                    <span class="grade">{objective.grade}</span>
                  {/if}
                  <span class="title">{objective.title}</span>
                  <span class="ts muted">{formatRelative(objective.createdAt)}</span>
                </button>
                {#if expanded}
                  {@const draft = ensureObjectiveDraft(objective)}
                  {@const eventDraft = ensureEventDraft(objective.id)}
                  {@const refDraft = ensureRefDraft(objective.id)}
                  <div class="detail">
                    <div class="objective-info">
                      <div class="objective-title-full">{objective.title}</div>
                      <div class="detail-meta">
                        <span class="meta-row">
                          <span class="meta-label">Status</span>
                          <span class="meta-value">{objective.status}</span>
                        </span>
                        {#if objective.grade}
                          <span class="meta-row">
                            <span class="meta-label">Grade</span>
                            <span class="meta-value">{objective.grade}</span>
                          </span>
                        {/if}
                        {#if objective.execHint}
                          <span class="meta-row">
                            <span class="meta-label">Exec</span>
                            <span class="meta-value">{objective.execHint}</span>
                          </span>
                        {/if}
                        <span class="meta-row">
                          <span class="meta-label">Created by</span>
                          <span class="meta-value">{objective.createdBy}</span>
                        </span>
                        {#if objective.assigneeShadowId}
                          <span class="meta-row">
                            <span class="meta-label">Assignee</span>
                            <span class="meta-value" title={objective.assigneeShadowId}>
                              {assigneeLabel(objective.assigneeShadowId)}
                            </span>
                          </span>
                        {/if}
                        <span class="meta-row">
                          <span class="meta-label">Created</span>
                          <span class="meta-value">{formatDateTime(objective.createdAt)}</span>
                        </span>
                        {#if objective.startedAt}
                          <span class="meta-row">
                            <span class="meta-label">Started</span>
                            <span class="meta-value">{formatDateTime(objective.startedAt)}</span>
                          </span>
                        {/if}
                        {#if objective.completedAt}
                          <span class="meta-row">
                            <span class="meta-label">Completed</span>
                            <span class="meta-value">{formatDateTime(objective.completedAt)}</span>
                          </span>
                        {/if}
                      </div>
                      {#if objective.description}
                        <p class="description">{objective.description}</p>
                      {/if}
                    </div>

                    <form class="objective-editor" onsubmit={(e) => { e.preventDefault(); saveObjectiveDraft(objective); }}>
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
                        <button type="submit" class="primary-btn" disabled={savingObjectiveId === objective.id}>
                          {savingObjectiveId === objective.id ? "Saving..." : "Save brief"}
                        </button>
                        <button type="button" class="ghost-btn" onclick={() => resetObjectiveDraft(objective)}>
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
                                onclick={() => deleteObjectiveRef(objective.id, ref)}
                                disabled={savingRefId === ref.id}
                                title="Delete reference"
                              >
                                ×
                              </button>
                            </div>
                          {/each}
                        </div>
                      {/if}
                      <form class="inline-form" onsubmit={(e) => { e.preventDefault(); submitObjectiveRef(objective.id); }}>
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
                          disabled={savingRefId === objective.id || !refDraft.target.trim()}
                        >
                          Add
                        </button>
                      </form>
                    </div>

                    {#if report}
                      <div class="report-panel">
                        <div class="section-title">
                          Objective Report
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

                    <form class="manual-event-form" onsubmit={(e) => { e.preventDefault(); submitManualEvent(objective.id); }}>
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
                          disabled={savingEventId === objective.id || !eventDraft.text.trim()}
                        >
                          {savingEventId === objective.id ? "Adding..." : "Add event"}
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
                            {@const actionOpen = objectiveState.expandedEventIds.has(ev.id)}
                            <button
                              type="button"
                              class="event-row event-toggle action-row"
                              onclick={() => objectiveStore.toggleEventExpand(agentId, ev.id)}
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
                                  title={planTitle(ev.objectiveId, ev.planItemId)}
                                >
                                  {planTitle(ev.objectiveId, ev.planItemId)}
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
                                    {@const toolOpen = objectiveState.expandedEventIds.has(child.id)}
                                    <button
                                      type="button"
                                      class="event-row event-toggle child-row tool-row"
                                      class:error={tool.isError}
                                      onclick={() => objectiveStore.toggleEventExpand(agentId, child.id)}
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
                                <span class="plan-chip">{planTitle(ev.objectiveId, planEvent.itemId)}</span>
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
                            {@const keeperOpen = objectiveState.expandedEventIds.has(ev.id)}
                            <button
                              type="button"
                              class="event-row event-toggle compaction-row"
                              onclick={() => objectiveStore.toggleEventExpand(agentId, ev.id)}
                              aria-expanded={keeperOpen}
                              title={cp ? keeperHint(cp.trigger) : "Curator memory summary"}
                            >
                              <span class="event-disclosure">{keeperOpen ? "▾" : "▸"}</span>
                              <span class="compaction-icon" title="Curator compaction tick">◈</span>
                              <span class="event-type compaction-type">
                                {cp ? keeperLabel(cp.trigger) : "Curator summary"}
                              </span>
                              <span class="muted small">{actorLabel(ev.actor)}</span>
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
                              <span class="muted small">{actorLabel(ev.actor)}</span>
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
                          {:else if ["scope_change", "direction_change", "objective_rationale_change", "objective_summary_change", "grade_change", "note", "blocker", "blocker_resolved", "question", "answer"].includes(ev.eventType)}
                            {@const text = eventText(ev.payloadJson)}
                            {@const rationale = eventRationale(ev.payloadJson)}
                            <div class="event-row objective-change-row">
                              <span class="objective-change-icon">●</span>
                              <span class="event-type objective-change-type">{manualEventLabel(ev.eventType)}</span>
                              <span class="muted small">{actorLabel(ev.actor)}</span>
                              <span class="muted small">{formatRelative(ev.createdAt)}</span>
                            </div>
                            {#if text}
                              <div class="objective-change-summary">{text}</div>
                            {/if}
                            {#if rationale}
                              <div class="objective-change-rationale">{rationale}</div>
                            {/if}
                          {:else}
                            <div class="event-row">
                              <span class="event-type">{ev.eventType}</span>
                              <span class="muted small">{actorLabel(ev.actor)}</span>
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
                      {#if objective.status !== "done" && objective.kind !== "campaign"}
                        <button
                          type="button"
                          class="done-btn"
                          onclick={(e) => {
                            e.stopPropagation();
                            markDone(objective);
                          }}
                          disabled={markingDoneId === objective.id}
                        >
                          {markingDoneId === objective.id ? "Closing..." : "Mark done"}
                        </button>
                      {/if}
                      <button
                        type="button"
                        class="ghost-btn"
                        onclick={(e) => {
                          e.stopPropagation();
                          objectiveStore.startCreate(agentId, objective.id);
                        }}
                      >
                        + Sub-objective
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
  .objective-tool {
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
  .objective-info {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .objective-title-full {
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
  .plan-objective-title {
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
  .objective-editor,
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
     dedicated icon set this kind of event apart from objective-status events
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

  .objective-change-row {
    padding: 3px 0;
  }
  .objective-change-icon {
    color: var(--accent);
    font-size: 8px;
  }
  .objective-change-type {
    color: var(--text-primary);
    font-weight: 600;
  }
  .objective-change-summary,
  .objective-change-rationale {
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
  .objective-change-rationale {
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
