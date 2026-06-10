# Monarch — Visual Direction Prompt Pack v2 (realigned to the Campaign Roadmap)

Supersedes the **surface prompts** in [`prompt-pack.md`](./prompt-pack.md). The **foundation prompts there (Prompt 0 + 0b) are unchanged and still canonical** — the visual *language* didn't change. What changed is what the surfaces are *of*: v1's prompts reskinned the old project→shadow→one-chat app; these target the architecture from `thoughts/design/shadow-cognition/roadmap-v2.md` (Campaign/Objective, two organs, the Architect, channels).

## How to use

1. Run **Prompt 0 + 0b** from `prompt-pack.md` first (already generated once as `~/Downloads/Monarch - Visual Language.html` — reuse that as the foundation in a fresh session).
2. Paste the **Shared Context Block** below (it now describes the real architecture).
3. Then a surface prompt. Same-session prompts inherit the foundation + context — don't re-paste.
4. These are **Arc I (single-shadow) surfaces.** Arc II surfaces (crew, channels, command) are a later batch — see the note at the end. Design Arc I surfaces so they **collapse-to-single but leave room for a crew** (the multi-shadow window is a superset).

Each prompt asks for a self-contained HTML/CSS mockup with realistic data + a short rationale, in the locked house style.

---

## Shared Context Block (v2)

> Paste at the top of any surface prompt if the session doesn't already have it.

```
PRODUCT
Monarch is a desktop "command center" (Tauri v2 + Svelte 5, macOS-first, dark). The user is the CAPTAIN.
They command a fleet of persistent AI agents called SHADOWS (ranked E→S; codex names; lasting memory).

THE MODEL (this is what the surfaces express — get it right):
- CAMPAIGN — a project's single living work-tree. One root per project. It is the backlog + plan + history
  fused: planned, in-progress, and done work all live as nodes in one tree. It replaces static roadmap docs
  and ad-hoc tickets. Unfinished work persists as a branch so nothing is forgotten.
- OBJECTIVE — a node in the campaign, at any granularity (root "ship the app" → leaf "fix the null check")
  and any state (pending/in_progress/blocked/done…). The unit of work. (This is the renamed "quest.")
- SHADOW — a persistent agent you talk to; holds identity + private memory; levels up over time.
- TWO ORGANS — a working shadow has a CHAT-SHADOW (you talk to it; reads the substrate; never touches the
  world) and an EXECUTOR (does the work). The captain ONLY ever talks; the executor is something you WATCH,
  never drive. So the workspace is a clean conversation + a separate execution timeline.
- THE KEEPER — background curator of the shadow's MEMORY tree (distills, merges, supersedes claims).
- THE ARCHITECT — background curator of the WORK tree and the intent engine. Turns what you say into placed
  objectives (classify → place → decompose). It runs behind the scenes but its REASONING IS ALWAYS VISIBLE.
- CHANNELS — a conversation is a participant set (+ optional work-scope): DM a shadow · a project chat · an
  objective "room" (a crew) · a hand-picked group chat. (Multi-shadow channels are Arc II.)

INTERACTION PRINCIPLES (let these shape layout):
- Two lenses you pivot between: FLEET (your shadows — who) and BOARD (campaigns/objectives — what).
- One tagged stream underneath; every "chat" and "timeline" is just a filtered projection of it.
- Objectives are project-owned, discoverable, continuable — their record is a handoff dossier.
- Ceremony scales with complexity automatically: a quick question makes no objective; real work does,
  silently. The captain is never made to file forms.
- Memory scopes: self / project / captain.

CURRENT AESTHETIC — you are EVOLVING the foundation already built (Prompt 0/0b), not inventing fresh.
- Dark, dense, command-center-at-night. Purple system; elevation ladder for depth.
- Inter for everything human-readable; JetBrains Mono ONLY for ids/metrics/paths/code.

CONSTRAINTS (hard):
- NO drop shadows / glows / blurs ever — depth = background elevation + 1px borders + space.
- Restrained radius (~2–6px); no pills/blobby cards; circle only for dots/avatars.
- Themeable via tokens; density is a feature (pro tool), but with real hierarchy and rhythm.
- Status never by color alone (shape + label). Legible contrast; clear focus states.
```

