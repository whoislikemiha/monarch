# Concurrent Attention

> **Status:** Idea document — exploratory, not a spec. Captures the thinking from an in-progress design conversation about how multiple attention threads share a shadow's substrate, and how the captain experiences that as one coherent entity. **Tech choices are tentative**; treat the conceptual model as load-bearing and the schema/API sketches as illustrative.
>
> **Sibling docs:** `substrate.md` (the four-layer self that threads attach to), `distillation.md` (the Keeper, compaction, atomic claims).

## Premise

A shadow has one self and many ways of using it. Until now, "agent" has meant a single linear conversation: captain types, agent works, agent replies, captain types. That model breaks when:

- A long task means the captain is locked out for the duration.
- Asking "what are you doing?" requires re-reading or re-summarizing.
- Wanting to redirect mid-execution requires interrupting cleanly.
- Wanting two parallel attempts means cloning the agent and losing shared knowledge.

This document defines the model that fixes all four: **one shadow, two organs**. The executor is the *hands* that act on the world. The chat-shadow is the *mouth* that speaks with the captain. Both are the same shadow — same identity, same memory, same voice. They share the cognitive substrate (`substrate.md`) and coordinate through the **quest tree**, which is the temporal spine of all activity.

To the captain, this looks like one entity. The dual-thread machinery is implementation detail — never narrated, never exposed as ritual.

## The captain experience comes first

Before any architecture, the UX principle: **the captain talks to one shadow.** No "switching between chat-Igris and executor-Igris." Same name, same memory, same way of speaking. When the captain says *"commit and push,"* the shadow commits and pushes. The fact that under the hood a chat-thread routed the intent and an executor-thread did the work is invisible.

```
Captain: commit and push
Igris:   on it
[timeline: Igris: commit and push staged changes
   ├ git add -A
   ├ git commit -m "feat(mon-82): wire classifier"
   └ git push origin mihabubnjevic/mon-82-classifier-...]
Igris:   pushed.
```

Not:
```
Captain: commit and push
Igris:   Creating subtask "commit and push"...
Igris:   Subtask queued. Notifying executor...
Igris:   Executor picking up...
...
```

The routing happens; it's just not narrated. This sets a quality bar on chat→executor handoff: **fast enough to feel seamless, coherent enough that what the shadow *says* and what the executor *does* always agree.**

## The quest tree is the spine

All concurrent activity hangs off the quest tree. A quest is not just a TODO item — it is **the temporal anchor for everything that happened during a piece of work**: actions, dialogue, decisions, investigations, blockers, forks, learnings.

When the captain (or shadow, or Keeper) asks *"what was going on with this task?"*, the answer is *"read this quest's event log."* Not "scroll the chat session, then scroll the execution log, then cross-reference." One spine, many event kinds, one chronological view.

This makes the quest tree the most load-bearing data structure in the system. It carries:

- **Identity:** title, description, scope, current direction.
- **Status:** active, paused, blocked, done, abandoned, forked.
- **Tree position:** parent, children, fork siblings.
- **Rationale:** *why* this quest exists, *why* its scope is what it is, *why* its direction shifted.
- **Event log:** the rich typed timeline of everything that happened.
- **Fork metadata:** worktree path if forked, sibling quests, merge resolution.
- **Outcome:** grade, summary, learnings extracted.

The MON-83 quest tree (Slice 2) is a starting point, but the model in this doc requires substantially richer event types and a real write API (chat-shadow needs to mutate quests in many ways, see "Routing captain input" below). The quest tree's API growth is the largest implementation surface implied by this doc.

## Two surfaces on the quest

The captain has two views into a quest's activity, both filtered from the same underlying event log:

### Chat surface

Clean, dialogue-only. Only events that are *meant for conversation* appear here:

- Captain's own messages.
- Shadow's spoken responses (dialogue turns, not internal reasoning).
- Observations the shadow chooses to surface ("done", "blocked, need input", "found something interesting you should know").
- Question/answer pairs between executor and captain.

What is **not** on the chat surface: tool calls, internal reasoning, low-level status pings, automated event noise. The chat reads like a conversation with a colleague who is also doing work, not like a console log.

### Execution timeline

Chronological list of **coherent actions** the executor takes. Each item shows a one-line declared intent ("Understand the failing authentication test"). Underneath, collapsed by default, are the actual tool calls and explicit decisions that fulfilled the intent.

