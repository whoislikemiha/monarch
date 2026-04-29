# Cognitive Substrate

> **Status:** Idea document — exploratory, not a spec. The model proposed here is a starting point captured from an in-progress design conversation. Concepts will evolve as we prototype, and **tech choices in particular are tentative** — anything in this doc that names a specific store, library, schema shape, or mechanism is a current best guess, not a commitment. Treat the layered model and the conceptual decisions as the load-bearing parts; treat the implementation sketches as illustrative.
>
> **Sibling docs:** `attention.md` (multi-thread interface), `distillation.md` (the Keeper, compaction, atomic claims), `flows.md` (per-turn loops, environment, coordination).

## Premise

A shadow is not a conversation. A shadow is a **persistent self** — an identity, a present state, an accumulated body of knowledge — against which one or more attention threads (model calls) attach to do work or hold dialogue.

Today, a shadow's "self" is implicit in: a row in `agents` (name, grade, project), the system prompt built by `shadow-oath.ts`, the message history of its sessions, and whatever the model can infer from those messages mid-turn. There is no continuous representation of "what does Igris know," "what is Igris doing right now," or "who is the captain to Igris." The model reconstructs a partial answer per turn from raw transcripts, which is why scaling, compaction, and concurrent interaction all break.

This document defines the **substrate** — the persistent representation of a shadow's self. It has four layers, each with a different freshness, density, and access pattern. Together they answer what a shadow is *between* turns.

## The four layers

| Layer | Name | Holds | Always in context? | Writer | Reader |
|-------|------|-------|---------------------|--------|--------|
| **L1** | Identity | Captain self + shadow self + always-true beliefs | Yes | Captain (UI), Keeper (promotions) | Every turn |
| **L2** | Working memory | "Where am I, what am I doing" — live state | Yes | Executor (ephemeral), Keeper (consolidation) | Every turn, every attention thread |
| **L3** | Knowledge tree | Accumulated atomic claims, organized by topic | Tree-walked, contextually surfaced | Keeper only | Every turn (subset), `memory_search` tool (full) |
| **L4** | Search | *Access pattern over L3*, not separate storage | On demand | — | Tool invocation |

L4 is not a layer of data — it is the explicit, on-demand access path *into* L3 when tree-walk doesn't surface what's needed. There is one knowledge tree; it is read two ways.

## L1 — Identity

The substrate's most stable layer. Always loaded into every turn for every attention thread of every shadow. Two tiers:

### L1a — Captain layer

The captain is a **singleton** in v1: one captain per Monarch installation, the local user. Future evolution to multi-captain is a promotion of the singleton to a first-class entity; the substrate model accommodates this without restructure.

The captain layer carries facts about the captain that every shadow on this captain inherits. Examples:

- **Name and presence:** "Miha — the Shadow Monarch."
- **Standing preferences:** "Prefers terse responses with no trailing summaries." "Prefers worktrees in `../`, not `.claude/worktrees/`."
- **Domain context:** "Building Monarch (multi-agent desktop command center)." "Working primarily in Rust + Svelte 5 + TypeScript."
- **Rituals and protocols:** "Linear-first development." "Always move tickets to In Review when opening a PR."
- **Long-term arcs:** what the captain is building toward, what they care about deeply.

A new shadow boots up already knowing these. Captain edits propagate immediately to every shadow's next turn — no per-shadow re-teaching.

The captain layer is the persistent substitute for what `auto memory` already does for Claude Code in this repo. The same content, durably typed, queryable, and visible to every shadow as a first-class part of their context.

### L1b — Shadow layer

The shadow's own identity — what makes Igris *Igris*, distinct from another shadow on the same captain.

- **Name and title:** as today (`agents.name`, `agents.title`).
- **Grade:** as today (`agents.grade`), with the descriptive material from `shadow-oath.ts` either remaining hardcoded (current behavior) or migrating into editable identity rows.
- **Oath / role:** the shadow's specific charge. "I am Igris, First Shadow, given Rust expertise and the responsibility of Monarch's backbone."
- **Personality directives:** how this shadow speaks, holds itself, what it cares about. Today derived from grade; evolves into per-shadow editable text.
- **Relationships:** "I serve the Monarch directly." Future: "I work alongside Beru on the sidecar."

