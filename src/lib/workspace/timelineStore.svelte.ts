/**
 * MON-124: paged per-agent execution-timeline feed. Backed by
 * `db_list_agent_timeline` — top-level objective_events newest-first across
 * every objective assigned to the agent, with nested children and objective
 * metadata riding along per page.
 *
 * The store is append-only backwards (older pages land at the tail, no
 * re-sorts → no scroll jumps) and reconciles live activity by re-fetching the
 * HEAD page on `objective-event-{id}` pings, debounced so tool bursts don't
 * hammer the DB. Keyed by agentId because workspace components stay mounted
 * across agent switches.
 */
import { SvelteMap } from "svelte/reactivity";
import { invoke, listen, type UnlistenFn } from "$lib/api";
import type {
  AgentTimelinePage,
  ObjectiveEventRow,
  ObjectiveReportRow,
  ObjectiveRow,
  TimelineCursor,
} from "$lib/bindings";
import {
  parseObjectiveReport,
  type ObjectiveReportView,
} from "$lib/toolbox/objectiveStore.svelte";

const PAGE_SIZE = 20;
const HEAD_REFRESH_DEBOUNCE_MS = 150;

export interface AgentTimelineState {
  agentId: string;
  /** Top-level events, newest-first, concatenated across pages. */
  entries: ObjectiveEventRow[];
  /** Nested events keyed by parent_event_id, oldest-first. */
  childrenByParent: SvelteMap<string, ObjectiveEventRow[]>;
  /** Metadata for every objective referenced by `entries`. */
  objectivesById: SvelteMap<string, ObjectiveRow>;
  /** Close-of-objective reports, lazily loaded. `null` = loaded, none exists. */
  reportsByObjective: SvelteMap<string, ObjectiveReportView | null>;
  hasMore: boolean;
  nextBefore: TimelineCursor | null;
  /** True during the initial head load only. */
  loading: boolean;
  /** True while an older page is being fetched. */
  loadingMore: boolean;
  error: string | null;
}

/** Strict (created_at, id) ordering — ISO timestamps compare lexically. */
function isOlder(a: ObjectiveEventRow, b: ObjectiveEventRow): boolean {
  return a.createdAt < b.createdAt || (a.createdAt === b.createdAt && a.id < b.id);
}

class TimelineStore {
  readonly byAgent = new SvelteMap<string, AgentTimelineState>();
  private subs = new Map<string, Array<Promise<UnlistenFn>>>();
  private subbedObjectives = new Map<string, Set<string>>();
  private headTimers = new Map<string, ReturnType<typeof setTimeout>>();

  ensure(agentId: string): AgentTimelineState {
    const existing = this.byAgent.get(agentId);
    if (existing) return existing;
    const entry: AgentTimelineState = $state({
      agentId,
      entries: [],
      childrenByParent: new SvelteMap(),
      objectivesById: new SvelteMap(),
      reportsByObjective: new SvelteMap(),
      hasMore: false,
      nextBefore: null,
      loading: false,
      loadingMore: false,
      error: null,
    });
    this.byAgent.set(agentId, entry);
    return entry;
  }

  /** First mount for an agent: load the head page and wire live updates. */
  async init(agentId: string): Promise<void> {
    const entry = this.ensure(agentId);
    if (!this.subs.has(agentId)) {
      this.subs.set(agentId, [
        listen<string>(`objective-created-for-agent-${agentId}`, () =>
          this.scheduleHeadRefresh(agentId),
        ),
      ]);
      this.subbedObjectives.set(agentId, new Set());
    }
    if (entry.entries.length === 0 && !entry.loading) {
      entry.loading = true;
      entry.error = null;
      try {
        await this.refreshHead(agentId);
      } catch (e) {
        entry.error = String(e);
      } finally {
        entry.loading = false;
      }
    }
  }

  /** Fetch the newest page and merge it over what we have. */
  async refreshHead(agentId: string): Promise<void> {
    const entry = this.ensure(agentId);
    const page = await invoke<AgentTimelinePage>("db_list_agent_timeline", {
      agentId,
      before: null,
      limit: PAGE_SIZE,
    });
    this.mergePage(entry, page, "head");
  }

  /** Fetch the next older page (infinite scroll). */
  async loadMore(agentId: string): Promise<void> {
    const entry = this.ensure(agentId);
    if (entry.loadingMore || !entry.hasMore || !entry.nextBefore) return;
    entry.loadingMore = true;
    try {
      const page = await invoke<AgentTimelinePage>("db_list_agent_timeline", {
        agentId,
        before: entry.nextBefore,
        limit: PAGE_SIZE,
      });
      this.mergePage(entry, page, "append");
    } catch (e) {
      entry.error = String(e);
    } finally {
      entry.loadingMore = false;
    }
  }

