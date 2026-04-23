# MON-83 — Quest schema & read-only UI (Slice 2)

PR: https://github.com/whoislikemiha/monarch/pull/73

## What was implemented

The data-model foundation for the Quest system plus a minimal live UI for
creating and inspecting quests manually. This is Slice 2 of the 9-slice MVP
laid out in `plans/quests.md`; it exists so every later slice (Architect,
Steward, EXP, Memory Keeper, Judge) has somewhere to write.

Concretely:

- Two new tables (`quest_nodes`, `quest_events`) and two nullable FKs
  (`messages.quest_id`, `agents.current_quest_id`) added as idempotent
  post-launch migrations. All finite enums are CHECK-encoded at the
  storage layer so Rust and the DB can't drift.
- Seven Tauri commands (`db_create_quest`, `db_update_quest`,
  `db_get_quest`, `db_list_quests_for_agent`, `db_get_quest_tree_for_root`,
  `db_record_quest_event`, `db_list_quest_events`), mirrored as
  WebSocket-bridge match arms.
- Writes broadcast `quest-created-{id}` / `quest-updated-{id}` /
  `quest-event-{questId}` via the shared `WsBroadcast` pipeline so any
  frontend observer stays in sync without polling.
- `create_quest_internal` seeds a `status_change: null → pending` event
  inside the same transaction — the event log is never empty, which is
  what makes the read-only timeline's "event log shows status transitions
  with actor attribution" success criterion hold without exposing any
  status-edit UI.
- New "Quests" toolbox tool: inline-expand tree (no dialog), per-node
  disclosure, status dot + grade badge + assignee avatar + relative
  timestamp. `+ New quest` and per-node `+ Sub-quest` share one form with
  sensible defaults.

## Key decisions

- **Inline expansion instead of a detail dialog.** The user explicitly
  rejected dialogs. The Slice 2 detail content is thin (title, status,
  grade, timestamps, one seeded event), so inline expansion fits now; a
  fuller view can graduate in later slices when Steward/Memory Keeper
  fatten the payload.
- **All code lives in `db.rs`, not a new `quest/` module.** Matches how
  `message_attachments`, `agent_stats`, etc. were added. Keeps the diff
  tight.
- **Command namespace is `db_*`, not `quest_*`.** Matches other DB-backed
  commands; no case for a separate namespace at this scale.
- **User-initiated writes bypass the MON-37 persistence pipeline.** That
  pipeline's contract is "sidecar-originated events"; quest creation is
  user-initiated from the frontend, so it calls `*_internal` directly.
  When the Architect (Slice 3) starts writing quests from sidecar output,
  it can take a different path.
- **`list_quests_for_agent` is assignee-only.** `agents.current_quest_id`
  is a pointer into the tree, not a list filter. Frontend filters the
  returned list to roots (`parentId === null`) and fetches each root's
  full tree separately so sub-quests with different assignees still
  render in context.
- **`emit_event` promoted `pub(super)` → `pub(crate)`.** Non-agent
  surfaces need the dual Tauri+WS emit path. Avoids rebuilding the
  broadcast plumbing in db.rs.
- **UUID v4 quest ids** (matches `agents.id` / `sessions.id`), generated
  server-side if the payload omits them.

## Files touched

- `src-tauri/src/db.rs` — migrations, types, methods, row mappers,
  commands.
- `src-tauri/src/lib.rs` — specta + `generate_handler!` registration,
  forced `QuestRow` type export.
- `src-tauri/src/ws.rs` — dispatch arms for every command.
- `src-tauri/src/agent/mod.rs` + `src-tauri/src/agent/event_handler.rs`
  — visibility promotion on `emit_event`.
- `src/lib/bindings.ts` — regenerated.
- `src/lib/toolbox/questStore.svelte.ts` — new per-agent reactive store.
- `src/lib/toolbox/tools/QuestTimelineTool.svelte` — new tool.
- `src/lib/toolbox/registry.ts` — register Quests at order 20.
- `CLAUDE.md`, `ONBOARDING.md` — update Start Here table, schema gotcha,
  data-model section.

## What was left out

- **Automatic decomposition** — Architect lives in MON-84 (Slice 3).
  Without it, quests only appear when someone clicks `+ New quest`.
- **Drift / dispute handling** — Steward is MON-85 (Slice 4).
- **Fork / worktree lineage rendering** — MON-86 / MON-87.
- **EXP + avatar tier unlocks** — MON-88.
- **Memory Keeper distillation on `done`** — MON-89.
- **Judge escalation** — MON-90.
- **Better `Option<QuestRow>` type export.** Specta inlines the
  anonymous shape for `db_get_quest`'s return even with an explicit
  `.typ::<QuestRow>()`. Functionally fine; skipped a workaround.
- **Event-subscription bookkeeping beyond root-level.** Sub-quest writes
  reconcile by re-fetching the root tree when its root's
  `quest-updated-{id}` fires. Finer-grained per-node subscriptions were
  deferred — the current approach is cheap enough for Slice 2's manual
  tree sizes.