L1b is small (a few hundred tokens at most). Edit is a captain action via the app UI.

### Storage and editing

L1 lives in SQLite, surfaced and edited entirely in-app — no file juggling. Schema sketch:

```sql
CREATE TABLE captain (
  id INTEGER PRIMARY KEY CHECK (id = 1),  -- enforce singleton
  name TEXT NOT NULL,
  current_version INTEGER NOT NULL REFERENCES captain_identity_versions(id)
);

CREATE TABLE captain_identity_versions (
  id INTEGER PRIMARY KEY,
  captain_id INTEGER NOT NULL REFERENCES captain(id),
  payload TEXT NOT NULL,  -- JSON: { preferences, domain, rituals, ... }
  created_at TEXT NOT NULL,
  supersedes_id INTEGER REFERENCES captain_identity_versions(id),
  edit_note TEXT
);

-- agents table extends:
ALTER TABLE agents ADD COLUMN identity_version_id INTEGER
  REFERENCES shadow_identity_versions(id);

CREATE TABLE shadow_identity_versions (
  id INTEGER PRIMARY KEY,
  agent_id TEXT NOT NULL REFERENCES agents(id),
  payload TEXT NOT NULL,
  created_at TEXT NOT NULL,
  supersedes_id INTEGER REFERENCES shadow_identity_versions(id),
  edit_note TEXT
);
```

**Versioning is row-per-version (full snapshot), with `supersedes_id` chains.** Same pattern we'll use for memories. Captain can roll any L1 row back to any prior version. Shadow identity edits are the same.

Versions are the historical record. The "live" identity is whichever row is pointed to by `current_version` (captain) or `identity_version_id` (shadow). Rolling back is a pointer flip — old versions stay queryable.

## L2 — Working memory

The shadow's live present. *Where am I, what am I doing right now.* Always loaded into every turn for every attention thread of this agent.

L2 exists so that the chat thread can answer "what are you working on?" by reading a structured field, not by replaying 20 minutes of executor tool calls. L2 is also what makes branching meaningful — when a shadow forks, each branch has its own L2 and they diverge from there.

### Schema

L2 is **structured, not prose** — both the executor and chat thread must agree deterministically on what counts as the current state. It lives as a per-agent JSON payload in `agent_working_memory`, not as a column on `agents`; the payload can be column-decomposed later if querying needs it.

```sql
CREATE TABLE agent_working_memory (
  agent_id TEXT PRIMARY KEY REFERENCES agents(id) ON DELETE CASCADE,
  payload_json TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
```

The P4 v0 shape is intentionally narrow:

```typescript
interface WorkingMemoryV0 {
  schema_version: 1;
  current_quest_id: string | null;        // FK into quest_nodes
  current_quest_path: string[];           // ["Project: Monarch", "MON-82", "Slice 1"]
  current_action: CurrentAction | null;   // active coherent_action event
  recent_actions: RecentAction[];         // last N (~10) completed / auto-closed coherent actions
  updated_at: string;
}

interface CurrentAction {
  event_id: string;
  quest_id: string;
  intent: string;
  started_at: string;
}

interface RecentAction {
  event_id: string;
  quest_id: string;
  intent: string;
  outcome: string;
  completed_at: string;
  auto_closed?: boolean;
}
```

Later phases extend the payload with plan, thread, blocker, and environment slices:

