# MON-76 — @-mention autocomplete for files and folders

Linear: https://linear.app/monarch-commander/issue/MON-76

## What landed

`@` in the ChatInput composer opens a keyboard-navigable dropdown of files and folders under the shadow's `cwd`. Accepting a suggestion inserts `@<relative-path>` at the caret. `../` prefixes in the query climb out of `cwd` so files elsewhere on disk are reachable without leaving the keyboard.

## Shape of the change

**Rust — `src-tauri/src/mention.rs`**
- One command, `list_paths(cwd, query)`, returning `Vec<PathSuggestion { path, is_dir }>`.
- Query is split into a leading-`../` anchor shift + a fuzzy needle (`split_query`). Only leading `../` segments shift the anchor — mid-query `..` stays literal, which keeps the mental model "path-ish" without being a full path parser.
- Walker is the `ignore` crate with defaults (gitignore + hidden-dir skip). That gives `.git/`, `node_modules/`, and `.DS_Store` filtering for free.
- Scoring is `nucleo-matcher` in path-matching mode. Empty query returns alphabetical; non-empty returns score-ranked with alpha tiebreak.
- Two caps: 20k entries walked (pathological-tree guard), 150 results returned (UI budget).
- Runs under `spawn_blocking` so the walker doesn't stall the async runtime on big trees.
- WS bridge mirrors the command in `ws.rs::dispatch_command` so browser-mode dev keeps working.

**Frontend — `src/lib/MentionAutocomplete.svelte`**
- Sibling to the host textarea; takes `{ textareaEl, cwd, bind:text }`.
- Token detector walks back from the caret: a valid `@` sits at the start of input or after whitespace/`(`, with no whitespace between `@` and the caret. That rules out `user@host` patterns already in the buffer.
- 80ms debounce + fetch-token pattern (mirrors `ModelSelector`) drops stale responses when the user is still typing.
- Keyboard: ArrowUp/Down cycle, Tab accepts, Escape closes, Enter is passed through to the host (the host's Enter = send).
- Position: `position: fixed`, computed from the textarea's `getBoundingClientRect()`. Flips above when the viewport is tight below (ChatInput lives at the bottom of the screen), caps `max-height` to the side with room.
- Listeners attach inside a `$effect` so the component re-subscribes if the parent re-binds `textareaEl`.

**Integration — `src/lib/ChatInput.svelte`**
- Added `cwd?: string` prop (wired from `AgentView.svelte: agent.cwd`).
- Drops `<MentionAutocomplete>` into the input row with `bind:text`.

## Decisions locked during review

- **SpawnForm is not wired.** MON-76's acceptance criteria called for `@` in the SpawnDialog initial-prompt surface, but `SpawnForm.svelte` no longer has a freeform prompt textarea (post-MON-52 decomposition). The reusable component is there for the next prompt surface that surfaces — scope is ChatInput only for this ticket.
- **No cwd → feature disabled.** If the shadow has no `cwd`, `list_paths_inner` returns `Ok(vec![])` and the dropdown never opens. Chose this over anchoring at `$HOME` to avoid surprising operators with unrelated suggestions.
- **Insert format is `@<path> ` with a trailing space.** Prevents the next keystroke from re-entering the mention token; matches the "compose more text after the reference" common case.

## Risks / follow-ups

- **Execution side of `@`.** This ticket is pure UX. Downstream expansion ("substitute @path with file contents" or "attach file to the prompt") is not implemented — the `@` lands in the outgoing message as-is. That's the expected state per the out-of-scope list on the issue.
- **Large repos.** 20k walked / 150 returned keeps the UI responsive. Walks are cold per keystroke beyond the debounce — a future optimization is caching the entry list per (cwd, ups) pair and re-ranking locally.
- **`PromptEditor.svelte` (shadow-oath editor)** is not wired. The reusable component could drop in there, but it was explicitly deferred.

## Rebase notes

Branch was rebased onto current `master` (past MON-77 and MON-78). Conflicts: `Cargo.toml` (both added deps to the same trailing block — merged as `toml` + `ignore` + `nucleo-matcher`). No logic conflicts in any code file.
