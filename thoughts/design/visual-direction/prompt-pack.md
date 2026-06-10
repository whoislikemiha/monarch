# Monarch — Visual Direction Prompt Pack

> ⚠️ **Prompt 0 + 0b (the foundation) below are still canonical.** The **surface prompts (1–8) are
> superseded by [`prompt-pack-v2.md`](./prompt-pack-v2.md)**, which re-aims them at the campaign roadmap
> architecture (two organs, campaign/objective, the Architect, channels) instead of the old chat-centric
> app. Use the foundation here; use v2 for the surfaces.

Prompts for driving a design AI (Claude artifacts, v0, etc.) to produce **visual direction** for each
existing Monarch surface. Goal: stop the app feeling like a raw dev-tool and give it a coherent,
intentional visual language — without losing the information density a power tool needs.

## How to use this pack

1. **Start a fresh design session.** Paste **Prompt 0** first. It establishes the shared visual
   language (tokens, type, component grammar) and produces a foundation artifact. Everything else
   inherits from it.
2. **In the same conversation, paste a feature prompt** (Prompts 1–8). Because the language is already
   set, each mockup comes out consistent with the others.
3. **Iterate by replying** — "make the timeline denser", "try a warmer accent", "show the empty state",
   etc.
4. If you must start a surface in a *fresh* session, paste the **Shared Context Block** (below) above
   that feature's prompt so it has the product + baseline context.

Each prompt asks for a **self-contained HTML/CSS mockup** (Claude can render it as an artifact) with
realistic dummy data, plus a short rationale. If your tool can't render HTML, change the ask to "a
detailed visual spec + annotated layout."

---

## Shared Context Block

> Paste this at the top of any prompt if you're starting a session without Prompt 0.

```
PRODUCT
Monarch is a multi-agent desktop "command center" (Tauri v2 + Svelte 5, macOS-first). The user is the
"Captain." They command a fleet of autonomous AI coding agents called "shadows." Each shadow works on
"quests" (tasks), forms structured "memories," and reports back. There's a distinct flavor — shadows are
"extracted" and "summoned," they have codex names, titles, and grades (E → D → C → B → A → S), and a
background process called "the Keeper" distills their memories. Think *operator's console / commander's
bridge*, not generic SaaS dashboard. It is a serious power tool used for hours at a time.

CURRENT AESTHETIC (the baseline you are evolving, not discarding)
- Dark, default theme is deep purple-black: app bg ~#120d1f, panels ~#171126 / ~#201734, sidebar ~#0c0816.
- Primary accent: bright purple #be95ff (hover #d5bbff). Secondary accent: cyan-blue #33b1ff.
- Status colors: success #42be65, warning #ffe97b, error/pink #ee5396.
- Text: near-white #f2f4f8 primary, #dde1e6 secondary, #8f7aa8 muted (purple-gray).
- Fonts: Inter for body; JetBrains Mono for labels/data/code. CURRENTLY monospace is overused — almost
  every label, button, and value is monospace, which makes it feel like a terminal.
- Tokens are CSS custom properties (no Tailwind, scoped per-component styles). Themeable: purple,
  obsidian, midnight, light variants exist.
- Spacing is tight (4–16px), radii 4–8px, lots of 1px subtle borders, small fonts (9–11px everywhere).
- Honest assessment: dense, functional, utilitarian, visually flat and a bit ugly. No real hierarchy,
  no motion, forms look like raw HTML, everything is the same size and weight.

CONSTRAINTS
- Desktop, dark-first. Density is a feature — this is a pro tool, do not turn it into an airy marketing
  page. But introduce hierarchy, rhythm, and intentional typography so density reads as "rich," not "cramped."
- Must stay themeable via design tokens (define a token set; don't hardcode).
- Keep the purple/shadow identity. You may refine the palette but keep the moody, focused, "command
  center at night" feeling.
- NO drop shadows / box-shadows / glows. Build depth from background elevation, 1px borders, and spacing
  alone. Surfaces stay flat and crisp — instrument-grade, not "card floating on a page."
- Restrained border-radius. Small, uniform radii only (~2–6px). No pill/fully-rounded containers, no big
  blobby rounded cards. Sharp and precise. (The only acceptable exception is a true circle for dots/avatars.)
- Accessibility: legible contrast, clear focus states, don't rely on color alone for status.
```

