# MON-79 — Gate Extract Shadow button on provider + model selection

## What was implemented

Extract Shadow in `SpawnDialog` is now disabled until the form has both a provider and a non-empty model. The gate applies to both the button click path and the Ctrl+Enter keyboard submit path, so neither can race past an unconfigured backend. A short hint line directly above the action row explains why the button is off, with provider-aware copy for the dynamic providers that can still be loading or unreachable (LM Studio, OpenRouter).

This closes the spawn-time hole we hit during MON-51 testing, where the sidecar session was happily created against no model and the failure only surfaced later when Pi's retry loop gave up. Spawns against an unusable model are now blocked at the UI; MON-51's retry-exhaustion surface remains the backstop for anything that still slips through.

## Key decisions

- **Loose "valid model" definition.** Any non-empty `model.trim()` is accepted — no membership check against `allModels`. OpenRouter users who paste a fresh slug aren't blocked, and LM Studio users can still type (though in practice the dropdown is the only sensible path). Confirmed with user 2026-04-17.
- **Loading / error state never disables the button.** Only `provider + model` does. Hint copy reflects the list status for dynamic providers, but the button's disabled flag is a pure function of `canSpawn`. This avoids a disabled-flash on Anthropic/Codex where the static catalogue is "loading" for one render.
- **Status surfaced as a single bindable object.** `ModelSelector` exposes a new `modelsStatus: { loading, error, count }` bindable that mirrors its internal fetch lifecycle — one new prop, not four booleans. The selector's existing inline error block + Retry remain the primary error surface; the hint in `SpawnForm` is a terse adjacent cue, not a duplicate.
- **Hardcoded catalogues surfaced but not fixed here.** During review we noticed Anthropic lists 4.6-era models (no Opus 4.7) and Codex is pinned to `gpt-5.4`. Spun out as **MON-81** for a dynamic-fetch refactor — explicitly out of scope for MON-79 per the original ticket.

## Files touched

- `src/lib/ModelSelector.svelte` — added `ModelsStatus` type, new bindable `modelsStatus` prop, `$effect` to sync from internal state.
- `src/lib/SpawnForm.svelte` — bound `modelsStatus`, added `canSpawn` / `disabledHint` deriveds, wired `disabled={!canSpawn}` on Extract, short-circuited `handleSpawn`, rendered `.action-hint` above `.actions`, added disabled-button CSS.

## What was left out

- No change to `spawn_agent`, `get_models`, or any sidecar/protocol surface — pure frontend validation fix, as the ticket specified.
- No refresh / retry affordance on the model list beyond what `ModelSelector` already has (Retry button on error).
- Dynamic Anthropic / Codex model lists — split out into **MON-81**.