```typescript
interface WorkingMemory {
  current_quest_id: string | null;        // FK into quest_nodes
  current_quest_path: string[];           // ["Project: Monarch", "MON-82", "Slice 1"]
  current_action: CurrentAction | null;
  recent_actions: RecentAction[];
  active_plan_item_id: string | null;     // P4b: current intended plan item
  next_plan_item_ids: string[];           // P4b: compact next-step slice
  blockers: string[];                     // ["awaiting captain on architecture choice"]
  open_threads: string[];                 // things not yet wrapped, may surface in next turn
  attention_threads: AttentionThread[];   // executor + chat + ... (see attention.md)
  environment: EnvironmentSnapshot;       // ambient state of the world
  updated_at: string;
}

interface EnvironmentSnapshot {
  cwd: string;
  git: {
    branch: string;
    dirty_files: string[];
    ahead_behind: { ahead: number; behind: number };
    last_commit_sha: string;
  };
  recent_files_touched: string[];
  active_processes: string[];             // dev servers, watch tasks
  updated_at: string;
}
```

L2 stores pointers and compact summaries. It does **not** duplicate file contents, full tool results, or the execution plan. Evidence lives in the nested quest events (`tool_call` children); durable intended route lives in `quest_plan_items` once P4b lands.

### Two writers, strict separation of concerns

**The executor writes ephemeral fields in real-time.** As the executor takes actions, it mutates `current_action`, appends to `recent_actions`, updates blockers when that field exists. This is its own working state and it owns the freshness.

**The Keeper writes consolidation at compaction ticks.** When distillation runs, the Keeper:
1. Reads the raw stream since the last tick.
2. Reads the executor's L2 mutations since the last tick.
3. Decides what's worth promoting to L3 (the executor never makes this decision — its job is to act and narrate, not to curate persistence).
4. Updates L2 to a clean post-compaction state: stable summary of where we are, recent_actions condensed.

The separation is: **executor is the thing happening; Keeper decides what's worth remembering.** The executor can't be trusted to be its own historian — that's a conflict of interest and a known failure mode of self-narrating agents.

### Single-writer ordering

Despite two writers, only one process at a time mutates L2. This is the same pattern as the existing single-consumer persistence pipeline (MON-37): writes are serialized through a Rust-owned channel. Cross-thread coherence is Rust's job. Detail in `attention.md`.

### Three layers of intention

Working memory and the quest tree (defined in `attention.md`) together express *what* a shadow is doing across three timescales:

- **Quest** — *what* the captain wants. Goals, scope, direction. Captain-managed via chat-shadow's plan-manipulation tools. Coarse-grained, deliberate, rationale-required to change. Lives in the quest tree.
- **Execution plan** — *how* the shadow currently intends to solve the current quest. Lives in durable quest-scoped `quest_plan_items` once P4b lands. Executor-managed and chat-shadow-editable. Provisional, reorderable, and allowed to be wrong. L2 carries only the active/next plan slice.
- **Coherent action** — *doing*. The current chunk of work the executor is narrating to the timeline (see `attention.md`). It may link to an execution plan item after P4b, but it is actual history, not the plan itself.

This separation matters because most "next steps" are tactical, not strategic. When the captain says *"after this, also grep for X,"* that's an addition to the execution plan — it doesn't merit a quest-tree subtask and doesn't surface as a captain-visible scope change. Frictionless.

Quest changes, by contrast, are first-class. Scope expansion, direction change, subtask addition — all carry rationale and surface to the captain on the timeline. The cost of changing a quest is intentional friction: it's the lever that actually defines the work. Chat-shadow's classification (`add_to_execution_plan` vs `add_subtask`) makes this distinction explicit per turn.

## L3 — Knowledge tree

The accumulated body of what the shadow has learned. A tree of atomic claims organized by topic. Position in the tree carries semantic meaning — a preference under `Tests` is operationally a test-preference because of where it lives, not because of how it's tagged.

### Top-level taxonomy (locked for v1)

Four reserved roots beneath each shadow:

```
ShadowRoot
├── Identity            ← claims about self that go beyond L1 essentials
├── CoreBeliefs         ← always-true things this shadow holds (cross-cutting principles)
├── GeneralKnowledge    ← cross-project learnings (languages, tools, patterns)
└── Projects            ← per-project subtrees (see "Project memory" below)
```