---

## Prompt 0 — Visual language foundation

> Run this FIRST. It produces the system every other prompt builds on.

```
You are a senior product designer establishing the visual language for Monarch.

[Paste the Shared Context Block above.]

TASK
Before designing any screen, define the foundation. Deliver ONE self-contained HTML/CSS artifact that is
a "visual language reference sheet" for Monarch, containing:

1. PALETTE — refine the dark purple system into a deliberate set of design tokens (backgrounds at 3–4
   elevations, text at 3 weights, primary/secondary accents, 4 status colors, border + overlay tokens).
   Show swatches with token names and hex. Keep the command-center-at-night mood.
2. TYPOGRAPHY — a type scale (display, title, body, label, mono-data) with sizes, weights, line-heights.
   Critically: pull body/labels OUT of monospace into Inter; reserve JetBrains Mono ONLY for data, IDs,
   code, and metrics. Show the scale rendered.
3. SPACING & RADIUS — a spacing scale and a deliberately SMALL radius scale (~2–6px max, one or two
   steps). No pill-rounding on containers, no large rounded cards. Show as a ruler/specimen.
   DEPTH RULE: convey elevation and separation with background tokens + 1px borders + spacing only —
   NO box-shadows, glows, or blurs anywhere. Flat and crisp is the house style.
4. CORE COMPONENT GRAMMAR — render the recurring atoms the rest of the app needs, all in the new language:
   - buttons (primary / ghost / danger / icon), inputs, textareas, selects (focus states shown)
   - badges & chips (status, grade E–S, kind/scope tags)
   - a "status dot" system, an avatar treatment for shadows
   - a card / panel container, a section header, an empty-state pattern
   - a small progress/meter bar
5. MOTION — one short paragraph: where subtle motion belongs (state transitions, streaming, expand/collapse).

Make it beautiful but serious. Annotate the decisions briefly. This sheet is the contract; later screens
must reuse these exact tokens and components.
```

---

## Prompt 0b — Extend the foundation (dense-screen atoms)

> Paste this in the SAME session, right after Prompt 0. The inspector + timeline screens are built almost
> entirely from patterns Prompt 0 didn't specify; this adds them so the dense surfaces stay coherent.

```
Good. Now EXTEND the same reference sheet — reuse the EXACT tokens, type scale, and rules you just
defined (no shadows/glows, small radius, Inter for language / JetBrains Mono only for data). Do not
restyle anything you've already made; ADD these missing atoms as new specimens, because Monarch's
densest screens (the inspectors and the quest timeline) are built almost entirely from them:

1. DATA ROW (label | value) — the workhorse of every inspector (Context, Memory provenance, Stats). A
   tight key/value row: muted Inter label left, value right. Show variants in one stack: plain value,
   mono value (ids/tokens/paths), value-as-status-badge, value-as-meter. Put a "section label + 1px
   rule" group header above them. Dense but legible at the compact end of the scale.

2. DISCLOSURE / TREE ROW — for the quest tree and memory tree. A collapsible row: chevron (rotates on
   expand), optional leading icon, title, trailing metadata (timestamp/count). Nested children indented
   with a visible 1px indent guide. Show 2–3 levels, both a collapsed and an expanded branch.

3. POPOVER / TOOLTIP — a small floating panel using --bg-overlay + 1px border, NO shadow. Show one as a
   key/value detail card (the hover-popover on a pill) and one as a one-line tooltip. Flat, with an
   optional small caret.

4. CODE / PRE BLOCK — monospace block for tool args/results and memory content: --bg-sink surface, 1px
   subtle border, small radius, comfortable line-height. Include a long-content variant that collapses
   to "show more", since these get long.

5. ICON GRAMMAR — define the icon system (target ~14px, stroke weight, outline style, on a small grid).
   Then render a STARTER SET for the timeline event types that replaces the current Unicode glyphs
   (◆ ◇ ● ◈): coherent-action, tool-call, outcome, decision, plan-change, keeper/memory-tick,
   manual-note, blocker. PRINCIPLE: event types differ primarily by ICON SHAPE, not by new colors —
   reuse existing status/accent tokens sparingly and default to neutral, so we never introduce a third
   color language that collides with the status and grade hues.

Then make two small token adjustments and note them in-line:
- Nudge --text-muted slightly lighter so it clears 4.5:1 on --bg-raised and --bg-overlay at 10–11px
  (it currently dips below AA on those raised surfaces).
- Confirm --border-subtle actually separates adjacent elevations; bump it a hair if it disappears.

Finally, add a "compact density" note: a data-density="compact" scope (or a tighter spacing step) for
inspector panels only ~300px wide, so these atoms stay tight there without shrinking type below
legibility.

Keep everything flat, crisp, token-driven, and seamless with what you already built — these should look
like they were always part of the same sheet.
```

