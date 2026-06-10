# Monarch — Element Inventory (for hand-crafting the design)

What each surface must *contain* — the content + data, not the layout. You bring the hierarchy/layout;
reuse the foundation atoms (`prompt-pack.md` Prompt 0/0b); this maps onto the real system
(`roadmap-v2.md` + the current schema). Arc I (single-shadow) surfaces; Arc II additions flagged so you
leave room.

## Two views (so the drawing is one system)

Two top-level views — two projections of the same stream — that you pivot between:

- **AGENTS view (who).** Your shadows + groups (incl. the **Architect** shadows). Open a shadow → its chats, a calm digest of what it's up to, stats, **self-memory**, history. The home for *just ask / chitchat*. **No live execution firehose here** — that's what de-crams it.
  - Holds: the agent roster/groups · **agent conversation** (un-scoped DM) · **S7** stats/identity · **S5** self-memory · history. Talk to the **Architect** here too (it's a shadow).
- **PROJECTS view (what).** Your projects + campaigns. Open a project → its campaign tree, objective detail, **watch the executor**, **project-memory**, and the **Architect planning conversation** (opened from the objective window). Where work gets planned and done.
  - Holds: **S2** campaign tree · **S3** objective detail · **S1** the dual surface (objective-scoped shadow chat + execution) · **S4** the Architect (planning conversation + reasoning trail) · project-memory · **S6** context.

Navigation: a shadow → its objectives (jump to Projects); an objective → the shadow on it (jump to Agents). **Memory scopes map onto the views:** self → Agents, project → Projects, captain → global. Inspectors open as panels within a view. The conversation references objectives via compact inline cards that link across.

---

## Frame (persistent chrome — draw once)

- **Top bar:** brand · current-context breadcrumb (project › shadow › objective) · **Fleet⇄Board toggle** · ⌘K command · theme switch · captain chip.
- **Fleet rail (left, collapsible to slim):**
  - header: "Fleet" · active count · Active/All filter · **Extract** (new shadow).
  - project groups (collapsible): name · count.
  - **shadow row:** avatar (rank ring + presence pip) · name · rank chip (E–S) · one-line status ("verifying the queue lock…") · spend · stop control when working.
- **Workspace:** the active surface (S1–S8 below).
- **Inspector access:** right rail of icons → opens Memory / Context / Architect / Stats / Identity.

---

## S1 — Solo workspace (the dual surface) ★ the heart

Two regions, **weighted** (conversation primary, execution a calmer companion — not 50/50).

- **Shadow header (slim):** avatar · name · rank · live status · spend. *(Secondary/popover: codex id, branch, current objective, memory count — keep them OFF the main strip.)*
- **Conversation (primary, wide, calm):**
  - message stream: captain turns + chat-shadow turns; quiet timestamps.
  - **inline "work spawned" card** (compact): objective title · #id · "placed by the Architect" · *Watch →* link. Not a full panel.
  - optional **Architect note** (compact, collapsible): "classified as refactor → filed under X."
  - **composer:** input · target chip (@shadow) · scope chip · model chip · Send / Stop.
  - **executing indicator** when working: "Onyx is executing · plan 4/5" + Stop.
- **Execution (companion, calmer, expandable):**
  - **NOW:** current action · running/paused state · pause control · "on task" timer.
  - **working-memory chips** (current files / focus) — secondary.
  - **PLAN** (collapsible): ordered plan items · status each (done / now / next).
  - **EXECUTION stream** (default: NOW + last 2–3 steps; expand for full): coherent actions → nested tool calls → outcomes / decisions. Use foundation event icons.
- **Arc II room:** leave space for a crew roster + a channel switcher (this surface is the collapse-to-solo case of the crew window).

---

## S2 — Campaign / Board (the objective tree)

- **Campaign header:** project · campaign root title · overall progress (e.g. n done / m total) · **add objective** (cheap capture).
- **Objective tree row:** disclosure ▸ · status (shape+color+label) · grade chip · title · assignee avatar (if any) · trailing meta (timestamp / child count). Deep indent + guide.
- **State mix to show:** pending/**planned** (first-class, not greyed-afterthought), in-progress, blocked, done.
- **View filter (optional):** all / active / planned / done.
- **Empty state:** no objectives yet → invitation.
- Row → S3.

---

## S3 — Objective detail

- **Brief:** title · status · grade · scope · current direction · rationale (editable) · created/started/completed.
- **Plan:** ordered plan items + status (the *intended route* — visually distinct from execution).
- **Notes & artifacts:** notes · refs (file / url / pr / issue / artifact) with type + target · add affordance.
- **Execution (this objective):** its timeline (link to / embed the S1 execution stream).
- **Continue / explore:** an affordance to pick up this objective in a fresh session (continuation rides the dossier, not session ancestry).
- **Report (at close):** summary · outcome · decisions+rationale · learned · artifacts · open threads · reflection · grade — as a readable artifact.
- **Arc II room:** crew / assignees / roles block.

---

## S4 — The Architect ★ (a planning-specialist shadow, one per project)

Two faces of the same entity:
- **Planning conversation** (foreground — the "talk through the job" surface). Reachable as a shadow in **Agents view** and opened from the **objective window in Projects view**. Elements: message stream · **proposed-objectives preview** (titles + where each would branch in the campaign) · accept / edit / **manual dispatch** controls (it proposes; you launch the shadow).
- **Reasoning trail** (background — its ambient decisions made visible). Header + scope toggle (recent / this objective). Entries (chronological, quiet, scannable): input/trigger · **Tier-1 classification** (chitchat / question / work · complexity) · **Tier-2 decision** (created · placed-under-X+dedupe · decomposed-into-N · skipped) · **rationale** · link to the objective. Reads like a chief-of-staff's notes, not a debug console.

---

## S5 — Memory (the Keeper)

- **Header:** memory count · **scope filter (self / project / captain)** · refresh.
- **Memory tree:** grouped by scope · row = title + kind badge · nesting · superseded shown.
- **Detail pane:** title · scope/kind badges · summary · content · **provenance** (source objective · source events · created · last-accessed · recall count · embedding model · supersede chain).
- **"Just learned: <claim>"** transient notice (optional).

---

## S6 — Context inspector

- **Fullness gauge:** used / total · headroom % · healthy → warning → critical.
- **Composition breakdown:** proportional/segmented by category (setup · conversation · thinking · tool calls · tool results · retrieved memory) — see what dominates before drilling.
- **Category list:** each expandable → entries (label · token size · preview).
- **Secondary stats:** billing total · turns · cache read/write.

---

## S7 — Identity & stats (presence)

- **Identity editor:** captain layer (global) + shadow layer (this agent), each a text area + **token-budget meter** (healthy / over).
- **Stats dossier:** codex name · title · rank/grade · primary specialization · experience bar · lifetime (tokens in/out · cost · objectives · sessions · turns) · specialization bars · tool usage (top tools + error counts).

---

## S8 — First-run / empty / just-ask

- **First run:** hero (extract first shadow) · a few starters · "bridge coming online" tone.
- **Just ask:** a bare conversation with a shadow — NO objective/plan/execution scaffolding (the lightest path).
- **Empty states:** no objectives, no memories — as invitations.

---

## Reuse these atoms (already in the foundation — don't redraw)

status dot (shape+color) · grade chip E–S · shadow avatar (rank ring + presence pip) · data row (label|value) · tree/disclosure row + indent guide · badges/chips · meter · code/pre block · event-type icons (action · tool · outcome · decision · plan · keeper · note · blocker) · popover · button set.

## Layout decisions to make as you draw

1. Where the **Fleet⇄Board** toggle lives (top bar vs. rail).
2. Is **execution** always-visible-but-calm, or a **"Watch" toggle** off a primarily-conversation screen? (Most aggressive de-cram.)
3. Are **inspectors** a right rail of panels, or full-screen overlays?
4. Does the **Board** live as a third lens, or as a panel you pop from a shadow?

## Suggested draw order

Frame → **S1 solo workspace** (the heart; settles everything) → S2 campaign → S3 objective detail → S4 Architect → S5/S6/S7 inspectors → S8 first-run.