The captain reading the timeline scrolls at the *intent level* — they see the work narrative, not the tool-call transcript. They can drill into any item to see what happened underneath if they care.

Plan-level changes (scope, direction, subtask added, fork) also appear on the timeline as first-class entries — when chat-shadow modifies the quest based on conversation, that modification is part of the work record.

The timeline is **actual history**, not the plan. Durable execution plans are a separate layer: the plan says what the shadow currently intends to do; the timeline says what actually happened. A future `plan_item_id` may link actions to plan items, but coherent actions are not themselves plan items.

### Surface routing

Each event kind has a default surface:

| Event kind | Default surface | Notes |
|------------|-----------------|-------|
| `coherent_action` | timeline | Always visible at top level |
| `tool_call` | timeline (collapsed) | Child of an action |
| `executor_decision` | timeline (collapsed) | Explicit decision; child of current action when possible |
| `chat_message` (captain → shadow) | chat | |
| `chat_message` (shadow → captain) | chat | The shadow's *spoken* turns |
| `observation` (shadow surfacing something) | chat | "I noticed the build is broken" |
| `pending_action` | both | Timeline shows it pending; chat surfaces the prompt |
| `pending_action_modified`, `_approved`, `_rejected` | timeline | The action's lifecycle |
| `blocker` | both | |
| `blocker_resolved` | timeline | |
| `question` (executor → captain) | chat | Dialogue-shaped |
| `answer` (captain → executor, via chat-shadow) | chat | |
| `investigation` (chat-shadow finding) | timeline | A chat-shadow-authored action |
| `scope_change`, `direction_change`, `subtask_added` | timeline | Quest changes are work |
| `note` | timeline | Free-form context |
| `forked`, `merged` | timeline | Branch points |
| `executor_action_outcome` | timeline | One-line closure of an action |
| `paused_by_chat`, `resumed_by_chat`, `stopped_by_chat` | timeline | Control-plane |

Surface is derived from kind by default; an event can carry an explicit `surface_override` if needed.

The chat surface is therefore *sparse and curated*; the timeline is *full but action-shaped*. Together they give the captain everything without overwhelming either view.

## Coherent atomic actions

The unit of executor narration. Defined as: **one declared intent that bundles several tool calls and observations needed to fulfill it.**

Granularity examples:

- *"Understand the failing authentication test"* — read the failing test, inspect the related handler, identify the expected behavior.
- *"Patch the session-expiry behavior"* — edit the handler and helper it calls.
- *"Verify the focused fix"* — run the relevant test or package check.
- *"Trace where the payment status is normalized"* — search for status mapping, read the service and serializer.
- *"Reproduce the reported build failure"* — run the failing command and inspect the first actionable error.

Counterexamples: *"Read file"*, *"Run grep"*, *"Use bash"*, *"Fix bug"*, *"Keep going"*, *"Implement feature"*, *"Check stuff"*. These are either tool-level noise or too broad to produce a useful outcome.

Mechanically, the executor model is prompted to:

1. **At each chunk boundary**, call `set_current_action(intent, previous_outcome?)`. When switching from one action to the next, `previous_outcome` closes the prior action and the new `intent` starts the next one.
2. **Execute** the tool calls that fulfill it. Tool calls become children of the active action, nested by `parent_event_id`.
3. **When done without starting a next action**, call `complete_action(outcome)` with a one-line result.
4. **When a significant approach decision happens**, optionally call `record_decision(decision, rationale?)`. Decisions are sparse and explicit, not a dump of raw model thinking.

If the executor starts a new action without closing the previous one, Rust auto-closes the previous action and marks it as such. If the executor ends/aborts while an action is open, Rust auto-closes it at the lifecycle boundary. Auto-close is a resilience mechanism, not the desired rhythm.

Action granularity is prompt guidance, not backend enforcement. A good action may include several tool calls. If one action grows beyond roughly 5-8 tool calls, the executor should consider whether the intent has become too broad and whether it should close with an outcome and start a sharper action.

The captain reading the timeline gets a narrative of the work — like reading a commit log instead of a diff. Drill in for the diff when needed.

### Three self-reporting cadences

This is the executor's *per-action* narration. The shadow reports about itself at three timescales:

