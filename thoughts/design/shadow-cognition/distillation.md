# Distillation

> **Status:** Idea document — exploratory, not a spec. Captures the conversation about how raw activity becomes lasting knowledge — the cognitive metabolism that turns a stream of tool calls and dialogue into the layered substrate. **Tech choices are tentative.** The conceptual model is load-bearing; specific models, schemas, and thresholds are illustrative.
>
> **Sibling docs:** `substrate.md` (the four-layer self), `attention.md` (chat + executor + quest tree), `flows.md` (per-turn loops, environment, coordination).

## Premise

A shadow accumulates raw activity faster than the model's context window can hold it: tool calls, observations, decisions, dialogue, blockers, redirects. Without something doing continuous condensation, every long task ends with the captain hitting "compact this" and the shadow forgetting most of what it just learned.

The premise of this doc is that **compaction and memory formation are the same operation at different timescales**. There is no separate "summarize the conversation" step and "extract long-term memories" step. There is one process — distillation — that runs continuously and produces structured persistent artifacts.

The component that runs distillation is **the Keeper**. It is the cognitive metabolism of the shadow: continuous, structured, and the only writer to the long-term memory tree.

## The three-layer record

Every piece of shadow activity exists in one of three forms, with distillation moving content from raw to structured as time passes:

**L0 — Raw stream.** Messages, tool calls, tool outputs, dialogue verbatim. Preserved indefinitely in `messages` / `quest_events`. High fidelity, high volume. Replayable. The source of truth for everything else.

**L1 — First-person quest report.** Written by the executor itself at quest close. A structured narrative: what the quest was, what was decided, what was learned, what was produced, what's left, brief reflection. The shadow's own voice. Captain-readable as a record. Feeds the Keeper as a high-quality input alongside L0.

**L2 — Third-person tree memories.** Written by the Keeper (continuously and at quest close). Atomic claims with provenance, deposited in the L3 knowledge tree by topic position. Optimized for retrieval and re-use across future quests.

These three are *additive*, not destructive. The first-person report does not replace the raw stream; the tree memories do not replace the report. Older raw streams stay archived and queryable — if Keeper v2 is smarter than v1, we can re-distill old transcripts into a better tree without losing anything.

## The Keeper

The Keeper is a continuously-running distillation process, scoped per shadow. It consumes raw stream + chat + first-person reports, produces structured outputs, writes to the substrate.

### Where it runs

