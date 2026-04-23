# Quests — The Fractal Unit of Work

> Design plan for Monarch's Quest system: the universal artifact that unifies task decomposition, delegation tracking, execution history, memory seeding, and Time Travel indexing into a single fractal primitive. This is a multi-ticket initiative; each MVP slice below becomes its own `MON-{N}` ticket linked back to this document.

## Why this exists

Shadows running SOTA models (Opus-4.7, Sonnet-4.6) aren't at full power without explicit multi-agent or plan mode. Decomposition, coordination, and drift-handling are left to the Monarch to invoke manually. We need those capabilities baked into the default operating mode of the fleet.

Symptoms without this system:

- Users must manually enter "plan mode" or hand-craft delegation for non-trivial tasks.
- Agents either run everything in one bloated context or require the user to spawn workers by hand.
- No structural record of what a shadow did — transcripts aren't searchable, memory entries are ad-hoc.
- Side-tracking is invisible until the operator notices in the transcript.
- No native way to explore N approaches in parallel and pick a winner.

## The Quest

A **Quest** is the atomic artifact of work in Monarch. A quest may be trivial (rename a variable) or vast (ship v2 of the product). Quests contain sub-quests to arbitrary depth. Every significant action a shadow takes belongs to some quest.

Quests unify what would otherwise be separate systems:

- **Plan** — the quest tree is the plan
- **Delegation graph** — sub-quests with assignees are delegation edges
- **Execution log** — messages and tool calls carry `quest_id`, filterable per-quest
- **Memory seed** — completed quests distill into memory entries
- **Time Travel index** — the quest tree is the primary scrubbing interface

### Fractal structure

Quests are scale-invariant. A one-line fix is a quest. An epic multi-month initiative is a quest. Quests nest with no depth limit. The shadow hierarchy mirrors the quest hierarchy — top-level quests go to orchestrators, sub-quests go to leads, atomic quests go to workers.

### Lifecycle

```
pending → in_progress → claimed_done ─┬─ verified → done
                                      ├─ disputed   (stays until resolved)
                                      └─ ambiguous → judge → done | disputed
```

Terminal states: `done`, `abandoned` (explicit termination without completion), `superseded` (a fork winner replaces this node).

### Grade

Every quest has a grade mirroring the shadow grade system:

| Grade | Scope                                       | Base EXP |
|-------|---------------------------------------------|----------|
| E     | Trivial atomic change (typo, one-liner)     | 1        |
| D     | Small, single function                      | 3        |
| C     | Routine feature, a few sub-quests           | 10       |
| B     | Meaningful, crosses a module                | 30       |
| A     | Architectural, deep tree or multi-fork      | 100      |
| S     | Project-scale initiative (this one is S)    | 500      |

The Architect assigns the initial grade at decomposition. The Steward may re-grade as scope evolves. Grade drives EXP, routing hints, and visual evolution of the assigned shadow.

## Architecture

Five roles, one shared artifact (the quest tree), one event stream.

```
        Monarch (you)
             │
      ┌──────┼──────┐
      ▼             ▼
 Architect     Orchestrator ── spawns ── Workers
 (one-shot,     (director,                (per quest,
  heavy,         reads plan,               each in its
  rare)          makes assignments)        own worktree)
                      │
                      ▼
                  Steward ── observes events, edits tree
                  (always-on, local, cheap)
                      │  (escalates ambiguity)
                      ▼
                  Judge (on-demand, cloud, rare)

  [Classifier — sidecar interceptor, not a shadow —
   fires on every user turn to gate everything above]
```

### Classifier

- **Where:** sidecar interceptor, runs before the message reaches the Pi session.
- **Model:** local 3-8B (Qwen3 4B or Llama 3.2 3B). Haiku fallback during calibration.
- **Input:** user prompt + last 1-2 turns of context.
- **Output:** `{ complexity: "chitchat" | "simple" | "decomposable" | "delegate", confidence, rationale }`.
- **Cost:** negligible. Runs on every user turn.
- **Purpose:** gate the expensive Architect. Only escalate when warranted.

