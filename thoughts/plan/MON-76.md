# MON-76 — @-mention autocomplete for files and folders in prompt inputs

Linear: https://linear.app/monarch-commander/issue/MON-76

## Summary

Typing `@` inside a Monarch prompt textarea should open a keyboard-navigable dropdown of files **and** folders matching whatever the user types after the `@`. Suggestions are drawn recursively from the shadow's working directory (`agents.cwd`), and the user can type `../` as part of the query to reach anything on disk outside that cwd. Arrow keys cycle the list, TAB accepts, Esc closes. Accepting a match inserts `@<relative-path>` into the textarea at the caret.

The feature must be **reusable**: there is currently one live surface (the AgentView message composer in `ChatInput.svelte`), but the same control should drop into any future prompt textarea without duplication. No ticket scope exists here for acting on the reference — inserting the `@path` token into the prompt is the full story; expanding it into file contents is a separate, later concern.

The backend side is a new Tauri command (plus its WebSocket twin) that walks a directory, respects `.gitignore`, skips noisy dirs (`node_modules`, `.git`), caps result count, and returns a deduped list of file + folder entries. The frontend side is a new mention-aware input component that watches for `@` in a textarea, issues the search, and renders the keyboard-navigable dropdown.

## Relevant files and areas

### Prompt textareas (the user surfaces)

