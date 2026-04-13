# MON-66 — Archive shadows: implementation notes

PR: https://github.com/whoislikemiha/monarch/pull/51

## What was implemented

Archive as a first-class lifecycle state on `agents`. Clicking X on a shadow opens a confirm dialog; accepting kills the sidecar, marks the row archived in SQLite, and removes it from the sidebar. An Active / All segmented toggle at the top of the sidebar controls whether archived shadows are surfaced (italic + dimmed, inline with active ones). Right-clicking an archived shadow exposes a "Summon back" item that clears the archive stamp. Right-clicking any shadow also exposes a separate red "Delete permanently" item with its own confirm dialog — the only UI path that permanently removes rows.

Nothing else in the app had to move. `killAgent` still exists and is used unchanged by `restartAgent` and exit handlers; archive is a strictly additive lifecycle layered *around* it.

## Key decisions

- **Timestamp column, not boolean**. `archived_at TEXT NULL` preserves *when* — useful for sort-order in the All view and for any future "recently dismissed" UX. NULL means active.
- **One Tauri command with a flag** for the list query (`db_get_agents(include_archived)`), not two separate commands. Default false keeps existing untyped `invoke("db_get_agents")` callers working; regenerated bindings made frontend migration mechanical.
- **Archive state survives upserts by omission**. `upsert_agent_internal` doesn't list `archived_at` in its INSERT columns or ON CONFLICT UPDATE clause, so new rows default to NULL and existing rows keep whatever they have. No defensive coding needed at call sites.
- **Imperative callback, not `$effect`, for the toggle**. Svelte 5 runes would have registered `activeTabId`/`openTabs` reads (inside `loadSavedAgents`) as dependencies of any effect that called it — creating a feedback loop. `setSidebarShowAll(next)` invoked from the sidebar keeps the reactivity graph clean.
- **Split `loadUiState` into `loadUiPrefs` + `loadTabState`**. Prefs (including `sidebarShowAll`) need to restore *before* agents load so the filter matches the user's last toggle; tab validation needs agents to already be loaded. Neither fit one function cleanly.
- **Summon back does not require confirmation**. Unarchive is trivially reversible (just dismiss again); adding a dialog would be friction. Permanent delete does require a separate, irreversibly-worded dialog.
- **Permanent delete surfaced on both active and archived shadows**. Hiding it behind archive would make it harder to discover without making it safer; the separate red styling + independent confirm dialog carries the warning.
- **Reusable `ConfirmDialog.svelte`**. The codebase had several bespoke modals (Settings, Spawn, Extension) but no shared confirm primitive. Added a minimal one — overlay + `role=dialog`, Escape cancels, Enter confirms, `danger` variant styles the button red.

## Files touched

### Rust

- `src-tauri/src/db.rs` — ALTER migration, `AgentRow.archived_at`, `map_agent` row 14, `get_agents_internal(include_archived)`, new `archive_agent_internal` / `unarchive_agent_internal`, new `db_archive_agent` / `db_unarchive_agent` Tauri commands.
- `src-tauri/src/agent.rs` — `archived_at: None` in the two `AgentRow` construction sites (spawn + ensure-exists fallback).
- `src-tauri/src/lib.rs` — registered new commands in both the specta builder and the `generate_handler!` macro.
- `src-tauri/src/ws.rs` — mirrored `db_get_agents` (with the new flag), `db_archive_agent`, `db_unarchive_agent` in `dispatch_command`.

### Frontend

- `src/lib/bindings.ts` — regenerated (auto).
- `src/lib/types.ts` — `Agent.archivedAt?: string`.
- `src/lib/ConfirmDialog.svelte` — new reusable confirm modal.
- `src/App.svelte` — `sidebarShowAll` state + persistence, split UI-state loaders, `AgentDbRow.archivedAt`, `loadSavedAgents(includeArchived)` with archive-aware tab/active fallback, `requestDismiss` / `requestDelete` / `summonAgent` / `confirmPending`, two `<ConfirmDialog>` instances (dismiss + delete), new sidebar props wired.
- `src/lib/Sidebar.svelte` — `showAll` prop + `ontoggleshowall`, `ondismiss` / `ondelete` / `onsummon` callbacks replace the old `onkill`, Active/All segmented toggle in the header, `class:archived` styling distinct from `.standby`, inline Summon-back button replaces X on archived rows, context menu expanded with Summon back (conditional) + Save as template + divider + red Delete permanently.

### Planning docs

- `thoughts/plan/MON-66.md` — research plan + locked decisions + 3-phase split (Phase 3 folded into Phase 2 during implementation).

## What was left out

- **Bulk archive / multi-select** — not needed for the "stop seeing old shadows" use case.
- **Auto-archive of long-idle agents** — behavior change the user didn't ask for; would need heuristics and a discoverability story.
- **Archive search / date filters** — premature until enough archived shadows exist to warrant it.
- **ONBOARDING.md / CLAUDE.md updates** — not blocking, intentionally deferred.
- **Keybinding for the Active/All toggle** — worth adding later if the toggle gets used often.
- **Rebase note**: this work was developed before PR #48 (only-restore-open-tabs) and PR #50 (my MON-57 avatar work) merged to master. Both were absorbed during the final rebase; my `loadSavedAgents` rewrite is a strict superset of PR #48's intent, and the sidebar template now uses the `ShadowAvatar` grid layout from MON-57 with archive-aware extras layered on.
