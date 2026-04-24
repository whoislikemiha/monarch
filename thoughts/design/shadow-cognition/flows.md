# Interaction Flows

> **Status:** Idea document — exploratory, not a spec. Captures how the pieces designed in `substrate.md` and `attention.md` actually run as a loop. **Tech choices and exact protocols remain tentative.**
>
> **Sibling docs:** `substrate.md` (the four-layer self), `attention.md` (chat + executor + quest tree), `distillation.md` (the Keeper, compaction).

## Premise

We've designed the pieces — substrate, threads, quest tree, two surfaces, event taxonomy, tools per thread. This doc designs the **loop**: how conversations begin, how each thread navigates a turn, how the environment is sensed, how chat and executor stay in sync without stepping on each other, what happens when the system goes idle or restarts.

The pieces only feel coherent if the loop is right. A great event taxonomy with a sloppy per-turn loop produces an agent that feels stuttery and confused. The loop deserves its own design pass.

## Conversation entry conditions

The captain doesn't "type to a shadow" in one mode. There are several distinct ways a conversation begins, each calling for different opening behavior. Chat-shadow's first move depends on which condition fired.

### Cold start
Captain opens the app fresh. No active quest, no in-flight chat. Shadow's first move:
- Loads L1 (captain identity + shadow identity), L3 root subtree.
- Surfaces anything notable: a paused quest, a recent unfinished thread, a captain-relevant context they may have forgotten.
- Opens with a status-aware prompt: *"What would you like to work on?"* or *"You were last working on MON-82 (classifier slice 1, paused mid-test) — pick up there?"*
- If captain types an instruction, may initiate quest creation.

### Warm start, idle shadow
A quest exists, executor is paused or between turns. Captain opens chat:
- Loads quest state and recent quest events (last N significant).
- Loads relevant L3 subtree (project memory).
- Opens contextually: *"Ready to pick up MON-82 — last thing was X, ready for Y"* or *"Anything you want me to check on MON-82?"*

### Warm start, active shadow
Captain opens chat while executor is mid-quest, possibly mid-coherent-action. Shadow:
- Reads L2 working memory: `current_action`, `recent_actions`, `planned_actions`.
- Reads recent quest events for fresh context.
- Opens with a status: *"I'm in the middle of the auth refactor (extracting middleware now), what's up?"*
- Captain can chat without disrupting; or interrupt explicitly.

### Notification-driven
Shadow surfaced something — a `blocker`, a `question`, a significant `observation`. Notification appears in captain's UI. Captain opens it:
- Conversation may already be at the issue. Notification → chat opens with the surfaced event in view.
- Shadow ready to discuss with full context loaded.

### Resumed conversation
Captain returns to an existing chat after time (hours, days). Chat history loads. Shadow:
- Refreshes L2 — executor may have done significant work since.
- Diffs: what's changed since last chat exchange?
- Greets contextually: *"We were discussing the test mocking strategy. Since then I finished the migration, ran tests, 2 still failing. Want to look at those?"*

### Captain-initiated quest creation
Captain says: *"I want to add complexity classifier to user messages."* Shadow:
- Recognizes quest-creation intent (no current matching quest).
- May invoke `/linear-to-plan` or similar quest-creation tooling.
- Drafts initial quest with scope, surfaces for captain confirmation.
- Once confirmed, ready to work on it.

## The chat-shadow per-turn loop

When the captain types a message, chat-shadow runs:

1. **Load context.**
   - L1 (captain + shadow identity) — always.
   - L2 working memory — fresh snapshot.
   - L3 — warm subtree based on current quest topic + any topic shift signaled by the message.
   - Current quest state + recent quest events (last N significant).
   - Chat history since last summarization tick.
   - The new message itself.

2. **Read the message.** Captain's input.

3. **Classify intent.** Per the routing table in `attention.md`:
   - Question about current work?
   - Question about something orthogonal?
   - Tactical addition vs new piece of work?
   - Redirect (immediate vs queued)?
   - Mid-action tweak?
   - Approval/rejection of a pending action?
   - Pause/stop?
   - Fork?
   - Pure dialogue?