- `src/lib/ChatInput.svelte:202` — the only live prompt textarea today. Multi-line, auto-resizing, `handleKeydown` at line 205 already owns Enter/Shift-Enter. This is where the mention trigger must hook in without breaking send/newline behavior.
- `src/lib/SpawnForm.svelte:164-169` — contrary to initial assumption, this is a single-line `cwd` input, **not** a prompt textarea. The spawn flow has no initial-message field today. The reusable mention control should be ready to drop in whenever one is added (see open question #1).
- `src/lib/PromptEditor.svelte:19` — shadow-oath / system-prompt editor. A textarea, but semantically a system prompt, not a user prompt. Probably out of scope unless we decide mentions are useful there too (open question).

### Working directory plumbing

- `src/lib/types.ts:50` — `Agent.cwd?: string` lives on the DB row. Optional.
- `src-tauri/src/db.rs:349,378,446` — `agents.cwd TEXT` column, set on insert, read on load.
- `src-tauri/src/agent/commands.rs:43` — `SpawnAgentRequest.cwd: Option<String>`.
- `src/lib/AgentView.svelte:36` — receives `agent: Agent` as a prop, so the component tree that hosts `ChatInput` already has `agent.cwd` in scope. `cwd` is **not** mirrored onto `LiveAgentState`, which is fine — we'll read it from the `Agent` row, not the live state.

### Filesystem / Tauri command pattern

- `src-tauri/Cargo.toml` — no `ignore`, `walkdir`, or fuzzy-match crate today. Needs `ignore` (or `walkdir` + manual gitignore) and a fuzzy matcher (`nucleo-matcher` or `fuzzy-matcher`).
- `src-tauri/src/project/commands.rs:13-17` — `detect_project()` is the closest existing fs-touching command. It is the template for a new `list_paths` (or similarly named) command.
- `src-tauri/src/lib.rs:45` (specta_builder) and `src-tauri/src/lib.rs:188` (`generate_handler!`) — every new Tauri command needs to be registered in both.
- `src-tauri/src/ws.rs:177` (`dispatch_command`) and `:245-248` (example: `detect_project` arm) — the WebSocket bridge must mirror every new command so browser-mode dev keeps working. Missing this silently breaks one of the two IPC paths.
- `src/lib/bindings.ts` — auto-generated, must be refreshed via `cargo run -- --export-bindings` from `src-tauri/`. Never hand-edited.
- `src/lib/api.ts` — unified IPC wrapper; frontend uses `invoke()` from here instead of `@tauri-apps/api` directly.

### Dropdown / keyboard-navigable UI precedent

- `src/lib/ModelSelector.svelte:31-32` (`showDropdown`, `highlightedIndex`), `:123-165` (`handleModelKeydown`: ArrowUp/Down/Enter/Escape), `:37-47` (filtering). This is the blueprint to copy — same highlighting model, same keybinds (swap Enter→TAB for acceptance), same Escape-stops-propagation convention.
- `src/lib/keybindings.svelte` — `matchBinding(e, "...")` is the customization layer. `dialog.confirm-spawn` is an example. For the mention dropdown, we likely want new bindings (`mention.accept`, `mention.next`, `mention.prev`, `mention.close`) rather than hardcoded keys, so power users can rebind.

### Docs that must stay in sync

- `CLAUDE.md` — component tree, Tauri command pattern section, "do not edit bindings.ts", WS parity rule — all need a nod if this ticket introduces a new reusable component or convention.
- `ONBOARDING.md` — section 12 (file reference) needs the new component + command.

## What needs to change

### Backend (Rust / `src-tauri`)

1. **Add a path-listing command.** A new Tauri command (working name: `list_paths`) takes an anchor directory and a query string, walks the tree beneath the anchor, applies gitignore + noisy-dir filtering, scores entries against the query with a fuzzy matcher, and returns a capped list of `{ path: String, is_dir: bool }` entries sorted by score.
   - Anchor is resolved by joining the shadow's `cwd` with any `../` or path-segment prefix in the query. Path traversal outside `/` is clamped.
   - Result cap is an order-of-magnitude safety net (e.g. 100–200); query-driven fuzzy scoring decides what makes the cut.
   - Walking must be non-blocking enough that rapid keystrokes don't pile up — tokio-spawned, or synchronous but bounded by the cap. Debouncing happens on the frontend, not here.
2. **Add a crate for walking.** `ignore` (the crate behind ripgrep) handles `.gitignore` + `.git`/`node_modules` skipping cleanly. Add to `src-tauri/Cargo.toml`.
3. **Add a fuzzy matcher.** `nucleo-matcher` is the current state of the art (used by Helix / zed-like tools). Alternative: `fuzzy-matcher`. Either is fine; pick one and stick with it.
4. **Register in both IPC paths.** `lib.rs::specta_builder()` + `generate_handler!` for Tauri, plus `ws.rs::dispatch_command` for the WebSocket bridge. Omitting the WS arm is a common foot-gun in this repo.
5. **Regenerate bindings.** `cargo run -- --export-bindings` updates `src/lib/bindings.ts` with the new command signature and return type.

### Frontend (Svelte / `src/lib`)

1. **Create `MentionAutocomplete.svelte` (or similar name).** A small component that accepts a `<textarea>` reference (or wraps one), a `cwd` prop, and owns:
   - Caret / content watching to detect when the user has an active `@…` token at the cursor.
   - A debounced call to `list_paths` through `api.ts`.
   - The floating dropdown (styled like `ModelSelector`'s dropdown) positioned near the caret, with highlighted-index keyboard nav.
   - Keybindings: ArrowUp/Down cycles; TAB (and click) accepts — inserts `@<relative-path>` at the caret and closes; Esc closes without insertion; Enter falls through to the host textarea (so Enter still sends in `ChatInput`).
   - Handling of the `../` prefix as an **anchor shift**, not a filter character — so typing `@../lib/Ag` searches relative to `cwd/..`.
2. **Wire it into `ChatInput.svelte`.** Minimal invasive change: render the component alongside the existing `<textarea>`, pass the textarea's element + the current shadow's `cwd`, and ensure the existing `handleKeydown` yields to the dropdown's handler when it's open.
3. **Thread `cwd` down.** `AgentView.svelte` already has `agent: Agent` in scope (line 36). Pass `agent.cwd` into `ChatInput` (or read it from a shared prop/context).
4. **Caret positioning for the dropdown.** A tricky bit — `<textarea>` does not expose caret coordinates natively. Either (a) use a mirror-div technique (a hidden `<div>` that shadows the textarea's content and layout, using the caret position there) or (b) start with a simpler anchor (above or below the whole textarea) and upgrade later. Recommend (b) for v1 unless it feels bad.
5. **Keybinding registry entries.** Add `mention.next`, `mention.prev`, `mention.accept`, `mention.close` to the keybindings map so users can rebind.

### Docs

- Update `CLAUDE.md` — add the new reusable component to the file reference table if it turns into a first-class thing.
- Update `ONBOARDING.md` § 12 with the new Tauri command and the new frontend component.

## Resolved decisions (confirmed with user)

1. **Surface scope.** Ship a reusable `MentionAutocomplete` control; wire it into `ChatInput.svelte` only for this ticket. SpawnForm and PromptEditor are **not** targets — adding an initial-prompt field to SpawnForm and deciding whether shadow-oath editing benefits from mentions are both separate tickets.

2. **Fallback when `cwd` is unset.** Disable the feature entirely — no dropdown when `agent.cwd` is missing. "No cwd" is the user's signal they haven't chosen a workspace.

3. **Fuzzy match style.** True fzf-style fuzzy via `nucleo-matcher`. Gaps allowed, subsequence scoring.

4. **TAB behavior.** TAB is only intercepted while the mention dropdown is open. Outside that window TAB keeps its default textarea behavior. `ChatInput` does not currently indent on TAB, so there is no conflict.

## Open questions (remain for implementation-time judgment)

1. **Gitignore / hidden file policy.** Default: honor `.gitignore`, skip `.git`, `node_modules`, dotfiles at the top level. No toggle in v1.

2. **Result ranking beyond fuzzy score.** Do we bias shallow paths over deep, or recently-touched files? v1: fuzzy score + alphabetical tiebreak. Ranking tweaks become follow-up tickets if they matter.

3. **Dropdown anchor position.** Caret-anchored (mirror-div technique) vs. anchored to the whole textarea (above/below). Start with textarea-anchored in v1 unless it feels bad in testing; upgrade only if needed.

4. **Wrapping vs. ref.** Implementation choice — is `MentionAutocomplete` a wrapper around `<textarea>` or a sibling that takes an `HTMLTextAreaElement` ref? Ref-based is less invasive to `ChatInput.svelte`. Lean ref-based for v1.

5. **Scanning cost on huge workspaces.** Even with `.gitignore`, pointing `cwd` at `$HOME` or a giant monorepo is possible. Start with hard cap (~100–200) + gitignore; add incremental/lazy walking only if benchmarks show a real pain point.

## Out of scope

- **Expanding `@path` into file contents.** The token stays a literal in the prompt string. Any downstream "attach the file to the agent's context" or "replace @path with the file's text before sending" is a separate ticket.
- **Mentioning things other than files/folders** — no `@agent`, `@session`, `@URL`, `@task`. Only local filesystem paths.
- **Remote / non-local references.** S3 objects, git refs, HTTP URLs — all out.
- **Rich previews** (thumbnails, hover-open file contents) in the dropdown.
- **Multi-select insertion.** One `@path` per selection.
- **Refactoring `PromptEditor.svelte`** to use the new control, unless it falls out naturally.
- **Workspace indexing / caching.** Each `@` scan goes straight to disk. A persistent file index is a future optimization if cold-scan performance is a pain point.
- **Mention decorations / rendered chips.** The inserted value is plain text (`@src/lib/foo.ts`), not a styled chip in the textarea. Chip-rendering is a separate UX layer.
