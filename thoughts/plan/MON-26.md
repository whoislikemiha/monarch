# MON-26 — Remove Council mode entirely

## Summary

Council mode is a top-level alternate view that broadcasts one prompt to ≥2 running agents, streams their responses in parallel, and lets the user pick a winner. It was implemented as a dedicated Svelte view (`src/lib/CouncilView.svelte`), a Sidebar toggle (now duplicated across the expanded section and the collapsed rail after MON-15's sidebar refactor), a `Ctrl+L` keyboard shortcut in `App.svelte`, a Rust command `broadcast_prompt` in `agent.rs` (plus a parallel `ws_broadcast_prompt` WebSocket path in `ws.rs`), and two hand-written TS types in `src/lib/types.ts`. This issue deletes every piece of it. The driving motivation is that MON-14 otherwise has to either migrate CouncilView to the new `agent-state-{id}` channel or leave it stuck on the soon-to-be-deprecated raw channel — deleting it narrows MON-14's frontend blast radius to `AgentView.svelte` only, and removes the only second consumer of the raw `agent-event-{id}` stream. The feature is not part of the ongoing product direction, so a clean deletion is preferable to a migration.

## Relevant files and areas

### Frontend — the main Council surfaces
- `src/lib/CouncilView.svelte` — the entire component. Subscribes to raw `agent-event-{id}` per member at `CouncilView.svelte:227-234`, tracks per-member `CouncilResponse` state, calls `invoke("broadcast_prompt", ...)` at `CouncilView.svelte:196`. Delete the whole file.
- `src/lib/types.ts:271-291` — `CouncilSession` and `CouncilResponse` interfaces. Delete the block including the leading `// Council — parallel prompt to multiple shadows` comment. Verify no other file imports either type.

### Frontend — wiring and keybinds
- `src/App.svelte:6` — `import CouncilView from "./lib/CouncilView.svelte"`.
- `src/App.svelte:31` — `let councilMode = $state(false);`.
- `src/App.svelte:65-66` — `let councilAgents = $derived(agents.filter((a) => a.status === "running"));` plus its comment.
- `src/App.svelte:474-478` — `Ctrl+L` keybind branch. Delete the whole `if` block and the comment above it; leave surrounding keybind handling intact.
- `src/App.svelte:539` — `!councilMode && activeAgent && currentLive`. The `!councilMode &&` prefix comes out; the remainder of the condition stays.
- `src/App.svelte:561` — `{councilMode}` prop forwarded to Sidebar.
- `src/App.svelte:565-567` — `oncouncil={() => { if (councilAgents.length >= 2) councilMode = !councilMode; }}` prop forwarded to Sidebar.
- `src/App.svelte:599-603` — `{#if councilMode && councilAgents.length >= 2}` branch that mounts `<CouncilView>`. The whole `{#if ... }<CouncilView .../>{:else}...` branch structure flattens; the `{:else}` content becomes unconditional (or whatever reshape leaves the single-agent path intact). Verify the branch structure after removal is clean — no dangling `{:else}` without an `{#if}`.
- `src/App.svelte:621` — hint text `Ctrl+N extract · Ctrl+B sidebar · Ctrl+L council · Ctrl+1-9 switch`. Strip the `· Ctrl+L council` middle token.

### Frontend — Sidebar (two mount sites now, post-MON-15)
- `src/lib/Sidebar.svelte:24, 28, 36, 40` — `councilMode` and `oncouncil` props in the `$props()` destructure and the type definition.
- `src/lib/Sidebar.svelte:114-115` — **collapsed rail** council button (`class="rail-btn council-rail-btn"`). This is new since the original MON-26 draft was written — MON-15 added a collapsed/icon rail form of the sidebar that duplicates the council toggle there.
- `src/lib/Sidebar.svelte:202-212` — **expanded section** council button (`.council-section` wrapper, `.council-btn`, `.council-shortcut`). Delete the whole `{#if runningCount >= 2 && oncouncil}` block.
- `src/lib/Sidebar.svelte:278-279` (approx) — `.council-rail-btn.active` CSS rule for the collapsed rail button.
- `src/lib/Sidebar.svelte:476-514` — `.council-section`, `.council-btn`, `.council-btn:hover`, `.council-btn.active`, `.council-shortcut` CSS rules.
- Verify after deletion that no dangling `runningCount` computation remains that was only used by the council gate (it might still be used elsewhere — grep before deleting).

### Rust — commands
- `src-tauri/src/agent.rs:762-790` — `#[tauri::command] pub fn broadcast_prompt(...)`. Delete the entire function and its doc comment.
- `src-tauri/src/agent.rs:1120-...` — `pub fn ws_broadcast_prompt(...)`. The WebSocket-path twin of `broadcast_prompt`, not mentioned in the original MON-26 issue draft but found during plan research. Delete the entire function.
- `src-tauri/src/lib.rs:56` — `agent::broadcast_prompt` in the `invoke_handler!` / `tauri::generate_handler!` registration. Remove the line; ensure the trailing commas in the macro invocation stay consistent.
- `src-tauri/src/ws.rs:195-200` — the `"broadcast_prompt" =>` match arm in the WebSocket command dispatcher, which calls `ws_broadcast_prompt`. Delete the whole arm.

### Docs
- `ONBOARDING.md:350` — `│   ├── Sidebar.svelte  — active + saved agents, council toggle` — drop `, council toggle` trailing clause.
- `ONBOARDING.md:353` — `│   ├── CouncilView.svelte  — multi-agent broadcast mode` — delete the whole line.
- `ONBOARDING.md:379` — `let councilMode: boolean = $state(false);` in the state-cheatsheet — delete the line.
- `ONBOARDING.md:477` — `- **Council mode needs ≥2 running agents.** ...` — delete the bullet.
- `ONBOARDING.md:574` — `| lib/Sidebar.svelte | Agent list + saved agents + council toggle. |` — strip `+ council toggle`.
- `ONBOARDING.md:577` — `| lib/CouncilView.svelte | Multi-agent broadcast view. |` — delete the row.
- `FEATURES.md:53` — `- Council mode broadcasting to multiple live agents` — delete the bullet.

### Out of scope (do NOT touch)
- `thoughts/plan/MON-10.md`, `thoughts/plan/MON-12.md`, `thoughts/plan/MON-15.md`, `thoughts/impl/MON-12.md` all reference council mode in their narrative or file lists. These are historical planning artifacts, not executable code. **Leave them unchanged** — editing committed plans is noise and obscures what each plan actually said at its time.
- `thoughts/plan/MON-14.md` already notes that Council is deleted by MON-26 as a prerequisite. No changes needed.

## What needs to change

At the module / concept level.

1. **Delete CouncilView wholesale.** `src/lib/CouncilView.svelte` is removed; `CouncilSession` and `CouncilResponse` types are removed from `src/lib/types.ts`. Any import chain that pulled in either disappears.

2. **Unwire Council from App.svelte.** The `councilMode` state, `councilAgents` derived, `Ctrl+L` keybind, the `!councilMode &&` condition gating the main-panel render, the Sidebar prop wiring, the `<CouncilView>` mount block, and the `Ctrl+L council` hint text token all go. The remaining single-agent render path becomes unconditional. The keyboard hint text is reconstructed without the middle token — double-check the punctuation separators render correctly after the removal (the string uses `&middot;` separators).

3. **Unwire Council from Sidebar.svelte — both mount sites.** Because MON-15 introduced a collapsed rail form of the sidebar, there are now **two** council button mount points to delete: the icon-rail button at `114-115` and the expanded-section button at `202-212`. Both props (`councilMode`, `oncouncil`) come out of the component's `$props()` type. All council-specific CSS (`.council-rail-btn.active`, `.council-section`, `.council-btn`, `.council-btn:hover`, `.council-btn.active`, `.council-shortcut`) goes. Verify `runningCount` isn't orphaned after the deletions — if it was only used to gate the council buttons, remove it too; if it's used elsewhere in the sidebar, leave it.

4. **Delete Rust broadcast commands.** Both `broadcast_prompt` (the Tauri command at `agent.rs:762-790`) and `ws_broadcast_prompt` (the WebSocket twin at `agent.rs:1120-...`) are removed. The registration in `lib.rs:56` comes out of the `generate_handler!` macro. The WebSocket dispatcher arm at `ws.rs:195-200` comes out. After the deletions, the WebSocket command dispatcher must still compile — check that removing the arm doesn't leave an unreachable fallback branch or break the match exhaustiveness.

5. **Clean docs.** Six lines in `ONBOARDING.md` and one in `FEATURES.md` are edited or deleted as listed above. The repo map in `ONBOARDING.md` §4 must render cleanly after `CouncilView.svelte` is removed from the tree — verify the preceding / following lines still make visual sense (no dangling `├──` that should be `└──`).

6. **Verify zero residue.** Final check: `grep -rn -i "council" src src-tauri ONBOARDING.md FEATURES.md` must return zero matches. Not CSS classes, not comments, not variable names, not doc strings. This is the acceptance gate; any match is a missed spot.

7. **Verify builds are clean.** `cargo check` (no dead-code warnings pointing at removed types or commands) and the Tauri dev build (TS compiles, no unresolved imports from the deleted types). The `tauri-specta` integration doesn't exist yet (that's MON-14), so the Rust side is just `cargo check`.

