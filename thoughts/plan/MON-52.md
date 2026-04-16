# MON-52 — Decompose SpawnDialog (1200 lines)

## Summary

`src/lib/SpawnDialog.svelte` has grown to 1202 lines and bundles together six unrelated concerns: the dialog shell, the template strip, provider/model/thinking selection (with network-driven model lists, auth status, LM Studio context auto-detect, fuzzy filter, and full keyboard navigation), CWD picker + project detection + project chips, shadow identity (name/grade/title), and the "save as template" helper. The goal is to split it along natural seams into three focused components — `SpawnForm.svelte`, `TemplateSelector.svelte`, `ModelSelector.svelte` — so each piece is small enough to reason about, and so `ModelSelector` becomes reusable for the future runtime model switcher that `AgentView.svelte` will need (the `set_model` command path already exists at `AgentView.svelte:367-369`, it has no UI yet). No user-visible behavior changes.

## Relevant files and areas

- **`src/lib/SpawnDialog.svelte`** (1202 LOC) — the component being decomposed. Structural landmarks:
  - L1–16: props (`onspawn`, `oncancel`).
  - L18–50: local state for model input, thinking level, cwd, shadow identity, providers list, refreshable-provider set, templates.
  - L52–86: local types (`ModelInfo`, `ProviderAuthStatus`, `DetectedProject`) and the async-fetch state (`allModels`, `modelsLoading`, `modelsError`, `modelFetchToken`, `authStatus`, `detectedProject`, dropdown/highlight state, `fixedModelId` derived).
  - L90–100: `filteredModels` fuzzy search.
  - L103–131: `fetchModels`, `fetchAuthStatus` with a token-based staleness guard.
  - L133–189: template CRUD (`loadTemplates`, `applyTemplate`, `persistCurrentAsTemplate`, `deleteTemplate`) and `onMount`.
  - L191–233: `detectProject`, provider-change `$effect`, cwd `$effect`.
  - L215–220: `selectedLmStudioModel` derived — drives read-only context display.
  - L235–277: model picker UX helpers (`selectModel`, `handleModelKeydown`, `refreshModels`).
  - L279–332: `handleSpawn` (composes `AgentConfig`, auto-saves template if requested), `browseFolder`, dialog-level `handleKeydown`.
  - L337–614: template markup — overlay/dialog, template chips, provider chips, auth status, model field + dropdown + LM Studio context, project chips, thinking + cwd row, project info, shadow identity, save-as-template checkbox, actions.
  - L616–1202: scoped styles (large, but chunked by feature).