Calibration strategy: ship with Haiku as baseline + local path from day one. Log both outputs for a week; flip default to local when agreement exceeds ~92%.

Bias the prompt toward escalation on ambiguity — a misclassified simple prompt is cheaper than a missed task.

### Architect

- **Where:** on-demand shadow, spawned when classifier returns `decomposable` or `delegate`.
- **Model:** Opus / Sonnet for hard tasks; configurable down to local 14B for routine.
- **Tools:** `read`, `grep`, `find`, `web_fetch`, `web_search`, `agent-browser` skill.
- **Input:** user prompt + project map + current conversation state.
- **Output:** a quest tree with per-node:
  - `title`, `description`
  - `exec_hint`: `in_context` | `delegate` | `explore_n=K`
  - `assignee_hint`: shadow role or grade target
  - `grade` estimate
  - Dependencies between sibling nodes
- **Cost:** expensive, but rare. Budget guard in system prompt: *browse only when the task specifically references an external library, API, or spec.*

### Orchestrator

Not a new role — reuses the existing top-level shadow (the one the Monarch is talking to).

Responsibilities gained:

- Read the Architect's tree each turn via context injection.
- Decide in-context vs delegate per node (the Architect's `exec_hint` is advisory).
- Spawn worker shadows with worktrees per `delegate` / `explore` quest.
- Coordinate fork execution; collect fork results; present to Monarch for selection.
- Signal completion via the `claim_complete(quest_id, evidence)` tool.
- Call `request_replan(quest_id, reason)` when reality no longer fits the plan.

**Tree authority:** the orchestrator does NOT write to the tree directly. Its one plan-aware tool is `claim_complete`. Everything else flows through the Steward.

### Steward

- **Where:** always-on observer, one (shared) per active orchestrator for MVP; may split to per-orchestrator later.
- **Model:** local 7-14B, cheap enough to keep hot.
- **Triggers:** sidecar events — `tool_call_complete`, `turn_end`, `user_message` mid-work, `tool_error`, `timeout`, heartbeat. Debounced ~1/sec.
- **Responsibilities:**
  - Transition quest status as evidence accumulates.
  - Add sub-quests when discoveries surface new work.
  - Flag **drift** when the orchestrator acts outside the tree scope.
  - Promote `claimed_done` → `verified` when evidence matches; mark `disputed` or `ambiguous` when it doesn't.
  - Adjust grade if scope evolves materially.
- **Tools:** none. Reads events, writes DB.

### Judge

- **Where:** on-demand cloud call, not a persistent shadow.
- **Model:** Opus / Sonnet.
- **Trigger:** Steward flags `ambiguous` (expected <5% of completions).
- **Tools:** read-only code access + the quest's event log.
- **Output:** verdict — verified, disputed, or needs-more-work — with written reasoning stored in `quest_events`.

If disputes prove frequent later, promote Judge to a persistent shadow.

## Data model

### `quest_nodes`

```sql
CREATE TABLE quest_nodes (
  id                   TEXT PRIMARY KEY,
  root_id              TEXT NOT NULL REFERENCES quest_nodes(id),
  parent_id            TEXT REFERENCES quest_nodes(id),
  title                TEXT NOT NULL,
  description          TEXT,
  status               TEXT NOT NULL CHECK (status IN (
                         'pending','in_progress','claimed_done',
                         'verified','disputed','ambiguous',
                         'done','abandoned','superseded'
                       )),
  grade                TEXT CHECK (grade IN ('E','D','C','B','A','S')),
  exec_hint            TEXT CHECK (exec_hint IN ('in_context','delegate','explore')),
  explore_fork_count   INTEGER,             -- non-null when exec_hint='explore'
  assignee_shadow_id   TEXT REFERENCES agents(id),
  worktree_path        TEXT,
  branch_name          TEXT,
  base_branch          TEXT,
  branched_from_id     TEXT REFERENCES quest_nodes(id),  -- fork lineage
  superseded_by_id     TEXT REFERENCES quest_nodes(id),  -- fork winner points back
  created_by           TEXT CHECK (created_by IN ('architect','steward','orchestrator','monarch')),
  created_at           INTEGER NOT NULL,
  started_at           INTEGER,
  completed_at         INTEGER,
  abandoned_at         INTEGER,
  estimated_tokens     INTEGER,
  actual_tokens        INTEGER,
  estimated_duration_ms INTEGER,
  actual_duration_ms    INTEGER,
  summary              TEXT                 -- Keeper-distilled, populated on done
);
```

### `quest_events` (audit trail)

```sql
CREATE TABLE quest_events (
  id           TEXT PRIMARY KEY,
  quest_id     TEXT NOT NULL REFERENCES quest_nodes(id),
  event_type   TEXT NOT NULL,  -- status_change, grade_change, drift_flagged,
                               -- dispute_opened, judge_verdict, scope_expanded,
                               -- fork_spawned, fork_winner_selected
  actor        TEXT,           -- role or shadow_id that produced this
  payload_json TEXT,
  created_at   INTEGER NOT NULL
);
```

### `classifications`

```sql
CREATE TABLE classifications (
  id          TEXT PRIMARY KEY,
  message_id  TEXT NOT NULL REFERENCES messages(id),
  agent_id    TEXT NOT NULL,
  complexity  TEXT NOT NULL,
  confidence  REAL,
  rationale   TEXT,
  model       TEXT,
  tokens_in   INTEGER,
  tokens_out  INTEGER,
  latency_ms  INTEGER,
  created_at  INTEGER NOT NULL
);
```

### Extensions to existing tables

- `messages`: add `quest_id TEXT REFERENCES quest_nodes(id)` — nullable (chitchat and meta messages have null). All tool calls inherit `quest_id` from their parent message.
- `agents`: add `total_exp INTEGER NOT NULL DEFAULT 0`, `grade_completions_json TEXT` (per-grade counts for specialization scoring), `current_quest_id TEXT REFERENCES quest_nodes(id)`.

### Orthogonality to sessions

Quests are **orthogonal to sessions**. A quest can span multiple sessions (continuation chain). A session can span multiple quests (Monarch switches context). The aggregation key for "what happened on this quest" is `quest_id`, not `session_id`. Do not couple the two.

## Event flow

### New sidecar events

- `agent-classification-{agentId}` — classifier output per user turn
- `quest-created-{questId}`, `quest-updated-{questId}` — tree state deltas
- `quest-drift-{questId}` — Steward flagged drift
- `quest-dispute-{questId}` — completion dispute opened
- `quest-judged-{questId}` — judge verdict written
- `exp-awarded-{agentId}` — completion EXP delta

### Turn-level flow

```
1. User sends message → Rust → sidecar.
2. Sidecar intercepts, calls Classifier (local).
3. Classification event emitted → Rust → frontend pill.
4. Branching:
   - chitchat | simple : pass through to Pi session unchanged.
   - decomposable      : invoke Architect (background), augment next turn with new quest tree.
   - delegate          : same as decomposable + orchestrator is nudged to spawn workers.
5. Architect produces tree, writes quest_nodes, emits quest-created.
6. Orchestrator's next turn sees the tree via context injection (below).
7. Orchestrator acts or spawns workers per exec_hints; worktrees created on delegate/explore.
8. As shadows act, Steward subscribes to their event streams and edits the tree.
9. On perceived completion: orchestrator calls claim_complete → Steward verifies → done | dispute | judge.
10. On done: Memory Keeper fires distillation; EXP awarded to assignee; frontend animates.
```

### Context injection

At the start of each orchestrator/worker turn, prepend to the system prompt:

```
## Current Quest: {root.title} (Grade {root.grade})

[1] Understand auth flow               [done]
[2] Find token leak                    [in_progress ← you]
  [2.1] Map token lifecycle            [done]
  [2.2] Identify leak site             [in_progress]
[3] Propose fix                        [pending]
[4] Write regression test              [pending]

You are on: [2.2] Identify leak site.
Status: in_progress. Exec hint: in_context.

If your next action doesn't fit this quest, say so in your response —
the Steward will reconcile. Call claim_complete(2.2) when finished.
Call request_replan(2.2, reason) if the plan no longer fits reality.
```

## Shadow Oath additions

Extend `sidecar/src/shadow-oath.ts` with a **Quest Protocol** section:

```
## Quest Protocol

You operate within Quests — the fractal artifacts of your work.

- Every significant action belongs to some quest.
- The current quest tree is injected at the start of each turn; read it before acting.
- Mark completion via `claim_complete(quest_id, evidence)` — do not self-declare
  completion in narration alone. Completion is a discrete state transition and
  deserves an explicit call.
- If you disagree with the tree, say so plainly in your response; the Steward
  will reconcile.
- If the plan no longer fits reality, call `request_replan(quest_id, reason)`.
- Forks and branches are allies — if you see multiple valid approaches, name them
  in your response; the Architect may spawn a fork.
- Higher-grade quests earn more EXP on completion. Treat every quest as a
  chance to grow.
```

## UI

### Quest Timeline (primary view)

- Horizontal axis: time (`started_at`).
- Vertical axis: tree depth or assignee lane.
- Nodes render as pills with:
  - Color by status
  - Grade badge (E-S)
  - Assignee avatar (Rive state-machine animated per VISION)
  - Progress indicator while `in_progress`
- Branches render as diverging lines; `superseded` nodes fade but remain visible.
- **Click node → Time Travel** jumps to that quest's start.
- Drag a node onto a shadow → reassign.

### Complexity pill

- Small pill near every user message showing classifier output (complexity + confidence).
- Click to expand rationale, model, tokens, latency.
- Monarch can override (promote `chitchat` → `decomposable`).

### Drift & dispute badges

- On quest: red dot = drift, yellow = dispute, purple = judge-adjudicated.
- Hover: summary. Click: full event log for that quest.

### Quest detail panel

- Full metadata + estimated vs actual
- Event log (`quest_events`)
- Associated messages / tool calls (filtered by `quest_id`)
- Sub-quest tree
- Re-grade button (Monarch override)
- "Branch from here" button (Time Travel fork)

### War Room integration

Avatars in the War Room (VISION) show each shadow's current quest grade and status as part of their state machine.

## Memory integration

### Distillation on completion

When a quest transitions to `done` (or `abandoned` with learning value), Memory Keeper fires:

1. Read full quest transcript — messages + tool calls filtered by `quest_id`.
2. Summarize into a memory entry:
   - Title: quest title
   - Summary: what was done, what was learned, any gotchas
   - Source: `quest_id`
   - Grade: inherited
   - Assignee: shadow_id
3. Write to `memory_entries` with layer = `warm`.
4. Populate `quest_nodes.summary` with the distilled text.

### Layered recall via quests

- **Core** — shadow identity (unchanged from VISION)
- **Hot** — current root quest + immediate children
- **Warm** — recently completed quests (this shadow, last N days or last K quests)
- **Cold** — all completed quests, searchable by title/description/summary

When a shadow starts a turn, Monarch loads core + hot + relevant warm. Relevance is computed from similarity between current quest description and past quest summaries.

### Specialization score

Derived on-demand from `agents.grade_completions_json` + `quest_nodes` kind/domain tags. A shadow with 50 completed auth-related quests *is* the auth expert — no labels required.

## Time Travel integration

The quest tree **is** the primary rewind index.

- **Quest timeline** is the scrubbable interface. Each quest defines a time range and an associated event stream.
- **Click quest → jump.** Agent state reconstructed from messages / tool calls up to that quest's start (or any point within).
- **Branch from Quest.** Right-click any completed quest → "Branch from here". Creates new quest with `branched_from_id`, new worktree from the branch's state at that point, new shadow (or same shadow continues in isolated fork).
- **Losing forks preserved.** `abandoned` / `superseded` quests keep their worktrees (archived, not deleted) so they stay rewindable. Their learnings aren't lost.

## EXP & grading

### Grade assignment

Architect heuristic at decomposition:

```
exec_hint == in_context && sub_quests == 0        → E
exec_hint == in_context && sub_quests ≤ 2         → D
exec_hint == delegate   && sub_quests ≤ 5         → C
exec_hint == delegate   && sub_quests ≤ 15        → B
exec_hint == explore    || sub_quests > 15        → A
cross-project || multi-week                       → S
```

Steward can re-grade on scope expansion (`scope_expanded` event).

### Base EXP

`E=1, D=3, C=10, B=30, A=100, S=500`

### Modifiers (multiplicative)

| Condition                                    | Multiplier |
|----------------------------------------------|------------|
| Clean completion, no disputes                | 1.0        |
| Disputes resolved in shadow's favor          | 1.2        |
| Judge called, shadow was right               | 1.5        |
| Judge called, shadow was wrong               | 0.5        |
| Abandoned                                    | 0.0        |
| Superseded by fork winner                    | 0.3        |
| Ahead of estimated cost                      | 1.2        |
| Significant drift logged by Steward          | 0.8        |

### Delegation credit

A parent shadow earns **20%** of the summed EXP of its directly-delegated children (management credit). Tunable from telemetry.

### Forks

- Winner: full EXP.
- Losers: 30% (exploration still has value).

### Visual progression (ties to VISION)

EXP thresholds unlock visual tiers on the shadow avatar:

| Total EXP  | Tier |
|------------|------|
| 0 – 100    | Base silhouette |
| 100 – 500  | Minor glow enhancement |
| 500 – 2000 | Particle effects |
| 2000 – 10000 | Grade promotion eligibility |
| 10000+     | Named shadow candidacy |

### Grade-based routing

High-grade shadows get routed harder quests by default. Positive feedback loop — an A-rank shadow gets A-rank work and keeps leveling. Monarch can override.

## MVP slicing

Nine slices, each shippable as its own PR + Linear ticket. Order respects dependencies.

### Slice 1 — Classifier & complexity pill
Sidecar interceptor, local model integration (reuses MON-7 LM Studio work), Haiku fallback, `classifications` table, event channel, frontend complexity pill with override.
*No automatic decomposition yet.*
**Est:** 3–5 days.

### Slice 2 — Quest schema & read-only UI
`quest_nodes` + `quest_events` tables, CRUD Rust commands, `messages.quest_id` FK, basic timeline view (single lane, no branches yet), manual quest creation from UI.
*Value on its own: quest-as-task-tracker.*
**Est:** 5–7 days.

### Slice 3 — Architect (one-shot decomposer)
Architect shadow type, tool bundle (read/grep/find/web/browser), invocation on `decomposable` / `delegate` classifications, tree emission, context injection into orchestrator's next turn.
*Static tree — Steward comes later.*
**Est:** 5–7 days.

### Slice 4 — Steward & disputes
Steward shadow (always-on, local), event subscription + debounce, tree update logic, drift detection, `claim_complete` tool, dispute badge UI, dispute resolution flow.
**Est:** 5–7 days.

### Slice 5 — Worktrees & forks
Worktree lifecycle tied to `delegate`/`explore` quests, N-way fork spawning, worktree archive on abandon, branches rendered in timeline, Monarch fork-winner selection UI.
**Est:** 7–10 days (coordinates with existing Git & Worktree Integration project).

### Slice 6 — Full timeline visualization
Timeline with branches, status/grade/avatar per node, click-to-TimeTravel integration, drag-to-reassign, detail panel.
**Est:** 5–7 days.

### Slice 7 — EXP & grading
Grade assignment logic, modifier application, `agents.total_exp` + `grade_completions_json` extension, avatar tier unlocks, `exp-awarded` event wiring, grade distribution analytics.
**Est:** 3–5 days.

### Slice 8 — Memory Keeper integration
Distillation trigger on `done`, summary writeback into `quest_nodes.summary`, `memory_entries` warm layer population, quest-similarity retrieval, specialization score derivation.
**Est:** 5–7 days (coordinates with Memory & Context Tools project).

### Slice 9 — Judge escalation
Judge shadow (on-demand cloud call), invocation on `ambiguous`, verdict writeback, judge-adjudicated badge. Promote to persistent if dispute rate demands.
**Est:** 3–5 days.

### Deferred

- `request_replan` tool + bounded replan loop
- Linear tool access for Architect
- Auto-promotion of Judge to persistent shadow
- Quest-similarity routing (route quests to most-specialized shadow)
- Monarch approval flows on every fork winner (currently manual)

## Open questions

1. **Classifier firing granularity.** Every user turn (MVP simple) vs. only "new topic" turns (cheaper, needs embedding distance heuristic). Start every turn; optimize later.
2. **Architect context for continuations.** On session continuation, should Architect see the prior root quest's summary? Lean yes — reuses prior plan structure.
3. **Steward sharing.** Singleton across orchestrators vs per-orchestrator. Start singleton; split if contention surfaces.
4. **Delegation EXP formula.** 20% parent credit is a guess. Tune from telemetry after slice 7.
5. **Grade inflation.** If grade distribution drifts upward over time, need recalibration. Monitor in analytics.
6. **Fork winner selection.** Always Monarch-picked, or can orchestrator auto-pick on clear dominance? Start: Monarch always.
7. **Quest assignment via drag-drop** vs only via orchestrator. UX question for slice 6.

## Success metrics

- **Classifier accuracy** — agreement with Haiku baseline ≥ 92% after local swap-in.
- **Architect usefulness** — % of orchestrator turns that reference the tree (via `claim_complete` calls or natural-language references).
- **Steward latency** — < 2s from event to tree update (p95).
- **Drift recovery rate** — % of drift flags resulting in tree correction vs Monarch intervention.
- **EXP distribution** — healthy spread across grades, not bunched at C.
- **Quest completion rate** — % of started quests reaching `done` (not `abandoned`).
- **Memory hit rate** — % of surfaced warm memories actually used by the shadow in its response.
- **Dispute rate** — < 5% of completions go to judge.

## Relationship to other projects

- **Agent Orchestration & Hierarchy** — Quests are the artifacts the hierarchy operates over. Tight interlock on `delegate` / `explore` exec hints.
- **Memory & Context Tools** — Quest distillation is the primary memory-population mechanism. Slice 8 is joint work.
- **Git & Worktree Integration** — Forks and delegation spawn worktrees. Slice 5 coordinates.
- **Shadow Avatars & Visual Identity** — EXP and grade drive avatar evolution. Slice 7 publishes the signals avatars consume.
- **Audit Trail & Observability** — `quest_events` becomes a primary observability stream.
- **Agent Tools & Capabilities** — `claim_complete`, `request_replan` are new Monarch-native tools.
- **Agent Loop** — Classifier interception is a hook in the loop; Steward event subscription lives on the loop's output stream.

## Out of scope

- Cross-monarch shared quest libraries (quest templates/marketplace)
- Quest predictions ("this quest resembles quest X, reuse its plan")
- Automatic merge of fork winners (currently manual)
- Natural language quest authoring from the user side (use the Architect instead)
- Retroactive quest backfill for pre-Quest-system sessions