1. **Per coherent action** — declare intent, execute, close with one-line outcome. Drives the timeline.
2. **Per quest** — first-person quest report at quest end. Structured: summary, outcome, decisions, learned, artifacts, open threads, reflection. Drives the quest summary view and feeds the Keeper. (Detail in `distillation.md`.)
3. **Continuous** — Keeper compaction ticks. Distills raw stream + reports into events/memories/artifacts. Drives long-term memory.

All three are first-person. All three produce structured records. All three serve different consumers. The executor's prompt has to teach this rhythm: work in coherent chunks, record the chunk, finish with a closure.

## Event taxonomy

The rich set of typed events that can appear on a quest. Each event has: `id`, `quest_id`, `parent_event_id` (nullable, for nesting), `kind`, `payload` (JSON), `author` (executor / chat-shadow / captain / keeper / system), `actor` (concrete agent/captain/process id or name), `created_at`, `surface_override` (nullable), and `payload_schema_version`.

**Executor activity (timeline):**
- `coherent_action` — declared intent, parent of nested tool calls.
- `tool_call` — single tool invocation with args + result.
- `executor_action_outcome` — closure of a coherent action.
- `executor_decision` — explicit decision the executor made, with optional rationale.

Raw model thinking is not persisted as quest timeline content in v1. If rationale matters, the shadow records an explicit `executor_decision`; otherwise the action intent, tool children, and outcome are the narrative.

**Captain↔shadow dialogue (chat):**
- `chat_message` — captain or shadow turn.
- `observation` — shadow surfacing something to the captain proactively.

**Permission and pending actions:**
- `pending_action` — executor proposed an action, awaiting decision.
- `pending_action_modified` — chat-shadow patched the proposal.
- `pending_action_approved` — chat-shadow released the executor.
- `pending_action_rejected` — chat-shadow blocked it; quest may need redirect.

**Blockers and questions (bidirectional):**
- `blocker` — executor self-pause with reason and hypotheses.
- `blocker_resolved` — unblocked, with what unblocked it.
- `question` — executor needs information from captain.
- `answer` — chat-shadow's typed answer back.