---

## Prompt 1 — App shell / command-center frame

```
Design the overall app frame for Monarch, reusing the visual language from the reference sheet.

WHAT THIS IS
The persistent shell that holds everything: a top/left agent roster, a center workspace, and a right-hand
"toolbox" of inspector panels. The Captain lives here all day, switching between shadows.

CURRENT ANATOMY (to redesign, not copy)
- A horizontal, scrollable "roster" rail (currently ~160px tall) listing shadow "pills" grouped by
  project. Each pill: 28px avatar, codex name, running cost, a one-line status, a dismiss/summon button.
  A streaming shadow shows a pulsing dot + a stop button. There's a brand wordmark "Monarch", an
  Active/All filter toggle, and a "+ New" (extract a shadow) button.
- Center: the active shadow's workspace (chat). Empty state today is just ">_  Extract a shadow to begin".
- Far right: a 44px vertical icon rail ("ToolRail") of inspector tools, with a resizable panel stack
  ("ToolPanelStack") that opens to the left of it. Each tool panel has an uppercase title + close ×.

THE PROBLEM
It reads as three disconnected gray strips. No sense of "command center," weak hierarchy between the
active shadow and the rest, the roster is cramped, and the empty state is bleak.

DELIVER
A self-contained HTML/CSS mockup of the full frame with realistic data (5–6 shadows across 2 projects,
one streaming, one idle, one archived). Show:
- the roster as a fleet you command (presence, status, project grouping, the streaming shadow standing out)
- the toolbox rail + an open panel, framed coherently
- a redesigned empty/first-run state that sets the tone
Use the established tokens/components. Briefly annotate the layout logic and how the eye should move.
```

---

## Prompt 2 — Conversation surface (chat + composer)

```
Design the shadow conversation surface for Monarch, reusing the visual language.

WHAT THIS IS
Where the Captain talks to a single shadow and watches it work. A scrolling message stream + a composer.
A small floating "portrait" control panel anchors in a corner.

CURRENT ANATOMY (to redesign)
- Message stream, gap ~16px. User messages: "You" label + a small complexity "classification pill"
  (colored dot + level + confidence %) + text + optional image thumbnails. Assistant messages: "Agent"
  label + model tag + token count + cost + duration, then markdown content, with a collapsible
  "Thought for Xs" thinking block.
- Tool executions render as grouped items. System status lines ("Agent started"). Inline error/warning
  notices. While streaming: pulsing dot + live duration counter.
- A thin "activity bar" can appear ("doing X…" + event count). An optional plan/action strip shows the
  active plan item + current action intent + last few actions.
- Composer: attach button, auto-growing textarea (40–200px) with @-mention file autocomplete, and a
  Send button that swaps to a red Stop button while streaming. Drag-drop images supported.
- A draggable "portrait" panel (model picker, thinking level, prompt edit, history, compact context,
  new session, corner reposition).

THE PROBLEM
The message metadata is noisy (everything monospace, same weight). User vs assistant turns don't read
distinctly. Streaming, thinking, and tool calls all compete. The composer is plain. It feels like a log,
not a conversation with a capable agent.

DELIVER
A self-contained HTML/CSS mockup of an active conversation with realistic content: a couple of user turns
(one with the complexity pill), assistant turns with a collapsed thinking block and a tool-call group, a
live streaming turn at the bottom, and the composer in both Send and Stop states. Show the portrait
control. Establish a clear visual rhythm for turn types and quiet, scannable metadata. Annotate decisions.
```

---

