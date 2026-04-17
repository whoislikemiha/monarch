# MON-79 — Gate Extract Shadow button on provider + model selection

## Summary

`SpawnDialog` → `SpawnForm` currently lets the user trigger the Extract action regardless of what the `ModelSelector` child knows about provider/model readiness. The backend happily creates a sidecar session without a model; the failure only surfaces much later when Pi's retry loop gives up against an unconfigured model (the retry-exhaustion path added in MON-51). The fix is a pure frontend validation: before `spawn_agent` is ever issued, the Extract button and the Ctrl+Enter keyboard submit must be disabled unless the form has both a provider and a valid model. For dynamic providers (LM Studio, OpenRouter) the gate also needs to account for the async model-list lifecycle — still-loading, empty, or failed-to-load — with a short status hint.

## Relevant files and areas

- `src/lib/SpawnDialog.svelte` (whole file) — thin overlay wrapper that mounts `SpawnForm`. No changes expected here; included only because the ticket title uses the dialog name.
- `src/lib/SpawnForm.svelte:113-157` — owns `handleSpawn()`, the Extract button, and the `handleKeydown` listener that maps `dialog.confirm-spawn` (Ctrl+Enter) to spawn. This is where the gating lives.
- `src/lib/SpawnForm.svelte:254-260` — the Extract `<button>` currently has no `disabled` binding; it's always clickable.
- `src/lib/ModelSelector.svelte:27-29` — `modelsLoading`, `modelsError`, `allModels` are private `$state` today. The gate needs visibility into these, so the selector has to surface them (bindable props or a derived "status" object) to the parent.
- `src/lib/ModelSelector.svelte:36` — `fixedModelId` for `openai-codex` means the model is always set for that provider; the gating rule must treat that as valid regardless of list state.
- `src/lib/ModelSelector.svelte:51-93` — `fetchModels` and the provider `$effect` that drives `modelsLoading` / `modelsError`. The new bindable status must track these transitions, including the initial mount for the default `openrouter` provider.
- `src/lib/ModelSelector.svelte:39-49` — `filteredModels` / `allModels` are the source of truth for "is there anything selectable". For strict validity, the gate may want to require the typed `model` to match an `allModels` entry (currently the input is free-form text).
- `src/lib/providers.ts` — `PROVIDERS` and `REFRESHABLE_PROVIDERS`. The latter is the natural discriminator for "dynamic vs. subscription-backed" when choosing the loading hint label. Anthropic already uses a static catalogue served by `get_models`, but it is fast/synchronous in practice — no UI cue needed.
- `src/lib/keybindings.svelte.ts` — the `dialog.confirm-spawn` binding is what `SpawnForm.handleKeydown` matches. The gate has to re-check validity inside `handleSpawn` (or short-circuit at `handleKeydown`) so keyboard submit can't bypass the disabled button.

## What needs to change

1. **Surface model-list status from `ModelSelector` to the parent.** Extend the selector's prop surface so `SpawnForm` can tell whether the list is loading, errored, or empty. Preferred shape: a read-only bindable `modelsStatus` object (e.g. `{ loading: boolean; error: string | null; count: number }`) that the parent binds alongside the existing four props. A single aggregate is cleaner than four new bindings, and it centralises the source of truth in one place instead of scattering derived booleans. The selector still renders its own inline error/hint — the new prop is only for the parent's gating.

2. **Compute a `canSpawn` derived in `SpawnForm`.** True iff: `provider` is non-empty AND (the provider is `openai-codex` OR `model.trim()` is non-empty AND the model-list status is not `loading` AND there is no model-list `error`). Use `REFRESHABLE_PROVIDERS` (imported from `providers.ts`) to decide whether loading/error states matter for the active provider, so subscription-backed providers (Anthropic) remain a simple `provider + model` check even if the list is slow to warm.

3. **Wire `canSpawn` into both submit paths.**
   - Bind `disabled={!canSpawn}` on the Extract `<button>`.
   - Early-return from `handleKeydown` / `handleSpawn` when `!canSpawn`, so Ctrl+Enter from the shadow-name field is gated too. Do not rely on the disabled button alone — the keyboard handler is at window scope.

4. **Add a short disabled-state hint near the actions row.** One line of copy that reflects the active reason the button is off:
   - "Select a provider to continue." (shouldn't happen because a default is picked, but safe fallback)
   - "Select a model." (provider chosen, no model typed/selected)
   - "Loading models…" (dynamic provider, `modelsStatus.loading`)
   - "Waiting for LM Studio…" (LM Studio, loading — from the ticket's copy)
   - "Model list unavailable — see error above." (dynamic provider, `modelsStatus.error`)
   Keep the copy in `SpawnForm` (it's the gating owner) rather than pushing a second hint surface into the selector. Provider-specific copy can key off `provider` directly.

5. **Make sure the existing `modelsError` block inside `ModelSelector` still renders as-is.** The new hint in `SpawnForm` is a terse action-adjacent cue; the detailed error + Retry button in the selector remains the primary error surface. Do not duplicate the message.

6. **No change to the `spawn_agent` command, backend, or protocol.** This is frontend-only; backend retry-exhaustion behaviour from MON-51 remains the backstop for any path that still slips through.

## Open questions

1. **"Valid model" definition.** Should `model` have to match an entry in `allModels` (strict), or is any non-empty `model.trim()` enough (loose)? LM Studio's dropdown only lists loaded models, so free-text there is almost always user error; OpenRouter supports free-form slugs that may not be in the cached list yet. My default is **loose** (non-empty text, no membership check) to avoid surprising power users who paste slugs. Confirm?
2. **Anthropic "loading" edge case.** The ticket explicitly says subscription-backed providers are unaffected. But `get_models` for `anthropic` still goes through the async path and could momentarily leave `modelsLoading = true`. Is it acceptable for the Extract button to briefly flash disabled on first Anthropic render, or should we gate Anthropic purely on `provider + model` regardless of list state?
3. **Default provider quirk.** `SpawnForm` starts with `provider = "openrouter"` and `model = ""`, so the button will now be disabled on dialog open until the user picks a model — that's the intended behaviour, but I want to flag the slight UX shift from "always clickable" to "disabled-by-default". OK to proceed?
4. **Hint placement.** Inline next to the Extract button, above the actions row, or as a `title`/tooltip on the disabled button? I'm leaning toward a tiny `.field-hint`-style line directly above `.actions`, left-aligned. Confirm.

## Out of scope

- Re-architecting `ModelSelector`, the provider preset grid, or any auth flow.
- Adding a refresh/retry affordance on top of what already exists in `ModelSelector` (a `Retry` button is already rendered for dynamic providers on error).
- Any backend change to `spawn_agent`, `get_models`, or sidecar session creation. The retry-exhaustion fallback from MON-51 stays as the last line of defence.
- Template application (`applyTemplate`) — it already writes `provider` and `model` via the existing bindables; no special case needed for the new gate.
- Persisting or restoring the validity state across dialog opens.