- **`src/lib/types.ts`** — `AgentConfig` (L104+) and `AgentTemplate` (L89–101). Both stay unchanged.
- **`src/lib/api.ts`** — `invoke` wrapper that all backend calls must go through. Every `invoke(...)` currently inside `SpawnDialog.svelte` must still resolve against this module, not `@tauri-apps/api`.
- **`src/lib/keybindings.svelte.ts`** — `matchBinding("dialog.confirm-spawn")` usage at L331. Stays in whichever component owns the top-level dialog keydown handler.
- **`src/lib/stores/agentStore.svelte.ts`** — used at L8 for `agentStore.projects`. Stays in whichever component renders the project chips.
- **`src/App.svelte`** L343–351 — the only consumer. It passes `onspawn(config)` and `oncancel()`. The external contract must not change, so whatever file name is kept publicly still exports those two props with the same shapes.
- **`src/lib/AgentView.svelte`** L367–369 — the future consumer of `ModelSelector`: `setModel(provider, modelId)` sends a `set_model` sidecar command. No UI wired up yet; this plan enables that follow-up without delivering it.
- **`ONBOARDING.md`** L369, 590 — the one-line description of `SpawnDialog.svelte`. Needs to be refreshed if the file is renamed or split into multiple entries.
- **`plans/mon-7-*.md`, `thoughts/plan/MON-9.md`, `thoughts/plan/MON-8.md`, `thoughts/impl/MON-9.md`, `thoughts/impl/MON-8.md`** — historical plans that reference specific line numbers inside `SpawnDialog.svelte`. No rewrite needed (they're history), but a mental note that line-number pointers will go stale after this change.

## What needs to change

### 1. `ModelSelector.svelte` — the most reusable slice

Owns everything that's specific to picking a provider + model + thinking level, plus LM Studio context detection. Designed so that a future `AgentView` runtime switcher can drop it in with no modification.

**Responsibilities:**
- Provider chip row (Anthropic / OpenAI Codex / OpenRouter / LM Studio).
- Auth-status banner (`get_provider_auth_status` results, with loading / ok / warn / neutral states).
- Model text input with:
  - fuzzy filter over `allModels`,
  - dropdown list of up to 50 matches,
  - keyboard navigation (ArrowUp/Down/Enter/Escape),
  - mouse selection with `onmousedown` preventDefault trick,
  - refresh button for `openrouter` / `lmstudio`,
  - read-only lock + hint for `openai-codex` (`fixedModelId`),
  - loading indicator, error card with retry, "no models" hint.
- LM Studio context display (auto-detected from `ModelInfo.contextWindow`).
- Thinking-level `<select>`.

**State it owns internally (not exposed):** `allModels`, `modelsLoading`, `modelsError`, `modelFetchToken`, `authStatus`, `authLoading`, `showDropdown`, `highlightedIndex`, `modelInputEl`, and the two `$effect`s that re-fetch when provider changes.

**Public contract (the shape that matters for reuse):**
- Two-way bindable values for the four user choices: `provider`, `model`, `thinkingLevel`, and — for LM Studio only — the auto-detected `contextWindow` (read-only from the caller's perspective, but exposed so the spawn handler can read it). Implementation will use Svelte 5's `$bindable()` rune on props; the exact prop names can follow existing patterns elsewhere in the codebase.
- A single `providers` config is hard-coded inside this component for now. The ticket's "reusable for runtime model switching" goal does not require making this prop-configurable yet — runtime switching will use the same list.

**CSS that moves here:** all styles scoped to `.preset-grid`, `.preset-btn`, `.auth-status*`, `.model-field`, `.model-input-wrap`, `.loading-indicator`, `.refresh-btn`, `.model-error*`, `.model-dropdown`, `.model-option*`, `.model-id`, `.model-name`, `.lmstudio-context`, `.lmstudio-ctx-value`, `@keyframes spin`.

### 2. `TemplateSelector.svelte` — thin presentational component

**Responsibilities:**
- Load templates on mount via `db_list_agent_templates`.
- Render chip row if templates exist (hidden if empty).
- Expose an `onselect(template)` callback so the parent can run its own apply logic (the microtask dance in `applyTemplate` is parent-owned because it must sequence writes against `ModelSelector`'s provider `$effect`).
- Handle delete via `db_delete_agent_template` and refresh its own list.

**Does not own:**
- The "Save as template" checkbox — that reads shadow name, provider, model, thinking level, cwd, grade, title, so it belongs on `SpawnForm`.
- The `persistCurrentAsTemplate` function — for the same reason.

**CSS that moves here:** `.template-chips`, `.template-chip*`, `.template-chip-name`, `.template-chip-del`.

### 3. `SpawnForm.svelte` — the composed form

**Responsibilities:**
- The outer overlay + dialog shell (`.overlay`, `.dialog`, `h2`).
- Top-level keydown handler: `Escape` → `oncancel()` (unless a model dropdown is open — see open question 1), and `matchBinding("dialog.confirm-spawn")` → submit.
- Compose `<TemplateSelector onselect={applyTemplate} />`.
- Compose `<ModelSelector bind:provider bind:model bind:thinkingLevel bind:contextWindow />`.
- Own shadow identity fields (name, grade, title) — three small inputs.
- Own CWD row (text input + browse button using `open` from `@tauri-apps/plugin-dialog`) and the CWD `$effect` that re-runs `detect_project`.
- Render the project chip row (reads `agentStore.projects`) and the detected-project info card.
- Own the "Save as template" checkbox and `persistCurrentAsTemplate` logic.
- Own the Extract / Cancel buttons and the `handleSpawn` assembly of `AgentConfig`.
- `applyTemplate(t)` with the existing microtask workaround so provider change's model-reset `$effect` inside `ModelSelector` doesn't clobber the template's model value.

**Props:** `onspawn: (config: AgentConfig) => void`, `oncancel: () => void` — identical to the current `SpawnDialog` surface.

**CSS that moves here:** the overlay/dialog chrome, `.section`, `.label`, `.row`, `.field`, `.flex-grow`, `.cwd-row`, `.browse-btn`, `.project-chips`, `.project-chip*`, `.project-info*`, `.template-save-check`, `.template-save-hint`, `.actions`, `.btn-cancel`, `.btn-spawn`, `.shortcut`, the shared `input,select` rules, and the mobile `@media (max-width: 640px)` block.

### 4. Reconcile `SpawnDialog.svelte` with the new layout — DECIDED: four files, split by concern

Four `.svelte` files, each named for its concern:

- **`SpawnDialog.svelte`** — outer chrome only. Owns `.overlay` + `.dialog` shell, `<svelte:window onkeydown>`, `h2` title, and relays `onspawn` / `oncancel` props down. ~30–50 LOC.
- **`SpawnForm.svelte`** — form body. Composes `<TemplateSelector>` + `<ModelSelector>`, owns shadow identity, CWD + project detection + project chips, save-as-template checkbox + `persistCurrentAsTemplate`, and `handleSpawn` that builds `AgentConfig`. Rendered as `SpawnDialog`'s only child. ~250–350 LOC.
- **`ModelSelector.svelte`** — see §1.
- **`TemplateSelector.svelte`** — see §2.

`App.svelte` keeps importing `SpawnDialog`. External contract unchanged.

Why split rather than collapse: `SpawnDialog` = "how this appears on screen" (overlay, escape-to-close, theming) vs. `SpawnForm` = "what the user is filling in" (state, backend calls, submit). Two concerns, two files.

### 5. Cross-component wiring concerns

- **Provider-change model reset.** Today the provider `$effect` inside `SpawnDialog` resets `modelInput` to `fixedModelId || ""`. After the split that `$effect` lives inside `ModelSelector`. Because `ModelSelector` owns `provider` and `model` as bindable props, the reset is still visible to the parent via the same bind — parent's `applyTemplate` can keep using `queueMicrotask` to set the model after the reset has propagated.
- **LM Studio context window.** The current code reads `selectedLmStudioModel?.contextWindow` inside `handleSpawn` in the outer component. After the split, `ModelSelector` owns the derived value; we expose it via a bindable `contextWindow` prop so `SpawnForm.handleSpawn` can still stamp it into `AgentConfig`.
- **Escape vs. dropdown — DECIDED:** `ModelSelector`'s keydown handler calls `e.stopPropagation()` (and `preventDefault()`) on Escape when its dropdown is open. That way `SpawnDialog`'s window-level handler can unconditionally `oncancel()` on Escape — no bindable `showDropdown` leaking out, no `!showDropdown` guard on the parent. Cleaner encapsulation than today.
- **Fixed model lock.** `fixedModelId` (derived from `provider === "openai-codex"`) makes the text input readonly and drives placeholder text. It's an internal concern of `ModelSelector`; nothing outside needs to know.
- **Save-as-template sequencing.** Today `handleSpawn` does `if (saveAsTemplate && shadowName.trim()) await persistCurrentAsTemplate();` before invoking `onspawn`. That stays in `SpawnForm.handleSpawn` — both the checkbox and the saver live there.

### 6. Docs

- Update `ONBOARDING.md` L369 (component tree) and L590 (file reference table) to reflect the new split. One line per file, matching the existing style.
- `CLAUDE.md` does not need changes (no rules, conventions, or key-file pointers are affected).

## Resolved decisions (from round 1)

1. **Filenames** — four files, named for their concerns: `SpawnDialog` (outer shell) + `SpawnForm` (body) + `ModelSelector` + `TemplateSelector`. See §4.
2. **Escape vs. dropdown** — `ModelSelector` swallows Escape via `e.stopPropagation()`; `SpawnDialog`'s Escape handler becomes unconditional. See §5.
3. **`ModelSelector` surface** — narrow bindable set (`provider`, `model`, `thinkingLevel`, `contextWindow`). No `allModels` / selected `ModelInfo` exposed until a concrete second caller needs it. Plus the opportunistic cleanups below.
4. **Thinking level** — lives in `ModelSelector`, matching the ticket. Resolved.
5. **CSS duplication** — each child owns its scoped styles. Shared `input, select` rules are duplicated per child. All children use the existing theme CSS variables (`var(--bg-panel-2)`, `var(--border-subtle)`, etc.), so theme responsiveness is preserved. No shared stylesheet.

## Opportunistic cleanups (in-scope, fall out naturally)

Small tidy-ups that become obvious once things are being moved — batching them into this refactor costs little and avoids a second pass:

- **Promote the local `ModelInfo`, `ProviderAuthStatus`, `DetectedProject` types to `src/lib/types.ts`.** They're declared inside `SpawnDialog.svelte` today; after the split they'd be declared-or-imported in at least two places (`ModelSelector` uses `ModelInfo` + `ProviderAuthStatus`, `SpawnForm` uses `DetectedProject`). Promoting centralises them and makes them usable by the future runtime switcher without another move. No semantic change — identical shapes.
- **Extract `providers`, `REFRESHABLE_PROVIDERS`, `thinkingLevels` constants into `src/lib/providers.ts`.** Same reasoning — they're provider catalogue facts, not `ModelSelector`-specific trivia. Keeps the runtime switcher's future consumption trivial.
- **Replace the existing `invoke<ModelInfo[]>` / `invoke<ProviderAuthStatus>` / `invoke<DetectedProject | null>` generic calls with the typed `commands.*` bindings from `src/lib/bindings.ts`** where equivalents exist. (Spot-check during implementation: if the auto-generated types match exactly, swap; otherwise leave the `invoke` generic in place. Not a goal by itself — only do the easy ones.)
- **Delete the `formatCtxTokens` helper from the module scope and co-locate it inside `ModelSelector`** (it's only used there today).

These are bundled into the refactor because they directly involve code being moved anyway. Hard no on anything else (no new abstractions, no `CwdField` spin-off, no pre-baked runtime-switcher scaffolding).

## Remaining open questions

None blocking implementation. Flag during implementation if any of the opportunistic cleanups turn out to be messier than expected, and back them out of scope if so.

## Out of scope

- Building the runtime model switcher in `AgentView` that will consume `ModelSelector`. This plan enables it, nothing more.
- Any change to `AgentTemplate`, `AgentConfig`, Tauri commands, or the backend.
- Visual redesign — the extracted components must render pixel-identical to today's `SpawnDialog`.
- Adding a frontend test harness. Verification is manual: spawn with each provider, apply a template, save as template, LM Studio context auto-detect, Escape closes dialog, Escape closes dropdown only, Ctrl+Enter submits, project chips select cwd. `npx svelte-check` must stay green.
