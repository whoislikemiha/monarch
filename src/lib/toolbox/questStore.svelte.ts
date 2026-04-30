import { SvelteMap, SvelteSet } from "svelte/reactivity";
import { invoke, listen, type UnlistenFn } from "$lib/api";
import type {
  AddPlanItemPayload,
  CreateQuestRefPayload,
  CreateQuestPayload,
  ManualQuestEventPayload,
  ManualQuestUpdatePayload,
  PlanItemInput,
  PlanItemRow,
  QuestEventRow,
  QuestRefRow,
  QuestRow,
  UpdatePlanItemPayload,
  UpdateQuestRefPayload,
  UpdateQuestPayload,
  WorkingMemoryPayload,
} from "../bindings";

/**
 * MON-83: per-agent quest state. Keyed by agentId because toolbox tools
 * stay mounted across agent switches (CLAUDE.md) — we don't want one
 * tool instance stomping another's loading state.
 *
 * Tree shape is derived on demand from `tree` (flat list from the backend
 * `get_quest_tree_for_root` call). Sub-quests with a different assignee
 * still show up inside their root's tree — that's intentional so a
 * Monarch looking at a shadow sees the full context the shadow works
 * inside, not just the nodes the shadow "owns".
 */
export interface AgentQuestState {
  agentId: string;
  /** Roots where this agent is the direct assignee, newest first. */
  roots: QuestRow[];
  /** Full tree per root_id (flat list ordered by created_at ASC). */
  treesByRoot: SvelteMap<string, QuestRow[]>;
  /** Events per quest_id, lazily loaded on expand. */
  eventsByQuest: SvelteMap<string, QuestEventRow[]>;
  /** Durable execution plan items per quest_id, lazily loaded beside events. */
  planItemsByQuest: SvelteMap<string, PlanItemRow[]>;
  /** External references per quest_id, lazily loaded with quest details. */
  refsByQuest: SvelteMap<string, QuestRefRow[]>;
  /** Quest ids currently expanded inline in the timeline view. */
  expandedQuestIds: SvelteSet<string>;
  /** Quest event ids expanded inside an open quest. */
  expandedEventIds: SvelteSet<string>;
  /** L2 v0: current action + recent actions for quick "what now?" UI. */
  workingMemory: WorkingMemoryPayload | null;
  /** True while the create-quest form is visible. */
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

class QuestStore {
  readonly byAgent = new SvelteMap<string, AgentQuestState>();
  private subs = new Map<string, AgentSubs>();
  private workingMemoryUnavailable = false;

