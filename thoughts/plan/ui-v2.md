# UI v2 — the command-center shell (plan)

Ticket: TBD (MON-N). Branch: `feat/ui-v2`. Status: planning.

## Context

Monarch's current UI was experimental and rushed — built feature-first, with everything (timeline,
objectives, classifier pills, memory/context/stats inspectors, sessions, a draggable floating "portrait
head") bolted onto a single chat-centric shell (`App.svelte` → `AgentView.svelte` + a right toolbox). It
works but it's cluttered and incoherent.

Meanwhile a full visual language was built (`src/lib/ui/`: tokens in `global.css` + `themes/*`, atom
classes in `styles/atoms.css`, live `Catalog.svelte` at `?catalog`) and **nothing uses it yet** — the
README is explicit: *build new surfaces with this system, do not copy legacy components; legacy gets
deleted as surfaces are rebuilt.*

This is a **full, from-scratch rebuild of the frontend.** We are **not** keeping the old UI, not building
a toggle, and not porting/redrawing legacy markup. We rebuild every surface clean on the design system,
following the vision — sharp, modern, dense, instrument-grade — and **delete legacy components as their
replacements land.** The backend / IPC / data layer does not change: stores, `api.ts`, and the
`bindings.ts` command surface are reused as-is. This is a new component tree + layout system only.

The product is a **command center**: two top-level views — **Agents (who)** and **Projects (what)** — on a
flexible, extensible frame we grow over time (file diffs, git view, war-room, etc.).

### The interaction model (load-bearing — drives the whole design)

- **Solo workspace = 50/50 split: a work _timeline_ and _chat_** (the collapsible fleet sidebar is
  separate, not part of the 50/50). Deliberately *not* chat-primary.
- The **timeline is the agent narrating its own work.** As it runs tools / does work it explains itself in
  rich prose ("writing tests for the queue lock", "checking the timeout path"). Raw tool calls are grouped
  into coherent, narrated actions you can drill into. The narration is what lets you *follow* the work — it
  is the point of the view.
- **Every timeline action is clickable → opens a conversation scoped to that piece of work**, talking to
  the *same agent that did the work* (context/memory injected so the answering agent speaks as the running
  agent).
- **Conversations are cheap and plural.** Open multiple chats, talk to the agent anytime, in any chat.
  Cheapness is the feature — it's what makes the whole thing feel interactive.

### Avatars (no animation)

- **No floating portrait head** — the draggable `AgentPortrait` is gone. Its controls relocate to clean
  homes (shadow header strip + composer chips + a controls menu).
- Avatars are **static images**, used for **visual tracking** — in the fleet rail (rank ring + presence
  dot) and the slim shadow header, so you can spot/track a shadow at a glance. The **full avatar image
  shows on inspect** (identity panel / click), not floating over the chat.
- **No Rive / animated avatar state machines** in this pass.

## Decisions (locked with the user)

| Question | Decision |
|---|---|
| Old UI | **Fully replaced.** No toggle, no coexistence. Rebuild from scratch on the design system; delete legacy components as replacements land. |
| Layout engine | **Zones + pinnable panels** — center surface + right/bottom docks; panels open/close/resize/pin/move; pinned panels keep their agent binding across views. Registry-driven so full drag-split docking can come later. Not dockview in v1. |
| Solo workspace | **50/50 timeline + chat**, with the ability to **open multiple chats**. Fleet sidebar separate + collapsible. |
| Avatars | **Static image, no animation.** No floating portrait. Avatar for visual tracking in rail + header; full image on inspect. |
| Narration | Design the timeline around a **rich per-action narration field**; render a graceful fallback (current_action / tool name) until the sidecar provides it. Backend enrichment is a separate follow-up. |
| Scope (this pass) | **Core flows functional**: shell + panel system + Agents view (roster, timeline+chat solo workspace, spawn) + Projects view (campaign/objective tree, objective detail). Memory/Context/Stats/Identity **rebuilt** as panels. Architect surface **stubbed** (no backend yet). |
| Branch base | Rebase `feat/ui-v2` onto **`origin/master`** once the design-system + `quest→objective` rename are merged (imminent). New UI speaks **"objective"**, not "quest". |

## Pre-req before coding — DONE

The `quest→objective` rename is now on `origin/master` (cherry-picked the 5 p1 commits on top of #101,
pushed 2026-06-11). `feat/ui-v2` is fast-forwarded onto it: design system (`src/lib/ui/`) + tokens +
`objectiveStore` + `dbCreateObjective` bindings are all present. The new UI uses **objective** naming
throughout. No rename churn ahead.