4. **Take the route action.** May involve:
   - Reading code/memory to investigate.
   - Writing events to the quest tree (`change_quest_direction`, `add_subtask`, `note_on_quest`, etc.).
   - Updating the executor's internal plan in L2 (`add_to_internal_plan`).
   - Pausing/resuming the executor.

5. **`speak` the response.** Goes to chat surface. Captain-tuned, brief.

6. **Optionally emit `observation` or `note_on_quest`.** When the conversation produces durable insights worth preserving on the quest's timeline.

The whole loop should complete in seconds for routine cases. Investigative questions (requiring read tools) take longer. Captain shouldn't perceive the routing — they should perceive a thoughtful response.

## The executor per-turn loop

The executor runs continuously while a quest is active and not paused. Between coherent actions it runs:

1. **Refresh L2.** Especially: `planned_actions` (chat-shadow may have inserted), pending redirects, environment if stale.

2. **Check thread status.** Was I paused by chat? Has the quest's direction changed since last action? Are there new notes? If paused → halt and wait for `resume_executor`.

3. **Determine next action.**
   - If `planned_actions` is non-empty → dequeue the next one.
   - If empty but quest has direction → generate a new planned action based on quest state + recent activity.
   - If quest is `done` → `complete_quest` with first-person report, transition status, idle.

4. **Declare intent.** Emit `coherent_action` event with the intent as title. Goes to timeline.

5. **Execute the chunk.** Run tool calls + reasoning. Each is nested under the coherent action by `parent_event_id`.

6. **Close the action.** Emit `executor_action_outcome` with one-line summary. Update L2 (`current_action` cleared, `recent_actions` appended).

7. **Loop.** Back to step 1.

Within step 5 (executing the chunk):
- After each tool call, briefly check L2 for `pause_signals` (allows fast pause without waiting for action boundary).
- If a write conflict / risky / out-of-scope situation arises → `propose_pending_action`, halt, wait for chat-shadow decision.
- If genuinely blocked → `report_blocker` with hypotheses, halt.
- If need information from captain → `request_information`, halt.

## Environment snapshot

The shadow doesn't operate in a vacuum. Environmental context lives in `L2.environment` (per `substrate.md`):

```typescript
interface EnvironmentSnapshot {
  cwd: string;
  git: { branch, dirty_files, ahead_behind, last_commit_sha };
  recent_files_touched: string[];
  active_processes: string[];   // dev servers, watch tasks
  updated_at: string;
}
```

### When it refreshes

- **After any executor action that touches the environment** — writes, mutating bash, file ops. Cheap incremental update.
- **On demand** — when chat-shadow or executor explicitly needs current state (e.g., chat-shadow about to answer "what's the dirty file list?").
- **Idle tick** — periodic refresh during inactivity to keep status current.

### Why it matters

- **Saves tool calls.** No need to `git status` every turn — it's already in L2.
- **Lets chat-shadow answer status questions instantly** — substrate read, not tool call.
- **Underpins stale-flagging.** Memories that reference files use `git.last_commit_sha` to detect drift (per `substrate.md` § L3 / stale-flagging).
- **Future hook for IDE integrations.** Editor state (open files, cursor position) can flow into `environment` if integrations push it.

## Coordination between chat and executor

Without explicit coordination, chat-shadow could redirect mid-action and the executor could finish the now-irrelevant work. Or two events could race on L2. Patterns to keep them coherent:

### Chat-shadow checks executor status before invasive operations

Reads `L2.attention_threads.executor.status` (one of: `idle`, `running_action`, `awaiting_decision`, `paused`, `blocked`). Adapts:

- `running_action` + tactical addition → `add_to_internal_plan` (no pause needed; executor reads on next loop).
- `running_action` + immediate redirect → `pause_executor` first, then change, then resume.
- `awaiting_decision` → handle the pending decision cleanly.
- `paused` → write changes, then `resume_executor`.

### Executor checks substrate between actions

Per the per-turn loop steps 1–2. Plan changes, redirects, notes are all picked up at action boundaries. Mid-action checks are for hard-stop signals only.

### Hard stops are race-free