  ensure(agentId: string): AgentQuestState {
    const existing = this.byAgent.get(agentId);
    if (existing) return existing;
    // `$state` must sit in a variable declaration initializer; the entry
    // then lives in the SvelteMap which stays reactive on its own.
    const entry: AgentQuestState = $state({
      agentId,
      roots: [],
      treesByRoot: new SvelteMap(),
      eventsByQuest: new SvelteMap(),
      planItemsByQuest: new SvelteMap(),
      refsByQuest: new SvelteMap(),
      expandedQuestIds: new SvelteSet(),
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
   * Load (or reload) this agent's quest roots + their trees.
   * Also subscribes to per-root `quest-updated-{rootId}` / `quest-created-*`
   * channels so another Monarch client's writes show up here.
   */
  async refresh(agentId: string): Promise<void> {
    const entry = this.ensure(agentId);
    entry.loading = true;
    entry.error = null;
    try {
      const all = await invoke<QuestRow[]>("db_list_quests_for_agent", {
        agentId,
      });
      // Keep only roots (parent_id null) in the timeline header list.
      const roots = all.filter((q) => q.parentId === null);
      entry.roots = roots;

      // Fetch each root's tree in parallel so sub-quests are visible even
      // when the assignee differs.
      const trees = await Promise.all(
        roots.map((r) =>
          invoke<QuestRow[]>("db_get_quest_tree_for_root", { rootId: r.id }),
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

  async createQuest(
    agentId: string,
    payload: CreateQuestPayload,
  ): Promise<string> {
    // Default the assignee to the current agent so the created quest
    // shows up in the timeline immediately.
    const withAssignee: CreateQuestPayload = {
      ...payload,
      assigneeShadowId: payload.assigneeShadowId ?? agentId,
    };
    const id = await invoke<string>("db_create_quest", {
      payload: withAssignee,
    });
    await this.refresh(agentId);
    return id;
  }

  async updateQuest(agentId: string, payload: UpdateQuestPayload): Promise<void> {
    await invoke("db_update_quest", { payload });
    await this.refresh(agentId);
  }

  async updateQuestManual(
    agentId: string,
    payload: ManualQuestUpdatePayload,
  ): Promise<void> {
    await invoke("db_update_quest_manual", { payload });
    await this.refresh(agentId);
    if (this.ensure(agentId).expandedQuestIds.has(payload.id)) {
      await this.loadEvents(agentId, payload.id);
    }
  }

  async recordManualQuestEvent(
    agentId: string,
    payload: ManualQuestEventPayload,
  ): Promise<string> {
    const id = await invoke<string>("db_record_manual_quest_event", { payload });
    await this.loadEvents(agentId, payload.questId);
    return id;
  }

  async loadEvents(agentId: string, questId: string): Promise<void> {
    const entry = this.ensure(agentId);
    const events = await invoke<QuestEventRow[]>("db_list_quest_events", {
      questId,
    });
    entry.eventsByQuest.set(questId, events);
  }

  async loadPlanItems(agentId: string, questId: string): Promise<void> {
    const entry = this.ensure(agentId);
    const items = await invoke<PlanItemRow[]>("db_list_plan_items", {
      questId,
    });
    entry.planItemsByQuest.set(questId, items);
  }

  async loadRefs(agentId: string, questId: string): Promise<void> {
    const entry = this.ensure(agentId);
    const refs = await invoke<QuestRefRow[]>("db_list_quest_refs", { questId });
    entry.refsByQuest.set(questId, refs);
  }

  async createQuestRef(
    agentId: string,
    payload: CreateQuestRefPayload,
  ): Promise<string> {
    const id = await invoke<string>("db_create_quest_ref", { payload });
    await this.loadRefs(agentId, payload.questId);
    return id;
  }

  async updateQuestRef(
    agentId: string,
    questId: string,
    payload: UpdateQuestRefPayload,
  ): Promise<void> {
    await invoke("db_update_quest_ref", { payload });
    await this.loadRefs(agentId, questId);
  }

  async deleteQuestRef(
    agentId: string,
    questId: string,
    refId: string,
  ): Promise<void> {
    await invoke("db_delete_quest_ref", { refId });
    await this.loadRefs(agentId, questId);
  }

  async refreshQuestPlan(agentId: string, questId: string): Promise<void> {
    const entry = this.ensure(agentId);
    await this.loadPlanItems(agentId, questId);
    if (entry.expandedQuestIds.has(questId)) {
      await this.loadEvents(agentId, questId);
    } else {
      entry.eventsByQuest.delete(questId);
    }
    await this.refreshWorkingMemory(agentId);
  }

  private async refreshAfterPlanMutation(agentId: string, questId: string): Promise<void> {
    const entry = this.ensure(agentId);
    try {
      await this.refreshQuestPlan(agentId, questId);
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
    await this.refreshAfterPlanMutation(agentId, payload.questId);
    return id;
  }

  async updatePlanItem(
    agentId: string,
    questId: string,
    payload: UpdatePlanItemPayload,
  ): Promise<void> {
    await invoke("db_update_plan_item", { payload });
    await this.refreshAfterPlanMutation(agentId, questId);
  }

  async deletePlanItem(
    agentId: string,
    questId: string,
    itemId: string,
  ): Promise<void> {
    await invoke("db_delete_plan_item", { itemId });
    await this.refreshAfterPlanMutation(agentId, questId);
  }

  async movePlanItem(
    agentId: string,
    questId: string,
    itemId: string,
    direction: -1 | 1,
  ): Promise<void> {
    const entry = this.ensure(agentId);
    let items = entry.planItemsByQuest.get(questId);
    if (!items) {
      await this.loadPlanItems(agentId, questId);
      items = entry.planItemsByQuest.get(questId) ?? [];
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
        questId,
        items: payloadItems,
        createdBy: "captain",
        rationale: "manual reorder",
      },
    });
    await this.refreshAfterPlanMutation(agentId, questId);
  }

  async startPlanItem(agentId: string, questId: string, itemId: string): Promise<void> {
    await invoke("db_start_plan_item", { itemId });
    await this.refreshAfterPlanMutation(agentId, questId);
  }

  async completePlanItem(
    agentId: string,
    questId: string,
    itemId: string,
    outcome: string | null = null,
  ): Promise<void> {
    await invoke("db_complete_plan_item", { itemId, outcome });
    await this.refreshAfterPlanMutation(agentId, questId);
  }

  async skipPlanItem(
    agentId: string,
    questId: string,
    itemId: string,
    reason: string | null = null,
  ): Promise<void> {
    await invoke("db_skip_plan_item", { itemId, reason });
    await this.refreshAfterPlanMutation(agentId, questId);
  }

  async blockPlanItem(
    agentId: string,
    questId: string,
    itemId: string,
    reason: string,
  ): Promise<void> {
    await invoke("db_block_plan_item", { itemId, reason });
    await this.refreshAfterPlanMutation(agentId, questId);
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
        // rendering the quest tree, so keep the timeline usable and let the
        // command appear after a full Tauri restart.
        this.workingMemoryUnavailable = true;
        entry.workingMemory = null;
        return;
      }
      entry.error = String(e);
    }
  }

  toggleExpand(agentId: string, questId: string): void {
    const entry = this.ensure(agentId);
    if (entry.expandedQuestIds.has(questId)) {
      entry.expandedQuestIds.delete(questId);
    } else {
      entry.expandedQuestIds.add(questId);
      // Lazy-load detail slices on first expand.
      if (!entry.eventsByQuest.has(questId)) {
        this.loadEvents(agentId, questId).catch((e) => {
          entry.error = String(e);
        });
      }
      if (!entry.planItemsByQuest.has(questId)) {
        this.loadPlanItems(agentId, questId).catch((e) => {
          entry.error = String(e);
        });
      }
      if (!entry.refsByQuest.has(questId)) {
        this.loadRefs(agentId, questId).catch((e) => {
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
   * We listen per-root-id on `quest-updated-{rootId}` / `quest-event-{rootId}`;
   * sub-quest updates still trigger a re-fetch on the root they belong to,
   * which refreshes the whole tree. `quest-created-{id}` fires once per new
   * node — we watch the root ids we already know about and fire a broad
   * refresh when a creation for an unknown id arrives (the root is us or a
   * sibling and a full list refresh is the simplest reconciliation).
   */
  private wireRootSubscriptions(entry: AgentQuestState): void {
    // Tear down previous subs for this agent.
    const prev = this.subs.get(entry.agentId);
    if (prev) {
      prev.unlisten.forEach((p) => p.then((fn) => fn()).catch(() => {}));
    }
    const subs: AgentSubs = { unlisten: [] };
    subs.unlisten.push(
      listen<string>(`quest-created-for-agent-${entry.agentId}`, () =>
        this.refresh(entry.agentId),
      ),
    );
    const questIds = new Set<string>();
    for (const tree of entry.treesByRoot.values()) {
      for (const quest of tree) questIds.add(quest.id);
    }
    for (const root of entry.roots) questIds.add(root.id);

    for (const root of entry.roots) {
      subs.unlisten.push(
        listen<string>(`quest-updated-${root.id}`, () =>
          this.refresh(entry.agentId),
        ),
      );
    }

    for (const questId of questIds) {
      subs.unlisten.push(
        listen<string>(`quest-event-${questId}`, () => {
          // Events on any visible quest: invalidate that quest's event cache
          // and refresh L2, because current/recent action pointers are updated
          // by the same persistence path.
          entry.eventsByQuest.delete(questId);
          if (entry.expandedQuestIds.has(questId)) {
            this.loadEvents(entry.agentId, questId).catch((e) => {
              entry.error = String(e);
            });
          }
          this.loadPlanItems(entry.agentId, questId).catch((e) => {
            entry.error = String(e);
          });
          this.refreshWorkingMemory(entry.agentId).catch((e) => {
            entry.error = String(e);
          });
        }),
      );
      subs.unlisten.push(
        listen<string>(`quest-refs-${questId}`, () => {
          if (entry.expandedQuestIds.has(questId)) {
            this.loadRefs(entry.agentId, questId).catch((e) => {
              entry.error = String(e);
            });
          } else {
            entry.refsByQuest.delete(questId);
          }
        }),
      );
    }
    this.subs.set(entry.agentId, subs);
  }
}

export const questStore = new QuestStore();
