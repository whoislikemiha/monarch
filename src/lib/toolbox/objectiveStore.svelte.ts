import { SvelteMap, SvelteSet } from "svelte/reactivity";
import { invoke, listen, type UnlistenFn } from "$lib/api";
import type {
  AddPlanItemPayload,
  CreateObjectiveRefPayload,
  CreateObjectivePayload,
  ManualObjectiveEventPayload,
  ManualObjectiveUpdatePayload,
  PlanItemInput,
  PlanItemRow,
  ObjectiveEventRow,
  ObjectiveRefRow,
  ObjectiveReportRow,
  ObjectiveRow,
  UpdatePlanItemPayload,
  UpdateObjectiveRefPayload,
  UpdateObjectivePayload,
  WorkingMemoryPayload,
} from "../bindings";

/**
 * P6 Slice C (MON-121): the parsed first-person objective report. Mirrors the
 * snake-case shape the executor's `complete_objective` tool emits and Rust stores
 * verbatim in `objective_reports.payload` (see `sidecar/src/report-tools.ts` and
 * `src-tauri/src/sidecar_protocol.rs` `ObjectiveReport`). Read-only here — the
 * report is the shadow's artifact, not captain-editable.
 */
export interface ObjectiveReportView {
  summary: string;
  outcome: string;
  decisions: { decision: string; rationale?: string | null }[];
  learned: string[];
  artifacts: { file: string; role: string }[];
  open_threads: string[];
  reflection: string;
  grade: string;
  /** Grade/outcome metadata from the row, useful for "when closed" context. */
  distilledByKeeperRunId: number | null;
  updatedAt: string;
  /** Set when the payload JSON failed to parse — raw text for fallback render. */
  raw?: string;
}

export function parseObjectiveReport(row: ObjectiveReportRow): ObjectiveReportView {
  const base: ObjectiveReportView = {
    summary: "",
    outcome: "",
    decisions: [],
    learned: [],
    artifacts: [],
    open_threads: [],
    reflection: "",
    grade: "",
    distilledByKeeperRunId: row.distilledByKeeperRunId,
    updatedAt: row.updatedAt,
  };
  try {
    const p = JSON.parse(row.payload) as Partial<ObjectiveReportView>;
    return {
      ...base,
      summary: typeof p.summary === "string" ? p.summary : "",
      outcome: typeof p.outcome === "string" ? p.outcome : "",
      decisions: Array.isArray(p.decisions) ? p.decisions : [],
      learned: Array.isArray(p.learned) ? p.learned : [],
      artifacts: Array.isArray(p.artifacts) ? p.artifacts : [],
      open_threads: Array.isArray(p.open_threads) ? p.open_threads : [],
      reflection: typeof p.reflection === "string" ? p.reflection : "",
      grade: typeof p.grade === "string" ? p.grade : "",
    };
  } catch {
    // Malformed payload must not crash the timeline — keep the raw text so the
    // captain at least sees something, and degrade the structured view.
    return { ...base, raw: row.payload };
  }
}

/**
 * MON-83: per-agent objective state. Keyed by agentId because toolbox tools
 * stay mounted across agent switches (CLAUDE.md) — we don't want one
 * tool instance stomping another's loading state.
 *
 * Tree shape is derived on demand from `tree` (flat list from the backend
 * `get_objective_tree_for_root` call). Sub-objectives with a different assignee
 * still show up inside their root's tree — that's intentional so a
 * Monarch looking at a shadow sees the full context the shadow works
 * inside, not just the nodes the shadow "owns".
 */