## Prompt 3 — Execution timeline + quest tree (the centerpiece)

```
Design Monarch's execution timeline + quest tree, reusing the visual language. This is the richest and
most important surface — give it the most care.

WHAT THIS IS
A shadow's work is organized as "quests" (a tree: root quests with sub-quests). Each quest has a live,
nested "event log" — the narrative of what the shadow actually did. The Captain reads this to understand
"what is it doing / what did it do."

CURRENT ANATOMY (to redesign)
- Quest tree: collapsible rows, indented by depth. Each row: disclosure triangle, 18px shadow avatar,
  a colored status dot, a grade chip (E–S), the title, a relative timestamp ("2h ago").
- Expanding a quest reveals a detail panel + the EVENT LOG, a nested timeline of typed events:
  - coherent_action (◆): an intent line ("Fix the off-by-one in parser.rs") + status chip + a link chip
    to its plan item + child count + an inline outcome line. Expands to show its children.
  - tool_call (child): tool name + status + duration + foldable args/result preview.
  - action_outcome (child), executor_decision (child, with rationale).
  - plan lifecycle events (◇): plan created/changed/item started/completed/skipped/blocked.
  - compaction_tick (◈): "Keeper checkpoint" + a "+N claims" pill + summary + run id.
  - memory_suggestion (◇): title + summary + details.
  - manual quest changes (●): scope_change / direction_change / note / blocker / question / answer.
- Nesting is shown with a left border + indentation. Icons are bare Unicode (◆ ◇ ● ◈). Fonts 9–11px.

THE PROBLEM
It's the app's most valuable view but currently reads like a raw log viewer: tiny text, Unicode glyphs,
flat density, no visual distinction between an "action the shadow took," a "decision," a "Keeper memory
event," and a "Captain note." Hard to skim the story at a glance.

DELIVER
A self-contained HTML/CSS mockup of one active quest expanded, with a realistic mixed event log: 3–4
coherent actions (one expanded into tool calls + an outcome + a decision), a plan-item-completed event,
a Keeper compaction tick with claims, and a Captain note. Plus the surrounding quest tree with a few
quests (different statuses/grades). Design:
- a real iconography + color system for event TYPES (action vs decision vs tool vs Keeper vs plan vs
  manual) so the timeline is skimmable
- clear, elegant nesting/indentation for parent→child
- a strong sense of "story over time" (a spine), with quiet metadata
Keep it dense — this is a power view — but give it hierarchy and rhythm. Annotate the event taxonomy
visualization. Show collapsed vs expanded states.

CARRY-IN FIXES from the foundation review — resolve these here, since the timeline is where they bite,
then update the reference sheet to match:
- HEADLINE ACTION ICON: the foundation's "coherent action" glyph is a plain outline circle — too generic
  for the most important event type. Give it a weightier, more evocative mark (target / filled chevron /
  play-like) so an action reads as heavier than the tool_call rows nested under it.
- DIAMOND COLLISION: the diamond currently does double duty — quest-node leading icon AND the "decision"
  event. They co-occur here. Reshape one so no two meanings share a glyph.
- DEEP-TREE INDENT: the foundation's tree indent + 1px guide are hardcoded to ~2 levels. Quest trees nest
  arbitrarily deep (indent scales per depth). Generalize the per-level indent and guide so depth 3–5 still
  reads cleanly.
- PATH / ID TRUNCATION: right-aligned data-row values clip on the LEFT with no ellipsis (a long path loses
  its `src/…` head silently). For path/id values, left-align with head-truncation so the filename/tail
  stays visible.
```

---

## Prompt 4 — Plan panel, quest detail & quest report