## Architecture

### Boot (`src/main.ts`, `index.html`)

- `main.ts` mounts the **new** `App.svelte` directly — no version routing. The legacy `App.svelte` and its
  tree are deleted/replaced as slices land (the shell first, then each surface). `?catalog` mount stays.
- All stores mount the same as today; no data-layer fork.

### Shell structure

Build clean under `src/lib/` (no `v2` prefix — there's no coexistence). Legacy files are removed as their
replacements ship. Target structure:

```
src/lib/
  App.svelte                   // shell: boot, top bar, view router, panel host, dialogs, notifications
  shell/
    TopBar.svelte              // brand · breadcrumb · Agents⇄Projects toggle · ⌘K (stub) · theme · captain chip
    FleetRail.svelte           // collapsible left sidebar: Active/All filter, Extract, project groups, shadow rows
    ShadowRow.svelte           // static avatar(rank ring+presence) · name · rank chip · 1-line status · spend · stop
    PanelHost.svelte           // renders center surface + right/bottom docks from the layout store
    Panel.svelte               // chrome: title, pin, collapse, resize, close (wraps .panel atom)
  layout/
    layoutStore.svelte.ts      // zones, open panels, sizes, pins; persisted via db_set_ui_state per view
    panelRegistry.ts           // { id, title, icon, component, zones[], view, defaultDock } — like toolbox registry
  views/
    AgentsView.svelte          // who: solo workspace (timeline+chat) for the selected shadow
    ProjectsView.svelte        // what: campaign tree + objective detail + watch/exec
  workspace/
    SoloWorkspace.svelte       // 50/50 split host: TimelinePane | ChatColumn(multiple chats)
    TimelinePane.svelte        // narrated work timeline; coherent actions; click → open scoped chat
    TimelineAction.svelte      // one narrated action: icon · narration · drill-in (nested tool calls) · "ask" affordance
    NowStrip.svelte            // NOW + plan progress + working-memory chips
    ShadowHeader.svelte        // slim: static avatar · name · rank · live status · spend (controls menu here)
    ChatColumn.svelte          // tabbed/stacked multiple chats; "+" new chat; each scoped (global | action | objective)
    ChatThread.svelte          // one conversation: message stream + composer
    Composer.svelte            // textarea · target/scope/model/thinking chips · attachments · @-mention · Send/Stop
    chatStore.svelte.ts        // per-agent open chats; scope = global | {action} | {objective}
    message/                   // freshly rebuilt message rendering: UserMsg, AssistantMsg, ToolGroup, ToolCall
  board/
    CampaignTree.svelte        // S2 objective tree: disclosure rows, status shape+label, grade chip, assignee
    ObjectiveDetail.svelte     // S3: brief · plan · notes/refs · execution · continue · report
  panels/                      // inspectors rebuilt as panels on atoms
    MemoryPanel.svelte ContextPanel.svelte StatsPanel.svelte IdentityPanel.svelte
    ArchitectPanel.svelte      // STUB: empty-state "coming online" (no backend yet)
  ui/                          // typed primitives wrapping atom classes, built on demand
    Button Badge Chip GradeChip StatusDot Avatar DataRow TreeRow Meter CodeBlock Popover EventIcon …
```

Dialogs rebuilt on atoms: Spawn (Extract), Settings, ProjectEditor, EditAgent, Confirm, ExtensionDialog,
HistoryPanel (session switch), PromptEditor.

### Layout = zones + pinnable panels

- **Zones:** `center` (active view's surface), `dockRight`, `dockBottom`. Center is owned by the view;
  docks hold panels from `panelRegistry`.
- `layoutStore` holds, **per view**, which panels are open, their dock, size, pin state — persisted via
  `db_set_ui_state("layout.v2.<view>", json)`. A **pinned** panel keeps its `agentId` binding when you
  switch views (e.g. pin Onyx's memory and it follows you to Projects).
- `panelRegistry` mirrors the toolbox-registry pattern (`src/lib/toolbox/registry.ts`) — stable `id`,
  `title`, SVG `icon`, `component`, allowed `zones`, owning `view`. This is the extension seam: future
  panels (git, diff, war-room) just register here.
- Panels receive an `agentContext` like today's `ToolProps` shape, so inspector logic ports cleanly even
  though the markup is rebuilt.

### Solo workspace (the heart)

- `SoloWorkspace` = resizable 50/50 split: **`TimelinePane`** | **`ChatColumn`**, with `ShadowHeader` slim
  on top.
- `TimelinePane` renders from `liveAgentStore` + `objectiveStore` (events/plan/working-memory). Each
  coherent action is a `TimelineAction`: **EventIcon · narration line · expand → nested tool calls/outcomes**.
  - Narration source: prefer a future `action.narration`; fall back to `current_action` / tool name + args
    today. Render identically so backend enrichment is a drop-in.
- **Click an action → `chatStore.openChat({ agentId, scope: { kind: "action", actionId } })`** → new tab in
  `ChatColumn` seeded with that action's context; the agent answers as itself (same memory).
- `ChatColumn` supports **multiple chats** (tabs): a default un-scoped DM + N scoped chats. New chat = one
  click. Each `ChatThread` is a thin conversation over the existing send path (`sendCommand` /
  `sendPiCommand`), using freshly rebuilt message components in `workspace/message/`.

> **chatStore scoping:** v1 backend has one live Pi session per agent. For this pass, scoped chats **share
> the agent's live session** and inject the scope context into the prompt (cheap, no backend change). True
> parallel side-sessions (attention threads) are a later backend ticket — leave the seam.

### Reused, unchanged

- Stores: `agentStore`, `liveAgentStore`, `objectiveStore` (was questStore), `classifierStore`,
  `notificationsStore`. IPC: `src/lib/api.ts`. Commands: `src/lib/bindings.ts`.
- Logic to lift (not markup) from `AgentView.svelte`: event subscription (`agent-state-{id}` etc.),
  send/abort, session new/switch, extension UI, attachments, @-mention.
- `ShadowAvatar` reused for the static image only (no animation path).

## Build sequence (slices, each independently testable)

1. **Empty shell + boot swap.** New `App.svelte` mounted by `main.ts`; TopBar with Agents⇄Projects toggle;
   empty FleetRail + PanelHost. Theme switch works. Legacy `App.svelte` retired.
2. **FleetRail + Agents view skeleton.** Roster from `agentStore` (project groups, Active/All, Extract→Spawn,
   ShadowRow with static avatar/status/spend/stop). Selecting a shadow shows an empty SoloWorkspace + header.
3. **Solo workspace — timeline.** `TimelinePane` + `NowStrip` from `liveAgentStore`/`objectiveStore`:
   coherent actions, drill-in, narration (+fallback). Read-only, live-updating.
4. **Solo workspace — chat + multiple chats.** `ChatColumn`/`ChatThread`/`Composer` + rebuilt message
   components over the existing send path; default DM streams + aborts; new-chat "+".
5. **Clickable actions → scoped chat.** Wire `TimelineAction` click → `chatStore.openChat(action scope)`;
   seed context; new tab opens; ask about that work.
6. **Panel system + inspectors.** `layoutStore`/`panelRegistry`/`Panel`/`PanelHost`; rebuild Memory,
   Context, Stats, Identity as panels on atoms; pin/move/resize persisted; Architect stub registered.
7. **Projects view.** `CampaignTree` (S2) + `ObjectiveDetail` (S3) from objective/plan/report commands;
   row→detail; breadcrumb updates; "watch executor" reuses TimelinePane bound to the objective's agent.
8. **Polish, dialogs, empty/first-run states.** Rebuild remaining dialogs on atoms; density + focus states;
   verify all 4 themes against `?catalog`; S8 empty/first-run; delete any remaining orphaned legacy files.

Primitives in `src/lib/ui/` are built on demand as slices need them (wrap atom classes per the README).

## Verification

- `npm run build:sidecar && npm run tauri dev`, then:
  - New shell boots directly (no toggle); theme switch persists across restart.
  - Agents view: spawn a shadow, send a prompt, watch the **timeline narrate** while it works; **click an
    action** and ask a question in the spawned scoped chat; open a second chat and talk in parallel.
  - Avatars: static image tracks the shadow in rail + header; full image shows on inspect; no floating head.
  - Panels: open/pin/resize Memory+Context; switch to Projects, confirm a pinned panel kept its binding.
  - Projects view: campaign tree renders objectives with status/grade; open one → detail (brief/plan/notes/
    report); "watch" shows the executor timeline.
- `npx svelte-check` clean. `npm test` green (add `chatStore`/`layoutStore` unit tests following
  `notificationsStore.test.ts` / `questStore.test.ts` patterns).
- Visual QA against `?catalog` across purple/obsidian/midnight/light — no hardcoded hex, no shadows,
  Inter for language / mono only for data.

## Explicitly deferred

Architect backend + real planning conversation (S4), true parallel side-session chats (attention threads),
git/diff/file-inspection panels, war-room, **Rive/animated avatars**, full drag-split docking, Arc II
crew/channels. The shell + registry are designed to absorb them without rework.
