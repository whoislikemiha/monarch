/**
 * Chat scope — the "talk to the same shadow about this piece of work" model.
 *
 * Clicking a timeline action sets a scope on the agent's chat: a chip is shown
 * and the action's context is injected into the *first* message sent under that
 * scope, so the shadow answers with that work in mind (it shares the same
 * memory/session — see [[monarch-ui-v2-interaction-model]]).
 *
 * True parallel side-conversations (a separate answering instance while the
 * executor keeps working) need the attention-threads backend and are deferred;
 * this delivers the cheap, immediate "ask about this" feel on the live session.
 */
import { SvelteMap, SvelteSet } from "svelte/reactivity";

export interface ChatScope {
  /** Stable id of the scoped thing (event id / objective id). */
  id: string;
  kind: "action" | "objective";
  /** Short label for the scope chip. */
  label: string;
  /** Context preamble injected on the first message sent under this scope. */
  context: string;
}

class ChatStore {
  /** Active scope per agent (null/absent = general chat). */
  private scopeByAgent = new SvelteMap<string, ChatScope | null>();
  /** `${agentId}:${scopeId}` once its context preamble has been sent. */
  private primed = new SvelteSet<string>();

  getScope(agentId: string): ChatScope | null {
    return this.scopeByAgent.get(agentId) ?? null;
  }

  setScope(agentId: string, scope: ChatScope): void {
    this.scopeByAgent.set(agentId, scope);
  }

  clearScope(agentId: string): void {
    this.scopeByAgent.set(agentId, null);
  }

  /**
   * Returns the context preamble to prepend for this scope's first message,
   * then marks it primed so later messages in the same scope send plain.
   */
  consumePrimer(agentId: string, scope: ChatScope): string | null {
    const key = `${agentId}:${scope.id}`;
    if (this.primed.has(key)) return null;
    this.primed.add(key);
    return scope.context;
  }
}

export const chatStore = new ChatStore();