export interface AgentObjectiveState {
  agentId: string;
  /** Roots where this agent is the direct assignee, newest first. */
  roots: ObjectiveRow[];
  /** Full tree per root_id (flat list ordered by created_at ASC). */
  treesByRoot: SvelteMap<string, ObjectiveRow[]>;
  /** Events per objective_id, lazily loaded on expand. */
  eventsByObjective: SvelteMap<string, ObjectiveEventRow[]>;
  /** Durable execution plan items per objective_id, lazily loaded beside events. */
  planItemsByObjective: SvelteMap<string, PlanItemRow[]>;
  /** External references per objective_id, lazily loaded with objective details. */
  refsByObjective: SvelteMap<string, ObjectiveRefRow[]>;
  /**
   * P6 Slice C: first-person objective report per objective_id, lazily loaded on
   * expand. A present `null` value means "loaded, no report exists"; an
   * absent key means "not yet fetched".
   */
  reportsByObjective: SvelteMap<string, ObjectiveReportView | null>;
  /** Objective ids currently expanded inline in the timeline view. */
  expandedObjectiveIds: SvelteSet<string>;
  /** Objective event ids expanded inside an open objective. */
  expandedEventIds: SvelteSet<string>;
  /** L2 v0: current action + recent actions for quick "what now?" UI. */
  workingMemory: WorkingMemoryPayload | null;
  /** True while the create-objective form is visible. */
  creating: boolean;
  /** Optional parent id preselected when opening the create form. */
  creatingParentId: string | null;
  loading: boolean;
  error: string | null;
}

/** Private per-agent subscription handles so we can tear them down on clear. */
interface AgentSubs {
  unlisten: Array<Promise<UnlistenFn>>;
}

class ObjectiveStore {
  readonly byAgent = new SvelteMap<string, AgentObjectiveState>();
  private subs = new Map<string, AgentSubs>();
  private workingMemoryUnavailable = false;

  ensure(agentId: string): AgentObjectiveState {
    const existing = this.byAgent.get(agentId);
    if (existing) return existing;
    // `$state` must sit in a variable declaration initializer; the entry
    // then lives in the SvelteMap which stays reactive on its own.
    const entry: AgentObjectiveState = $state({
      agentId,
      roots: [],
      treesByRoot: new SvelteMap(),
      eventsByObjective: new SvelteMap(),
      planItemsByObjective: new SvelteMap(),
      refsByObjective: new SvelteMap(),
      reportsByObjective: new SvelteMap(),
      expandedObjectiveIds: new SvelteSet(),
      expandedEventIds: new SvelteSet(),
      workingMemory: null,
      creating: false,
      creatingParentId: null,
      loading: false,
      error: null,
    });
    this.byAgent.set(agentId, entry);
    return entry;
  }

  /**
   * Load (or reload) this agent's objective roots + their trees.
   * Also subscribes to per-root `objective-updated-{rootId}` / `objective-created-*`
   * channels so another Monarch client's writes show up here.
   */
  async refresh(agentId: string): Promise<void> {
    const entry = this.ensure(agentId);
    entry.loading = true;
    entry.error = null;
    try {
      const all = await invoke<ObjectiveRow[]>("db_list_objectives_for_agent", {
        agentId,
      });
      // Keep only roots (parent_id null) in the timeline header list.
      const roots = all.filter((q) => q.parentId === null);
      entry.roots = roots;

      // Fetch each root's tree in parallel so sub-objectives are visible even
      // when the assignee differs.
      const trees = await Promise.all(
        roots.map((r) =>
          invoke<ObjectiveRow[]>("db_get_objective_tree_for_root", { rootId: r.id }),
        ),
      );
      entry.treesByRoot.clear();
      roots.forEach((r, i) => entry.treesByRoot.set(r.id, trees[i]));
      await this.refreshWorkingMemory(agentId);

      // Resubscribe event listeners for the current set of roots.
      this.wireRootSubscriptions(entry);
    } catch (e) {
      entry.error = String(e);
    } finally {
      entry.loading = false;
    }
  }

  async createObjective(
    agentId: string,
    payload: CreateObjectivePayload,
  ): Promise<string> {
    // Default the assignee to the current agent so the created objective
    // shows up in the timeline immediately.
    const withAssignee: CreateObjectivePayload = {
      ...payload,
      assigneeShadowId: payload.assigneeShadowId ?? agentId,
    };
    const id = await invoke<string>("db_create_objective", {
      payload: withAssignee,
    });
    await this.refresh(agentId);
    return id;
  }

  async updateObjective(agentId: string, payload: UpdateObjectivePayload): Promise<void> {
    await invoke("db_update_objective", { payload });
    await this.refresh(agentId);
  }