Below depth 1, structure is **organic** — the Keeper grows the tree as topics accumulate. New top-level roots can be added in later versions; they are reserved in v1.

### Inner nodes vs leaves

- **Leaves** are atomic claims. Single assertion, self-contained, with provenance. Defined in `distillation.md`.
- **Inner nodes** are topics. They have a *summary* (what is this branch about) but no atomic content of their own. Their content is the union of their leaves.

Inner-node summaries are Keeper-generated and re-generated when the subtree changes substantially. Captain can edit them by hand in the Memory Inspector.

An inner node is born when a topic accumulates roughly 5–10 leaves and the Keeper recognizes a coherent grouping. The Keeper rebalances periodically — too-flat trees get nodes inserted, too-deep trees get collapsed.

### Project memory as shared substrate

This is the most important property of L3. **Project subtrees are shared across all shadows working on that project.** Igris's `Projects/Monarch/Architecture` is the same subtree another shadow on Monarch reads and writes.

Concretely, this transforms the project subtree from "memory of this project" into **the project's living understanding of itself, written by its shadow workers**. It carries:

- **Architecture understanding:** "Rust owns state, sidecar operates, frontend displays. SQLite is canonical. Pi is execution engine, not session authority." — written once, read by every shadow on the project from then on.
- **Research findings:** "Investigated MON-82 sidecar event ordering on 2026-04-22. Root cause: classification arrives before user message_end. Fix: classification_id on message_end, FK backfill." with references to the relevant files.
- **File status with stale-flagging:** memories that reference specific files (`src-tauri/src/agent/manager.rs:42`) are checked against git on load. If the file has been changed by a commit *after* the memory was written, the memory is flagged `stale_at: <commit_sha>`. Doesn't archive — signals "verify before relying." More on this below.
- **Cross-shadow handoffs:** when a different shadow continues work on a project, the project subtree gives them what was happening, what was decided, and where things stand — without re-reading the codebase and git history from scratch.
- **Decisions and conventions:** what the project has chosen, why, what's been tried and rejected.

A new shadow joining a project doesn't start from zero. It inherits the project's accumulated self-knowledge as starting context. The "first day on the team" cost collapses.

This is what `CLAUDE.md` and `ONBOARDING.md` already try to be — project self-knowledge. The L3 project subtree is the structured, machine-queryable, continuously-updated version of the same idea. The MD files remain valuable as human-curated overviews; L3 holds the granular, per-claim accumulation.

#### Multi-writer discipline

Shared project subtrees mean multiple shadows can write to the same branches. To avoid duplicate or conflicting claims:

- All L3 writes go through the Keeper, never directly from any shadow.
- Keeper writes are serialized per project (single-consumer queue per project subtree).
- Before insert, the Keeper hybrid-searches the local subtree; matches above a threshold trigger merge or supersede instead of insert.

Conflict resolution sits in `distillation.md`. The substrate's role is just to declare the contract: one writer per project subtree at a time, multiple readers.

#### Shadow-private subtrees

Not everything a shadow learns belongs to the project. A shadow may have:

- **Self-reflection:** "I tend to over-engineer when given vague tasks." Lives under `Identity` for that shadow only.
- **Personal style:** "I prefer to read tests before reading source." Lives under `CoreBeliefs` for that shadow.

These are written into the shadow's own `Identity` / `CoreBeliefs` / `GeneralKnowledge` subtrees, which are not shared. Project subtrees are shared; the other three roots are per-shadow.

### Stale-flagging via git

Memories that reference files have a `file_refs` array of `{path, anchor_sha}`. At load time, when the L3 retrieval surfaces such a memory, Rust checks: has any commit touched this file *after* `anchor_sha`? If yes, the memory is loaded with a `stale: true` annotation. The model sees the claim plus the warning, and can either trust it conditionally or call a verification tool.

The Keeper periodically sweeps stale memories and re-validates the most-accessed ones. Stale memories are never auto-archived — code drift doesn't always invalidate the underlying claim, and false-archives are catastrophic for memory quality.