---

## Prompt 1 — Command-center frame (Fleet ⇄ Board)

```
Design the app frame for Monarch, reusing the foundation reference sheet.

WHAT THIS IS
The persistent shell with two lenses the captain pivots between:
- FLEET — your shadows (persistent agents), grouped by project, with live status (idle / working / blocked /
  needs-input) and rank. This is "who can I command / who's working."
- BOARD — the campaigns & objectives (the work). Per project: the living objective tree (planned / active /
  done). This is "what's being worked on."
Plus the active workspace in the center, and inspectors (memory, context, the Architect log) on the right.

THE PROBLEM TO SOLVE
Make Fleet and Board feel like two faces of one console, not two apps. The captain should always know
whether they're looking at WHO (fleet) or WHAT (board), and pivot in one move (a shadow → its objectives;
an objective → its shadow). The streaming/working shadow should be alive in the periphery without stealing
focus.

DELIVER
A self-contained HTML/CSS mockup of the full frame with realistic data (5–6 shadows across 2 projects, one
working). Show the Fleet lens and how the Board lens is reached, the active workspace area framed, the
inspector rail, and a redesigned first-run/empty state ("extract your first shadow"). Annotate the
two-lens navigation and the eye path.
```

---

## Prompt 2 — The shadow workspace: the two-organ dual surface  ★ the heart

```
Design the single-shadow workspace for Monarch — the most important surface. Reuse the foundation.

WHAT THIS IS
You command ONE shadow. The workspace has two organs side by side:
- CONVERSATION (the chat-shadow) — a clean dialogue. You talk; it answers, reads the substrate, and
  dispatches/steers work. NO raw tool calls here — dialogue only.
- EXECUTION TIMELINE (the executor) — what the shadow is actually doing: coherent actions, nested tool
  calls, outcomes, plan progress. This is what you WATCH; you never type into it.
For a solo shadow these are two altitudes of one relationship: talk about the *job* on the left, watch the
*doing* on the right. (The composer sends to the shadow; the chat-shadow routes it.)

KEY BEHAVIORS TO SHOW
- A clean conversation with a couple of turns (a question answered directly; a directive that spawned work).
- The execution timeline live: an active coherent action with nested tool calls, a completed one with an
  outcome, the current plan item. Skimmable as a story (use the foundation's event-icon set).
- A "now" readout — what is it doing this second (from working memory) — answerable at a glance.
- Pause/resume control over the executor.

CONSTRAINTS SPECIFIC TO THIS SURFACE
- The chat must read as conversation, not a log. The timeline carries the mechanics.
- Design it to COLLAPSE TO SOLO cleanly but LEAVE ROOM for a crew later (in Arc II this same surface gains
  a crew roster + per-shadow timelines + channel switcher — don't paint yourself into a corner).

DELIVER
A self-contained HTML/CSS mockup of the dual surface with realistic content, plus the composer (idle +
working/Stop states). Annotate how the two organs relate and how the eye moves between talking and watching.
```

---

## Prompt 3 — The Campaign (the Board): the living work-tree

