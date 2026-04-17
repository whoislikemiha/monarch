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

2. **Compute a `canSpawn` derived in `SpawnForm`.** True iff: `provider` is non-empty AND (the provider is `openai-codex` OR `model.trim()` is non-empty). Membership in `allModels` is **not** required — "loose" validity, any non-empty typed id is accepted so users pasting a fresh OpenRouter slug aren't blocked. Loading and error states of the model list do **not** themselves disable the button; they only drive the hint copy. This keeps subscription-backed providers (Anthropic) on a pure `provider + model` check and avoids briefly disabling Extract while `get_models` is in-flight for static catalogues.

3. **Wire `canSpawn` into both submit paths.**
   - Bind `disabled={!canSpawn}` on the Extract `<button>`.
   - Early-return from `handleKeydown` / `handleSpawn` when `!canSpawn`, so Ctrl+Enter from the shadow-name field is gated too. Do not rely on the disabled button alone — the keyboard handler is at window scope.

4. **Add a short disabled-state hint directly above the `.actions` row** (left-aligned, styled like `.field-hint`). One line of copy reflecting the active reason the button is off — the hint leans on the list status even though the status doesn't flip the gate:
   - "Select a model." (no model typed/selected, list ready)
   - "Loading models…" (OpenRouter, still loading)
   - "Waiting for LM Studio…" (LM Studio, still loading — from the ticket's copy)
   - "Model list unavailable — see error above." (dynamic provider, `modelsStatus.error`)
   When `canSpawn` is true the hint is not rendered. Keep the copy in `SpawnForm` (it's the gating owner) rather than pushing a second hint surface into the selector. Provider-specific copy keys off `provider` directly.

5. **Make sure the existing `modelsError` block inside `ModelSelector` still renders as-is.** The new hint in `SpawnForm` is a terse action-adjacent cue; the detailed error + Retry button in the selector remains the primary error surface. Do not duplicate the message.

6. **No change to the `spawn_agent` command, backend, or protocol.** This is frontend-only; backend retry-exhaustion behaviour from MON-51 remains the backstop for any path that still slips through.

## Resolved decisions

1. **Valid model = loose.** Any non-empty `model.trim()` counts; no membership check against `allModels`. Confirmed 2026-04-17.
2. **No provider-specific loading gate.** Loading/error states never disable Extract — only the `provider + model` combination does. The button stays disabled until a model is selected; no flicker on Anthropic. Confirmed 2026-04-17.
3. **Disabled-by-default on dialog open.** Intentional UX shift — confirmed 2026-04-17.
4. **Hint placement.** `.field-hint`-style line directly above `.actions`, left-aligned. Confirmed 2026-04-17.

## Out of scope

- Re-architecting `ModelSelector`, the provider preset grid, or any auth flow.
- Adding a refresh/retry affordance on top of what already exists in `ModelSelector` (a `Retry` button is already rendered for dynamic providers on error).
- Any backend change to `spawn_agent`, `get_models`, or sidecar session creation. The retry-exhaustion fallback from MON-51 stays as the last line of defence.
- Template application (`applyTemplate`) — it already writes `provider` and `model` via the existing bindables; no special case needed for the new gate.
- Persisting or restoring the validity state across dialog opens.