```
Design the quest plan + detail + report surfaces for Monarch, reusing the visual language. These live in
the same panel as the timeline (Prompt 3) — keep them consistent with it.

WHAT THIS IS
Three connected pieces a Captain uses to read and steer a quest:

1. ACTIVE PLAN — the durable, ordered execution plan for the current quest ("intended route").
   Currently: header (quest title + "+ Item"); a list of plan items, each = index badge + editable
   title + status chip (pending/active/completed/skipped/blocked) + optional rationale line + action
   buttons (↑ ↓ Start Done Skip Block ×). Reorder is via arrow buttons (no drag handles).
2. QUEST DETAIL / BRIEF — an inline editor: a read-only meta grid (status, grade, exec hint, created/
   started/completed timestamps, creator) + editable fields: status & grade selects, and textareas for
   scope, current direction, rationale, summary, plus a "change rationale" audit field, with Save/Reset.
   Also a REFERENCES panel: typed external refs (linear / github_issue / github_pr / file / url /
   artifact) as rows (type badge + target + delete), with an add form.
3. QUEST REPORT — a read-only first-person retrospective the shadow writes at quest close: outcome badge
   + grade, a summary, then sections: Decisions (each with rationale), Learned, Artifacts (file + role),
   Open threads, and an italic Reflection.

THE PROBLEM
Forms look like raw HTML — editable fields are indistinguishable from read-only text, no validation/
character affordances, plan items feel cramped, reorder-by-arrows is clumsy, the report (a genuinely
delightful artifact) is dense and undersold.

DELIVER
A self-contained HTML/CSS mockup showing: (a) an active plan with ~4 items in mixed states, (b) the quest
brief/detail editor with the read-only meta grid clearly distinct from editable fields, the refs panel,
and (c) a finished quest report rendered as a polished, readable artifact. Make editing affordances
obvious, give the plan items a draggable feel and clear status, and make the report feel like a reward
for finishing. Annotate decisions.
```

---

## Prompt 5 — Memory Inspector

```
Design the Memory Inspector for Monarch, reusing the visual language.

WHAT THIS IS
A browse view over a shadow's structured long-term memory. Memories are atomic "claims" the Keeper
distills, organized in a tree by scope (self / project / captain), each with provenance.

CURRENT ANATOMY (to redesign)
- Header: "N memories" + Refresh.
- Two-pane split (~180px tree | detail): LEFT a tree grouped by scope sections (SELF / PROJECT /
  CAPTAIN) each with a count, memories as rows (title + kind badge), nested children indented. RIGHT a
  detail pane for the selected memory: title + scope/kind/layer badges, then SUMMARY, CONTENT (mono
  block), PROVENANCE (memory id, created, last accessed, access count, source quest, source events,
  embedding model, parent), FILE REFS, and a SUPERSEDES CHAIN of clickable links.
- It's "v0 browse-only" — no edit/archive yet. Empty state mentions a debug insert helper.

THE PROBLEM
It looks like a database browser. The fact that these are a shadow's *evolving knowledge* doesn't come
through. The tree is plain, provenance is a flat key/value dump, and there's no sense of memory as a
living, growing structure.

DELIVER
A self-contained HTML/CSS mockup with a realistic memory tree (a dozen memories across the three scopes,
some nested, some superseded) and a rich detail view for one selected memory. Make the scope/kind/layer
system visually legible, present provenance as something you can trust and trace (source quest, freshness,
how often it's been recalled), and convey "this shadow knows things." Design the empty state as an
invitation, not a debug note. Even though it's browse-only today, leave clear room for future
edit/archive/promote affordances. Annotate decisions.
```

---

## Prompt 6 — Context management (Context Inspector)

```
Design the Context Inspector for Monarch, reusing the visual language.

WHAT THIS IS
A live, honest accounting of what is currently inside the shadow's context window — so the Captain can
see what's taking up space and how close to the limit they are.

CURRENT ANATOMY (to redesign)
- SUMMARY block: "CONTEXT SNAPSHOT" with used/total (e.g. 123.5K / 128K), HEADROOM (free + %), SOURCE
  (live telemetry vs estimated), a thin 4px health bar that changes color by fullness (green/yellow/red),
  plus billing total + cost, turn count, cache read/write.
- CATEGORIES (collapsible): SETUP (custom prompt / project instructions / shadow identity), USER
  MESSAGES, ASSISTANT MESSAGES, THINKING, TOOL CALLS, TOOL RESULTS. Each header shows entry count + a
  token sum; expanding shows entries (label + token count + optional "error" badge + a truncated
  preview that toggles to full text on click).

THE PROBLEM
It's a useful idea presented as a flat spreadsheet. The single thin health bar undersells the most
important info (how full am I, what's eating the budget). No visual "composition" of the window — you
can't see at a glance that, say, tool results dominate.

DELIVER
A self-contained HTML/CSS mockup with realistic numbers (near-full window). Design:
- a strong, glanceable "fullness" indicator and headroom warning system (healthy/warning/critical)
- a visual BREAKDOWN of what's consuming the window (e.g. a stacked/segmented bar or proportional
  treatment by category), so composition is obvious before drilling in
- the expandable category list with quiet per-entry detail
Make it feel like an instrument panel / fuel gauge for the mind, not a table. Show healthy and
near-critical states. Annotate decisions.
```