**Plan manipulation (chat-shadow's verbs):**
- `scope_change` — addition or removal with rationale.
- `direction_change` — new approach with rationale.
- `subtask_added` — new sub-quest with rationale.
- `note` — free-form context attached to the quest.
- `investigation` — chat-shadow's findings from reading/searching.

**Control plane:**
- `paused_by_chat` — chat-shadow paused executor with reason.
- `resumed_by_chat` — chat-shadow resumed.
- `stopped_by_chat` — chat-shadow hard-stopped (also via captain UI button).

**Structural:**
- `forked` — parallel-attempt branches created.
- `merged` — captain selected a winner; loser archived.
- `archived` — quest closed without merge.

**Compaction:**
- `compaction_tick` — Keeper ran distillation; payload includes summary of what was extracted.

This list will grow. The model is: **events are the universal connective tissue between threads, surfaces, and the substrate.** When a new kind of thing happens, give it an event type with a clear payload schema and a default surface.

## Thread types

A shadow has at most one of each:

### Executor thread (the hands)

The thread that acts on the world. Has the full tool set. Reads from the substrate, writes pending actions and tool calls and outcomes back to the quest's event log. Mutates L2 working memory in real time (`current_action`, `recent_actions`).

One executor per active quest. When the captain isn't in dialogue with the shadow, this is the only thread running.

### Chat-shadow thread (the mouth)

The dialogue thread. Same shadow — same L1 identity, same L2 working memory (read-only on the executor's working state, write on its own dialogue context), same L3 tree. Reads to understand, writes to *direct* (modify the quest) and to *speak* (chat messages, observations, answers).

Spawns when the captain opens chat. Persists across the conversation. Compactable like any other context.

### Observer threads (the eyes)

Future / optional. Read-only attention threads that watch for specific patterns and surface notifications. Examples: a watchdog observer that flags risky operations, a steward observer that detects relevance drift in retrieval. Out of scope for v1; mentioned for forward-compat.

### Same shadow, multiple sessions

Each thread is implemented as its own Pi session under the hood, but the *system prompt* (built from L1 captain + L1 shadow) makes both sessions express the same identity. The executor and chat-shadow are not different agents — they are different attention contexts of the same agent, reading the same substrate.

This is what makes the captain's experience coherent: ask chat-shadow *"what are you doing?"* and the answer matches what executor is actually doing, because both threads see the same L2 working memory and the same quest event log.

## Tool taxonomy by thread

The executor and chat-shadow have different capabilities, by design.

### Executor — full world-mutation set

- `read`, `write`, `edit`, `bash`, `grep`, `find`, `ls`
- `memory_search`, `recall_actions`
- `set_current_action(intent, previous_outcome?)` — records the current coherent action. If another action is already active, `previous_outcome` closes it before opening the new one.
- `complete_action(outcome)` — closes the current coherent action when the executor is done without immediately starting another action.
- `record_decision(decision, rationale?)` — records a sparse, explicit approach/architecture/safety/scope decision. Child of current action when possible.
- `update_execution_plan(actions)` — manages durable quest plan items once P4b exists. Add, reorder, skip, mark active/done. Before P4b, the executor uses only coherent action narration, not a durable plan table.
- `propose_pending_action(action, context, reason)` — **rare**: only for actions that genuinely need captain eyes (destructive, irreversible, out-of-scope, or against a captain-set permission gate). See "When pending actions apply" below.
- `request_information(question, reason)` — emits a `question` event.
- `report_blocker(reason, hypotheses)` — emits a `blocker` event.
- `complete_quest(report)` — emits the first-person quest report and transitions status.

The executor is the only thread that mutates the codebase, runs destructive bash, or otherwise changes external state.

#### When pending actions apply

Most executor actions just happen. The chat-shadow approving every write would be meaningless friction — same model, same memory, same reasoning, it would approve 99% of what the executor would do anyway. The cycle is bureaucracy.

`propose_pending_action` is reserved for the genuinely escalation-worthy cases:

- **Destructive or irreversible** — `rm -rf`, force push, drop table, hitting prod systems.
- **Out of declared scope** — action would expand the quest beyond what's been agreed.
- **Captain-set permission gate** — per quest, per file pattern, per action type, captain may have configured "always confirm this kind of thing."
- **Conflict-prone** — action conflicts with something the executor already did and confirmation reduces risk.

Default permission posture is **trust within scope**. Most quests have zero pending actions. The mechanism exists for the cases where it matters, not as a routine ritual.

### Chat-shadow — read, direct, control

**Observational (free use):**
- `read`, `grep`, `find`, `ls`
- Read-only bash: `git status`, `git log`, `git diff`, `cargo check`, `cargo test` — idempotent observation, even if it touches the build cache.
- `memory_search`, `recall_actions`

**Plan manipulation (writes events to the quest or plan store):**
- `add_to_execution_plan(intent, rationale?)` — **tactical, frictionless.** Inserts a plan item into the current quest's durable execution plan (P4b). Use for "after this, also do X" type instructions.
- `add_subtask(parent_quest_id, description, rationale)` — **strategic, surfaced.** Creates a sub-quest. Use when the addition is its own piece of work the captain should see as scope.
- `change_quest_scope(quest_id, addition_or_removal, rationale)`
- `change_quest_direction(quest_id, new_approach, rationale)`
- `note_on_quest(quest_id, observation)` — context the executor should see on next turn boundary; lighter than a direction change.
- `mark_quest_blocked(quest_id, reason)` (rare; mostly the executor does this)
- `fork_quest(quest_id, branches, rationale)`
- `complete_quest_intent(quest_id)` — captain says "we're done with this," chat-shadow closes it.

The `add_to_execution_plan` vs `add_subtask` distinction is the most-used classification chat-shadow makes per turn. Most captain instructions are tactical and go into the plan. A subtask is reserved for genuinely separable work the captain wants to see as a unit.

**Pending action mediation:**
- `modify_pending_action(action_id, patch, rationale)`
- `approve_pending_action(action_id)`
- `reject_pending_action(action_id, rationale)`

**Question/answer:**
- `answer_question(question_id, answer)` — captain's words routed into a typed answer event.

**Executor control:**
- `pause_executor(reason)`
- `resume_executor()`
- `stop_executor(reason)` — hard stop, same effect as captain's UI button.

**Speaking:**
- `speak(message)` — emits a `chat_message` (shadow → captain).
- `surface_observation(message)` — emits an `observation` for captain visibility.

**Explicitly absent:** `write`, `edit`, mutating `bash`, anything that changes the world. Chat-shadow does not act on the codebase. Period.

## Routing captain input

When the captain types something into chat, chat-shadow classifies intent and takes the appropriate action. Common patterns:

| Captain intent | Chat-shadow action |
|----------------|---------------------|
| Question about the work | `read` / `grep` / `memory_search` to investigate, then `speak` answer. Optionally emit `investigation` event if findings are durable. |
| Question about something orthogonal | Answer from L3 / external lookups. No quest event. |
| Tactical addition ("also rename X", "after this, grep for Y") | `add_to_execution_plan` — frictionless plan update, no quest scope mutation. |
| New piece of work ("now let's also refactor the test suite") | `add_subtask` with rationale. Surfaces as scope. |
| Routine command ("commit and push") | Default to `add_to_execution_plan` (executor does it next). Promote to `add_subtask` only if it's genuinely separable work. Either way reply: "on it." |
| Redirect ("change of plans, do Y") | Default to queued: `change_quest_direction` only — executor picks up at next action boundary. Captain can say "now" to escalate to immediate (`pause_executor` → change → `resume_executor`). |
| Mid-action tweak ("but use pub(crate) instead of pub") | `pause_executor` → `note_on_quest` with the tweak → `resume_executor`. Executor adapts mid-action. |
| Approval/rejection of pending action | `approve_pending_action` or `reject_pending_action` (with rationale if reject). Rare — pending actions are escalation-only. |
| Modification of pending action | `modify_pending_action` with the patch from dialogue. |
| Answer to a `question` event | `answer_question` (typed) so executor can consume cleanly. |
| Observation / FYI ("by the way, X") | `note_on_quest` so executor sees the context. |
| Pause / stop | `pause_executor` / `stop_executor`. |
| Fork ("try it two ways") | `fork_quest` with branches. |
| Conversation only ("interesting, why does the team prefer X?") | `speak` reply, no quest mutation. |

The classification doesn't have to be perfect — chat-shadow can ask clarifying questions when intent is ambiguous ("now or after current step?"), and the routing is correctable (chat-shadow can revert its own quest changes if it misread).

## Walkthrough — one extended scenario

To ground the model, one example showing both surfaces in parallel under the *trust within scope* posture (no permission rituals).

**Setup.** Igris is mid-quest: refactoring the auth middleware. Captain has chat open. Several coherent actions already on the timeline (read auth files, identified extraction points).

---

Igris starts a new coherent action: extracting the middleware. Begins executing.

```
[timeline: Igris: extracting middleware to auth.rs
   ├ read auth.rs
   ├ read handlers.rs
   └ (in progress: editing auth.rs...)]
```

Captain in chat: *"wait, why this instead of refactoring the existing handler?"*

Chat-shadow recognizes a question that may need to halt the in-flight write before it lands. Calls `pause_executor("captain question about approach")`. Executor halts after its current tool call (write hasn't happened yet).

Chat-shadow reads `auth.rs` and adjacent files, articulates the tradeoff:

```
[chat]
Captain: wait, why this instead of refactoring the existing handler?
Igris: extraction gives us a reusable middleware function we can apply to
       other routes — there are 3 more endpoints that need the same check.
       inline refactor solves this one but duplicates when we add the
       others. tradeoff: bigger blast radius right now.
```

Captain: *"OK do it but make `verify_token` `pub(crate)` not `pub`."*

This is a tactical refinement to the in-flight work. Chat-shadow:
- `note_on_quest("for the extraction in progress: verify_token should be pub(crate), not pub", rationale: <captain ask>)`
- `resume_executor()`

Executor wakes, reads the note before continuing. Adapts the in-flight action — uses `pub(crate)` for `verify_token`. Completes:

```
[timeline: Igris: extracting middleware to auth.rs
   ├ read auth.rs
   ├ read handlers.rs
   ├ edit auth.rs (3 changes)
   ├ bash: cargo check
   └ outcome: middleware extracted. verify_token is pub(crate) per captain.]
```

Chat:
```
Igris: done. verify_token is pub(crate).
```

Captain: *"also rename `verify` to `validate` in the same file."*

Chat-shadow classifies this as tactical follow-up, not a quest scope expansion. Calls `add_to_execution_plan("rename verify → validate in auth.rs and callers", rationale: "captain follow-up after middleware extraction")`. No quest scope mutation — the planned action is queued in the quest's execution plan.

Executor picks it up after closing current action:

```
[timeline: Igris: rename verify → validate in auth.rs
   ├ read auth.rs
   ├ edit auth.rs (3 replacements)
   ├ grep for verify in callers
   ├ edit handlers.rs (1 replacement)
   ├ edit middleware.rs (2 replacements)
   ├ bash: cargo check
   └ outcome: renamed in 3 files, cargo check passes.]
```

Chat:
```
Igris: renamed. cargo check passes.
```

What the captain experiences: a clean conversation with Igris on the chat surface and a parallel timeline of work on the timeline surface. No "approve this write" friction. The pause-discuss-resume happened invisibly when the captain interjected mid-action. The follow-up rename was added to the executor's plan, not as a captain-visible scope change.

Under the hood: chat-shadow handled dialogue + one pause + one note + one resume + one tactical plan addition. The executor did read×4 + edit×7 + grep + bash×2 across two coherent actions. The quest tree gained 2 coherent actions and 1 chat exchange (with embedded pause/note/resume metadata), all in one event log. Pending actions never appeared — they would have if Igris were about to do something destructive, out-of-scope, or against a captain-set permission gate. Routine work just flowed.

## Branching as multi-thread + multi-quest

When the captain forks a shadow ("try this two ways"), it creates parallel attention threads, parallel substrate forks (per `substrate.md`), and parallel quest subtrees:

- `fork_quest(parent, ["approach A", "approach B"], rationale)` creates two child quests under a shared parent.
- Each child quest gets its own executor thread (Pi session), its own L2 working memory, and a fork-local subtree namespace for L3 writes.
- Each child quest also gets its own chat-shadow when the captain enters dialogue with that fork.
- The parent quest's chat-shadow is the *meta-observer* — it can read both children's quests, surface comparative status, and host conversations like "how are both forks doing?" Captain can talk to a fork specifically, or to the parent for the comparative view.
- Code-side: `git worktree add ../monarch-fork-A <branch>` per fork. Worktree path stored on the fork's quest node.
- When captain picks a winner: `merge_quest(winner_id, into: parent)` — winner's branch-local L3 claims promote to project subtree (Keeper-mediated), loser's archive, winner's git branch merges, loser's drops or stays per captain.

Branching is therefore: code worktrees + working memory forks + quest subtree forks + per-fork attention threads. The substrate keeps L1 + L3 read coherent across forks; the threads and quests give each fork its own working surface.

## Implications for the data model

> Schema sketches are **illustrative**, not prescriptive. Real shapes get worked out in implementation tickets. Storage choices remain open.

**Quest tree** — substantial expansion of `quest_nodes`:

```sql
ALTER TABLE quest_nodes ADD COLUMN status TEXT NOT NULL DEFAULT 'active';
  -- active | paused | blocked | done | abandoned | forked
ALTER TABLE quest_nodes ADD COLUMN scope TEXT;          -- structured or freeform
ALTER TABLE quest_nodes ADD COLUMN current_direction TEXT;
ALTER TABLE quest_nodes ADD COLUMN rationale TEXT;
ALTER TABLE quest_nodes ADD COLUMN fork_parent_id TEXT REFERENCES quest_nodes(id);
ALTER TABLE quest_nodes ADD COLUMN worktree_path TEXT;  -- when forked
ALTER TABLE quest_nodes ADD COLUMN grade TEXT;          -- E..S after completion
ALTER TABLE quest_nodes ADD COLUMN summary TEXT;        -- post-completion
```

**Quest events** — much richer than today, with nesting and surface routing:

```sql
ALTER TABLE quest_events ADD COLUMN parent_event_id INTEGER REFERENCES quest_events(id);
ALTER TABLE quest_events ADD COLUMN author TEXT NOT NULL;
  -- executor | chat_shadow | captain | keeper
ALTER TABLE quest_events ADD COLUMN surface_override TEXT;
  -- timeline | chat | both | hidden — when not derivable from kind
ALTER TABLE quest_events ADD COLUMN payload_schema_version INTEGER NOT NULL DEFAULT 1;
```

The `kind` column already exists; the taxonomy expands per the event list above. Each kind has a typed payload schema; versioned for evolution.

**Thread registry** — track which sessions belong to which agent and which thread role:

```sql
CREATE TABLE attention_threads (
  id TEXT PRIMARY KEY,
  agent_id TEXT NOT NULL REFERENCES agents(id),
  role TEXT NOT NULL,         -- executor | chat | observer
  pi_session_id TEXT,         -- the underlying Pi session
  current_quest_id TEXT REFERENCES quest_nodes(id),
  status TEXT NOT NULL,       -- active | paused | stopped
  created_at TEXT NOT NULL,
  ended_at TEXT
);
```

This gives Rust a clean place to track which Pi sessions exist per agent, what role each plays, and how to route events to UI.

**Chat sessions** — chat-shadow's dialogue history. Could be its own table or fold into `messages` with a thread role. Either way: separate from quest events (chat_messages can emit *into* quests via events, but the chat conversation has its own log).

## Active chat + execution timeline as a transferable pattern

This UX pattern — **clean dialogue surface + intent-level work timeline, both anchored to the same task** — solves a problem most agentic UIs get wrong today. Two common failure modes elsewhere:

- **Dump everything into chat.** Tool calls, raw JSON, model thinking, all interleaved with dialogue. Captain has to skim past mechanical noise to find conversation. Hard to follow either thread.
- **Hide everything.** Just show the chat. Captain has no insight into what the agent is actually doing. Trust collapses when something goes wrong because they can't see why.

The two-surface model splits the difference: chat stays *clean* (only dialogue), timeline stays *visible* (work is observable), and both stay *coherent* because the executor narrates intent at the action level. The captain reads at whatever depth they want.

This isn't Monarch-specific. It's a general pattern for surfacing agents that anyone running multi-tool agents would benefit from. We should claim it as a default pattern: **active chat + execution timeline**, anchored on a shared task spine.

## What this document does not cover

By design, the following are out of scope and live elsewhere:

- **The substrate's four-layer model** (L1 identity, L2 working memory, L3 tree, L4 search) — see `substrate.md`.
- **The Keeper, distillation triggers, atomic claims, first-person quest report shape** — see `distillation.md`.
- **Concrete schema migrations** — implementation tickets own actual `ALTER TABLE` blocks.
- **UI for chat panel and timeline panel** — feature tickets.
- **Pi compatibility tests for parallel sessions per agent** — implementation spike.
- **Routing intent classifier prompt for chat-shadow** — implementation work informed by this design.

## Working assumptions captured here

Listed for cross-doc reference. Treat as current direction, not final calls. Most likely candidates for revision: #5 (chat-shadow tool list — may grow or contract once we use it), #11 (the exact event taxonomy — will evolve with use).

1. **One shadow, two organs.** Executor (hands) and chat-shadow (mouth) are the same shadow with shared L1/L2/L3.
2. **Captain experiences one shadow.** The dual-thread machinery is invisible; routing is fast and unnarrated.
3. **The quest tree is the temporal spine.** All activity hangs off quests as typed events.
4. **Two surfaces on each quest:** chat (dialogue only) and execution timeline (intent-level actions). Surface is derived from event kind, with override.
5. **Chat-shadow has read tools + plan-manipulation tools + executor-control tools, but no world-mutation tools.** It directs, it does not act.
6. **Coherent atomic action is the unit of executor narration.** Declared intent + nested tool calls + closing outcome.
7. **Three self-reporting cadences:** per coherent action (timeline), per quest (first-person report), continuous (Keeper compaction).
8. **Default to queued redirect; immediate on captain's request; hard stop as escape hatch.** Most redirects pick up at the next coherent action boundary (`change_quest_direction` only). Captain can say "now" to escalate to immediate (`pause_executor` → change → `resume_executor`). Hard stop via `stop_executor` (chat-shadow tool or captain UI button).
9. **Pending actions are reserved for escalation, not routine.** Default posture is trust within scope; pending actions surface only for destructive, irreversible, out-of-scope, or permission-gated operations. Chat-shadow can modify them in flight when they do appear, not just yes/no.
10. **Question/answer is its own event pair.** Distinct from blocker/redirect.
11. **Rich event taxonomy is the universal connective tissue.** When a new kind of thing happens, it gets a typed event.
12. **Branching = code worktrees + working memory forks + quest subtree forks + per-fork attention threads.** L1 + L3 stay shared (read).
13. **Each thread = one Pi session.** Same identity in the system prompt. Substrate makes them coherent.
14. **Active chat + execution timeline is a transferable agent UX pattern.** We adopt it as default; worth surfacing beyond Monarch.
15. **Three layers of intention: quest, plan, action.** Quest = captain's lever (deliberate, surfaced, rationale-required). Durable execution plan = provisional intended route. Coherent action = actual narrated execution chunk. Chat-shadow distinguishes tactical plan additions (`add_to_execution_plan`) from quest changes (`add_subtask`, `change_quest_direction`) per turn.
