# Shadow Cognition — Design Notes

This folder collects the design conversation around how a shadow **thinks, remembers, and interacts**. It started as "design the memory system" and grew into the broader cognitive architecture: what a shadow *is* between turns, how it shares state across multiple attention threads, how it processes its own activity into lasting knowledge, and how the captain experiences the whole thing.

These are **idea documents**, not specs. They capture the model we're converging on. Tech choices and exact protocols remain tentative — anything that names a specific store, library, schema shape, or mechanism is current best guess. Treat the conceptual model as load-bearing; treat implementation sketches as illustrative.

Implementation lives in feature tickets, not here.

## Read in this order

1. **[`substrate.md`](./substrate.md)** — The four-layer self. What a shadow *is* between turns: identity (L1), working memory (L2), knowledge tree (L3), search (L4). Captain layer. Project memory as shared substrate. Branching semantics. *Locks the model the rest of the docs build on.*

2. **[`attention.md`](./attention.md)** — Concurrent attention. One shadow, two organs (executor = hands, chat-shadow = mouth). The quest tree as the temporal spine. Two surfaces (chat + execution timeline). Coherent atomic actions. Event taxonomy. Tool taxonomy by thread. Routing captain input. *Defines the runtime model.*

3. **[`flows.md`](./flows.md)** — Interaction flows. The loop view. How conversations begin (cold start, warm start, notification-driven, resumed, quest-creation). Per-turn loops for each thread. Environment snapshot. Coordination patterns. Idle behavior. Death and resurrection. *Designs how the pieces actually run together.*

4. **[`distillation.md`](./distillation.md)** — The Keeper. Compaction triggers (token threshold, quest end, idle). What gets distilled (events, memories, artifacts, working-memory updates). Three-layer record (raw stream → first-person quest report → third-person atomic claims). Atomic claim definition, types, what to capture, what to exclude. Stale-flagging, multi-shadow project memory, observability, captain edits. *The cognitive metabolism.*

5. **[`roadmap.md`](./roadmap.md)** — Implementation phasing. Turns the idea docs into testable product slices. The P4/P4b split is especially important: P4 is actual executor narration (coherent actions, nested tool calls, L2 current/recent actions); P4b is durable execution plans (intended route, plan item UI, action-to-plan links).

## What's outside this folder

- **CLAUDE.md / VISION.md / ONBOARDING.md** — current product/architecture documentation. These docs propose evolutions of those.
- Implementation tickets — when these designs become real code, the migrations and APIs land in MON-tickets, not here.

## How to use these docs

- **Reading them cold:** start at `substrate.md` and go in order. Each doc references concepts defined in earlier ones.
- **Cross-linking:** docs reference each other by bare filename (e.g., `attention.md`). Section anchors when relevant.
- **Working assumptions:** each doc ends with a numbered list. When sibling docs reference "assumption #N from substrate.md," that's what they mean.
- **Evolving them:** prefer in-place edits with the exploratory framing intact. Major shifts get a "what changed and why" note near the top of the affected doc.