  async updateObjectiveManual(
    agentId: string,
    payload: ManualObjectiveUpdatePayload,
  ): Promise<void> {
    await invoke("db_update_objective_manual", { payload });
    await this.refresh(agentId);
    if (this.ensure(agentId).expandedObjectiveIds.has(payload.id)) {
      await this.loadEvents(agentId, payload.id);
    }
  }

  async recordManualObjectiveEvent(
    agentId: string,
    payload: ManualObjectiveEventPayload,
  ): Promise<string> {
    const id = await invoke<string>("db_record_manual_objective_event", { payload });
    await this.loadEvents(agentId, payload.objectiveId);
    return id;
  }

  async loadEvents(agentId: string, objectiveId: string): Promise<void> {
    const entry = this.ensure(agentId);
    const events = await invoke<ObjectiveEventRow[]>("db_list_objective_events", {
      objectiveId,
    });
    entry.eventsByObjective.set(objectiveId, events);
  }

  async loadPlanItems(agentId: string, objectiveId: string): Promise<void> {
    const entry = this.ensure(agentId);
    const items = await invoke<PlanItemRow[]>("db_list_plan_items", {
      objectiveId,
    });
    entry.planItemsByObjective.set(objectiveId, items);
  }

  async loadRefs(agentId: string, objectiveId: string): Promise<void> {
    const entry = this.ensure(agentId);
    const refs = await invoke<ObjectiveRefRow[]>("db_list_objective_refs", { objectiveId });
    entry.refsByObjective.set(objectiveId, refs);
  }

  /**
   * P6 Slice C: load this objective's first-person report (if any). Stores `null`
   * when no report exists so the UI can distinguish "loaded, empty" from
   * "not yet fetched". Backend command shipped in Slice A (MON-119).
   */
  async loadReport(agentId: string, objectiveId: string): Promise<void> {
    const entry = this.ensure(agentId);
    const row = await invoke<ObjectiveReportRow | null>("db_get_objective_report", {
      objectiveId,
    });
    entry.reportsByObjective.set(objectiveId, row ? parseObjectiveReport(row) : null);
  }

  async createObjectiveRef(
    agentId: string,
    payload: CreateObjectiveRefPayload,
  ): Promise<string> {
    const id = await invoke<string>("db_create_objective_ref", { payload });
    await this.loadRefs(agentId, payload.objectiveId);
    return id;
  }

  async updateObjectiveRef(
    agentId: string,
    objectiveId: string,
    payload: UpdateObjectiveRefPayload,
  ): Promise<void> {
    await invoke("db_update_objective_ref", { payload });
    await this.loadRefs(agentId, objectiveId);
  }

  async deleteObjectiveRef(
    agentId: string,
    objectiveId: string,
    refId: string,
  ): Promise<void> {
    await invoke("db_delete_objective_ref", { refId });
    await this.loadRefs(agentId, objectiveId);
  }

  async refreshObjectivePlan(agentId: string, objectiveId: string): Promise<void> {
    const entry = this.ensure(agentId);
    await this.loadPlanItems(agentId, objectiveId);
    if (entry.expandedObjectiveIds.has(objectiveId)) {
      await this.loadEvents(agentId, objectiveId);
    } else {
      entry.eventsByObjective.delete(objectiveId);
    }
    await this.refreshWorkingMemory(agentId);
  }

  private async refreshAfterPlanMutation(agentId: string, objectiveId: string): Promise<void> {
    const entry = this.ensure(agentId);
    try {
      await this.refreshObjectivePlan(agentId, objectiveId);
    } catch (e) {
      entry.error = String(e);
    }
  }

  async addPlanItem(
    agentId: string,
    payload: AddPlanItemPayload,
  ): Promise<string> {
    const id = await invoke<string>("db_add_plan_item", {
      payload: { ...payload, createdBy: payload.createdBy ?? "captain" },
    });
    await this.refreshAfterPlanMutation(agentId, payload.objectiveId);
    return id;
  }

  async updatePlanItem(
    agentId: string,
    objectiveId: string,
    payload: UpdatePlanItemPayload,
  ): Promise<void> {
    await invoke("db_update_plan_item", { payload });
    await this.refreshAfterPlanMutation(agentId, objectiveId);
  }