---

## Prompt 7 — Shadow identity & stats (presence)

```
Design the shadow identity + stats surfaces for Monarch, reusing the visual language. Together these are
where a shadow feels like a *character* you've leveled up.

WHAT THIS IS
Two related inspector panels:

1. IDENTITY editor — edits the system-prompt "identity" injected for shadows. Two sections: CAPTAIN (L1a,
   global, applies to all shadows) and SHADOW (L1b, this agent). Each: a name + a large textarea + a
   "~N tokens" estimate. A shared token-budget bar (combined limit ~3000) that turns warning/over as it
   fills. Save buttons per section.
2. STATS — lifetime dashboard for one shadow: codex name (bold) + title + grade + primary specialization;
   an EXP bar; LIFETIME metrics (tokens in/out, cost, sessions, messages, turns); SPECIALIZATION scores
   as labeled bars (coding/research/testing…); TOOL usage (top tools by call count, with error counts).

THE PROBLEM
The identity editor is a plain form; the stats panel is a decent but flat "formatted JSON" dashboard.
Neither conveys the game-like "this is Igris, an A-grade shadow you've trained" identity the product
leans into.

DELIVER
A self-contained HTML/CSS mockup of: (a) the identity editor with the captain/shadow split and a clear,
satisfying token-budget meter (healthy + over states), and (b) a stats panel for a named shadow that
feels like a character sheet / unit card — grade, specializations, experience, lifetime feats — while
staying legible and data-honest. Lean into the shadow/command-center identity. Annotate decisions.
```

---

## Prompt 8 — Small surfaces: classification pill, notifications, classifier settings

```
Design three small recurring Monarch surfaces, reusing the visual language. Keep them tiny and quiet —
they support the main views, they don't compete with them.

1. CLASSIFICATION PILL — a tiny inline badge next to each user message showing turn complexity: a colored
   dot + level (low/medium/high/critical, or "failed") + confidence %. On click, a small popover with a
   key/value detail grid: complexity, rationale, model, tokens (in/out), latency (and an error row when
   failed). Design the resting pill, the four severity colors, and the popover.

2. NOTIFICATION TOASTS — a fixed top-right stack (max ~5 visible, overflow collapses to "+N more"). Each
   card: a level (error/warning/info) with a left-edge accent + subtle gradient, an optional agent link
   (jump to that shadow), a message, a dismiss ×, and a count badge when coalesced. Auto-dismiss (~5s,
   paused on hover). Design the three levels + the stacked + collapsed states.

3. CLASSIFIER SETTINGS — a plain global config form: an Enabled toggle; PRIMARY provider/model inputs; an
   optional FALLBACK provider/model (revealed by a toggle); a timeout (ms) number field; a read-only
   system-prompt block; and the config file path. Keep it a clean settings form — this is the "calm,
   boring on purpose" surface, but make it feel designed, not raw HTML.

DELIVER
One self-contained HTML/CSS mockup showing all three with realistic data and their key states. These set
the standard for "small components done right." Annotate briefly.
```

---

## Suggested order to actually run these

1. **Prompt 0** (foundation) — non-negotiable first; it's what makes the rest cohere.
2. **Prompt 3** (timeline/quest) — the centerpiece and hardest; settle it early so the language is
   stress-tested against the densest surface.
3. **Prompt 1 + 2** (frame + chat) — the surfaces you look at most.
4. **Prompts 4, 5, 6** (plan/detail/report, memory, context) — the inspector trio you called out.
5. **Prompts 7, 8** (presence + small bits) — polish and identity.

After a few of these land, the recurring components (status dots, grade chips, event icons, meters,
cards) will stabilize into a real design system you can then implement against the existing CSS tokens.
```
