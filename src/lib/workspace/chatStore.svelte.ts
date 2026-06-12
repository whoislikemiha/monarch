/**
 * Workspace tiles — the timeline and any number of chat panes, as one ordered,
 * arrangeable stack. The captain drags tiles to reorder them, resizes them, and
 * closes chats; the timeline is just another tile (id = TIMELINE_TILE) so it can
 * be moved among the chats.
 *
 * Chats share the agent's one live Pi session; to keep them distinct we track
 * *turn membership* (a pane's sends tag the resulting turn to it, so each pane
 * shows only its own exchange). Scoped panes inject the action's context on the
 * first message. See [[monarch-ui-v2-interaction-model]]. True parallel live
 * threads still need the attention-threads backend.
 */
import { SvelteMap, SvelteSet } from "svelte/reactivity";
import { invoke } from "$lib/api";

export const TIMELINE_TILE = "__timeline__";

/** Stable default so reads before ensure() don't thrash derivations. */
const DEFAULT_TILES: readonly string[] = [TIMELINE_TILE, "general"];
const DEFAULT_GENERAL: ChatPane = { id: "general", scope: null, title: "Chat" };

/** ui_state key holding one agent's persisted workspace arrangement. */
const chatKey = (agentId: string) => `v2.chat.${agentId}`;

/** Serialized shape stored in ui_state — tile order, panes, turn membership. */
interface PersistedChat {
  tiles: string[];
  panes: ChatPane[];
  turns: [number, string][];
  seq: number;
}

export interface ChatScope {
  id: string;
  kind: "action" | "objective";
  label: string;
  context: string;
}

export interface ChatPane {
  id: string;
  scope: ChatScope | null;
  title: string;
}

class ChatStore {
  /** Ordered tile ids per agent: TIMELINE_TILE + chat pane ids, in display order. */
  private tilesByAgent = new SvelteMap<string, string[]>();
  /** Chat pane metadata per agent, keyed by pane id. */
  private panesByAgent = new SvelteMap<string, SvelteMap<string, ChatPane>>();
  /** userOrdinal → paneId, per agent. Unassigned ordinals fall to the general pane. */
  private turnsByAgent = new SvelteMap<string, SvelteMap<number, string>>();
  private primed = new SvelteSet<string>();
  private seq = 0;
  /** Agents whose persisted arrangement has been loaded — gates writes so we
   *  never clobber saved state with defaults before hydration completes. */
  private hydrated = new Set<string>();

  /**
   * Initialize an agent's tiles. MUST be called from a non-reactive context
   * (onMount / event handler), never from a $derived — it mutates state.
   *
   * Seeds defaults synchronously (so the first render has tiles) then loads any
   * persisted arrangement from ui_state and swaps it in when it arrives.
   */
  ensure(agentId: string): void {
    if (this.tilesByAgent.has(agentId)) return;
    this.tilesByAgent.set(agentId, [...DEFAULT_TILES]);
    const panes = new SvelteMap<string, ChatPane>();
    panes.set("general", { ...DEFAULT_GENERAL });
    this.panesByAgent.set(agentId, panes);
    void this.hydrate(agentId);
  }

  /** Load this agent's saved arrangement and replace the in-memory defaults. */
  private async hydrate(agentId: string): Promise<void> {
    try {
      const raw = await invoke<string | null>("db_get_ui_state", { key: chatKey(agentId) });
      if (raw) {
        const s = JSON.parse(raw) as PersistedChat;
        const panes = new SvelteMap<string, ChatPane>();
        for (const p of s.panes ?? []) panes.set(p.id, p);
        if (!panes.has("general")) panes.set("general", { ...DEFAULT_GENERAL });
        // Keep only tile ids we can still resolve (timeline or a known pane).
        const tiles = (s.tiles ?? [...DEFAULT_TILES]).filter(
          (t) => t === TIMELINE_TILE || panes.has(t),
        );
        if (!tiles.includes(TIMELINE_TILE)) tiles.unshift(TIMELINE_TILE);
        this.panesByAgent.set(agentId, panes);
        this.tilesByAgent.set(agentId, tiles);
        const turns = new SvelteMap<number, string>();
        for (const [ord, pid] of s.turns ?? []) turns.set(ord, panes.has(pid) ? pid : "general");
        this.turnsByAgent.set(agentId, turns);
        // seq is process-global; advance past every restored id so new panes
        // can't collide with ones we just rehydrated.
        this.seq = Math.max(this.seq, s.seq ?? 0);
      }
    } catch {}
    this.hydrated.add(agentId);
  }

