# MON-9 — LM Studio context window auto-detect

## What was implemented

Monarch now auto-detects the LM Studio context window at spawn time by querying LM Studio's native `/api/v0/models` endpoint. Only models the server reports as currently loaded are surfaced in the picker, and each listed model carries its `loaded_context_length` straight through to the sidecar as `CreateSessionCommand.contextWindow`. The MON-8 manual input (slider, presets, typed override) has been removed entirely — the spawn dialog now shows the detected value read-only.

When the native endpoint isn't available (older LM Studio build), discovery falls back to the OpenAI-compatible `/v1/models` path. That endpoint is already loaded-only, so no filtering is needed there; it just doesn't carry a pre-detected context window, so spawns on that path use the sidecar's 32k default.

## Key decisions

- **Loaded models only.** LM Studio's `/v1/models` endpoint is already scoped to loaded models, while `/api/v0/models` returns every installed model with a `state` flag. Filtering the native response to `state == "loaded"` keeps the two discovery paths semantically equivalent and keeps the picker honest — every listed model is one you can actually talk to right now. Dropped the earlier plan to surface `max_context_length` for unloaded models.
- **No manual override.** The plan originally had auto-detect pre-filling a still-editable slider; the user asked for read-only on the grounds that auto-detect is reliable enough to trust. Removed the slider, presets, override-tracking state, and the `lmstudio-ctx-*` CSS. `handleSpawn` now pulls `contextWindow` straight off the selected `ModelInfo`.
- **Base URL rework.** `lmstudio_base_url` became `lmstudio_host_root`, which strips a trailing `/v1` from `LMSTUDIO_BASE_URL` so both `/v1/models` and `/api/v0/models` can be composed from the same host root. The default moved from `http://127.0.0.1:1234/v1` to `http://127.0.0.1:1234`; existing `/v1`-suffixed env vars still resolve correctly.
- **Dropdown anchoring fix.** Adding the LM Studio context block inside `.model-field` (which is `position: relative`) pushed the absolutely-positioned model dropdown below the context block, because `top: 100%` was measuring against the whole field. Moved the dropdown inside `.model-input-wrap` so it re-anchors to the input itself.

## Files touched

- `src-tauri/src/models.rs` — new `LmStudioNativeResponse` / `LmStudioNativeModel` types, `fetch_lmstudio_models_native` + `fetch_lmstudio_models_openai` split, loaded-only filter, `lmstudio_host_root`. `ModelInfo` gained an optional `contextWindow` field.
- `src/lib/SpawnDialog.svelte` — removed slider/preset state and UI, added `selectedLmStudioModel` derived, new read-only context-window display, dropdown anchoring fix. `handleSpawn` reads context from the selected `ModelInfo`.
- `ONBOARDING.md` — LM Studio paragraph rewritten to describe native discovery, loaded-only filtering, and the fallback.
- `thoughts/plan/MON-9.md` — plan rewritten mid-implementation to reflect the loaded-only scope change.

## What was left out

- `max_context_length` / "not loaded — using max" path. Dropped in favour of loaded-only listing.
- Any manual-override UI for the detected context window. Read-only by design.
- Mid-session polling for context-length changes. One-shot discovery at spawn time only.
- `types.ts` mirror of the new `ModelInfo.contextWindow` field — `ModelInfo` lives as a local interface inside `SpawnDialog.svelte`, no shared type exists to extend.
