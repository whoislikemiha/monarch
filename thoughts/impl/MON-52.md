# MON-52 — Decompose SpawnDialog — Implementation Notes

## What was implemented

`src/lib/SpawnDialog.svelte` (1202 LOC) was split along its natural concern seams into four files:

- **`SpawnDialog.svelte`** (74 LOC) — modal chrome only: overlay, dialog container, title, responsive padding. Forwards `onspawn` / `oncancel` to `SpawnForm`. `App.svelte` still imports this name; the external contract is unchanged.
- **`SpawnForm.svelte`** (489 LOC) — form body. Owns all shared form state, window-level keydown (`Escape` / `Ctrl+Enter`), shadow identity, CWD + project detection + project chips, save-as-template checkbox, `persistCurrentAsTemplate`, `handleSpawn`, `browseFolder`, and the `queueMicrotask`-based template-apply sequencing.
- **`ModelSelector.svelte`** (609 LOC) — reusable model/provider UI. Owns provider chips, auth-status banner, model text input + fuzzy dropdown + keyboard nav, refresh button, LM Studio context auto-detect, and the thinking-level select. Exposes four bindable props: `provider`, `model`, `thinkingLevel`, `contextWindow`. Will plug straight into the future runtime model switcher that `AgentView.svelte`'s `set_model` path is waiting for.
- **`TemplateSelector.svelte`** (129 LOC) — chip row that loads `db_list_agent_templates`, renders chips, handles delete. Emits `onselect(template)` so the parent can coordinate the apply sequencing.
- **`providers.ts`** (new, 31 LOC) — exports `PROVIDERS`, `REFRESHABLE_PROVIDERS`, `THINKING_LEVELS` for reuse.

## Key decisions

- **`ModelSelector` swallows Escape when its dropdown is open** (`e.stopPropagation()`) instead of leaking a bindable `showDropdown` to the parent. `SpawnForm`'s Escape handler is unconditional now — cleaner encapsulation than the prior `if (!showDropdown) oncancel()` dance.
- **Thinking-level select lives inside `ModelSelector`** per the ticket. This moved it visually: it used to share a row with the CWD input, now it renders at the bottom of `ModelSelector` below the LM Studio context hint. Confirmed acceptable with the user during review.
- **Types consolidated against `bindings.ts`.** Deleted the local `AgentTemplate` interface in `types.ts` in favour of the auto-generated `AgentTemplateRow` (identical shape, single source of truth). `ModelInfo` and `ProviderAuthStatus` are imported from `bindings.ts` directly — already existed there with matching shapes. `DetectedProject` promoted to `types.ts` because `commands.detectProject`'s auto-gen is a serde_json::Value emission mess.
- **`invoke<T>` kept, typed `commands.*` cleanup dropped.** Only two files in the repo use `commands.*` (for `spawnAgent`). Swapping here would have been inconsistent. Also, `commands.detectProject` is unusable anyway.
- **CSS duplicated per child.** Each component owns its scoped styles. All children use the existing theme CSS variables (`var(--bg-panel-2)` etc.), so theme responsiveness is preserved. `formatCtxTokens` moved into `ModelSelector` — only caller.
- **Four files, not three.** The plan added `SpawnDialog` (thin shell) alongside the three new files the ticket called for. Separates "how the modal appears" from "what the user fills in" — `SpawnForm` could theoretically be reused in a non-modal context.

## Files touched

Added:
- `src/lib/ModelSelector.svelte`
- `src/lib/TemplateSelector.svelte`
- `src/lib/SpawnForm.svelte`
- `src/lib/providers.ts`

Modified:
- `src/lib/SpawnDialog.svelte` (1202 LOC → 74 LOC)
- `src/lib/types.ts` (removed `AgentTemplate`, added `DetectedProject`)
- `ONBOARDING.md` (component tree + file reference table)

Unchanged: `App.svelte`, `bindings.ts`, any Rust / sidecar code, any Tauri commands.

## What was left out

- **No new UI consuming `ModelSelector`** — the runtime model switcher in `AgentView` was explicitly out of scope. `AgentView.svelte:367-369` still has the `setModel` path with no caller; this PR only enables the follow-up.
- **No tests** — no frontend test harness exists in the repo. Verification was manual against the acceptance criteria checklist.
- **No visual redesign** beyond the thinking-select row change noted above.
- **LOC target not strictly hit.** `SpawnForm` (489) and `ModelSelector` (609) exceed the "~400 LOC" plan target, mostly due to scoped CSS. Script+template sizes (230 and 305 respectively) are within the intended cognitive budget. Flagged during review; user accepted.