Inside the long-lived sidecar (per Monarch's existing architecture), as a per-shadow worker. One Keeper instance per shadow, queue-based, processing distillation requests as they arrive. Backpressure: if Keeper falls behind, executor doesn't block — distillations queue up and get processed when capacity allows.

Crash recovery: on sidecar restart, the Keeper scans for incomplete distillations (e.g., quests with `status='done'` but no completed compaction tick covering the closing event) and resumes.

### What model it uses

Two-tier configuration in `~/.config/monarch/memory.toml`:

- **Routine compaction** — defaults to a **local model** (e.g., Qwen 2.5 14B via LM Studio). Cheap, private, offline-capable. Most distillation is read-heavy + structured extraction; a 14B class model handles this well.
- **Quest-end deep distillation** — optionally a **cloud model** (e.g., Anthropic Haiku) for higher-quality consolidation passes. Configurable per-captain.

Sub-8B local models are too small for reliable merge/supersede decisions; 14B+ is the floor. The Keeper sees the *full raw transcript* of everything the shadow does, so privacy weighting toward local is real, not just performance optimization.

The reason for two tiers: continuous compaction at fleet scale could be hundreds of Keeper calls per day. Local handles the metabolism cheaply; cloud is reserved for the deeper reflection pass at quest close where extra quality matters.

### What context it sees

Per distillation call, the Keeper assembles:

- **Raw stream slice** — events since the last compaction tick (executor activity + dialogue + plan changes).
- **Relevant tree slice** — hybrid-search the existing memory tree for topics related to the slice. Crucial: without this, the Keeper has no way to know whether a candidate claim is novel or already known.
- **Working memory snapshot** — `current_quest_path`, `recent_actions`, `blockers`, `open_threads`. Tells the Keeper what context it's distilling from.
- **First-person quest report** — when distilling at quest close.

Output per call: structured JSON with proposed events, memories, artifacts, working-memory updates. Rust applies the writes through the single-consumer pipeline.

## Compaction triggers

The Keeper runs in response to several triggers, each with different cadence and depth:

### Continuous (token-pressure)

The workhorse. Fires roughly every ~30k tokens of accumulated raw stream per shadow. Hierarchy:

- **Soft trigger** at ~25k tokens — wait for the next natural breakpoint (end of coherent action, end of model response, idle moment) within ~5k more tokens. Compact then. Avoids cutting mid-action.
- **Hard trigger** at 30k — force a clean cut regardless. Backstop for cases where breakpoints don't arrive.

The 30k threshold is configurable. Conservative starting value; calibrated by usage.

### Semantic (event-driven)

Some moments deserve a distillation pass regardless of token pressure:

- **Quest completion** (`status` transitions to `done`) — fires the *deep* distillation pass. Full first-person report from executor + raw stream + tree slice. Higher-quality model if configured. This is the consolidation moment for what the quest taught.
- **Quest abandonment** — lighter pass; capture lessons-learned without expecting durable claims.
- **Captain command** ("checkpoint this") — explicit captain trigger, e.g., before they leave for the day.
- **Major decision detected** — heuristic; if the chat or executor stream contains decision-shaped events with rationale, fire a distillation tick to capture the decision before it gets buried.

### Idle

When the shadow has been inactive for N minutes (configurable, default ~10), fire a low-pressure idle tick. Consolidates working memory into a clean state, refreshes environment snapshot, may run a re-verification sweep on the most-accessed stale memories (see "Stale-flagging" below).

Idle ticks are also the right moment for the Keeper to do "deep" work that's not urgent: regenerate inner-node summaries for subtrees that have changed substantially, prune long-unused memories, etc.

## What the Keeper produces per tick

Each Keeper run emits some subset of:

**Events** (`compaction_tick` and others). Significant moments worth pinning to the quest's event log even if they don't become long-term memories. "Decided to use tree, not graph at 14:32." "Tool X failed three times before working." Lives in `quest_events`, indexed by quest, surfaced on the timeline.

**Memories** (atomic claims for L3). New facts, decisions, conventions, corrections, preferences, landmarks. Written into the tree at the appropriate topic position. Most ticks produce zero or one new memory; a breakthrough tick produces several. See "Atomic claims" below for definition.

**Artifacts** (references to things produced). Files written, plans drafted, PRs created. These have their own existence outside the conversation; the memory tree references them, doesn't duplicate them.

**Working-memory updates.** A clean post-compaction L2 state: condensed `recent_actions`, refreshed `current_action` pointer/summary, refined `open_threads` once that field exists. The executor mutates L2 in real-time during work; the Keeper consolidates at compaction ticks (see `substrate.md` § L2).

The Keeper's output is structured JSON; Rust's persistence pipeline applies the writes atomically.

## Atomic claims

The unit of L3 memory.

### Definition

A claim is **a single assertion that could be right or wrong, self-contained enough to stand alone when pulled into context.**

The test:
- Read this claim without its neighbors. Do you understand it? If no → it's too small or missing context. Add a parenthetical, or expand.
- Does it contain two assertions? If yes → split.
- Could the claim be refuted by a counterexample? If no (e.g., "Auth is important") → it's not a claim, it's an opinion stub. Reject or specify.

Roughly one sentence for `summary`, 1–3 sentences for `content`. Examples of well-formed claims:

- "Session token TTL is 24 hours."
- "Token rotation invalidates existing sessions, which can log users out unexpectedly."
- "Auth TTL constraint comes from compliance, not product preference."
- "We explored shortening TTL to 1 hour but rejected it due to UX impact on long-running sessions."

Each is one sentence, one assertion, falsifiable, makes sense alone.

### Claim types

Position in the tree carries the primary semantic load (a preference under `Tests` is operationally a test-preference because of *where* it lives, not because of how it's tagged). But an explicit `kind` field still helps for: the Keeper's extraction prompt (knows what to look for), the Memory Inspector UI (filters), and merge logic (only merge claims of the same kind).

Types:

- **Fact** — "The database is Postgres 15."
- **Decision** — "We chose 24h token TTL over 1h."
- **Constraint** — "Compliance requires ≥24h token TTL."
- **Convention** — "Tests use Vitest, not Jest."
- **Preference** — "Captain prefers terse responses with no trailing summaries."
- **Correction** — "Don't use `tauri::invoke` directly; route through `src/lib/api.ts`."
- **Landmark** — "Shipped MON-82 on 2026-04-22, classifier live."

Not exclusive — a claim can be both a Decision and a Constraint. The Keeper picks the dominant type; multiple types possible if the model is confident.

### Sources

Memories come from two streams:

- **Executor activity** — what was done, what was tried, what failed, what was decided in the work.
- **Captain ↔ shadow dialogue** — what was discussed, decided in conversation, observed by the captain. **Chat is a first-class Keeper input alongside executor stream.** Half the meaningful learning happens in dialogue (preferences, redirects, observations); we'd lose it if the Keeper only watched the executor.

Both feed into the same distillation pass, both contribute to the same tree.

### What NOT to capture

This is maybe the most important section. Memory systems fail by capturing too much. Explicit exclusions:

- **Conversational chitchat.** "thanks", "ok", "looks good." Zero retrieval value.
- **Tool outputs verbatim.** Already in `messages` / `events`. Memories cite them; never duplicate them.
- **Transient state.** Currently-open files, working git branch, in-flight test runs. Lives in `L2.environment`, refreshed continuously.
- **Anything trivially derivable from code or git history.** "Function `verify_token` is in `auth.rs`." Just grep. Don't poison the tree with searchable facts.
- **Duplicate learnings.** The Keeper's merge job is to prevent these; if a candidate matches an existing claim above threshold, merge or supersede instead of inserting.
- **Sessile observations during routine work.** "Read `auth.rs`." Routine work doesn't generate memories; *learning* generates memories.
- **Procedural knowledge (skills, recipes, how-tos).** These live as files separately (`~/.config/monarch/skills/{skill_id}.md`), not in L3. Same pattern as prompts and avatars — hand-curated, version-controllable, code-shaped rather than data-shaped. L3 holds *declarative* knowledge (facts, decisions, conventions); procedural knowledge is its own surface.

A useful negative test: would I want to find this claim again in 6 months? If not, don't capture.

## Merge / supersede / insert / archive logic

Before the Keeper inserts a new candidate claim, it does a hybrid-search (BM25 + vector) over the relevant local subtree for similar claims. Cosine thresholds drive the decision:

| Cosine | Same topic? | Action |
|--------|-------------|--------|
| ≥ 0.9 | Same | **Merge** — edit existing claim's content/summary to incorporate the new information; don't create a new row. |
| 0.8–0.9 | Same | **Supersede** — archive the old claim, write the new one with `supersedes_id` linking back. Preserves edit history. |
| 0.7–0.8 | Same | **Insert as sibling** — distinct claim that's semantically related. Both stay. |
| < 0.7 or different topic | — | **Insert under best-fit topic.** Possibly create a new inner node if the topic doesn't have a home yet. |

Thresholds are starting points; calibrate after eval (see "What this doc does not cover" → eval harness).

### Archive triggers

Memories are never hard-deleted. Archive triggers:

- **Superseded** — `supersedes_id` chain replaces it.
- **Source quest rolled back** — quest archived → memories born from that quest archive (preserve provenance, don't lose forever).
- **Captain action** — explicit "this is wrong, archive it" via Memory Inspector.
- **Long-unused + low-confidence** — future heuristic; not v1.
- **Re-verification refuted** — see "Stale-flagging" below.

Archive is a flag (`archived_at`), not a delete. Captain can un-archive. The Keeper's retrieval queries filter `archived_at IS NULL` by default but can include archived for forensics.

## Inner nodes and tree growth

Inner nodes are *topics*. They have a summary (one paragraph, "what is this branch about") but no atomic content of their own — their content is the union of their leaves.

### Generation

Inner-node summaries are **Keeper-generated** and **regenerated when the subtree changes substantially** (new claims that shift the topic's boundaries, supersessions that change the dominant claims). Generation cadence:

- After merging or inserting into a subtree → opportunistic regenerate if change is substantial.
- During idle ticks → regenerate stale inner-node summaries proactively.
- On Memory Inspector view → regenerate on demand if user requests.

Captain can edit inner-node summaries by hand in the Memory Inspector; manual edits get a `manual_override: true` flag, the Keeper preserves them and only regenerates if explicitly asked.

### Birth and rebalancing

An inner node is born when the Keeper recognizes a coherent grouping among ~5–10 sibling claims. Until then, claims live as siblings under their parent topic.

The Keeper rebalances periodically: too-flat trees get inner nodes inserted (new topic emerges), too-deep trees get inner nodes collapsed (topic was over-decomposed). Rebalancing is a background activity, never destructive — claims keep their content, only their tree position changes.

## Multi-shadow project memory

Project subtrees are shared across shadows working on the same project (per `substrate.md`). Multiple shadows producing memories simultaneously creates a write-coordination problem.

### Ownership model

- **Per-shadow Keeper** for each shadow's private subtrees (`Identity`, `CoreBeliefs`, `GeneralKnowledge`).
- **Per-project serializer** for project subtree writes. Any shadow's Keeper proposing a write to `Projects/<P>/...` routes through a single consumer queue scoped to project P. Atomic, ordered, dedup-aware.

The serializer is a Rust component, not its own LLM — it just receives proposals from Keepers, applies merge/supersede/insert logic against the project subtree, and writes results. The LLM-driven decisions still happen in the Keeper; the serializer enforces single-writer ordering.

### Cross-shadow read flow

A new shadow joining a project doesn't start from zero. It reads the project's accumulated subtree as starting context — *this is the project explaining itself*. Architecture, conventions, recent research, file references with stale flags, decisions and rationales. The cost of "first day on the team" collapses.

When shadow A finishes a research task ("investigated MON-82 sidecar event ordering, root cause was X, fix is Y"), shadow B working on a related task next month reads that finding directly. No re-investigation. No "let me read the code from scratch."

### Conflict resolution

Two shadows generating slightly different claims about the same thing within the same window (rare but possible):

- Both go through the project serializer.
- Serializer applies merge/supersede logic per the threshold table.
- If both claims survive (siblings, threshold 0.7–0.8), captain can later consolidate via Memory Inspector if desired.

The hard rule: two simultaneous writes to the same memory row are impossible because the serializer is the single writer. Logical conflicts (different shadows asserting different things) are accepted as siblings until resolved.

## Stale-flagging and organic re-verification

Memories about code reference files via `file_refs: [{path, anchor_sha, sections?}]`. Code drifts. Memories rot. The system needs to know.

### Lazy flagging at load time

Every memory load checks `file_refs` against current git state:

- File at `path` unchanged since `anchor_sha` → memory is fresh, no annotation.
- File changed since `anchor_sha` → memory is loaded with `stale: true` annotation, plus a brief diff summary if useful.

The flag is **visible to the consumer**. Executor or chat-shadow sees the claim with the stale signal, can choose to: trust it, verify it, ignore it, or treat it as "probably true but verify before relying."

This is cheap (one git query per loaded memory), comprehensive (every load is checked), and correct (consumer makes the call based on context).

### Organic re-verification

When the executor reads a stale-flagged memory and *naturally verifies it as part of its work* (reads the file, observes whether the claim still holds), that verification is signal. Three outcomes:

- **Confirmed** — the file changed but the claim still holds. Re-anchor: update the memory's `file_refs.anchor_sha` to the current commit. Memory is fresh again.
- **Refuted** — the claim no longer holds. Supersede with a corrected claim, or archive if no longer applicable.
- **Partially valid** — split the claim if needed, supersede the broken half.

The executor's verification work feeds back to the Keeper, which writes the appropriate result. The system *learns from being read* without doing speculative re-verification work.

### Background sweeps (optional)

During idle ticks, the Keeper *can* re-verify the most-accessed stale memories proactively (sample top-N by access count, do a small Keeper call to check against current file content). This is a "nice to have" for long-uninspected high-value memories. **Not required for v1.** Lazy + organic covers the common case; sweeps are an optimization.

### Section precision (future)

If memories carry `sections` (line ranges, function names) inside `file_refs`, staleness can be scoped: only flag stale if the *referenced sections* changed, not any change to the file. Reduces false positives substantially. v1 can omit this; v2 adds it as Keeper extraction prompts get richer.

## First-person quest report

Written by the **executor** at quest close, before the deep-distillation Keeper tick. Structured but with prose fields where they help.

### Structure

```yaml
summary: "Wired up complexity classifier with Haiku primary + LM Studio fallback."
outcome: done | blocked | abandoned | partial
decisions:
  - decision: "Used parallel one-shot calls instead of streaming"
    rationale: "latency over completeness"
  - decision: "Linked classification to user message via classification_id FK"
    rationale: "after race bug surfaced ordering issue"
learned:
  # the agent's own suggestions to the Keeper
  - "Pi modelRegistry must be the single API key resolver — env fallback breaks subscriptions."
  - "Sidecar event ordering is not guaranteed; never assume FK targets exist on receive."
artifacts:
  - file: "src-tauri/src/classifier_config.rs"
    role: created
  - file: "sidecar/src/classifier.ts"
    role: created
  - file: "thoughts/impl/MON-82.md"
    role: documentation
open_threads:
  - "No consumer of classification yet — Slice 3 (Architect) is the first reader."
reflection: "Slice was small and tight, ordering bug was the only real surprise."
grade: A  # self-suggested, Keeper can override
```

### Why two layers (executor report + Keeper distillation)

- **The report is first-person.** Captain reads it as "what Igris did and why." It's the shadow's voice on its own work. Compelling UX moment when a quest closes.
- **The Keeper's distillation is third-person.** Atomic claims for the tree, optimized for cross-quest re-use.

The Keeper consumes the report PLUS the raw stream and produces tree memories. The report has already done some of the distillation work; the Keeper deepens, normalizes, and integrates.

Both layers are preserved. The report lives as a quest artifact (own row referenced from the quest); the tree memories live in L3.

## Compaction is observable

A core UX commitment: **the captain can watch distillation happen in real time.**

- Each Keeper tick emits a `compaction_tick` event to the quest's timeline.
- Payload includes: what was distilled (raw stream window), what was extracted (claims proposed, events emitted, working-memory updates), what was merged or superseded.
- The chat surface optionally surfaces a brief notice when meaningful memories are formed ("Igris just learned: <claim summary>").
- Memory Inspector shows the live state of the tree, highlighting recently-touched subtrees.

This isn't decoration. It's a **trust mechanism**. Captains who can see what their shadows are remembering trust the system; captains who can't, don't. Watching your shadow form a memory ("just learned: Postgres JSONB index needs explicit ops class") is one of the most compelling UX moments in the whole product.

## Captain can edit memories

A core trust commitment: **captain has full inspect/edit/archive control over memories.**

The **Memory Inspector** (Toolbox tool) provides:

- Browse the tree by topic / shadow / project / scope.
- View any memory's full content, provenance (source quest, source events, file_refs, anchor_sha), confidence, type, supersedes-chain history.
- **Edit** any memory's content, summary, type, position. Edits create new versions via the supersedes pattern; nothing destructive.
- **Archive** memories that are wrong or no longer useful. Archive is reversible.
- **Promote scope** — move a self-scoped memory to project scope, or project to captain-shared. Gated by a confirm step.
- **Trigger re-verification** of stale memories on demand.
- **Trigger Keeper distillation** manually ("checkpoint this conversation").

Mirrors the captain-as-supreme-authority model from the product vision. The Keeper does the bulk work; the captain has final say.

## Memory poisoning firewall

A hard architectural rule from the very first design conversation:

**The executor never writes memories directly. Only the Keeper writes.**

The executor can *propose* a memory (via `suggest_memory(claim, type, rationale)` tool, queued to the Keeper), but the Keeper decides whether to accept, modify, or reject. Same philosophy as the classifier (advisory, not authoritative): the executing shadow can't be its own historian — that's a conflict of interest and a known failure mode of self-narrating agents.

Why this matters: if any shadow can directly mutate its own memory, a single bad turn poisons the shadow permanently. The Keeper's separation gives us:

- **Curation discipline.** The Keeper applies the merge/supersede thresholds and the "what NOT to capture" exclusions. Executor can't bypass.
- **Single-writer guarantee.** Combined with the project serializer, this gives one writer per L3 row at any moment. No races.
- **Auditability.** Every memory has a Keeper run as provenance. Captains can replay the input that produced it.

The chat-shadow also can't write memories directly — same rule. Both organs propose; the Keeper decides.

## Implications for the data model

> Schema sketches are **illustrative**, not prescriptive. Real shapes get worked out in implementation tickets. The underlying storage stack (SQLite BLOB + HNSW sidecar via `instant-distance` + `bge-small-en-v1.5` via `ort`) is **validated by [MON-91](../spike/MON-91-storage.md)**; remaining openness is about row shape above that stack.

**`memories` table** — the L3 leaves.

```sql
-- Conceptually:
CREATE TABLE memories (
  id INTEGER PRIMARY KEY,
  shadow_id TEXT,                          -- nullable for project/shared scopes
  scope TEXT NOT NULL,                     -- self | project | captain | global
  project_id TEXT REFERENCES projects(id),
  parent_id INTEGER REFERENCES memories(id), -- tree edge
  layer TEXT NOT NULL DEFAULT 'leaf',      -- leaf | inner
  kind TEXT,                               -- fact | decision | constraint | ...
  title TEXT NOT NULL,
  summary TEXT NOT NULL,                   -- one sentence
  content TEXT,                            -- 1-3 sentences for leaf; paragraph for inner
  manual_override BOOLEAN DEFAULT FALSE,   -- inner nodes: captain edited
  source_quest_id TEXT REFERENCES quest_nodes(id),
  source_session_id TEXT,
  source_events TEXT,                      -- JSON array of event ids
  file_refs TEXT,                          -- JSON: [{path, anchor_sha, sections?}]
  embedding BLOB,                          -- vector of the summary field (NOT raw content/transcript); size per embedding model
  embedding_model_id TEXT NOT NULL,        -- "bge-small-en-v1.5", etc.
  supersedes_id INTEGER REFERENCES memories(id),
  archived_at TEXT,
  created_at TEXT NOT NULL,
  last_accessed_at TEXT,
  access_count INTEGER DEFAULT 0
);
```

**`memories_fts`** — FTS5 virtual table on title + summary + content.

**`memory_index`** — Rust-side HNSW sidecar file via `instant-distance`, keyed by memory id, rebuildable from `embedding` BLOBs. Validated by [MON-91](../spike/MON-91-storage.md).

**`memory_keeper_runs`** — provenance for every Keeper invocation.

```sql
CREATE TABLE memory_keeper_runs (
  id INTEGER PRIMARY KEY,
  shadow_id TEXT NOT NULL,
  trigger TEXT NOT NULL,                    -- continuous | quest_close | idle | captain | semantic
  started_at TEXT NOT NULL,
  completed_at TEXT,
  raw_stream_start_event_id INTEGER,
  raw_stream_end_event_id INTEGER,
  tree_slice_query TEXT,                    -- what was searched
  tokens_input INTEGER,
  tokens_output INTEGER,
  model_id TEXT NOT NULL,
  output_summary TEXT,                      -- what was produced (for compaction_tick events)
  outcome TEXT NOT NULL                     -- ok | failed | partial
);
```

**`quest_reports`** — first-person reports.

```sql
CREATE TABLE quest_reports (
  quest_id TEXT PRIMARY KEY REFERENCES quest_nodes(id),
  payload TEXT NOT NULL,                    -- JSON per the report structure
  written_by_shadow_id TEXT NOT NULL,
  written_at TEXT NOT NULL,
  keeper_run_id INTEGER REFERENCES memory_keeper_runs(id) -- when it was distilled
);
```

**Project serializer queue** — likely an in-process Rust queue per project, not a DB-backed queue; events emitted to `quest_events` for visibility but the queue itself is volatile.

## Open questions / current direction

- **Eval harness before scaling.** Before relying on the Keeper at scale, we need a small eval: seed 50 memories, write 20 queries with expected recalls, measure recall@5 and merge-quality. If recall is <70%, the retrieval stack needs work before context injection ships. **Non-negotiable** but lives in implementation, not design.
- **Embedding model.** Default to a small, locally-runnable model (`bge-small-en-v1.5` or `nomic-embed-text` via LM Studio, or shipped via ONNX Runtime). `embedding_model_id` per row enables migration when we change models.
- **Token-budget defaults.** 30k for hard trigger is a starting point. Should be calibrated by actual usage patterns. Configurable in `memory.toml`.
- **Section precision in `file_refs`.** v1 uses file paths only. v2 adds line ranges / function names for more precise stale flagging.
- **Background re-verification policy.** v1 is lazy + organic. Background sweeps stay configurable but optional.
- **Reranking the hybrid pool.** Hybrid retrieval typically returns K=20 candidates; the top-K=5 actually injected into context should be reranked by relevance to the current query. Cheapest reranker: a small Haiku/local-model call ("which of these are relevant to: X?"). Reranking is where most RAG stacks win or lose — worth real attention before context injection ships.
- **Embedding scope.** The Keeper embeds the *summary* field, not the raw content or transcript. Summaries are dense and intent-anchored; raw content is noisy and dilutes vector neighborhoods. This is implementation guidance, not architectural — but easy to get wrong, so worth being explicit.

## What this document does not cover

- The four-layer substrate model — see `substrate.md`.
- Thread types, surfaces, event taxonomy, tools — see `attention.md`.
- Conversation entry conditions, per-turn loops, environment, coordination — see `flows.md`.
- Concrete schema migrations — implementation tickets.
- Memory Inspector UI specifics — feature tickets.
- Prompt engineering for the Keeper — implementation work informed by this design.
- Eval harness construction — separate implementation spike.

## Working assumptions captured here

Listed for cross-doc reference. Treat as current direction, not final calls. Most likely candidates for revision: #5 (model choice — local vs cloud defaults will shift with experience), #8 (cosine thresholds — calibrated by eval), #11 (token-budget defaults).

1. **Compaction and memory formation are the same operation.** Distillation runs continuously and produces structured persistent artifacts.
2. **Three-layer record:** L0 raw stream (preserved), L1 first-person quest report (executor-written), L2 third-person tree memories (Keeper-written). Additive, not destructive.
3. **The Keeper is the only writer to L3.** Executor and chat-shadow can propose; Keeper decides. Memory poisoning firewall is hard architectural rule.
4. **One Keeper per shadow** for shadow-private subtrees. **Per-project serializer** for project subtree writes. Multi-shadow project memory is shared.
5. **Two-tier Keeper:** local model (Qwen 2.5 14B class) for routine compaction, cloud model (Haiku) optionally for quest-close deep distillation. Configurable in `memory.toml`. Default lean toward local for privacy and cost.
6. **Compaction triggers:** continuous (token-pressure, soft at ~25k / hard at 30k), semantic (quest close, captain checkpoint, major decision), idle (consolidation + optional re-verification sweeps).
7. **Atomic claims:** one falsifiable assertion per claim, self-contained. 1-sentence summary, 1-3 sentence content. Type as hint; position carries dominant semantic load.
8. **Merge/supersede/insert thresholds:** ≥0.9 merge, 0.8–0.9 supersede, 0.7–0.8 sibling, <0.7 new topic. Calibrated by eval.
9. **What NOT to capture:** chitchat, verbatim tool outputs, transient state, code/git-derivable facts, duplicates, sessile observations during routine work.
10. **Inner nodes are Keeper-generated**; manual overrides preserved with `manual_override` flag.
11. **Stale-flagging is lazy at load time, verification is organic via executor work.** Background sweeps optional, not required for v1. `file_refs` carry path + anchor_sha; sections precision is v2.
12. **Chat is a first-class Keeper input.** Dialogue produces memories alongside executor activity.
13. **First-person quest report written by executor at quest close.** Structured. Captain-readable as a record. Feeds Keeper as a high-quality input.
14. **Compaction is observable.** Captain watches distillation in real-time via `compaction_tick` events on the timeline; Memory Inspector shows live tree state. Trust mechanism, not decoration.
15. **Captain can inspect/edit/archive any memory.** Memory Inspector tool provides full control. Edits create new versions via supersedes; archive is reversible.
16. **Distillation is additive.** Older raw streams stay archived and queryable. If Keeper improves, re-distill old transcripts into a better tree without losing anything.