  async deletePlanItem(
    agentId: string,
    objectiveId: string,
    itemId: string,
  ): Promise<void> {
    await invoke("db_delete_plan_item", { itemId });
    await this.refreshAfterPlanMutation(agentId, objectiveId);
  }

  async movePlanItem(
    agentId: string,
    objectiveId: string,
    itemId: string,
    direction: -1 | 1,
  ): Promise<void> {
    const entry = this.ensure(agentId);
    let items = entry.planItemsByObjective.get(objectiveId);
    if (!items) {
      await this.loadPlanItems(agentId, objectiveId);
      items = entry.planItemsByObjective.get(objectiveId) ?? [];
    }
    const next = [...items];
    const index = next.findIndex((item) => item.id === itemId);
    const swapIndex = index + direction;
    if (index < 0 || swapIndex < 0 || swapIndex >= next.length) return;
    [next[index], next[swapIndex]] = [next[swapIndex], next[index]];
    const payloadItems: PlanItemInput[] = next.map((item) => ({
      id: item.id,
      title: item.title,
      rationale: item.rationale,
      status: item.status,
      parentId: item.parentId,
    }));
    await invoke("db_set_plan", {
      payload: {
        objectiveId,
        items: payloadItems,
        createdBy: "captain",
        rationale: "manual reorder",
      },
    });
    await this.refreshAfterPlanMutation(agentId, objectiveId);
  }

  async startPlanItem(agentId: string, objectiveId: string, itemId: string): Promise<void> {
    await invoke("db_start_plan_item", { itemId });
    await this.refreshAfterPlanMutation(agentId, objectiveId);
  }

  async completePlanItem(
    agentId: string,
    objectiveId: string,
    itemId: string,
    outcome: string | null = null,
  ): Promise<void> {
    await invoke("db_complete_plan_item", { itemId, outcome });
    await this.refreshAfterPlanMutation(agentId, objectiveId);
  }

  async skipPlanItem(
    agentId: string,
    objectiveId: string,
    itemId: string,
    reason: string | null = null,
  ): Promise<void> {
    await invoke("db_skip_plan_item", { itemId, reason });
    await this.refreshAfterPlanMutation(agentId, objectiveId);
  }

  async blockPlanItem(
    agentId: string,
    objectiveId: string,
    itemId: string,
    reason: string,
  ): Promise<void> {
    await invoke("db_block_plan_item", { itemId, reason });
    await this.refreshAfterPlanMutation(agentId, objectiveId);
  }

  async refreshWorkingMemory(agentId: string): Promise<void> {
    const entry = this.ensure(agentId);
    if (this.workingMemoryUnavailable) return;
    try {
      entry.workingMemory = await invoke<WorkingMemoryPayload | null>(
        "db_get_working_memory",
        { agentId },
      );
    } catch (e) {
      if (String(e).includes("db_get_working_memory")) {
        // A frontend HMR refresh can briefly talk to an older Rust process
        // that does not have MON-109's read command yet. L2 is optional for
        // rendering the objective tree, so keep the timeline usable and let the
        // command appear after a full Tauri restart.
        this.workingMemoryUnavailable = true;
        entry.workingMemory = null;
        return;
      }
      entry.error = String(e);
    }
  }

  toggleExpand(agentId: string, objectiveId: string): void {
    const entry = this.ensure(agentId);
    if (entry.expandedObjectiveIds.has(objectiveId)) {
      entry.expandedObjectiveIds.delete(objectiveId);
    } else {
      entry.expandedObjectiveIds.add(objectiveId);
      // Lazy-load detail slices on first expand.
      if (!entry.eventsByObjective.has(objectiveId)) {
        this.loadEvents(agentId, objectiveId).catch((e) => {
          entry.error = String(e);
        });
      }
      if (!entry.planItemsByObjective.has(objectiveId)) {
        this.loadPlanItems(agentId, objectiveId).catch((e) => {
          entry.error = String(e);
        });
      }
      if (!entry.refsByObjective.has(objectiveId)) {
        this.loadRefs(agentId, objectiveId).catch((e) => {
          entry.error = String(e);
        });
      }
      if (!entry.reportsByObjective.has(objectiveId)) {
        this.loadReport(agentId, objectiveId).catch((e) => {
          entry.error = String(e);
        });
      }
    }
  }

