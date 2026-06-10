# Monarch — Product Roadmap v2 (The Campaign Roadmap)

> **Status:** Supersedes [`roadmap.md`](./roadmap.md) (the "Shadow Cognition" roadmap), which is now archived. That doc sequenced the cognitive substrate (memory, attention, distillation) and got us most of the single-shadow *data* layer. This doc re-frames the whole product around the model we converged on: a **Campaign** of **Objectives** worked by **Shadows**, curated by the **Keeper** (memory) and the **Architect** (work), experienced through a **two-organ** shadow (chat + executor) in a **redesigned UI**.
>
> **The bet of this roadmap:** finish **Arc I** and you have a complete, polished *single-shadow* product — summon a shadow, talk to it, it works objectives under a project's campaign, remembers, and curates its own backlog, all in the new visual language. **Arc II** then layers the fleet (multi-shadow crews, channels, coordination) on top. **Arc III** is depth and scale.
>
> **Phase rule (unchanged from v1):** a phase is a phase only if, at the end of it, you can sit with the running app and tell whether it works *without* the next phase. Shared infra rides inside the phase that first needs it.

---

## The model & vocabulary (what changed)

| Term | Means | Was called |
|------|-------|------------|
| **Campaign** | A project's single, living work-tree. One root per project; never "done"; all work branches under it. | (the root quest) |
| **Objective** | A node in the campaign, at any granularity (root → leaf) and any state (planned → done). The unit of work. Replaces "quest". | quest / sub-quest |
| **Shadow** | A persistent agent — who you talk to; holds identity + private memory; gets better over time. | (unchanged) |
| **Captain** | You. Commands shadows. | (unchanged) |
| **Keeper** | Curator of the **memory** tree (distills, merges, supersedes claims). | (unchanged) |
| **Architect** | A **planning-specialist shadow** (one per project) — your chief-of-staff for the campaign. Two modes: a *background* curator (triages, places, decomposes captured work — reasoning always visible) and a *foreground* **planning conversation** you sit down with to shape the work before dispatching executor shadows. Lives in the Agents view like any shadow; also openable from a project's objective window. | (was the MON-84 "classifier reader") |
| **Two organs** | A working shadow has a **chat-shadow** (talks, reads substrate, plans — never mutates the world) and an **executor** (acts). The captain only ever *talks*; the executor is something you *watch*. | (unbuilt) |
| **Channel** | A conversation = a participant set + an optional work-scope. DM (1 shadow) · project chat · objective room (a crew) · group chat (hand-picked). | (just "chat") |