  /** Persist this agent's current arrangement. No-op until hydration finishes. */
  private persist(agentId: string): void {
    if (!this.hydrated.has(agentId)) return;
    const panes = this.panesByAgent.get(agentId);
    const data: PersistedChat = {
      tiles: [...this.tiles(agentId)],
      panes: panes ? [...panes.values()] : [],
      turns: [...(this.turnsByAgent.get(agentId)?.entries() ?? [])],
      seq: this.seq,
    };
    invoke("db_set_ui_state", { key: chatKey(agentId), value: JSON.stringify(data) }).catch(() => {});
  }

  /** Ordered tile ids for the workspace stack. Read-only — safe in derivations. */
  tiles(agentId: string): readonly string[] {
    return this.tilesByAgent.get(agentId) ?? DEFAULT_TILES;
  }

  isTimeline(id: string): boolean {
    return id === TIMELINE_TILE;
  }

  pane(agentId: string, paneId: string): ChatPane | undefined {
    return this.panesByAgent.get(agentId)?.get(paneId) ?? (paneId === "general" ? DEFAULT_GENERAL : undefined);
  }

  chatCount(agentId: string): number {
    return this.tiles(agentId).filter((t) => t !== TIMELINE_TILE).length;
  }

  /** Add another general chat tile (appended). */
  addPane(agentId: string): string {
    this.ensure(agentId);
    const id = `c${++this.seq}`;
    this.panesByAgent.get(agentId)!.set(id, { id, scope: null, title: "Chat" });
    this.tilesByAgent.set(agentId, [...this.tiles(agentId), id]);
    this.persist(agentId);
    return id;
  }

  /** True when a pane scoped to this work id is currently open. */
  hasScopedPane(agentId: string, scopeId: string): boolean {
    const panes = this.panesByAgent.get(agentId);
    if (!panes) return false;
    for (const p of panes.values()) if (p.scope?.id === scopeId) return true;
    return false;
  }

  /** Open (or focus) a tile scoped to a piece of work. */
  openScopedPane(agentId: string, scope: ChatScope): string {
    this.ensure(agentId);
    const panes = this.panesByAgent.get(agentId)!;
    for (const p of panes.values()) if (p.scope?.id === scope.id) return p.id;
    const id = `c${++this.seq}`;
    panes.set(id, { id, scope, title: scope.label });
    this.tilesByAgent.set(agentId, [...this.tiles(agentId), id]);
    this.persist(agentId);
    return id;
  }

  closePane(agentId: string, paneId: string): void {
    if (paneId === TIMELINE_TILE) return;
    this.panesByAgent.get(agentId)?.delete(paneId);
    this.tilesByAgent.set(agentId, this.tiles(agentId).filter((t) => t !== paneId));
    const turns = this.turnsByAgent.get(agentId);
    if (turns) for (const [ord, pid] of turns) if (pid === paneId) turns.set(ord, "general");
    this.persist(agentId);
  }

  reorderTiles(agentId: string, fromIdx: number, toIdx: number): void {
    const tiles = [...this.tiles(agentId)];
    if (fromIdx < 0 || fromIdx >= tiles.length || toIdx < 0 || toIdx >= tiles.length) return;
    const [moved] = tiles.splice(fromIdx, 1);
    tiles.splice(toIdx, 0, moved);
    this.tilesByAgent.set(agentId, tiles);
    this.persist(agentId);
  }

  // --- turn membership ---

  private turns(agentId: string): SvelteMap<number, string> {
    let t = this.turnsByAgent.get(agentId);
    if (!t) {
      t = new SvelteMap();
      this.turnsByAgent.set(agentId, t);
    }
    return t;
  }

  assignTurn(agentId: string, userOrdinal: number, paneId: string): void {
    this.turns(agentId).set(userOrdinal, paneId);
    this.persist(agentId);
  }

  paneForOrdinal(agentId: string, userOrdinal: number): string {
    return this.turns(agentId).get(userOrdinal) ?? "general";
  }

  consumePrimer(agentId: string, scope: ChatScope): string | null {
    const key = `${agentId}:${scope.id}`;
    if (this.primed.has(key)) return null;
    this.primed.add(key);
    return scope.context;
  }
}

export const chatStore = new ChatStore();