```
Design the Campaign view for Monarch — a project's living objective tree. Reuse the foundation.

WHAT THIS IS
One project = one campaign = one tree. Objectives are nodes at any depth and any state, all visible at once:
PLANNED (pending, sitting in the backlog), IN-PROGRESS, BLOCKED, DONE. This single tree IS the backlog,
the plan, and the history — it replaces roadmap docs and tickets. Unfinished work persists as a branch.

THE PROBLEM TO SOLVE
Make a tree that fuses backlog + active + history readable at a glance: you should see the shape of the
work, what's planned vs underway vs finished, and where the shadow is right now — without it looking like a
file tree or a Jira board. Planned/unstarted objectives must feel first-class (they're how you "never
forget" work), not greyed-out afterthoughts.

KEY BEHAVIORS TO SHOW
- The tree with a realistic mix: a root campaign, a few in-progress objectives (one with the active shadow
  on it), several pending/planned branches, a couple done, one blocked. Status by shape+color+label.
- A low-friction "capture future work" affordance — drop a new pending objective into the tree ("we should
  also do X") that just persists.
- Drill from an objective into its detail (Prompt 4) and into its execution.

DELIVER
A self-contained HTML/CSS mockup of a campaign tree with realistic objectives across states, the deep-tree
indent done right (the foundation's tree atom, generalized for depth 3–5), and the capture affordance.
Annotate how planned vs active vs done read distinctly.
```

---

## Prompt 4 — Objective detail (scope · plan · notes · report)

```
Design the objective detail surface for Monarch (opens from the campaign tree). Reuse the foundation; keep
it consistent with the campaign and timeline.

WHAT THIS IS
One objective, opened up:
- BRIEF — title, status, grade, scope, current direction, rationale (captain-editable).
- PLAN — the intended route: ordered plan items with status (the "what it means to do"), distinct from the
  execution timeline (the "what actually happened").
- NOTES & ARTIFACTS — notes, external refs (file / url / pr / issue / artifact), all visible.
- REPORT — the shadow's first-person retrospective when the objective closes (summary, outcome, decisions +
  rationale, learned, artifacts, open threads, reflection, grade). A reward for finishing.

THE PROBLEM TO SOLVE
Editable fields must be obviously editable (v1's forms looked like raw HTML). The intended PLAN and the
actual TIMELINE must read as two different things, not be conflated. The REPORT should feel like a polished
artifact, not a dense dump.

DELIVER
A self-contained HTML/CSS mockup showing an in-progress objective (brief + plan + notes/refs) and a finished
one (with its report rendered). Annotate the plan-vs-timeline distinction and the editing affordances.
```

---

## Prompt 5 — The Architect: glass-walled intent engine  ★ new

```
Design the Architect surface for Monarch — there is no precedent for this; design it from the model. Reuse
the foundation.

WHAT THIS IS
The Architect is the background brain that turns what the captain says into placed objectives. It runs
behind the scenes, but its REASONING IS ALWAYS VISIBLE — this surface is that glass wall. It has two tiers:
- TRIAGE (cheap, every turn): chitchat / question / work · complexity.
- REASONING (only when it's "work"): where the objective goes in the campaign tree, dedupe vs existing
  nodes, blocks/dependencies flagged, and decomposition of a complex ask into sub-objectives.
Every decision carries a rationale.

THE PROBLEM TO SOLVE
Make the captain TRUST it by making its thinking legible — especially while calibrating. Show, for a recent
stretch of input: what it classified each turn as and why; when it created/placed an objective and the
rationale ("related to #O-14, same module — filed as a child, not a new root"); when it decomposed and how;
and when it SKIPPED (a question → no objective). This should feel like reading an attentive chief-of-staff's
notes, not a debug console — quiet, scannable, trustworthy.

DELIVER
A self-contained HTML/CSS mockup of the Architect's reasoning trail with realistic entries (a few triage
calls, one placement with rationale, one decomposition into sub-objectives, one skip). Show how a rationale
links to the objective it created. Annotate how it stays calm and legible rather than noisy.
```

---

## Prompt 6 — Memory (the Keeper): self · project · captain

```
Design the memory surface for Monarch. Reuse the foundation.

WHAT THIS IS
A browse/inspect view over the shadow's structured memory — atomic claims the Keeper distills, in a tree by
SCOPE: self (this shadow's private knowledge), project (shared across the project), captain (your standing
preferences). Each memory has provenance: source objective, source events, freshness, recall count,
supersede chain.

THE PROBLEM TO SOLVE
Make it feel like a shadow's evolving knowledge, not a database browser. The three scopes must be legible.
Provenance should be traceable and trustworthy (where did this come from, how fresh, how often recalled).
Convey "this shadow knows things." A quiet "just learned: <claim>" notice when a memory forms.

DELIVER
A self-contained HTML/CSS mockup: a memory tree across the three scopes (some nested, some superseded) and a
rich detail view for one memory. Design the scope system and provenance presentation. Leave room for future
edit/archive/promote. Annotate decisions.
```

