# MON-9 — Auto-detect LM Studio context window via `/api/v0/models`

## Summary

MON-8 introduces a per-agent "context window" input in the spawn dialog, plumbs it through the Tauri command layer into the sidecar, and uses it as the `contextWindow` of the dynamic LM Studio `Model` object. That fixes the honesty-of-the-meter problem but leaves a UX gap: the user has to know, and correctly type, the context length they loaded each model with in LM Studio. This issue closes that gap by querying LM Studio's newer native REST API (`/api/v0/models`) at spawn time, filtering to currently-loaded models, and sending each model's `loaded_context_length` straight through to the sidecar. The MON-8 manual input is removed entirely.

## Scope decision — loaded models only

LM Studio's OpenAI-compatible `/v1/models` endpoint only returns *loaded* models, while the native `/api/v0/models` returns every model the user has installed (loaded and unloaded). To keep the spawn-dialog model picker honest — every listed model should be one you can actually talk to right now — Monarch filters the native response to `state == "loaded"` entries only. Unloaded models are dropped before they reach the frontend, and there is no "pre-fill with max_context_length" path.

## Relevant files and areas

- `src-tauri/src/models.rs`
  - `fetch_lmstudio_models` currently hits only `/v1/models`. Upgrade to try `/api/v0/models` first, fall back to `/v1/models` on failure.
  - `LmStudioModel` / `LmStudioResponse` structs — add parallel types for the native payload covering `id`, `state`, and `loaded_context_length`.
  - `ModelInfo` — add an optional `context_window` field (serialized `contextWindow`) that only LM Studio entries populate.
  - `lmstudio_base_url` — rework so both `/v1/models` and `/api/v0/models` can be composed from the same host root; accept a trailing `/v1` on `LMSTUDIO_BASE_URL` for backward compatibility.
- `src/lib/SpawnDialog.svelte`
  - Remove the MON-8 slider, presets, and typed input for LM Studio context window.
  - When the user selects an LM Studio model, show its `contextWindow` read-only and send it on spawn. If discovery didn't populate a value (fallback path, unknown id), omit `contextWindow` from `AgentConfig` and let the sidecar apply its default.
- `ONBOARDING.md`
  - LM Studio section gets rewritten to describe the auto-detect + fallback flow.

## What needs to change

**1. Native LM Studio discovery.** Add a fetch against `/api/v0/models` in `src-tauri/src/models.rs`. The endpoint returns a richer payload than `/v1/models` — each entry includes fields like `id`, `state` (loaded/not-loaded), `loaded_context_length`, `max_context_length`, and type metadata. Define Rust types for the subset Monarch cares about (`id`, `state`, `loaded_context_length`).

Behaviour:
- Call `/api/v0/models` first. If it returns 200 with a parseable body, filter to entries where `state == "loaded"` and map each to a `ModelInfo` with `context_window = loaded_context_length`.
- On 404 / connection error / parse failure, fall back to the existing `/v1/models` path. That endpoint is already loaded-models-only, so no filtering is needed; `context_window` is left `None` because the OpenAI-compatible endpoint doesn't expose it.
- Timeout stays at the existing 3s budget.

**2. Propagate context window in `ModelInfo`.** Extend `ModelInfo` with an optional `context_window` (`contextWindow` on the wire). Other providers leave it `None`.

**3. Read-only display in `SpawnDialog.svelte`.** Drop the slider, presets, and all override-tracking state from MON-8. When the user selects an LM Studio model whose `ModelInfo.contextWindow` is populated, show it read-only below the model picker. On spawn, pull the value straight off the selected model and send it as `AgentConfig.contextWindow`. If the value is missing (fallback path, model id not in list), omit it and let the sidecar's 32k default take over.

**4. Docs.** Rewrite `ONBOARDING.md`'s LM Studio paragraph: native endpoint, loaded-only filter, `/v1/models` fallback, read-only display, sidecar default when no value is available.

## Out of scope reminders

- Any change to the live-context meter numerator (MON-8).
- Any change to the session-lifetime billing readout (MON-8).
- Polling LM Studio for live context-length changes mid-session.
- Auto-loading models into LM Studio on Monarch's behalf.
- Surfacing unloaded models in the picker with a "not loaded" badge.
- Non-LM-Studio providers.

## Dependencies

- **Blocked by MON-8.** This plan builds on the `contextWindow` field on the sidecar `CreateSessionCommand`, the `agents.context_window` column, and the meter denominator wiring MON-8 introduces — even though MON-9 removes the typed input MON-8 added on top of that plumbing.
