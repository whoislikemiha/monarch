import { SvelteMap, SvelteSet } from "svelte/reactivity";
import { invoke, listen, type UnlistenFn } from "$lib/api";
import type {
  CreateQuestPayload,
  QuestEventRow,
  QuestRow,
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
  /** Quest ids currently expanded inline in the timeline view. */
  expandedQuestIds: SvelteSet<string>;
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
      expandedQuestIds: new SvelteSet(),
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

  async loadEvents(questId: string): Promise<void> {
    // Owning agent state not required — events are keyed globally by quest.
    for (const entry of this.byAgent.values()) {
      const events = await invoke<QuestEventRow[]>("db_list_quest_events", {
        questId,
      });
      entry.eventsByQuest.set(questId, events);
      return; // only need to write once; the Map lookup is shared via reference
    }
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
      // Lazy-load events on first expand.
      if (!entry.eventsByQuest.has(questId)) {
        this.loadEvents(questId).catch((e) => {
          entry.error = String(e);
        });
      }
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
            this.loadEvents(questId).catch((e) => {
              entry.error = String(e);
            });
          }
          this.refreshWorkingMemory(entry.agentId).catch((e) => {
            entry.error = String(e);
          });
        }),
      );
    }
    this.subs.set(entry.agentId, subs);
  }
}

export const questStore = new QuestStore();
