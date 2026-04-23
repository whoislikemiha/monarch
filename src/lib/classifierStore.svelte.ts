/**
 * MON-82 — Slice 1: per-agent map of classifier outputs.
 *
 * Shape: `ordinalMap: SvelteMap<userOrdinal, ClassificationInfo>` keyed by
 * the 0-based position among user messages. MessageList already derives the
 * same ordinal (`userIndexAt`), so rendering is a direct lookup.
 *
 * Classifier events arrive once per user turn in FIFO order, so we assign
 * each incoming event the next unfilled ordinal. This avoids threading a
 * classification-id FK through the live items (which would require Rust to
 * extend `LiveAgentState`). On reload, historical classifications can be
 * fetched via `db_list_classifications_for_agent` and slotted into the same
 * map by matching `messageId` to the user message ordinals.
 */
import { SvelteMap } from "svelte/reactivity";
import { listen, invoke, type UnlistenFn } from "$lib/api";
import type { ComplexityLabel } from "./classifier-types";

export interface ClassificationInfo {
  id: string;
  complexity?: ComplexityLabel;
  confidence?: number;
  rationale?: string;
  model?: string;
  tokensIn?: number;
  tokensOut?: number;
  latencyMs?: number;
  error?: string;
}

interface AgentClassifierState {
  agentId: string;
  ordinalMap: SvelteMap<number, ClassificationInfo>;
  nextOrdinal: number;
}

interface AgentSubs {
  unlisten: Promise<UnlistenFn>;
}

class ClassifierStore {
  readonly byAgent = new SvelteMap<string, AgentClassifierState>();
  private subs = new Map<string, AgentSubs>();

  ensure(agentId: string): AgentClassifierState {
    const existing = this.byAgent.get(agentId);
    if (existing) return existing;
    const entry: AgentClassifierState = $state({
      agentId,
      ordinalMap: new SvelteMap(),
      nextOrdinal: 0,
    });
    this.byAgent.set(agentId, entry);
    this.subscribe(entry);
    return entry;
  }

  private subscribe(entry: AgentClassifierState): void {
    if (this.subs.has(entry.agentId)) return;
    const un = listen<ClassificationInfo>(
      `agent-classification-${entry.agentId}`,
      (ev) => {
        const info = ev.payload;
        const ordinal = entry.nextOrdinal++;
        entry.ordinalMap.set(ordinal, info);
      },
    );
    this.subs.set(entry.agentId, { unlisten: un });
  }

  /**
   * Called when the bound session changes — the ordinal counter resets so
   * classifications line up with the replayed user messages.
   */
  reset(agentId: string): void {
    const entry = this.byAgent.get(agentId);
    if (!entry) return;
    entry.ordinalMap.clear();
    entry.nextOrdinal = 0;
  }

  /**
   * Populate the store from DB (on session reload). Pairs by message_id
   * against a supplied ordered list of user message ids; classifications
   * whose `messageId` isn't in the list (e.g. pre-backfill) are ignored.
   */
  async hydrateFromDb(
    agentId: string,
    userMessageIdsInOrder: number[],
  ): Promise<void> {
    const entry = this.ensure(agentId);
    entry.ordinalMap.clear();
    entry.nextOrdinal = userMessageIdsInOrder.length;
    try {
      const rows = (await invoke("db_list_classifications_for_agent", {
        agentId,
        limit: 500,
      })) as Array<{
        id: string;
        messageId: number | null;
        complexity: string | null;
        confidence: number | null;
        rationale: string | null;
        model: string | null;
        tokensIn: number | null;
        tokensOut: number | null;
        latencyMs: number | null;
        error: string | null;
      }>;
      const byMessageId = new Map<number, (typeof rows)[number]>();
      for (const r of rows) {
        if (r.messageId != null) byMessageId.set(r.messageId, r);
      }
      userMessageIdsInOrder.forEach((mid, ordinal) => {
        const r = byMessageId.get(mid);
        if (!r) return;
        entry.ordinalMap.set(ordinal, {
          id: r.id,
          complexity: (r.complexity ?? undefined) as ComplexityLabel | undefined,
          confidence: r.confidence ?? undefined,
          rationale: r.rationale ?? undefined,
          model: r.model ?? undefined,
          tokensIn: r.tokensIn ?? undefined,
          tokensOut: r.tokensOut ?? undefined,
          latencyMs: r.latencyMs ?? undefined,
          error: r.error ?? undefined,
        });
      });
    } catch (e) {
      console.error("[classifier] hydrate failed", e);
    }
  }
}

export const classifierStore = new ClassifierStore();