`stop_executor` halts the in-flight Pi call. Executor is killed before the next action can be issued. No race window.

### Plan changes happen at action boundaries, not mid-action

Adding to `planned_actions` while executor is mid-action just means the new action runs after current completes. Mid-action redirects via `note_on_quest` + `pause_executor` + `resume_executor` are explicit and intentional.

### Conflict scenarios

- **Two chat-shadow tool calls in flight at once.** Shouldn't happen — chat-shadow turns are sequential per Pi session.
- **Chat-shadow update lands while executor is reading L2.** Single-writer pipeline (Rust serializer) prevents observable inconsistency. The executor sees a consistent snapshot.
- **Chat-shadow redirects while executor's pending_action is awaiting decision.** Chat-shadow's redirect supersedes; pending action is auto-rejected with rationale `"superseded by redirect"`.

## Idle behavior

What does a shadow do when there's nothing to do?

- **Executor:**
  - If quest is `done` → wrote first-person report, transitioned status, idle.
  - If quest is `paused` (by chat or by blocker) → wait.
  - If `planned_actions` is empty and quest has no remaining direction → typically a signal the quest needs new direction from captain; surface `observation` if appropriate.

- **Periodically while idle:**
  - Keeper compaction tick (per `distillation.md`).
  - Environment snapshot refresh (cheap).
  - Future: proactive notice of drift, stale memories, opportunities. Out of scope for v1.

- **Resource-wise:** idle threads release Pi sessions or hold them open per provider cost. Default: hold for short idle (minutes), release for long idle (hours). Reattaches on new activity.

## Death and resurrection

The substrate is canonical. Anything in volatile memory can be rebuilt from the database.

- **Sidecar restart.** Agent state reconstructs: substrate (L1/L2/L3) from DB, quests + events from DB, chat history from DB. Pi sessions rebuild from the message history they had pre-restart.
- **Pi session restart within a running sidecar.** Same: rebuild from messages + substrate.
- **Captain app restart.** Frontend re-subscribes to event channels, fetches initial state, reconciles by `stateVersion`. Same pattern as today.
- **L2 corruption.** Can be reconstructed from the most recent quest events + recent chat history (L2 is itself derivable from event log if needed).
- **L3 corruption.** Re-run Keeper distillation over raw transcripts. Slow but lossless for conceptual content.

Nothing critical lives only in volatile memory. The substrate's reconstructability is a load-bearing property — it's what makes long-running shadows safe.

## What this document does not cover

- The substrate's layered model — see `substrate.md`.
- Thread types, tool taxonomy, surface routing, event taxonomy — see `attention.md`.
- The Keeper's distillation logic, atomic claim definition, compaction triggers — see `distillation.md`.
- UI specifics for chat panel and timeline panel — feature tickets.
- Authentication, API quota handling, error recovery — implementation details.

## Working assumptions captured here

Listed for cross-doc reference. Treat as current direction, not final calls. Most likely candidates for revision: #5 (Pi session lifecycle policy), #6 (idle behavior — proactive shadows are an open question), #8 (environment snapshot refresh policy may shift).

1. **Conversations have entry conditions, not one mode.** Chat-shadow's opening behavior depends on cold/warm/active/notification/resumed/quest-creation start.
2. **Chat-shadow per-turn loop is: load → read → classify → route → speak → optionally observe.**
3. **Executor per-turn loop is: refresh L2 → check status → determine next action → declare intent → execute → close → loop.**
4. **Mid-action interrupts go through pause + modify + resume.** Plan changes flow through L2 between actions; only hard stops break in-flight calls.
5. **Each thread = one Pi session per agent.** Shared identity via L1; same shadow.
6. **Idle shadows do periodic Keeper ticks and environment refreshes.** Proactive surfacing is out of scope for v1.
7. **Substrate is canonical; everything else is reconstructable.** Crash recovery is "rebuild from DB."
8. **Environment snapshot lives in L2; refreshed after touch + on demand + periodically.** Cheap reads, cheap updates.
9. **Conflict resolution between chat and executor is explicit, not optimistic.** Chat-shadow checks executor status before invasive operations; executor checks substrate between actions.
