# Shadow Cognition — Implementation Roadmap

> **Status:** Direction document, not a commitment. Sequences the work implied by the four design docs in this folder (`substrate.md`, `attention.md`, `distillation.md`, `flows.md`) into phases that each ship a tangible, testable result. Phase contents are illustrative — when a phase opens, the actual ticket scope gets locked in `thoughts/plan/MON-{N}.md`.
>
> **Sibling docs:** the four design docs above. Read those for the *what* and *why*; this doc is *what order, what tickets, what's testable at end of phase*.

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
| Storage stack viability | [MON-91](https://linear.app/monarch-commander/issue/MON-91) — validated. No production code yet. |
| Quest tree (data) | [MON-83](https://linear.app/monarch-commander/issue/MON-83) — `quest_nodes`, `quest_events`, `agents.current_quest_id`. Read-only timeline tool, manual create. Skeletal. |
| Execution timeline (UI) | MON-83 `QuestTimelineTool` (toolbox tool, read-only). |
| Per-turn classifier | [MON-82](https://linear.app/monarch-commander/issue/MON-82) — Slice 1 advisory. First reader (Architect, [MON-84](https://linear.app/monarch-commander/issue/MON-84)) not built. |
| Auto-memory (the proxy for L1) | Anthropic-side per-project memory in `~/.claude/projects/.../memory/`. De-facto stand-in until P1 ships in-app L1. |

Everything else implied by the design docs (in-app L1, L2, L3, Keeper, two-organ split, project sharing, forking, stale-flagging, Memory Inspector) is unbuilt.

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
| **P3a** Eval harness | Recall/merge metrics on a fixed seed | — | — | — |
| **P3b** Reranker | Top-K=20 → top-K=5 reranker pass | — | — | — |
| **P3c** Rebuild worker | Background HNSW rebuild + atomic swap | — | — | — |
| **P3d** Incremental insert | Per-memory write-into-graph path | — | — | — |
| **P4** Executor narration | L2 schema (current_action, recent_actions); executor prompt update | `coherent_action`, `executor_action_outcome`, `tool_call`, `thinking` events with `parent_event_id` + `author` | **Becomes a real execution narrative** — collapsible action parents | Working-memory preview in agent view |
| **P5** Rich quest + manual editor | — | `status`, `scope`, `current_direction`, `rationale`, `grade`, `summary`, `subtask_added`, `scope_change`, `direction_change`, `note` | First-class plan-change entries | Quest detail panel + manual editor |
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

P2 ──► P4 ──► P5 ──► P6 ──► P7 ──► P8 ──► (P9 | P10 | P11) ──► P12
```

- **P1 is independent.** Land any time. Replaces the auto-memory pattern in-app.
- **P2 is the gate to everything cognitive.** Without it, no memory exists to read or write.
- **P3a–d improves P2** but doesn't block P4+. Pick when memory volume warrants.
- **P4 → P5 → P6** is sequential because each builds on the prior's surface (narration before manual editor before reports).
- **P7 needs P1 + P4** at minimum (shared identity + L2 to read).
- **P8 needs P7** (the second voice exists before it gains plan-manipulation tools).
- **P9, P10, P11** are independent of each other after P8. Pick by need.
- **P12** is final-form polish, sits at the end.

## Phases

### P1 — Captain identity, end-to-end

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

**Goal.** A shadow forms a memory at quest-close, the captain browses it, and the next turn that memory surfaces in retrieval.

**Test scenario.** Captain runs a multi-turn task with a shadow. Marks the quest done. Keeper fires at quest-close, writes one or more atomic claims into the shadow's private subtree. `compaction_tick` event appears in the existing `QuestTimelineTool`. Captain opens Memory Inspector, sees the new memories with provenance. Starts a new task on a related topic; the shadow's tree-walk surfaces the relevant memory in its agent context.

**Tickets:**
- [**MON-95**](https://linear.app/monarch-commander/issue/MON-95) — `memories_fts` (FTS5 mirror of title/summary/content). Lands inside this phase as part of "wire retrieval end-to-end."
- *(new)* `memories` table per `distillation.md` § Implications-for-the-data-model. Includes `parent_id`, `scope`, `kind`, `summary`, `content`, `embedding`, `embedding_model_id`, `supersedes_id`, `archived_at`, `source_quest_id`, `source_events`, `file_refs`.
- *(new)* `memory_keeper_runs` provenance table.
- *(new)* HNSW sidecar file via `instant-distance` — minimal: full rebuild on cold start, no background rebuild worker (P3c), no incremental insert (P3d). At first-memory volumes brute-force is fine.
- *(new)* Keeper sidecar worker. **Quest-close trigger only** for v1; continuous and idle triggers come later. Single shadow, private subtree only — no per-project serializer (P9).
- *(new)* Hybrid retrieval (BM25 + brute-force vector top-K) read into agent context. **No reranker** (P3b); top-K from the merged pool goes straight to context.
- *(new)* `compaction_tick` event kind in `quest_events`; rendered in existing `QuestTimelineTool`.
- *(new)* Memory Inspector v0 — toolbox tool. Browse-only. Tree by topic, per-memory drill-in (provenance, source events, file refs). No edit (P12).
- *(new)* `suggest_memory` tool for executor proposals (Keeper still decides — memory poisoning firewall preserved from day one).

**Tracks.** Backend (heavy) + Quest tree (light: one event kind + provenance FK) + Timeline (reuse existing) + UI/UX (Memory Inspector v0).

**Depends on.** Nothing strictly; benefits from P1 if shipped first (so claims about captain preferences land cleanly).

**Defers.** Eval harness (P3a). Reranker (P3b). Background rebuild + incremental insert (P3c, P3d). Continuous + idle triggers. Project subtree writes. First-person reports as Keeper input (P6 backfills this). Captain edit / archive / promote (P12).

---

### P3a — Eval harness

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

**Goal.** Top-K=20 candidates from hybrid retrieval get reranked to top-K=5 before context injection. Recall@5 from P3a improves measurably.

**Test scenario.** Run P3a's harness with reranker enabled vs disabled. Recall@5 with reranker > recall@5 without, by some material delta.

**Tickets:**
- [**MON-93**](https://linear.app/monarch-commander/issue/MON-93) — design + impl the BM25+vector reranker.

**Tracks.** Backend only.

**Depends on.** P3a (need the metric to design against).

---

### P3c — Background HNSW rebuild + atomic swap

**Goal.** At realistic memory volumes (10k+), HNSW rebuilds happen in a background worker without blocking writes or reads.

**Test scenario.** Seed 100k memories. Trigger rebuild. Observe: reads continue serving from the previous "last good index" throughout the rebuild. New index swaps in atomically when ready.

**Tickets:**
- [**MON-96**](https://linear.app/monarch-commander/issue/MON-96) — background HNSW rebuild worker with atomic read swap.

**Depends on.** P2 (rebuild path exists in degenerate form).

---

### P3d — Incremental HNSW insert

**Goal.** A new memory written by the Keeper becomes queryable in seconds, not after the next scheduled rebuild.

**Test scenario.** Write a memory. Within 2 seconds, query for it via the production retrieval path. Hit.

**Tickets:**
- [**MON-97**](https://linear.app/monarch-commander/issue/MON-97) — incremental HNSW insert path for per-memory writes.

**Depends on.** P2.

---

### P4 — Executor narration (coherent actions + L2 v0)

**Goal.** Executor declares intent before each chunk of work, executes nested tool calls, and closes with a one-line outcome. The captain reads the timeline at intent level. L2 working memory carries the live present.

**Test scenario.** Captain watches a quest run. The timeline shows a sequence of collapsible coherent actions ("Read failing test files", "Fix the off-by-one in `parser.rs`", "Run the test"), each expandable into its underlying tool calls. The agent view shows `current_action` + last few `recent_actions` from L2 — captain can answer "what is it doing right now?" without scrolling.

**Tickets:** *(unticketed — file at phase open)*
- *(new)* `quest_events` migration: add `parent_event_id`, `author`, `surface_override`, `payload_schema_version`. (Hot-table ALTER, idempotent block per CLAUDE.md § schema evolves.)
- *(new)* New event kinds: `coherent_action`, `executor_action_outcome`, `tool_call`, `thinking`, `executor_decision`. Renderer in `QuestTimelineTool` for nested children.
- *(new)* L2 schema. Start with JSON blob on `agents` (column-decompose later if querying needs it). Fields for v0: `current_action`, `recent_actions`, `current_quest_id`, `current_quest_path`, `updated_at`. **Not in v0:** `planned_actions` (P8), `attention_threads` (P7), `environment` (P11 or its own phase if needed sooner), `blockers`/`open_threads` (P5).
- *(new)* Single-writer pipeline extension for L2 mutations (reuse MON-37 pattern).
- *(new)* Executor system-prompt update: teach intent-declaration + outcome-closure rhythm.
- *(new)* `complete_action(outcome)` tool for the executor.
- *(new)* Working-memory preview in agent view UI.

**Tracks.** Backend + Quest tree (event taxonomy expansion) + Timeline (collapsible-children rendering) + UI/UX (working-memory preview).

**Depends on.** Nothing schema-wise that isn't already there; builds on MON-83's quest skeleton.

**Defers.** Status/scope/direction/rationale on quest_nodes (P5). First-person reports (P6). `planned_actions` field (lands when chat-shadow can write it in P8).

---

### P5 — Rich quest model + manual editor

**Goal.** Quests carry status, scope, current direction, rationale, grade, summary. Captain edits these manually. Plan-change events (`scope_change`, `direction_change`, `subtask_added`, `note`) appear on the timeline.

**Test scenario.** Captain opens a quest detail panel. Edits scope ("expanded to also cover the auth refactor"), supplies rationale. The change persists, surfaces on the timeline as a `scope_change` event with rationale. Closes the quest by setting `status='done'`. Status transition is reflected in the agent view.

**Tickets:** *(unticketed — file at phase open)*
- *(new)* `quest_nodes` migration: `status`, `scope`, `current_direction`, `rationale`, `fork_parent_id` (defined now, used in P10), `worktree_path` (same), `grade`, `summary`. Idempotent ALTER.
- *(new)* New event kinds: `scope_change`, `direction_change`, `subtask_added`, `note`, `blocker`, `blocker_resolved`, `question`, `answer`. (Question/answer wired in event-only form here; chat-shadow consumes them in P8.)
- *(new)* Quest detail panel UI (read + manual edit).
- *(new)* Manual-editor write path (Tauri commands + Rust persistence, single-writer).

**Tracks.** Backend + Quest tree + UI/UX.

**Depends on.** P4 (timeline already renders events; P5 just adds new kinds).

**Defers.** Auto-decomposition by Architect (P8 — subsumes [MON-84](https://linear.app/monarch-commander/issue/MON-84) here, since the Architect is conceptually a chat-shadow tool). Captain-set permission gates on quests (P8 territory).

---

### P6 — First-person quest reports

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

**Defers.** Plan-manipulation tools (`add_to_internal_plan`, `add_subtask`, `change_quest_*`, `note_on_quest`) — P8. Routing intent classifier — P8. Question/answer mediation — P8. Pending-action mediation — P8.

---

### P8 — chat-shadow full + routing classifier

**Goal.** Captain types into chat; chat-shadow classifies intent and takes the appropriate action — adding to plan, expanding scope, redirecting, answering questions, mediating pending actions, speaking back. Subsumes the Architect ([MON-84](https://linear.app/monarch-commander/issue/MON-84)) as the auto-decomposer for high-complexity captain inputs.

**Test scenario.** From the routing table in `attention.md`: captain says "after this also rename `verify` to `validate`" — `add_to_internal_plan`; executor picks up after current action. Captain says "now do Y instead" — `change_quest_direction`; executor switches at next boundary. Captain says "let's now also refactor the test suite" — Architect/auto-decomposer fires (high complexity classification from MON-82) → `add_subtask` with rationale. Captain answers a `question` event — typed `answer` flows back to executor.

**Tickets:** *(unticketed — file at phase open; **subsumes [MON-84](https://linear.app/monarch-commander/issue/MON-84)** — the Architect's role is the auto-decomposer arm of the routing classifier)*
- *(new)* Chat-shadow plan-manipulation tools: `add_to_internal_plan`, `add_subtask`, `change_quest_scope`, `change_quest_direction`, `note_on_quest`, `mark_quest_blocked`, `complete_quest_intent`.
- *(new)* L2 `planned_actions` field (deferred from P4 — first writer is chat-shadow here).
- *(new)* Routing intent classifier — chat-shadow's per-turn classification of captain input, consuming MON-82's classification authoritatively (vs MON-82 Slice 1 which is advisory). Plus the Architect's auto-decomposition path for high-complexity inputs.
- *(new)* `pending_action` family of tools: `propose_pending_action` (executor), `modify_pending_action` / `approve_pending_action` / `reject_pending_action` (chat-shadow). Captain-set permission gates configurable per-quest.
- *(new)* Question/answer mediation: chat-shadow's `answer_question` consumes captain's words and emits typed `answer` events.
- *(new)* Fork-quest tool stub (real semantics in P10).

**Tracks.** Backend + Quest tree (full taxonomy) + UI/UX (routing-driven affordances; pending-action UI).

**Depends on.** P7 (chat-shadow exists) + P5 (rich quest model to mutate).

---

### P9 — Project subtree sharing

**Goal.** Multiple shadows on the same project share `Projects/<P>/...` as living project knowledge. New shadows on a project don't start from zero.

**Test scenario.** Shadow A on project Monarch finishes a task; Keeper writes a `Projects/Monarch/Architecture` claim ("Pi is execution engine, not session authority"). Shadow B (different shadow, same project) starts a related task next day; tree-walk surfaces A's claim into B's context.

**Tickets:** *(unticketed)*
- *(new)* Per-project Keeper serializer (Rust component, single-consumer queue scoped to project_id). Reuses MON-37 pattern.
- *(new)* Project-scoped read flow.
- *(new)* Memory Inspector scope filter (self / project / captain / global).

**Depends on.** P8.

---

### P10 — Forking with worktrees

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
| **P2** | First memory forms during work; `compaction_tick` events appear; Memory Inspector v0 browse. **First "the shadow remembered something" moment.** |
| **P3a** | A trustworthy recall@5 number we can target. |
| **P3b** | Recall@5 measurably improves. |
| **P3c** | Memory works at 1M scale without blocking writes. |
| **P3d** | New memories queryable in seconds. |
| **P4** | Timeline reads as a real execution narrative; captain sees `current_action` in the agent view. **Shadow stops feeling like a chat log.** |
| **P5** | Quests have rich fields; captain edits scope/direction with rationale. |
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