  toggleEventExpand(agentId: string, eventId: string): void {
    const entry = this.ensure(agentId);
    if (entry.expandedEventIds.has(eventId)) {
      entry.expandedEventIds.delete(eventId);
    } else {
      entry.expandedEventIds.add(eventId);
    }
  }

  startCreate(agentId: string, parentId: string | null = null): void {
    const entry = this.ensure(agentId);
    entry.creating = true;
    entry.creatingParentId = parentId;
  }

  cancelCreate(agentId: string): void {
    const entry = this.ensure(agentId);
    entry.creating = false;
    entry.creatingParentId = null;
  }

  clear(agentId: string): void {
    const subs = this.subs.get(agentId);
    if (subs) {
      subs.unlisten.forEach((p) => p.then((fn) => fn()).catch(() => {}));
      this.subs.delete(agentId);
    }
    this.byAgent.delete(agentId);
  }

  /**
   * Rebuild the event subscription set for this agent's current roots.
   * We listen per-root-id on `objective-updated-{rootId}` / `objective-event-{rootId}`;
   * sub-objective updates still trigger a re-fetch on the root they belong to,
   * which refreshes the whole tree. `objective-created-{id}` fires once per new
   * node — we watch the root ids we already know about and fire a broad
   * refresh when a creation for an unknown id arrives (the root is us or a
   * sibling and a full list refresh is the simplest reconciliation).
   */
  private wireRootSubscriptions(entry: AgentObjectiveState): void {
    // Tear down previous subs for this agent.
    const prev = this.subs.get(entry.agentId);
    if (prev) {
      prev.unlisten.forEach((p) => p.then((fn) => fn()).catch(() => {}));
    }
    const subs: AgentSubs = { unlisten: [] };
    subs.unlisten.push(
      listen<string>(`objective-created-for-agent-${entry.agentId}`, () =>
        this.refresh(entry.agentId),
      ),
    );
    const objectiveIds = new Set<string>();
    for (const tree of entry.treesByRoot.values()) {
      for (const objective of tree) objectiveIds.add(objective.id);
    }
    for (const root of entry.roots) objectiveIds.add(root.id);

    for (const root of entry.roots) {
      subs.unlisten.push(
        listen<string>(`objective-updated-${root.id}`, () =>
          this.refresh(entry.agentId),
        ),
      );
    }

    for (const objectiveId of objectiveIds) {
      subs.unlisten.push(
        listen<string>(`objective-event-${objectiveId}`, () => {
          // Events on any visible objective: invalidate that objective's event cache
          // and refresh L2, because current/recent action pointers are updated
          // by the same persistence path.
          entry.eventsByObjective.delete(objectiveId);
          if (entry.expandedObjectiveIds.has(objectiveId)) {
            this.loadEvents(entry.agentId, objectiveId).catch((e) => {
              entry.error = String(e);
            });
          }
          this.loadPlanItems(entry.agentId, objectiveId).catch((e) => {
            entry.error = String(e);
          });
          this.refreshWorkingMemory(entry.agentId).catch((e) => {
            entry.error = String(e);
          });
        }),
      );
      subs.unlisten.push(
        listen<string>(`objective-refs-${objectiveId}`, () => {
          if (entry.expandedObjectiveIds.has(objectiveId)) {
            this.loadRefs(entry.agentId, objectiveId).catch((e) => {
              entry.error = String(e);
            });
          } else {
            entry.refsByObjective.delete(objectiveId);
          }
        }),
      );
      // P6 Slice C: a `complete_objective` write emits `objective-report-{objectiveId}`.
      // Refresh the report when the objective is open; otherwise drop the cache so
      // the next expand re-fetches.
      subs.unlisten.push(
        listen<string>(`objective-report-${objectiveId}`, () => {
          if (entry.expandedObjectiveIds.has(objectiveId)) {
            this.loadReport(entry.agentId, objectiveId).catch((e) => {
              entry.error = String(e);
            });
          } else {
            entry.reportsByObjective.delete(objectiveId);
          }
        }),
      );
    }
    this.subs.set(entry.agentId, subs);
  }
}

export const objectiveStore = new ObjectiveStore();