8. **Smoke test.** Run the app with ≥2 running agents. Confirm:
   - No Sidebar council button in either rail or expanded mode.
   - `Ctrl+L` does nothing (or whatever the keybind handler does with the removed branch — verify it falls through cleanly).
   - Single-agent view is unchanged.
   - No runtime console errors from missing imports or undefined props.

## Open questions

None blocking implementation. Minor confirmations to make during the work:

- `runningCount` in `Sidebar.svelte` — check whether it has any remaining callers after the council buttons are removed. If not, delete it.
- Whether the `agents` filter shape used by `councilAgents` (`agents.filter((a) => a.status === "running")`) is useful elsewhere and worth keeping under a different name. Default: no — if MON-26 is the only user, delete it wholesale.
- Whether the app still needs a `Ctrl+L` keybind for something else (currently it's unused beyond council). If not, leave the slot empty; don't repurpose it in this PR.

## Out of scope reminders

- No replacement for Council. No "multi-agent compare" view, no parallel-prompt tool, nothing in the toolbox rail. Complete deletion is the scope.
- No other refactoring of `App.svelte`, `Sidebar.svelte`, or `agent.rs` beyond removing Council references. Leave surrounding layout, keybind handling, and command registration intact.
- No changes to `thoughts/plan/*` or `thoughts/impl/*` files. Historical planning artifacts stay as they were.
- No changes to MON-14's plan or Linear description — it already accounts for MON-26 as a prerequisite.
- No database schema changes. `broadcast_prompt` does not touch persistence.
- No sidecar protocol changes.
- No `Ctrl+L` keybind repurposing — the slot goes dark in this PR.