---

## Prompt 7 — Context inspector (the shadow's live window)

```
Design the context inspector for Monarch. Reuse the foundation. (This surface exists today as a flat table —
redesign it.)

WHAT THIS IS
A live, honest accounting of what's in the shadow's context window right now: a fullness gauge + headroom,
and a breakdown by category (setup / conversation / thinking / tool calls / tool results / retrieved
memory), each drillable into entries with token sizes.

THE PROBLEM TO SOLVE
Turn a spreadsheet into an instrument panel. A glanceable "how full / how close to the limit" with a
healthy→warning→critical system, and a visual COMPOSITION of what's eating the window (proportional/segmented)
so you see at a glance that, say, tool results dominate — before drilling in.

DELIVER
A self-contained HTML/CSS mockup with realistic near-full numbers, the fullness gauge, the composition
breakdown, and the drillable categories. Show healthy and near-critical states. Annotate decisions.
```

---

## Prompt 8 — Shadow identity & stats (presence)

```
Design the shadow identity + stats surfaces for Monarch — where a shadow feels like a character you've
leveled up. Reuse the foundation.

WHAT THIS IS
- IDENTITY editor — the system-prompt identity: a captain (global) layer + a shadow (this agent) layer, each
  with a token-budget meter.
- STATS — a character/unit card for one shadow: codex name, title, rank/grade, specializations, an
  experience bar, lifetime feats (tokens, cost, objectives worked), tool usage.

THE PROBLEM TO SOLVE
Lean into the persistent-shadow identity (rank, specialization, experience) so it reads like a character
sheet / operative dossier — while staying legible and data-honest. The identity editor needs a clear,
satisfying token-budget meter (healthy + over states).

DELIVER
A self-contained HTML/CSS mockup of both: the identity editor (captain + shadow layers, budget meter) and a
stats dossier for a named, ranked shadow. Annotate decisions.
```

---

## Prompt 9 — First-run, empty & "just ask"

```
Design the cold-start and edge states for Monarch. Reuse the foundation.

WHAT THIS IS
- FIRST RUN — no shadows yet: extract your first shadow and point it at a project. Set the tone (command
  center coming online).
- "JUST ASK" — talking to a shadow with no project and no objective: a quick question that stays ephemeral
  conversation (no campaign, no timeline). The lightest possible path.
- EMPTY STATES for the main surfaces (no objectives yet, no memories yet) as invitations, not dead ends.

DELIVER
A self-contained HTML/CSS mockup of the first-run hero, the bare "just ask" conversation (no work scaffolding
around it), and 2–3 empty states. Annotate how the app scales from "ask one thing" up to "run a campaign"
without the lightweight path ever feeling heavy.
```

---

## Suggested run order

Foundation (0/0b — done) → **1 frame** → **2 dual surface** (the heart — settle it early) → **3 campaign** → **4 objective detail** → **5 Architect** → **6 memory / 7 context / 8 identity** → **9 first-run/just-ask**.

## Arc II surfaces — a later batch (design when P7/P8 open)

Not now (they need the multi-agent model built), but flagged so Arc I leaves room:
- **Objective room (crew):** the objective window with a crew roster + roles, a "who's doing what" tree, per-shadow timelines, click-a-shadow-opens-its-chat.
- **Channels & command:** DM / project chat / objective room / group chat; command altitudes (room via lead, DM direct); the channel switcher; agent↔agent messages.

Design these in the **same language**, and recall the rule from Prompt 2: the single-shadow dual surface is the **collapse-to-solo** case of the crew window — so build it now in a way that can grow a crew roster and a channel switcher without a rewrite.