## L4 — Search as access pattern

L4 is not a separate data tier. It is the explicit, on-demand access path *into* L3 when contextual tree-walk doesn't surface what's needed.

Two access modes over the same L3 store:

- **Tree-walk (always-on):** load L1 + L2 + walk relevant L3 subtrees based on `current_quest_path` and topic context. Budget-capped. This is what every turn gets for free.
- **`memory_search(query, scope?, k?)` tool (on demand):** the shadow (or the Keeper, or the chat thread) explicitly requests a hybrid BM25 + vector search across all of L3 the shadow can see. Returns top-K with provenance.

When the always-on tree-walk fails to surface something the shadow needs, the shadow notices and reaches for the search tool. Tool invocations are logged — patterns of "had to search for X" are signal that X should be promoted into the always-on band.

There is one tree, one set of indices (FTS5 + per-row embeddings), two read paths. No cold-tier separate storage.

## Loading the substrate for a turn

Every attention thread, on every turn, assembles its context window in this order:

```
[Pi system prompt]                          ← built from L1
[L1 Captain identity]                       ← always
[L1 Shadow identity]                        ← always
[L2 Working memory]                         ← always (fresh snapshot)
[L3 Tree-walk: current quest path,
     contextually-surfaced subtrees]        ← budget-capped
[Recent significant events]                 ← last K from event log
[Raw conversation since last compaction]    ← high-fidelity recent
[The new turn's input]                      ← user message or tool result
```

### Token budget

Indicative starting budget for a 200k-context model:

| Layer | Target | Hard cap |
|-------|--------|---------|
| L1 captain + shadow | 500–2000 | 3000 |
| L2 working memory | 500–1000 | 2000 |
| L3 tree-walk (warm subtrees) | 1000–3000 | 5000 |
| Recent events | 500–1500 | 2500 |
| Raw conversation since compaction | up to remaining budget, triggers compaction at 30k | 30k |

Budgets are configurable in `~/.config/monarch/memory.toml` (mirrors `classifier.toml`, `thinking.toml`). When L3 walk would exceed its cap, the Keeper's pre-turn pass selects the highest-priority subtree slice based on quest context and recent search activity. Selection logic lives in `distillation.md`.

## Branching the substrate

When the captain forks a shadow to try multiple approaches in parallel:

- **L1 (captain + shadow identity):** **shared.** Same self in both branches.
- **L2 (working memory):** **forked.** Each branch gets its own working memory, both starting from the snapshot at fork time and diverging.
- **L3 (knowledge tree):** **shared-read, fork-local writes.** Both branches see the same accumulated knowledge as starting context. Any new claims either branch forms during the fork live in a branch-local subtree namespace under the active quest. They are not visible to other shadows or to the unforked main tree until merged.
- **Quest tree:** the fork creates parallel sub-quests under a shared parent. Each branch has its own quest subtree.
- **Code:** git worktrees handle the code side. `git worktree add ../monarch-fork-A <branch>` per fork. Worktree path is recorded on the fork's quest node.

When the captain picks a winner:
- Winner's branch-local L3 claims promote into the main project subtree (Keeper-mediated merge).
- Loser's branch-local L3 claims archive (preserved for forensics, not retrieved).
- Winner's git branch merges; loser's branch is dropped or kept for reference per captain's call.
- Both branches' working memories archive into events.

Branching is therefore *git for the cognitive substrate*: code, working state, and emerging knowledge all fork together and reconcile on captain decision. The substrate is what makes this coherent — without shared L1 the two branches would feel like different shadows; without forked L2 they'd interfere; without shared-read L3 they'd start from amnesia.

## Implications for the data model

> The schema sketches below are **illustrative, not prescriptive.** They show *roughly* what the model implies, not what we will build. Real shapes (column choices, table boundaries, whether to use JSON blobs vs decomposed columns, etc.) get worked out in implementation tickets. The underlying storage stack (SQLite + Rust-side HNSW + ONNX-shipped embedding model) is **validated by [MON-91](../../spike/MON-91-storage/) — see [`thoughts/spike/MON-91-storage.md`](../spike/MON-91-storage.md)**; remaining openness is about row-shape choices above that stack, not about the stack itself.

