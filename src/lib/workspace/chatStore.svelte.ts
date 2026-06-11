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

export const TIMELINE_TILE = "__timeline__";

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

  private ensure(agentId: string): void {
    if (this.tilesByAgent.has(agentId)) return;
    this.tilesByAgent.set(agentId, [TIMELINE_TILE, "general"]);
    const panes = new SvelteMap<string, ChatPane>();
    panes.set("general", { id: "general", scope: null, title: "Chat" });
    this.panesByAgent.set(agentId, panes);
  }

  /** Ordered tile ids for the workspace stack (includes the timeline tile). */
  tiles(agentId: string): string[] {
    this.ensure(agentId);
    return this.tilesByAgent.get(agentId)!;
  }

  isTimeline(id: string): boolean {
    return id === TIMELINE_TILE;
  }

  pane(agentId: string, paneId: string): ChatPane | undefined {
    this.ensure(agentId);
    return this.panesByAgent.get(agentId)!.get(paneId);
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
    return id;
  }

  /** Open (or focus) a tile scoped to a piece of work. */
  openScopedPane(agentId: string, scope: ChatScope): string {
    this.ensure(agentId);
    const panes = this.panesByAgent.get(agentId)!;
    for (const p of panes.values()) if (p.scope?.id === scope.id) return p.id;
    const id = `c${++this.seq}`;
    panes.set(id, { id, scope, title: scope.label });
    this.tilesByAgent.set(agentId, [...this.tiles(agentId), id]);
    return id;
  }

  closePane(agentId: string, paneId: string): void {
    if (paneId === TIMELINE_TILE) return;
    this.panesByAgent.get(agentId)?.delete(paneId);
    this.tilesByAgent.set(agentId, this.tiles(agentId).filter((t) => t !== paneId));
    const turns = this.turnsByAgent.get(agentId);
    if (turns) for (const [ord, pid] of turns) if (pid === paneId) turns.set(ord, "general");
  }

  reorderTiles(agentId: string, fromIdx: number, toIdx: number): void {
    const tiles = [...this.tiles(agentId)];
    if (fromIdx < 0 || fromIdx >= tiles.length || toIdx < 0 || toIdx >= tiles.length) return;
    const [moved] = tiles.splice(fromIdx, 1);
    tiles.splice(toIdx, 0, moved);
    this.tilesByAgent.set(agentId, tiles);
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
