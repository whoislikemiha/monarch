/**
 * Chat panes — multiple lightweight conversation threads with the same shadow,
 * arrangeable by the captain (drag to reorder, resize, close).
 *
 * The backend has one live Pi session per agent, so every pane shares that
 * session + memory. To keep panes meaningfully distinct we track *turn
 * membership* on the frontend: when a pane sends a message, the resulting user
 * turn (and its assistant reply) is tagged to that pane, so each pane shows only
 * its own exchange. Scoped panes (opened from a timeline action) also inject the
 * action's context into their first message. See [[monarch-ui-v2-interaction-model]].
 *
 * True *parallel* live threads (separate executor contexts) still need the
 * attention-threads backend; this is the cheap shared-session version.
 */
import { SvelteMap, SvelteSet } from "svelte/reactivity";

export interface ChatScope {
  id: string;
  kind: "action" | "objective";
  label: string;
  context: string;
}

export interface ChatPane {
  id: string;
  /** null = a general chat; set = scoped to a piece of work. */
  scope: ChatScope | null;
  title: string;
}

const GENERAL_ID = "general";

function generalPane(): ChatPane {
  return { id: GENERAL_ID, scope: null, title: "Chat" };
}

class ChatStore {
  private panesByAgent = new SvelteMap<string, ChatPane[]>();
  /** userOrdinal → paneId, per agent. Unassigned ordinals fall to the general pane. */
  private turnsByAgent = new SvelteMap<string, SvelteMap<number, string>>();
  private primed = new SvelteSet<string>();
  private seq = 0;

  panes(agentId: string): ChatPane[] {
    let p = this.panesByAgent.get(agentId);
    if (!p) {
      p = [generalPane()];
      this.panesByAgent.set(agentId, p);
    }
    return p;
  }

  /** Add another general chat thread. */
  addPane(agentId: string): string {
    const id = `c${++this.seq}`;
    this.panesByAgent.set(agentId, [...this.panes(agentId), { id, scope: null, title: "Chat" }]);
    return id;
  }

  /** Open (or focus) a pane scoped to a piece of work. Returns the pane id. */
  openScopedPane(agentId: string, scope: ChatScope): string {
    const panes = this.panes(agentId);
    const existing = panes.find((p) => p.scope?.id === scope.id);
    if (existing) return existing.id;
    const id = `c${++this.seq}`;
    this.panesByAgent.set(agentId, [...panes, { id, scope, title: scope.label }]);
    return id;
  }

  closePane(agentId: string, paneId: string): void {
    const remaining = this.panes(agentId).filter((p) => p.id !== paneId);
    this.panesByAgent.set(agentId, remaining.length ? remaining : [generalPane()]);
    // Re-home this pane's turns so they don't vanish from every pane.
    const turns = this.turnsByAgent.get(agentId);
    if (turns) {
      for (const [ord, pid] of turns) if (pid === paneId) turns.set(ord, GENERAL_ID);
    }
  }

  reorder(agentId: string, fromIdx: number, toIdx: number): void {
    const panes = [...this.panes(agentId)];
    if (fromIdx < 0 || fromIdx >= panes.length || toIdx < 0 || toIdx >= panes.length) return;
    const [moved] = panes.splice(fromIdx, 1);
    panes.splice(toIdx, 0, moved);
    this.panesByAgent.set(agentId, panes);
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
    return this.turns(agentId).get(userOrdinal) ?? GENERAL_ID;
  }

  // --- scope priming (first scoped message carries the context) ---

  consumePrimer(agentId: string, scope: ChatScope): string | null {
    const key = `${agentId}:${scope.id}`;
    if (this.primed.has(key)) return null;
    this.primed.add(key);
    return scope.context;
  }
}

export const chatStore = new ChatStore();