- `captain` table (singleton, id=1). Holds captain row + pointer to current identity version.
- `captain_identity_versions` table. Row-per-version with supersedes chain.
- `shadow_identity_versions` table. Same pattern, FK to `agents`.
- `agents.identity_version_id` column, FK to `shadow_identity_versions`.
- `agent_working_memory` table. Per-agent JSON payload for L2; starts with `current_action` / `recent_actions` and grows by phase.
- `memories` table redesign per `distillation.md`. Includes `parent_id` (tree edge), `scope` (self|project|captain|global), `project_id`, `supersedes_id`, `archived_at`, `file_refs`, `embedding_model_id`, etc.
- `memories_fts` virtual table (FTS5).
- Vector index: SQLite BLOBs + Rust-side HNSW sidecar file via `instant-distance`. Rebuildable from BLOBs on cold start. **Validated by [MON-91](../spike/MON-91-storage.md)** — at 1M synthetic vectors, p99 query = 5.8 ms, binary +25 MiB; at 10k real embeddings from `bge-small-en-v1.5`, recall@10 = 1.000.
- Forks: a `quest_nodes.fork_parent_id` (or sibling table) for parallel-attempt subquest groupings. Worktree path stored on quest.

## What this document does not cover

By design, the following are out of scope and live in sibling docs:

- **How attention threads share the substrate** — single-writer rules, event streams, the "talk to the agent while it works" mechanics. → `attention.md`
- **What an atomic claim is** — definition, types, what to capture, what to exclude. → `distillation.md`
- **The Keeper** — when it runs, what model, what it produces, three-layer record (raw stream / first-person quest report / third-person atomic claims). → `distillation.md`
- **Compaction triggers** — token thresholds, semantic triggers, idle ticks. → `distillation.md`
- **Concrete schema migrations** — the SQL above is a sketch; implementation tickets own the actual `ALTER TABLE` blocks.
- **UI for editing identity / browsing memory** — captured in feature tickets, not design docs.

## Working assumptions captured here

These are the conceptual positions the doc currently rests on, listed so sibling docs and later conversations can reference them by number. **Treat them as the current direction, not as final calls.** Any of them can be revisited — most likely candidate for revision is #6 (taxonomy is locked *for v1*, not forever). #12 is no longer tentative after MON-91.

1. **Four layers:** L1 identity, L2 working memory, L3 tree, L4 search-as-access-pattern over L3.
2. **Captain is a singleton in v1**, first-class evolution preserved.
3. **L1 is row-per-version with supersedes chains.** Captain edits via UI.
4. **L2 has two writers** (executor real-time, Keeper consolidation), one serializer (Rust). Schema is structured, not prose.
5. **L3 is a tree, not a graph.** Position carries relevance.
6. **Top-level taxonomy is locked for v1:** Identity / CoreBeliefs / GeneralKnowledge / Projects.
7. **Project subtrees are shared across shadows on the project.** Other roots are per-shadow.
8. **All L3 writes go through the Keeper.** No shadow writes directly to its own memory.
9. **Stale-flagging is annotation, never auto-archive.**
10. **Branching is shared L1, forked L2, shared-read + fork-local-write L3, plus git worktrees for code.**
11. **One tree, one set of indices, two read paths** (tree-walk + search). No separate cold tier.
12. **Storage stack (validated by [MON-91](../spike/MON-91-storage.md)):** SQLite canonical, vector index as a Rust-side HNSW sidecar file via `instant-distance` rebuildable from SQLite BLOBs, embeddings via `bge-small-en-v1.5` through the `ort` crate. Alternatives (Kùzu, LanceDB, SurrealDB) considered and rejected — the spike confirmed the simpler SQLite-plus-sidecar path clears every bar.