  /** Debounced head refresh — coalesces bursts of objective-event pings. */
  scheduleHeadRefresh(agentId: string): void {
    const prev = this.headTimers.get(agentId);
    if (prev) clearTimeout(prev);
    this.headTimers.set(
      agentId,
      setTimeout(() => {
        this.headTimers.delete(agentId);
        this.refreshHead(agentId).catch((e) => {
          this.ensure(agentId).error = String(e);
        });
      }, HEAD_REFRESH_DEBOUNCE_MS),
    );
  }

  /** Lazily load a closed objective's first-person report. */
  async loadReport(agentId: string, objectiveId: string): Promise<void> {
    const entry = this.ensure(agentId);
    if (entry.reportsByObjective.has(objectiveId)) return;
    try {
      const row = await invoke<ObjectiveReportRow | null>("db_get_objective_report", {
        objectiveId,
      });
      entry.reportsByObjective.set(objectiveId, row ? parseObjectiveReport(row) : null);
    } catch {
      // Report is decoration on the timeline — never let it break the stream.
    }
  }

  private mergePage(
    entry: AgentTimelineState,
    page: AgentTimelinePage,
    mode: "head" | "append",
  ): void {
    for (const obj of page.objectives) entry.objectivesById.set(obj.id, obj);
    // Children arrive complete for every entry in the page — group and replace.
    const grouped = new Map<string, ObjectiveEventRow[]>();
    for (const child of page.children) {
      if (!child.parentEventId) continue;
      const list = grouped.get(child.parentEventId);
      if (list) list.push(child);
      else grouped.set(child.parentEventId, [child]);
    }
    for (const e of page.entries) {
      entry.childrenByParent.set(e.id, grouped.get(e.id) ?? []);
    }

    if (mode === "append") {
      const have = new Set(entry.entries.map((e) => e.id));
      entry.entries = [...entry.entries, ...page.entries.filter((e) => !have.has(e.id))];
      entry.hasMore = page.hasMore;
      entry.nextBefore = page.nextBefore;
    } else if (entry.entries.length === 0) {
      entry.entries = page.entries;
      entry.hasMore = page.hasMore;
      entry.nextBefore = page.nextBefore;
    } else {
      // Head refresh over an existing list: the fetched page is the newest N
      // top-levels. Keep everything we already had that is strictly older than
      // the page's oldest row; the page replaces/refreshes the overlap.
      const headIds = new Set(page.entries.map((e) => e.id));
      const oldest = page.entries[page.entries.length - 1];
      const keep = entry.entries.filter(
        (e) => !headIds.has(e.id) && (!oldest || isOlder(e, oldest)),
      );
      entry.entries = [...page.entries, ...keep];
      if (keep.length === 0) {
        // Nothing older retained — the page's cursor is the whole story.
        entry.hasMore = page.hasMore;
        entry.nextBefore = page.nextBefore;
      }
    }
    this.wireObjectiveSubs(entry);
  }

  /** Subscribe to event/report pings for objectives newly seen in the feed. */
  private wireObjectiveSubs(entry: AgentTimelineState): void {
    const subs = this.subs.get(entry.agentId);
    const seen = this.subbedObjectives.get(entry.agentId);
    if (!subs || !seen) return;
    for (const objectiveId of entry.objectivesById.keys()) {
      if (seen.has(objectiveId)) continue;
      seen.add(objectiveId);
      subs.push(
        listen<string>(`objective-event-${objectiveId}`, () =>
          this.scheduleHeadRefresh(entry.agentId),
        ),
        listen<string>(`objective-report-${objectiveId}`, () => {
          entry.reportsByObjective.delete(objectiveId);
          this.loadReport(entry.agentId, objectiveId).catch(() => {});
        }),
        listen<string>(`objective-updated-${objectiveId}`, () =>
          this.scheduleHeadRefresh(entry.agentId),
        ),
      );
    }
  }

  clear(agentId: string): void {
    const subs = this.subs.get(agentId);
    if (subs) {
      subs.forEach((p) => p.then((fn) => fn()).catch(() => {}));
      this.subs.delete(agentId);
    }
    this.subbedObjectives.delete(agentId);
    const timer = this.headTimers.get(agentId);
    if (timer) clearTimeout(timer);
    this.headTimers.delete(agentId);
    this.byAgent.delete(agentId);
  }
}

export const timelineStore = new TimelineStore();