**Load-bearing principles** (the conclusions from design, encoded so phases don't drift):

1. **One tagged turn-stream.** Every turn/event carries `{author, audience, objective?, session, project?}`. "Chats" and "timelines" are just *projections* (filters) over that one stream — that's what makes per-shadow history, per-objective history, captain↔shadow, and agent↔agent all the same machinery. The app surfaces this as **two top-level views — Agents (who) and Projects (what) — two projections you pivot between**, which is what keeps either screen from cramming.
2. **Objectives are project-owned, discoverable, continuable.** A shadow is *assigned* to an objective; it doesn't own it. The objective's record (timeline + plan + report) is a **handoff dossier** — complete enough that a different shadow can pick it up. (This is *why* narration/plans/reports exist.)
3. **Private words, public actions.** A DM to a shadow is private; its resulting *actions* always land on the shared objective timeline. Coordination never desyncs.
4. **Capture cheap, curate lazy.** Anyone can append an objective anywhere (even to an inbox). Placing it correctly + reasoning implications is the Architect's job, done later — exactly how the Keeper treats raw memory.
5. **Ceremony scales with complexity, automatically.** The classifier (MON-82) decides per turn: *is this meaningful work?* → objective gets created (silently); *how complex?* → how much planning/decomposition surfaces. Solo trivial work = zero ceremony. The record is always free; only big work summons structure.
6. **Memory scopes: self / project / captain. No group scope.** Group-chat knowledge flows to each participant's `self` and (if work) to the project. (See P4.)
7. **The campaign tree is the living backlog.** Work lives as durable nodes in the tree — planned, in-progress, and done all in one place — replacing static roadmap files and ad-hoc tickets. Unfinished work persists as a branch so it's never forgotten.
8. **Visual house style** (locked in the Prompt 0/0b foundation): dark, dense, flat — **no shadows/glows ever** (depth = elevation + 1px borders + space), restrained radius, **Inter for language / JetBrains Mono only for data**, themeable tokens.

---

## What's shipped (the record)

The v1 roadmap delivered the **single-shadow data substrate** and the identity/memory cognition. By ticket:

| Surface | Tickets | State |
|---------|---------|-------|
| Captain + shadow identity (L1) | MON-98 | ✅ |
| Memory substrate + Keeper write/read + executor `suggest_memory` | MON-99/100/101/102 | ✅ (only `self` scope actually written) |
| Compaction split (Pi owns live context; Keeper = memory-only) | MON-123 | ✅ Slice A + trigger fix; B/C open |
| Auto-create current quest on meaningful turns | MON-105 | ✅ (per-turn root — **changes in P1**) |
| Executor narration + L2 working memory + coherent-action timeline | MON-107/108/109 | ✅ |
| Durable execution plans (intended route) | MON-110/111/112/114 | ✅ |
| Rich quest model + manual editor + refs | MON-115/116/117 | ✅ |
| First-person quest reports | MON-118/119/120/121/122 | ✅ |
| Per-turn complexity classifier (advisory) | MON-82 | ✅ (no consumer yet) |
| **Visual language foundation** (tokens, type, components, inspector atoms) | Prompt 0/0b | ✅ as a design artifact — **not yet in the app** |

**The schema is ahead of the behavior.** `quest_nodes` already carries `root_id`/`parent_id` (tree), `assignee_shadow_id` (multi-shadow), `created_by ∈ {monarch, architect, steward, orchestrator}` (the curator was anticipated), a `pending` status (backlog nodes already legal), and `worktree_path`/`branched_from_id` (forking). Much of the vision below is **agents + wiring + UI**, not new tables.

**The real gaps:** no project↔campaign-root link · one Pi session per agent (no two-organ split) · the Architect is unbuilt (classifier's `decomposable`/`delegate` labels go nowhere) · only `self` memory scope is written · the redesign isn't in the app · all multi-agent.

---

# Arc I — The Single Shadow

> Goal of the arc: a complete, polished single-agent product. Finish Arc I and Monarch is *usable and good* for one shadow at a time.

### P1 — Campaign & Objective

**Goal.** Rename and restructure: a project gets **one campaign root**, and all work branches under it; planned/backlog objectives are first-class; every objective surfaces its notes & artifacts.

**Test scenario.** Open a project → it has a campaign root automatically. Give a shadow meaningful work → it appears as a **branch** under that root (not a new root). Type "we should also migrate the auth module" → it lands as a **pending** objective that just sits in the tree until someone starts it. Open any objective → its notes, refs, and report are visible.

**Builds.**
- `projects.root_objective_id` (or a `campaigns` row) linking each project to its single root objective; auto-created on project detect / first meaningful work.
- Rework MON-105: meaningful turns create **branches under the project's campaign root**, not fresh per-turn roots. `agents.current_quest_id` → points into the campaign tree (rename to `current_objective_id`).
- First-class **capture-future-work** flow (the `pending` status already exists): a low-friction "add objective" that persists unstarted, placeable anywhere in the tree.
- **Eager creation:** a meaningful turn (per the classifier) creates the objective immediately and visibly; the Architect (P2) curates/merges/abandons later. Capture cheap, curate lazy.
- **Project-less "just ask"** stays ephemeral conversation by default; if it turns into work, it promotes into a per-captain **scratch campaign** so unscoped work is still recoverable.
- Surface notes / refs / artifacts on every objective node (already in `quest_refs` + manual events — make it uniform).
- The **rename** quest→objective / campaign across schema views, `bindings.ts`, UI, sidecar tools, prompts, and docs.
- **Objectives are continuable, not session-bound:** an explore/continue-objective tool loads an objective's dossier so a fresh session (or another shadow) can pick up the work — continuation rides the objective record, not session ancestry.

**Depends on.** Nothing new — refactor on shipped substrate.

**Defers.** Intelligent placement / implications (that's the Architect, P2).

---

### P2 — The Architect: planning shadow + intent engine (single-shadow)

**The Architect is a planning-specialist shadow (one per project) — the largest single component of Arc I.** It's the chief-of-staff for a project's campaign, and it has **two modes**:
- **Foreground — a planning conversation.** You sit down with it and talk through the job: it proposes objectives / sub-objectives, you refine, it branches the campaign — then **you manually dispatch** an executor shadow. This is the "plan, then implement" loop. The Architect is reachable as a shadow in **Agents view** and openable from the **objective window in Projects view**.
- **Background — an intent engine.** Behind the scenes it classifies intent and curates the work tree, and **its reasoning is always visible** so you can trust *why* it classified and placed something.

**Goal.** Make "plan with the Architect → branch objectives → dispatch a shadow" a first-class loop, while the background engine keeps the campaign coherent automatically with an inspectable reasoning trail. The Keeper, but for work — and the intent classifier, finally consumed.

**Two tiers (the background engine: cheap gate → expensive brain).**
- **Tier 1 — Triage** is the existing per-turn classifier (MON-82), today advisory and going nowhere. Now it's the Architect's front door: every turn gets cheap, fast labels and gates whether Tier 2 runs — you never spend a heavy reasoning pass on "hi".
- **Tier 2 — Reasoning** fires only when Tier 1 says "work": create/place the objective under the right parent, dedupe against existing nodes, flag blocks/dependencies, propagate scope changes upward, decompose a complex ask into sub-objectives. Every decision emits its rationale.

**Test scenario.** Open a project → talk to the Architect: "let's add token-budget enforcement." It proposes 3 sub-objectives under the right campaign branch, with rationale; you tweak one, accept; you dispatch a shadow at the first. Later, jump back to the Architect: "also add a warning banner." It branches a new objective; you dispatch again. Separately, type "we also need to handle the null case" mid-work → the *background* engine files it under the open queue objective (not a duplicate) and the trail shows why. "fix this typo" → filed, no decomposition. "what's the db path?" → Tier 1 says *question*; no objective.

**Builds.**
- The **Architect** as a per-project planning shadow (creates with `created_by='architect'`; lazy curation/merge can use the reserved `'steward'` role). A **planning conversation** surface (propose → refine → branch → manual dispatch), reachable from both views.
- Background engine consuming MON-82 as Tier-1 triage — its first real consumer; placement + implication logic mirroring the Keeper's merge/supersede/insert.
- **Observable reasoning** — every classification/placement/decomposition emits a rationale, surfaced as a reasoning trail on the objective + a running Architect log. First-class, not debug output.
- **Eager creation, lazy curation:** a meaningful turn creates the objective immediately (P1); the Architect then refines — merges, re-places, or abandons what fizzled.
- **Manual dispatch:** the Architect proposes/assigns; the captain launches the shadow. The direct-to-shadow path stays for quick work (planning is never mandatory).

**Depends on.** P1 (a campaign tree to curate) + MON-82 (promoted from advisory to Tier-1 triage).

**Defers.** Crew assignment / multi-shadow decomposition + auto-dispatch (P9).

---

### P3 — Two organs: chat-shadow + executor (single-shadow)

**Goal.** The interaction model. A shadow you **talk to** (chat-shadow) while it **works** (executor). Clean chat surface; execution timeline as a sibling. The captain never drives the executor — they talk, and watch.

**Test scenario.** Open a shadow mid-objective. Ask "what are you doing?" → a one-sentence answer from L2 (`current_action` + recent actions), no tool spam in the chat. The execution timeline beside it shows the actual work. Say "pause" → executor halts at the next action boundary; "resume" → it continues. The chat panel never shows raw tool calls; the timeline does.

**Builds.**
- **Pi multiplexing per agent** — a chat-shadow session alongside the executor session (the flagged spike: confirm Pi handles two concurrent sessions per agent gracefully). This is the single biggest infra lift in Arc I.
- Chat-shadow toolset (read-only): `read`/`grep`/`memory_search`/`recall_actions`/`speak`/`pause_executor`/`resume_executor`/`stop_executor`. Explicit deny on world-mutation.
- `attention_threads` (session role: executor / chat).
- Dual-surface UI: clean chat + execution timeline as siblings; the "talk about the *job*" vs "watch the *doing*" altitudes for one shadow.

**Depends on.** P1 (the objective tree to narrate against), benefits from P2. The chat-shadow reads L2/memory that already exist.

**Defers.** Plan-manipulation & routing tools (those bloom in Arc II). Multi-shadow channels (P8).

---

### P4 — Living memory: self + project + captain scopes

**Goal.** Make the memory scopes real. Today only `self` is written; a shadow should also accrete **project** memory (shared substrate) and read **captain** preferences, so a shadow's *next* objective on a project benefits from its *last*.

**Test scenario.** A shadow finishes an objective on project Aurora; the Keeper writes a `project`-scoped claim ("the Keeper batches in `queue.rs`; never call distill twice"). The shadow's next objective on Aurora surfaces that claim in context — without re-deriving it. Captain edits a preference; it lands as `captain` scope and shapes the next turn.

**Builds.**
- Keeper writes `project` and `captain` scoped memories (not just `self`); scope-aware retrieval (`self` ∪ `project` ∪ `captain`).
- Project memory as the shared substrate keyed by `project_id` (single-shadow slice: one shadow, many objectives on one project).
- Memory Inspector scope filter.

**Depends on.** P1 (project as a first-class home). Independent of P2/P3.

**Defers.** *Cross-shadow* project sharing (multiple shadows reading each other's project memory) — that's an Arc II concern once crews exist.

---

### P5 — The redesign applied (core surfaces)

**Goal.** Get the visual language out of the artifact and into the app, organized as **two top-level views — Agents and Projects** (per `thoughts/design/visual-direction/element-inventory.md`). Agents view = talk to / manage a shadow (its chats, stats, self-memory, history; the just-ask home). Projects view = campaign tree, objectives, watch-the-work, and the Architect. Rebuilt in the locked house style — **layout hand-crafted by the captain, not AI-generated** (the AI kept failing at hierarchy).

**Test scenario.** Launch the app: it reads as the foundation sheet — flat, no shadows, Inter for language, mono only for data, the elevation ladder doing the depth. You can switch between the Agents view (calm, conversational, no execution firehose) and the Projects view (campaign + watch-the-work). Neither screen feels crammed. Theme switching works off tokens.

**Builds.** Implement the surfaces from `element-inventory.md` against the real `global.css` tokens, split across the two views — Agents (roster/groups · agent conversation · stats · self-memory · history) and Projects (campaign tree · objective detail · the dual surface · the Architect conversation + reasoning trail · project-memory). Foundation atoms first, then the two view frames, then per-surface. Carry the 4 timeline backport fixes (headline-action icon, diamond collision, deep-tree indent, path truncation).

**Depends on.** Visual foundation (shipped). Parallel-safe with P1–P4; but the objective-timeline redesign wants P1's campaign structure to be real.

**Defers.** Multi-agent surfaces (objective room, channels, crew rosters) — they get designed when their phase opens (Arc II), in the same language.

---

### P6 — Full single-shadow experience (integration milestone)

**Goal.** The milestone. Everything wired into one coherent product: summon a shadow → talk to it (chat-shadow) → the Architect files work into the project's campaign → the executor works objectives → the Keeper distills self+project memory → you watch via the dual surface → all in the redesigned UI. Both the "just ask" path (no objective) and the "meaningful work auto-becomes an objective" path feel right.

**Test scenario.** Cold start: extract a shadow on a fresh project. Ask it an unrelated question → it just answers, no objective created. Give it a real task → an objective appears under the campaign, the executor works it, the chat stays clean while the timeline fills, the Keeper writes a couple of memories, a report lands at close. Come back next day, start a related task → retrieval surfaces yesterday's memory; the Architect files a follow-up you mentioned as a pending branch. It feels like commanding one capable, remembering agent.

**Builds.** Integration, gap-filling, and polish across P1–P5. The ad-hoc "just ask" path; the auto-objective path; end-to-end flow hardening; the empty/first-run experience.

**Depends on.** P1–P5.

---

# Arc II — The Fleet (multi-agent)

> Built on Arc I's two organs + Architect + campaign. Everything here degrades cleanly to the single-shadow case.

### P7 — Crew on an objective

**Goal.** More than one shadow on a piece of work. Roles, per-shadow timelines under one objective, and the objective window you designed (crew + tree + per-shadow drill-in + click-shadow-opens-chat).

**Test scenario.** Assign two shadows to an objective. Each owns a distinct **sub-branch** with its **own timeline**; the objective window shows a **tree of who's doing what** + a **crew roster** with roles (lead / worker / reviewer). Click a shadow → its own stream + its chat open. Mark the objective handoff-able → a third shadow can read the dossier and continue a branch.

**Builds.** Activate `assignee_shadow_id` (already in schema); roles; per-shadow timeline projection under one objective; the objective window UI (crew · tree with attribution · per-shadow drill-in). Objectives-as-handoff-dossiers made real (any shadow can continue).

**Depends on.** Arc I (P3 two organs, P1 campaign). Pre-wired by the schema.

**Defers.** Same-node parallel attempts (forking — P11).

---

### P8 — Channels & command

**Goal.** The full conversation model: DM · project chat · objective room · hand-assembled group chats; command at two altitudes; agent↔agent comms; private-words/public-actions.

**Test scenario.** Talk to an objective **room** → the **lead** receives and fans out. **DM** a worker → private to you two, but its **actions still appear on the shared timeline**. Create a **group chat** with three hand-picked shadows (cross-project) → ask them all something. Shadows message each other (`Igris → Wren: ready for review`) visibly in the room. A new shadow joins a project and reads the team's accreted context.

**Builds.** Channels as `{participants, optional work-scope}` over the one turn-stream. Command altitudes (room-via-lead / DM-direct). `audience`-aware turns; agent↔agent messages. Created group chats (project-agnostic rosters). Surface routing (chat vs timeline). Group-chat memory routes to participants' `self` + (if work) project — **no group scope**.

**Depends on.** P7 (crews exist) + P3 (two organs).

---

### P9 — The Architect II: crew decomposer & lead

**Goal.** The Architect now decomposes objectives **across a crew** and acts as the **lead** — splitting scope into branches, assigning shadows by rank+specialization, coordinating. The captain commands the effort through it.

**Test scenario.** "You three, rebuild the auth flow." → the Architect (a planning-specialist lead shadow) decomposes into branches, assigns Igris/Vesper/Wren by fit, and you steer the whole effort by talking to the lead — while still able to DM any worker. New work mid-effort gets filed and re-assigned without you placing it by hand.

**Builds.** Architect's crew-level decomposition + assignment (rank + specialization); the **lead** role (`created_by='orchestrator'` is pre-reserved); captain→room routed through the lead. Promotes P2's single-shadow Architect to the multi-agent planner.

**Depends on.** P7 + P8 + P2.

---

# Arc III — Depth & scale (interleave / later)

These improve things that already work; none gate Arc I or II. Pick by need.

### P10 — Memory quality & scale
Eval harness (recall@5 + merge quality on a fixed seed), reranker (top-20→top-5), background HNSW rebuild + atomic swap, incremental insert. *(old P3a–d / MON-94/93/96/97.)* Pick up when memory volume warrants.

### P11 — Forking & parallel attempts
"Try it two ways": same objective forked into N attempts, each a shadow + git worktree, comparative view + merge. *(old P10; schema already has `worktree_path`/`branch_name`/`branched_from_id`/`explore_fork_count`.)* Deferred by the captain for now.

### P12 — Stale-flagging & re-verification
Memories that reference files know when those files changed (`file_refs.anchor_sha`); organic re-verification re-anchors/supersedes. *(old P11.)*

### P13 — Inspector & observability polish
Full memory edit/archive/promote/supersede; campaign-tree visualization; Architect & Keeper observability ("Igris just learned…", "Architect filed 2 objectives"); idle sweeps; manual checkpoint. *(old P12.)*

---

## Critical path

```
P1 Campaign/Objective ─┬─► P2 Architect I ─────────────┐
                       ├─► P3 Two organs ──────────────┤
                       ├─► P4 Memory scopes ───────────┼─► P6 Full single-shadow ─► P7 Crew ─► P8 Channels ─► P9 Architect II
                       └─► P5 Redesign (parallel) ─────┘
                                                                         Arc III (P10–P13) interleaves after its prerequisite
```

- **P1 is the keystone** — the rename + campaign restructure everything else assumes.
- **P3 (two organs) is the biggest single lift** in Arc I and gates all of Arc II; start the Pi-multiplexing spike early.
- **P2, P4, P5 are largely parallel** after P1.
- **P6 is the gate** between "we have parts" and "we have the single-shadow product."
- **Arc II is strictly after Arc I** (it needs two organs + Architect + campaign).

## Captain-visible milestones

| After | What changes for the captain |
|-------|------------------------------|
| **P1** | Work lives in one living campaign tree per project; you can drop in future work as a branch that persists — the living backlog, not static docs. |
| **P2** | "We should also do X" just *works* — filed in the right place, complex asks become sub-objectives, trivial ones don't — and you can see the Architect's reasoning for every call. |
| **P3** | You **talk** to a shadow while it works — clean chat, separate execution timeline. "What are you doing?" gets a real answer. |
| **P4** | A shadow *remembers the project*, not just the task — its next objective builds on its last. |
| **P5** | The app looks designed — flat, sharp, coherent — instead of a utilitarian dev tool. |
| **P6** | **The full single-shadow product.** Summon, talk, command, watch, remember — and it feels like one capable agent. |
| **P7** | Put a crew on one objective; see who's doing what; hand work between shadows. |
| **P8** | Command the room or DM one operative; shadows coordinate with each other; spin up group chats. |
| **P9** | "You three, do this" — the Architect splits and assigns; you steer through the lead. |

## Decisions (locked) & the one open unknown

**Locked:**
- **Two top-level views — Agents (who) and Projects (what).** Two projections of the one stream; pivot between them. Agents view = talk to / manage a shadow (its chats, stats, self-memory, history; the just-ask home; calm, no execution firehose). Projects view = campaign, objectives, watch-the-work, the Architect. Memory scopes map onto the views: self → Agents, project → Projects, captain → global. This split is what de-crams the app.
- **The Architect is a planning-specialist shadow** (one per project), with two modes: a *background* intent engine (absorbs MON-82 as Tier-1 triage, places/decomposes captured work, reasoning always visible) and a *foreground* **planning conversation**. It's a shadow in Agents view and is openable from the objective window in Projects view.
- **Plan-then-implement is the core loop.** Talk through the job with the Architect → objectives branch → dispatch a shadow. **Dispatch is manual** (the Architect proposes/assigns; the captain launches). The direct-to-shadow path stays for quick work — planning is never mandatory.
- **Eager creation, lazy curation.** A meaningful turn creates the objective immediately; the Architect merges / re-places / abandons later.
- **Objectives are continuable, not session-bound.** An explore/continue-objective tool lets a fresh session or another shadow pick up an objective from its dossier.
- **Project-less "just ask" = ephemeral chat by default,** promoted into a per-captain **scratch campaign** only if it becomes real work.

**Open unknown:**
- **Pi concurrency.** The two-session-per-agent spike (P3) is the one real technical risk; everything in Arc II rides on it. De-risk it first thing in P3.

## What this roadmap is not

A schedule (no dates), a complete ticket list (tickets get filed when a phase opens, scope locked in `thoughts/plan/MON-{N}.md`), or final. The **two-arc shape** (single shadow → fleet) and the **phase-rule** are the load-bearing claims; phase boundaries will move.
