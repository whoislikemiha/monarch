# Shadow Cognition — Implementation Roadmap

> **Status:** Direction document, not a commitment. Sequences the work implied by the four design docs in this folder (`substrate.md`, `attention.md`, `distillation.md`, `flows.md`) into phases that each ship a tangible, testable result. Phase contents are illustrative — when a phase opens, the actual ticket scope gets locked in `thoughts/plan/MON-{N}.md`.
>
> **Sibling docs:** the four design docs above. Read those for the *what* and *why*; this doc is *what order, what tickets, what's testable at end of phase*.

## How to use this roadmap

Before opening a ticket in any phase, **read the design docs the phase references**. The "Read first" lines under each phase point to specific doc sections — they carry conceptual context (premises, working assumptions, alternatives considered and rejected) that is not duplicated here. The roadmap is *what to build, in what order, to what end*; the design docs are *why those choices, what they imply*. Skipping the design read produces drifted implementations — Slice B's first plan is a worked example.

**Reference docs (read in order on first pass):**

1. [`substrate.md`](./substrate.md) — Four-layer self (L1 identity, L2 working memory, L3 knowledge tree, L4 search). Captain layer. Project memory as shared substrate. Branching. Per-turn context loading.
2. [`attention.md`](./attention.md) — One shadow, two organs. Quest tree as temporal spine. Two surfaces (chat + execution timeline). Coherent atomic actions. Event taxonomy. Tool taxonomy by thread. Routing captain input.
3. [`distillation.md`](./distillation.md) — The Keeper. Three-layer record (raw / first-person report / third-person claims). Compaction triggers (continuous / semantic / idle). Atomic claims. Merge / supersede / insert / archive logic. Stale-flagging. Memory poisoning firewall.
4. [`flows.md`](./flows.md) — Per-turn loops (chat-shadow + executor). Conversation entry conditions. Environment snapshot. Coordination patterns. Idle behavior. Death and resurrection.

## The phase rule

A phase is a phase only if, at the end of it, you can sit down with the running app and tell whether it works *without* needing the next phase. Two consequences worth being explicit about:

1. **Shared infra rides inside the phase that first needs it.** There is no "wire up storage" phase, no "schema migrations" phase, no "set up Pi multiplexing" phase. Infra is the cost of whatever vertical phase first uses it, not its own phase.
2. **Quality and scale work each get their own phase, but only because each has its own testable result.** Eval harness ships "we have a recall@5 number." Reranker ships "recall went from X to Y." Rebuild worker ships "1M memories don't block writes." Each is a phase. None of them are blockers for the *first* memory shipping — they improve a thing that already works.

Two corollaries:

- **No phase ships schema fields no one writes.** If a phase adds columns, that phase also adds whichever code path reads or writes them. Otherwise it fails the test.
- **Phases can run in parallel when independent.** P1 is parallel-safe with all of P2/P3. The post-P2 quality cluster (P3a–d) can interleave with P4–P6 if bandwidth allows.

## Already shipped (orientation)

| Surface | Status |
|---------|--------|
| Storage stack viability | [MON-91](https://linear.app/monarch-commander/issue/MON-91) — validated. Production stack live as of MON-99. |
| Quest tree (data) | [MON-83](https://linear.app/monarch-commander/issue/MON-83), [MON-105](https://linear.app/monarch-commander/issue/MON-105), [MON-116](https://linear.app/monarch-commander/issue/MON-116) — `quest_nodes`, `quest_events`, `agents.current_quest_id`, rich quest what/why fields, and `quest_refs`. Manual create + rich edit surface shipped on master. Still pre-chat-shadow and pre-automatic decomposition. |
| Execution timeline (UI) | [MON-109](https://linear.app/monarch-commander/issue/MON-109), [MON-114](https://linear.app/monarch-commander/issue/MON-114), [MON-117](https://linear.app/monarch-commander/issue/MON-117) — nested coherent-action renderer, plan-aware chips/panel, rich quest detail editor, manual events, and refs panel all shipped on master. |
| Per-turn classifier | [MON-82](https://linear.app/monarch-commander/issue/MON-82) — Slice 1 advisory. First reader (Architect, [MON-84](https://linear.app/monarch-commander/issue/MON-84)) not built. |
| Captain identity (L1) | [MON-98](https://linear.app/monarch-commander/issue/MON-98) — captain + shadow identity payloads in DB, settings UI, wired into `shadow-oath.ts` system prompt. P1 territory; shipped early. |
| Memory substrate (L3 storage) | [MON-99](https://linear.app/monarch-commander/issue/MON-99), [MON-100](https://linear.app/monarch-commander/issue/MON-100), [MON-101](https://linear.app/monarch-commander/issue/MON-101), [MON-102](https://linear.app/monarch-commander/issue/MON-102), [MON-103](https://linear.app/monarch-commander/issue/MON-103) — storage, Keeper writes, retrieval injection, executor suggestions, and quest-close trigger are wired. Memory Inspector remains browse-only; eval/reranker/scale work remains P3/P12. |
| Executor narration + L2 | [MON-107](https://linear.app/monarch-commander/issue/MON-107), [MON-108](https://linear.app/monarch-commander/issue/MON-108), [MON-109](https://linear.app/monarch-commander/issue/MON-109) — nested quest events, `agent_working_memory`, sidecar narration tools, nested timeline, and Agent View Now strip are wired. |
| Durable execution plans | [MON-110](https://linear.app/monarch-commander/issue/MON-110), [MON-111](https://linear.app/monarch-commander/issue/MON-111), [MON-112](https://linear.app/monarch-commander/issue/MON-112), [MON-114](https://linear.app/monarch-commander/issue/MON-114) — `quest_plan_items`, plan lifecycle events/tools, active/next L2 slice, action `plan_item_id` links, plan panel, and plan-aware Now strip all shipped on master. |
| Auto-memory (the proxy for L1) | Anthropic-side per-project memory in `~/.claude/projects/.../memory/`. Co-exists with MON-98's in-app L1; deprecate when L1 has been in production long enough. |

Everything else implied by the design docs (chat-shadow/two-organ split, project sharing, forking, stale-flagging, quest reports, full Memory Inspector editing, and automatic decomposition) is unbuilt.

## Tracks

The phases are *temporal*. The work itself runs on four cross-cutting tracks; each phase advances some subset.

| Track | Owns |
|-------|------|
| **Backend / data** | SQLite schema, persistence pipeline, Keeper, sidecar Pi sessions, retrieval, embedding, indexing |
| **Quest tree** | `quest_nodes` + `quest_events` shape, event taxonomy, surface routing, status/scope/rationale |
| **Execution timeline (UI surface)** | Coherent-action rendering, nested children, plan-change events, drill-in, primary-panel promotion |
| **UI/UX** | Memory Inspector, identity editors, chat surface vs timeline split, fork views, captain edit affordances |

### Track evolution

| Phase | Backend / data | Quest tree | Timeline | UI/UX |
|-------|----------------|------------|----------|-------|
| **P1** Captain identity | `captain` + identity-version tables | — | — | Identity editors; auto-memory migration |
| **P2** First memory | `memories`, `memory_keeper_runs`, FTS5, HNSW basics, Keeper at quest-close, brute-force retrieval | `compaction_tick` event kind; `source_quest_id` provenance | `compaction_tick` rendered in existing tool | Memory Inspector v0 (browse) |
| **P2b** Auto quest spine | — | Auto-created root quest + `agents.current_quest_id` on meaningful user turns | Existing QuestTimeline wakes on first root | Empty timeline becomes live without manual quest creation |
| **P3a** Eval harness | Recall/merge metrics on a fixed seed | — | — | — |
| **P3b** Reranker | Top-K=20 → top-K=5 reranker pass | — | — | — |
| **P3c** Rebuild worker | Background HNSW rebuild + atomic swap | — | — | — |
| **P3d** Incremental insert | Per-memory write-into-graph path | — | — | — |
| **P4** Executor narration | `agent_working_memory` L2 v0; executor narration tools + prompt block | `coherent_action`, `action_outcome`, `tool_call`, `executor_decision` events with `parent_event_id` + `author` | **Becomes a real execution narrative** — collapsible action parents | Working-memory preview in agent view |
| **P4b** Execution plans | `quest_plan_items`; L2 active/next plan slice **shipped** | Plan lifecycle events; actions link to `plan_item_id` **shipped** | Plan-aware timeline: intended vs actual **shipped** | Lightweight plan panel **shipped** |
| **P5** Rich quest + manual editor | `quest_refs`; rich quest metadata **shipped** | `scope`, `current_direction`, `rationale`, `grade`, `summary`, `scope_change`, `direction_change`, `note`, `blocker`, `question`, `answer` **shipped** | First-class quest-change/manual-event rows **shipped** | Inline quest detail panel + refs panel **shipped** |
| **P6** Quest reports | `quest_reports` table; executor `complete_quest` tool | (uses `status` from P5) | — | First-person report rendered at close |
| **P7** chat-shadow read-only | Pi multiplexing per agent; `attention_threads`; chat-shadow read tools + `speak`; pause/resume/stop control | `chat_message`, `observation`, `paused_by_chat`, `resumed_by_chat`, `stopped_by_chat` | **Promoted to primary panel** | Dual-surface layout (clean chat + timeline) |
| **P8** chat-shadow full | Plan-manipulation tools, routing classifier, question/answer, pending-action mediation | All remaining event kinds; surface routing applied | Surface override respected | Routing-driven UX |
| **P9** Project sharing | Per-project Keeper serializer | — | — | Memory Inspector scope filter |
| **P10** Forking | Branch-local L3 namespace; merge logic; worktree integration | `fork_parent_id`, `worktree_path`, `forked`, `merged` | Per-fork timelines + parent comparative view | Fork creation + merge resolution UI |
| **P11** Stale-flagging | `file_refs` with `anchor_sha`; lazy load check; organic re-verification | (optional) `re_verification` events | Stale badges on drill-in | Stale badges + verify affordance |
| **P12** Memory Inspector polish | Edit/archive/promote/supersede APIs; tree visualization data; idle sweeps; manual checkpoint | — | Compaction observability polish | Full Memory Inspector |

## Critical path

```
P1 ──────────────────────────────────────────────────────────────► (parallel-safe)

P2 ──► P3a ──► P3b
   └──► P3c, P3d (after P2; can run in parallel)

P2 ──► P2b ──► P4 ──► P4b ──► P5 ──► P6 ──► P7 ──► P8 ──► (P9 | P10 | P11) ──► P12
```

- **P1 is independent.** Land any time. Replaces the auto-memory pattern in-app.
- **P2 is the gate to everything cognitive.** Without it, no memory exists to read or write.
- **P2b makes the quest spine real.** It is a bridge slice: no decomposition, just a current root quest so MON-103, P4, and later timeline work have somewhere to attach.
- **P3a–d improves P2** but doesn't block P4+. Pick when memory volume warrants.
- **P4 → P4b → P5 → P6** is sequential because each builds on the prior's surface: actual execution narrative, then intended execution plan, then rich quest editing, then reports.
- **P7 needs P1 + P4** at minimum (shared identity + L2 to read).
- **P8 needs P7 + P4b + P5** (the second voice exists, durable plans exist to manipulate, and rich quest fields exist to mutate).
- **P9, P10, P11** are independent of each other after P8. Pick by need.
- **P12** is final-form polish, sits at the end.

## Phases

### P1 — Captain identity, end-to-end

**Read first.** [`substrate.md`](./substrate.md) § L1 (captain layer + shadow layer + storage and editing).

**Goal.** Captain edits identity once in-app; every shadow reads it next turn.

**Test scenario.** Captain opens Identity tool, edits "I prefer terse responses." Closes the tool. Spawns a new shadow. Asks it for a status. Response is terse — and the L1 captain layer rendered into the system prompt contains the new line. Roll back the edit; next turn, the shadow reverts.

**Tickets:** *(unticketed — file at phase open)*
- *(new)* `captain` + `captain_identity_versions` schema; singleton enforced.
- *(new)* `shadow_identity_versions` + `agents.identity_version_id`.
- *(new)* Identity toolbox tool (or settings panel — decide at ticket time). Edits → new version row + `current_version` pointer flip.
- *(new)* System-prompt builder (`shadow-oath.ts`) reads L1 captain + L1 shadow into the prompt.
- *(new, optional)* One-time migration that imports current `auto memory` content into v1 of the captain layer behind a confirm.

**Tracks.** Backend (schema + persistence) + UI/UX. No quest tree, no timeline.

**Depends on.** Nothing.

**Defers.** Multi-captain support (the singleton constraint accommodates a future promotion). Inner-node summary editing (P12).

---

### P2 — First memory, end-to-end

**Read first.** [`distillation.md`](./distillation.md) (whole doc — load-bearing for this phase, especially § Premise, § The Keeper, § Compaction triggers, § Memory poisoning firewall). [`substrate.md`](./substrate.md) § L3 + § "Loading the substrate for a turn" + § "Project memory as shared substrate" (for what we are *not* shipping yet — single-shadow private subtree only).

**Goal.** A shadow forms structured memories continuously as it works. Compaction (forming memories) is the same operation as context shrinkage — every ~30k tokens of executor activity, the Keeper distills the recent stream into atomic claims and resets the executor's raw context window. The captain browses memories in the Inspector and watches them form via `compaction_tick` events. The next turn after a memory forms surfaces it in retrieval (sequenced through MON-101).

**Test scenario.** Captain runs a long-running multi-turn task with a shadow. After ~30k tokens of conversation + tool activity, a Keeper tick fires automatically — soft trigger looks for a natural breakpoint within the next ~5k tokens, hard trigger forces a clean cut at 30k. Atomic claims appear in the Memory Inspector with provenance (source quest id when present, source events, keeper run id). `compaction_tick` event lands on the QuestTimelineTool when the agent has a `current_quest_id`; otherwise the run is visible only via `memory_keeper_runs`. The executor's next turn proceeds with a *synthesized context*: the Keeper's compaction summary rendered as a one-shot user/assistant scaffold, plus L3 retrieval of relevant memories, plus the raw messages since the tick — not the full prior history. Captain starts a follow-up task on a related topic; the shadow's tree-walk surfaces the relevant memory in its context.

**Tickets:**
- [**MON-99**](https://linear.app/monarch-commander/issue/MON-99) — Slice A: substrate. **Shipped on master.** `memories` schema (incl. `parent_id`, `scope`, `kind`, `summary`, `content`, `embedding`, `embedding_model_id`, `supersedes_id`, `archived_at`, `source_quest_id`, `source_events`, `file_refs`), `memories_fts` FTS5 mirror, `memory_keeper_runs` provenance table, embedding pipeline (`bge-small-en-v1.5` via ONNX), HNSW sidecar file (`instant-distance` — full rebuild only; P3c/d defer background + incremental), Memory Inspector v0 (browse-only — no edit / archive / promote, those are P12), debug-only `memory_smoke_insert`. Subsumes the original MON-95 (`memories_fts` rolled in).
- [**MON-100**](https://linear.app/monarch-commander/issue/MON-100) — Slice B: **token-pressure-triggered Keeper write path.** Continuous compaction loop. Token counter per agent (sums `usage.total_tokens` since last tick). Soft trigger at ~25k looking for next natural breakpoint within ~5k more; hard trigger at 30k. Sidecar Keeper worker (`sidecar/src/keeper.ts`, structurally mirrors `classifier.ts`). Structured-JSON claim extraction. Memory writes via the existing single-consumer pipeline (`PersistCommand` variants). HNSW rebuild after each successful run. `compaction_tick` on `quest_events` when `agents.current_quest_id` is set; otherwise visible only via `memory_keeper_runs`. Pi `state.messages` rewrite after the tick — synthesized [user: "previously summarized" + assistant: "ack" + raw-since-tick] scaffold replaces the pre-tick raw conversation. Silent no-op when no Keeper model configured.
- [**MON-102**](https://linear.app/monarch-commander/issue/MON-102) — Executor `suggest_memory` tool. Memory-poisoning firewall input path (executor proposes via `quest_events` row; Keeper still decides at next tick). Independent of MON-100 at the produce side.
- [**MON-101**](https://linear.app/monarch-commander/issue/MON-101) — Slice C: hybrid retrieval read path. On user turn, surfaces relevant memories via FTS5 + brute-force vector top-K (no reranker — P3b). Surfaces as `## Relevant Memories` adjacent to the new user message. `access_count` / `last_accessed_at` updated on retrieval.
- [**MON-105**](https://linear.app/monarch-commander/issue/MON-105) — Bridge: auto-create an active root quest for meaningful user turns. Populates `agents.current_quest_id` before the sidecar starts work so compaction ticks, MON-103 quest-close, and P4 narration have a quest spine without requiring manual QuestTimeline setup.
- [**MON-103**](https://linear.app/monarch-commander/issue/MON-103) — Quest-close *semantic* trigger. Reuses MON-100's Keeper plumbing; adds status-transition detection in `db_update_quest`, `trigger='quest_close'` labeling, and a "Mark done" UI affordance. v1 uses the same model + prompt as MON-100; per `distillation.md` § "Compaction triggers" the design supports a deeper-pass model tier here, deferred until calibration shows it helps.
- *(deferred)* Idle compaction trigger. Small follow-up after MON-100 — same Keeper plumbing, idle-timer-based dispatch.

**Tracks.** Backend (heavy: Keeper worker + token-pressure trigger + context-rewrite mechanism) + Quest tree (light: one event kind + provenance FK) + Timeline (reuse existing) + UI/UX (Memory Inspector v0 + observable compaction).

**Depends on.** P1 ([MON-98](https://linear.app/monarch-commander/issue/MON-98)) shipped — so captain preferences land cleanly into Keeper context.

**Defers.** Eval harness (P3a / [MON-94](https://linear.app/monarch-commander/issue/MON-94)). Reranker (P3b / [MON-93](https://linear.app/monarch-commander/issue/MON-93)). Background rebuild + incremental insert (P3c/d / [MON-96](https://linear.app/monarch-commander/issue/MON-96), [MON-97](https://linear.app/monarch-commander/issue/MON-97)). Idle compaction trigger. Project subtree writes (P9). First-person reports as Keeper input (P6 backfills). Captain edit / archive / promote / supersede in Inspector (P12). Stale-flagging via `file_refs.anchor_sha` (P11). Two-tier Keeper (local / cloud) — P2 ships single-tier; tier split is a `memory.toml` extension, not a phase. L2 working memory and durable execution plans are no longer P2 deferrals; they landed in P4/P4b.

---

### P3a — Eval harness

**Read first.** [`distillation.md`](./distillation.md) § "Open questions / current direction" — the eval harness premise. § "Merge / supersede / insert / archive logic" for what merge quality is being measured against.

**Goal.** A recall@5 + merge-quality number we trust against a fixed seed.

**Test scenario.** Run `eval` against a seeded 50-memory tree with 20 queries. Output: recall@5 score, merge/supersede decision quality on synthetic conflicts, summary report. Numbers are reproducible across runs.

**Tickets:**
- [**MON-94**](https://linear.app/monarch-commander/issue/MON-94) — eval harness, 50 memories / 20 queries.

**Tracks.** Backend only.

**Depends on.** P2 (the retrieval stack must exist).

**Why now (after P2, not before).** Eval needs a stack to evaluate. With P2 shipped, the harness measures the actual production retrieval path — not a prototype.

**Defers.** Eval-driven calibration of cosine thresholds (uses results to inform P3b reranker tuning, but threshold values land in their own tickets as we calibrate).

---

### P3b — Reranker

**Read first.** [`distillation.md`](./distillation.md) § "Open questions / current direction" — "Reranking the hybrid pool."

**Goal.** Top-K=20 candidates from hybrid retrieval get reranked to top-K=5 before context injection. Recall@5 from P3a improves measurably.

**Test scenario.** Run P3a's harness with reranker enabled vs disabled. Recall@5 with reranker > recall@5 without, by some material delta.

**Tickets:**
- [**MON-93**](https://linear.app/monarch-commander/issue/MON-93) — design + impl the BM25+vector reranker.

**Tracks.** Backend only.

**Depends on.** P3a (need the metric to design against).

---

### P3c — Background HNSW rebuild + atomic swap

**Read first.** [`substrate.md`](./substrate.md) § "Implications for the data model" (vector index notes). MON-91 spike for the validated stack.

**Goal.** At realistic memory volumes (10k+), HNSW rebuilds happen in a background worker without blocking writes or reads.

**Test scenario.** Seed 100k memories. Trigger rebuild. Observe: reads continue serving from the previous "last good index" throughout the rebuild. New index swaps in atomically when ready.

**Tickets:**
- [**MON-96**](https://linear.app/monarch-commander/issue/MON-96) — background HNSW rebuild worker with atomic read swap.

**Depends on.** P2 (rebuild path exists in degenerate form).

---

### P3d — Incremental HNSW insert

**Read first.** Same as P3c.

**Goal.** A new memory written by the Keeper becomes queryable in seconds, not after the next scheduled rebuild.

**Test scenario.** Write a memory. Within 2 seconds, query for it via the production retrieval path. Hit.

**Tickets:**
- [**MON-97**](https://linear.app/monarch-commander/issue/MON-97) — incremental HNSW insert path for per-memory writes.

**Depends on.** P2.

---

### P4 — Executor narration (coherent actions + L2 v0)

**Read first.** [`substrate.md`](./substrate.md) § L2 (full section, including "Two writers, strict separation of concerns" and "Three layers of intention"). [`attention.md`](./attention.md) § "Coherent atomic actions" + § "Three self-reporting cadences" + § "Event taxonomy" (executor-activity events). [`flows.md`](./flows.md) § "The executor per-turn loop" + § "Environment snapshot".

**Goal.** Executor declares intent before each coherent work chunk, executes nested tool calls, and closes with a one-line outcome. The captain reads the timeline at intent level. L2 working memory carries the live present.

**Note.** P2 leaves L2 textual-only (Keeper consolidation lives in `memory_keeper_runs.output_summary`). P4 is the first phase that introduces structured `WorkingMemory` in a separate `agent_working_memory` table. V0 fields: `current_action`, `recent_actions`, `current_quest_id`, `current_quest_path`, `updated_at`. `current_action` and `recent_actions` are structured pointers into the quest event log, not free text. This makes L2 an index into the canonical timeline rather than a second history.

**Test scenario.** Captain watches a quest run. The timeline shows a sequence of collapsible coherent actions ("Read failing test files", "Fix the off-by-one in `parser.rs`", "Run the test"), each expandable into its underlying tool calls. The agent view shows `current_action` + last few `recent_actions` from L2 — captain can answer "what is it doing right now?" without scrolling.

**Tickets:**
- [**MON-107**](https://linear.app/monarch-commander/issue/MON-107) — `quest_events` migration: add `parent_event_id`, `author`, `surface_override`, `payload_schema_version`. Keep `actor` as the concrete writer id/name; `author` is semantic (`executor` / `chat_shadow` / `captain` / `keeper` / `system`). Adds event kinds `coherent_action`, `action_outcome`, `tool_call`, `executor_decision`; L2 schema in `agent_working_memory(agent_id, payload_json, updated_at)`; and single-writer persistence for action / tool-call / L2 mutations.
- [**MON-108**](https://linear.app/monarch-commander/issue/MON-108) — Executor narration tools: `set_current_action(intent, previous_outcome?)`, `complete_action(outcome)`, `record_decision(decision, rationale?)`. Tools emit semantic inner events; Rust owns quest event IDs, active quest lookup, nesting, and L2 updates. Adds executor prompt guidance for coherent chunk granularity, transition/outcome rhythm, and sparse decision recording.
- [**MON-109**](https://linear.app/monarch-commander/issue/MON-109) — Timeline renderer for nested children and working-memory preview in agent view UI.

**Deferred from P4.** L2 rebuild fallback from quest events when `agent_working_memory` is missing or invalid. Durable plans stay in P4b.

**Tracks.** Backend + Quest tree (event taxonomy expansion) + Timeline (collapsible-children rendering) + UI/UX (working-memory preview).

**Depends on.** Nothing schema-wise that isn't already there; builds on MON-83's quest skeleton.

**Defers.** Durable execution plans / plan UI / plan-to-action linking (P4b). Status/scope/direction/rationale on quest_nodes and attachments / external refs (P5). First-person reports (P6). Chat-shadow, orchestrator, and automatic decomposition (P7/P8). Raw thinking persistence (intentionally not part of the roadmap unless replaced by explicit structured events).

---

### P4b — Execution plans (intended route)

**Read first.** [`substrate.md`](./substrate.md) § L2 "Three layers of intention". [`attention.md`](./attention.md) § "Coherent atomic actions" (especially the distinction between plan and timeline). [`flows.md`](./flows.md) § "The executor per-turn loop".

**Goal.** A quest has a durable, visible, provisional execution plan: what the shadow currently intends to do next. P4 showed what actually happened; P4b adds the intended route without conflating the two.

**Test scenario.** Captain starts a medium coding task. The quest shows a lightweight plan ("inspect auth flow", "patch expiry handler", "run focused tests"). Executor marks the first item active, performs multiple coherent actions under it, marks it done, then moves to the next. The timeline shows both plan lifecycle events and actual actions, with actions linked to the active plan item.

**Tickets:**
- [**MON-110**](https://linear.app/monarch-commander/issue/MON-110) — Parent issue for P4b.
- [**MON-111**](https://linear.app/monarch-commander/issue/MON-111) — Slice A: backend substrate. `quest_plan_items` table, `quest_events.plan_item_id`, L2 active/next plan slice, plan lifecycle persistence, Tauri commands, and focused Rust tests. **Shipped on master.**
- [**MON-112**](https://linear.app/monarch-commander/issue/MON-112) — Slice B: sidecar plan tools and prompt guidance. Adds executor-facing plan lifecycle tools (`set_plan`, `start_plan_item`, `complete_plan_item`, `skip_plan_item`, `block_plan_item`) and wires them through the inner-event persistence path. **Shipped on master.**
- [**MON-114**](https://linear.app/monarch-commander/issue/MON-114) — Slice C: plan panel and plan-aware timeline UI. Adds captain-visible plan add/edit/delete/reorder/status controls, plan lifecycle rows, action plan chips, Now-strip active/next plan context, WebSocket parity, and regenerated bindings. **Shipped on master.**

**Tracks.** Backend + Quest tree + Timeline + UI/UX.

**Depends on.** P4 (actions exist and can link to plan items).

**Current status.** P4b is shipped on master through Slice C. Remaining work is richer plan editing and chat-shadow/Architect plan manipulation in P8, not core P4b substrate.

**Defers.** Architect/orchestrator planning agent, automatic subquest decomposition, multi-agent delegation, and chat-shadow routing (P8). Rich quest fields and attachments / refs (P5).

---

### P5 — Rich quest model + manual editor

**Read first.** [`attention.md`](./attention.md) § "The quest tree is the spine" + § "Event taxonomy" (quest-change events). [`substrate.md`](./substrate.md) § L2 "Three layers of intention" (quest vs execution plan vs coherent action distinction).

**Goal.** Quests carry status, scope, current direction, rationale, grade, summary, and attachments / external refs. Captain edits these manually. Quest-change events (`scope_change`, `direction_change`, `subtask_added`, `note`) appear on the timeline.

**Test scenario.** Captain opens a quest detail panel. Edits scope ("expanded to also cover the auth refactor"), supplies rationale. The change persists, surfaces on the timeline as a `scope_change` event with rationale. Closes the quest by setting `status='done'`. Status transition is reflected in the agent view.

**Tickets:**
- [**MON-115**](https://linear.app/monarch-commander/issue/MON-115) — Parent issue for P5 rich quest model and manual editor.
- [**MON-116**](https://linear.app/monarch-commander/issue/MON-116) — Slice A: backend/data contract. Adds `quest_nodes.scope`, `quest_nodes.current_direction`, `quest_nodes.rationale`, `quest_nodes.fork_parent_id`, `quest_refs`, manual quest update commands, manual quest event command, WebSocket parity, bindings, and docs. Existing MON-83 `status`, `grade`, `worktree_path`, and `summary` remain canonical; no base-table rewrite. **Shipped on master.**
- [**MON-117**](https://linear.app/monarch-commander/issue/MON-117) — Slice B: captain-visible editor. Extends `questStore` with P5 commands and renders inline expanded-quest controls for status, grade, scope, current direction, rationale, summary, manual events (`note`, `blocker`, `blocker_resolved`, `question`, `answer`), and external refs (`linear`, `github_issue`, `github_pr`, `file`, `url`, `artifact`). **Shipped on master.**
- *(next / optional)* Slice C: timeline/editor polish. Likely areas: inline ref editing, richer typed event payload rendering, detail-panel ergonomics, and any app-level affordances discovered during manual Tauri testing.

**Tracks.** Backend + Quest tree + UI/UX.

**Depends on.** P4b (timeline already renders actual actions and intended plan; P5 enriches the quest object around them).

**Current status.** P5 is shipped on master through Slice B (MON-116 backend + MON-117 inline UI). It is usable as a manual editor / refs panel today. It does not yet include chat-shadow mutation tools, Architect decomposition, or a dedicated full-page quest detail surface.

**Defers.** Auto-decomposition by Architect (P8 — subsumes [MON-84](https://linear.app/monarch-commander/issue/MON-84) here, since the Architect is conceptually a chat-shadow tool). Captain-set permission gates on quests (P8 territory). Fork semantics for `fork_parent_id` (P10). First-person quest reports (P6).

---

### P6 — First-person quest reports

**Read first.** [`distillation.md`](./distillation.md) § "First-person quest report" + § "The three-layer record" (L0 / L1 / L2 framing — distinct from substrate L1/L2/L3 numbering).

**Goal.** When a quest closes, the executor writes a structured first-person report. Captain reads it as a quest-close artifact. The Keeper consumes it as a high-quality input.

**Test scenario.** Captain marks a quest done. Executor runs `complete_quest(report)`. Structured payload (summary, outcome, decisions, learned, artifacts, open_threads, reflection, grade) lands in `quest_reports`. UI surfaces the report below the quest detail. Next Keeper tick at quest-close consumes both the raw stream and the report; produces noticeably better claims than P2's report-less Keeper.

**Tickets:** *(unticketed — file at phase open)*
- *(new)* `quest_reports` table. FK to `quest_nodes`, FK to `memory_keeper_runs` once distilled.
- *(new)* Executor system-prompt update: teach the report format and when to emit it.
- *(new)* `complete_quest(report)` tool.
- *(new)* Report renderer in quest detail panel.
- *(new)* Keeper input pipeline: include `quest_reports.payload` alongside raw stream slice on quest-close ticks.

**Tracks.** Backend + Quest tree (uses `status` from P5) + UI/UX.

**Depends on.** P5 (status transitions trigger the report).

**Defers.** Captain-edited grades / summaries on the quest node itself (already editable in P5; report is its own artifact).

---

### P7 — chat-shadow read-only + dual surface

**Read first.** [`attention.md`](./attention.md) (whole doc — load-bearing for this phase, especially § "The captain experience comes first", § "Two surfaces on the quest", § "Thread types", § "Tool taxonomy by thread"). [`flows.md`](./flows.md) § "The chat-shadow per-turn loop" + § "Coordination between chat and executor" + § "Conversation entry conditions".

**Goal.** Captain talks to a chat-shadow that runs alongside the executor. Chat-shadow reads from substrate but takes no actions on the world. Chat surface stays clean (dialogue only); execution timeline is a sibling primary panel.

**Test scenario.** Captain opens chat with a shadow that's mid-quest. Asks "what are you doing?" Chat-shadow reads L2 + recent quest events, answers in one or two sentences derived from `current_action` + last `recent_actions`. Captain says "pause for a sec" — `pause_executor` fires, executor halts at next action boundary. "Ok resume" — `resume_executor`, executor continues. Throughout, the chat panel never shows tool calls; the timeline panel does.

**Tickets:** *(unticketed — file at phase open)*
- *(new)* Pi-session multiplexing per agent — the sidecar spawns a chat-shadow Pi session alongside the executor session for each active agent.
- *(new, possibly an implementation spike)* Confirm Pi handles two concurrent sessions per agent gracefully. Flagged in `attention.md` as an open question.
- *(new)* `attention_threads` table tracking session role (executor / chat / observer).
- *(new)* Chat-shadow tool set, **minimum**: `read`, `grep`, `find`, `ls`, read-only bash, `memory_search`, `recall_actions`, `speak`, `surface_observation`, `pause_executor`, `resume_executor`, `stop_executor`. Explicit deny on world-mutation tools.
- *(new)* New event kinds: `chat_message` (captain ↔ shadow), `observation`, `paused_by_chat`, `resumed_by_chat`, `stopped_by_chat`.
- *(new)* Surface routing rules — events split between chat and timeline by kind, with `surface_override` honored.
- *(new)* Dual-surface UI: clean chat panel + execution timeline panel as siblings. Drill-in expands collapsed actions.

**Tracks.** Backend (heavy: Pi multiplexing) + Quest tree (chat-side event kinds) + Timeline (promoted to primary panel) + UI/UX (layout split).

**Depends on.** P1 (shared L1 between threads) + P4 (L2 to read; coherent actions to filter to timeline).

**Defers.** Chat-shadow plan-manipulation tools (`add_to_execution_plan`, `add_subtask`, `change_quest_*`, `note_on_quest`) — P8. Routing intent classifier — P8. Question/answer mediation — P8. Pending-action mediation — P8.

---

### P8 — chat-shadow full + routing classifier

**Read first.** [`attention.md`](./attention.md) § "Routing captain input" + § "Tool taxonomy by thread" (chat-shadow plan-manipulation tools + pending-action mediation). [`flows.md`](./flows.md) § "Coordination between chat and executor".

**Goal.** Captain types into chat; chat-shadow classifies intent and takes the appropriate action — adding to plan, expanding scope, redirecting, answering questions, mediating pending actions, speaking back. Subsumes the Architect ([MON-84](https://linear.app/monarch-commander/issue/MON-84)) as the auto-decomposer for high-complexity captain inputs.

**Test scenario.** From the routing table in `attention.md`: captain says "after this also rename `verify` to `validate`" — `add_to_execution_plan`; executor picks up after current action. Captain says "now do Y instead" — `change_quest_direction`; executor switches at next boundary. Captain says "let's now also refactor the test suite" — Architect/auto-decomposer fires (high complexity classification from MON-82) → `add_subtask` with rationale. Captain answers a `question` event — typed `answer` flows back to executor.

**Tickets:** *(unticketed — file at phase open; **subsumes [MON-84](https://linear.app/monarch-commander/issue/MON-84)** — the Architect's role is the auto-decomposer arm of the routing classifier)*
- *(new)* Chat-shadow plan-manipulation tools: `add_to_execution_plan`, `add_subtask`, `change_quest_scope`, `change_quest_direction`, `note_on_quest`, `mark_quest_blocked`, `complete_quest_intent`.
- *(new)* Chat-shadow writes to the durable execution plan from P4b; L2 carries only active/next plan pointers.
- *(new)* Routing intent classifier — chat-shadow's per-turn classification of captain input, consuming MON-82's classification authoritatively (vs MON-82 Slice 1 which is advisory). Plus the Architect's auto-decomposition path for high-complexity inputs.
- *(new)* `pending_action` family of tools: `propose_pending_action` (executor), `modify_pending_action` / `approve_pending_action` / `reject_pending_action` (chat-shadow). Captain-set permission gates configurable per-quest.
- *(new)* Question/answer mediation: chat-shadow's `answer_question` consumes captain's words and emits typed `answer` events.
- *(new)* Fork-quest tool stub (real semantics in P10).

**Tracks.** Backend + Quest tree (full taxonomy) + UI/UX (routing-driven affordances; pending-action UI).

**Depends on.** P7 (chat-shadow exists) + P4b (durable execution plans to manipulate) + P5 (rich quest model to mutate).

---

### P9 — Project subtree sharing

**Read first.** [`substrate.md`](./substrate.md) § "Project memory as shared substrate" (especially "Multi-writer discipline"). [`distillation.md`](./distillation.md) § "Multi-shadow project memory".

**Goal.** Multiple shadows on the same project share `Projects/<P>/...` as living project knowledge. New shadows on a project don't start from zero.

**Test scenario.** Shadow A on project Monarch finishes a task; Keeper writes a `Projects/Monarch/Architecture` claim ("Pi is execution engine, not session authority"). Shadow B (different shadow, same project) starts a related task next day; tree-walk surfaces A's claim into B's context.

**Tickets:** *(unticketed)*
- *(new)* Per-project Keeper serializer (Rust component, single-consumer queue scoped to project_id). Reuses MON-37 pattern.
- *(new)* Project-scoped read flow.
- *(new)* Memory Inspector scope filter (self / project / captain / global).

**Depends on.** P8.

---

### P10 — Forking with worktrees

**Read first.** [`substrate.md`](./substrate.md) § "Branching the substrate" (the L1-shared / L2-forked / L3-shared-read+fork-local-write pattern). [`attention.md`](./attention.md) § "Branching as multi-thread + multi-quest".

**Goal.** Captain forks a shadow ("try it two ways"); each fork has its own L2, fork-local L3, executor + chat-shadow, and git worktree. Captain picks a winner; merge promotes.

**Test scenario.** Captain says "fork this two ways: approach A using middleware extraction, approach B using inline refactor." `fork_quest` creates two child quests + two worktrees. Each fork runs its own executor + chat-shadow. Captain reads parent quest comparative view. Picks A. `merge_quest` promotes A's branch-local L3 claims to project subtree, archives B's, merges A's git branch.

**Tickets:** *(unticketed)*
- *(new)* Branch-local L3 namespace under active quest.
- *(new)* Worktree integration (`git worktree add`, path stored on quest).
- *(new)* Fork-local L2 working memories.
- *(new)* `merge_quest` semantics: Keeper-mediated promotion + archive of loser claims.
- *(new)* Fork creation UI; comparative parent view; merge resolution UI.
- *(new)* `forked` and `merged` event authoring.

**Depends on.** P8 (chat-shadow's `fork_quest` tool stub becomes real here).

---

### P11 — Stale-flagging + organic re-verification

**Read first.** [`substrate.md`](./substrate.md) § "Stale-flagging via git". [`distillation.md`](./distillation.md) § "Stale-flagging and organic re-verification".

**Goal.** Memories that reference files know when their files have changed and surface that to the consumer. When the executor naturally verifies a stale claim, the Keeper updates anchor or supersedes.

**Test scenario.** Keeper writes a memory referencing `src-tauri/src/agent/manager.rs` at commit `abc123`. Captain commits a change to that file; new commit `def456`. Next time the memory loads into agent context, it carries `stale: true`. Executor reads the file as part of natural work, observes the claim still holds, the Keeper re-anchors `file_refs.anchor_sha = def456`. Memory is fresh again.

**Tickets:** *(unticketed)*
- *(new)* `file_refs` populated on Keeper writes (`{path, anchor_sha, sections?}` — sections deferred to v2).
- *(new)* Lazy load-time check vs git for staleness; `stale: true` annotation in agent context.
- *(new)* Organic re-verification feedback path: executor's natural verification flows back to Keeper, which re-anchors / supersedes / archives.
- *(new)* Stale badges in Memory Inspector + timeline drill-in.

**Depends on.** P8 (executor + chat-shadow loop is stable enough to teach the verification rhythm). P9 is independent.

**Defers.** Background re-verification sweeps (P12 if at all). Section-precision file refs (v2; outside this roadmap).

---

### P12 — Memory Inspector polish + observability

**Read first.** [`distillation.md`](./distillation.md) § "Captain can edit memories" + § "Compaction is observable" + § "Inner nodes and tree growth" (inner-node summary regeneration cadence).

**Goal.** Captain has full inspect/edit/archive control over the memory tree, with rich provenance and live observability of compaction.

**Test scenario.** Captain opens Memory Inspector, browses the tree by topic. Edits a claim's content (creates a new version via `supersedes_id`). Archives a memory; un-archives. Promotes a self-scoped memory to project scope. Triggers Keeper distillation manually ("checkpoint this"). Inner-node summaries regenerate when subtree changes. Watching a quest run, sees the chat surface flash a brief "Igris just learned: <claim>" notice when meaningful memories form.

**Tickets:** *(unticketed)*
- *(new)* Edit / archive / promote-scope / supersede APIs + UI.
- *(new)* Tree visualization.
- *(new)* Inner-node summary regeneration (auto + manual).
- *(new)* Idle compaction sweeps + opportunistic re-verification (per `distillation.md` § Idle).
- *(new)* Manual `checkpoint` Keeper trigger.
- *(new)* Compaction observability polish — "Igris just learned" chat surface notice.

**Depends on.** P8 at minimum; benefits from P9/P10/P11 if the captain wants full project + fork + stale browsing.

## Captain-visible milestones

| After | What changes for the captain |
|-------|-------------------------------|
| **P1** | Identity edits in-app; auto-memory pattern replaced by first-class L1. |
| **P2** | First memory forms *continuously during work* (token-pressure-triggered, ~30k token cadence); compaction shrinks the executor's context as memories form; `compaction_tick` events appear when a quest is current; Memory Inspector v0 browse; retrieval surfaces relevant memories on follow-up turns. **First "the shadow remembered something" moment.** |
| **P3a** | A trustworthy recall@5 number we can target. |
| **P3b** | Recall@5 measurably improves. |
| **P3c** | Memory works at 1M scale without blocking writes. |
| **P3d** | New memories queryable in seconds. |
| **P4** | Timeline reads as a real execution narrative; captain sees `current_action` in the agent view. **Shadow stops feeling like a chat log.** |
| **P4b** | Current quest has a visible intended plan; actions link to plan items. **Captain can distinguish intended route from actual execution.** Shipped on master through MON-114. |
| **P5** | Quests have rich fields, manual events, and external refs; captain edits scope/direction with rationale from the inline quest detail panel. Shipped on master through MON-116/MON-117. |
| **P6** | Quests close with a first-person report. **Compelling captain UX moment.** |
| **P7** | Chat surface stays clean during work; timeline panel runs in parallel; captain can ask "what are you doing?" while the shadow works. **Two-organ vision becomes the daily UX.** |
| **P8** | Captain redirects, expands, mediates pending actions through chat without ritual; Architect auto-decomposes complex inputs into subquests. |
| **P9** | Cross-shadow project knowledge; new shadows on a project inherit understanding. |
| **P10** | "Try it two ways" with worktrees works end-to-end. |
| **P11** | Memories surface stale annotations; verification flows naturally from work. |
| **P12** | Full editorial control over the memory tree; satisfying compaction observability. |

## Open meta-questions

These don't block any phase but should be revisited as we go.

- **Local vs cloud Keeper defaults.** Lean local for privacy; calibrate by usage. Configurable in `memory.toml` from P2 onward.
- **Token-budget defaults** for L1/L2/L3 in agent context. Conservative starting values; calibrate by usage. Configurable from P2 onward.
- **Cosine thresholds** for merge/supersede/insert/sibling. Calibrated by P3a's eval, not pre-decided.
- **Pi-session multiplexing per agent.** Implementation spike inside P7 to confirm Pi handles concurrent sessions per agent gracefully. Flagged in `attention.md` as an open question.
- **Architect (MON-84) framing.** Currently scoped as the first reader of MON-82's classifier, conceived before this design crystallized. P8 subsumes its responsibilities under chat-shadow's routing classifier + auto-decomposition path. We should close MON-84 as "subsumed by P8" or repurpose its branch when P8 opens — flag at P8 ticket-creation time.
- **Auto-memory deprecation.** Auto-memory keeps working alongside P1's L1. Deprecate when L1 has been in use long enough that the captain trusts it.
- **Where the timeline panel lives** in the dual-surface UI (sibling, tabbed, floating). UI work in P7 picks.

## What this roadmap is not

- **A schedule.** No dates, no story points. Phase sizes are estimable; calendar is the user's call.
- **A complete ticket list.** Tickets get filed when each phase opens, with scope locked in `thoughts/plan/MON-{N}.md`.
- **Final.** Phases are likely to split or merge as we work through them. The order and the testable-result rule are the load-bearing claims; phase boundaries are not.
