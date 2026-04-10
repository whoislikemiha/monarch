# MON-9 — Auto-detect LM Studio context window via `/api/v0/models`

## Summary

MON-8 introduces a per-agent "context window" input in the spawn dialog, plumbs it through the Tauri command layer into the sidecar, and uses it as the `contextWindow` of the dynamic LM Studio `Model` object. That fixes the honesty-of-the-meter problem but leaves a UX gap: the user has to know, and correctly type, the context length they loaded each model with in LM Studio. This issue closes that gap by querying LM Studio's newer native REST API (`/api/v0/models`) at spawn time, pulling `loaded_context_length` for the selected model, and using it to pre-fill (and optionally lock) the context window input. It's a pure enhancement on top of MON-8 — no change to the meter, no change to billing, no change to non-LM-Studio providers.

## Relevant files and areas

- `src-tauri/src/models.rs`
  - `fetch_lmstudio_models` (line 117) currently hits `/v1/models` — the OpenAI-compatible endpoint — which only returns model IDs. This is where a new native call to `/api/v0/models` should land, either as a second function or as an upgraded primary discovery path that falls back to `/v1/models`.
  - `LmStudioModel` / `LmStudioResponse` structs (lines 103–110) — need extending or parallel types to capture `loaded_context_length`, `max_context_length`, and a loaded/unloaded flag.
  - `ModelInfo` — the shape sent to the frontend. Needs an optional `contextWindow` (and maybe `maxContextWindow`) field for LM Studio entries. Other providers leave it unset.
  - `lmstudio_base_url` (line 113) — the base URL logic stays the same; `/api/v0/models` lives on the same host and port as `/v1/models` (LM Studio's default `http://127.0.0.1:1234`).
  - `get_models` (line 182) — routes provider → fetcher. The LM Studio branch calls the new discovery function.
- `src/lib/SpawnDialog.svelte`
  - The context-window input added in MON-8 is the consumer. When the selected model's `ModelInfo` comes back with a `contextWindow`, the input pre-fills with it. If the user edits the value, their edit wins (manual override).
  - Model list refresh flow (via `REFRESHABLE_PROVIDERS`) already exists for LM Studio — re-running discovery picks up new `loaded_context_length` values as the user loads/unloads models in LM Studio.
- `src/lib/types.ts`
  - `ModelInfo` type needs the optional `contextWindow` field mirror.
- `ONBOARDING.md`
  - LM Studio section gets a one-line update: "Monarch will pre-fill the context window from LM Studio's REST API when available; otherwise set it manually."

## What needs to change

**1. Native LM Studio discovery.** Add a fetch against `/api/v0/models` in `src-tauri/src/models.rs`. The endpoint returns a richer payload than `/v1/models` — each entry includes fields like `id`, `state` (loaded/not-loaded), `loaded_context_length`, `max_context_length`, and type metadata. Define Rust types for the subset Monarch cares about.

Behaviour:
- Call `/api/v0/models` first. If it returns 200 with a parseable body, use it as the source of truth.
- On 404 / connection error / parse failure, fall back to the existing `/v1/models` path. This preserves compatibility with older LM Studio versions that don't expose the native API.
- Timeout stays at the existing 3s budget; don't slow the spawn dialog down for users without LM Studio running.

**2. Propagate context window in `ModelInfo`.** Extend `ModelInfo` with an optional `contextWindow` (and possibly `maxContextWindow`). For entries coming from `/api/v0/models`, populate from `loaded_context_length` when the model is loaded, or from `max_context_length` when it is not. For `/v1/models` fallback entries, leave the field unset.

**3. Pre-fill behaviour in `SpawnDialog.svelte`.** When the user selects an LM Studio model whose `ModelInfo.contextWindow` is populated, set the context window input to that value. Track whether the user has manually edited the input so that subsequent model selections don't clobber explicit overrides. If the field is empty because the selected model's value is missing, fall back to the MON-8 default (empty → user types, or the last-used value).

**4. Loaded-vs-not-loaded decision.** If `/api/v0/models` reports a model with a usable `loaded_context_length`, use it directly. If the model is known but not loaded (no `loaded_context_length`, only `max_context_length`), the plan's default is to pre-fill with `max_context_length` so the user gets a ceiling that won't under-report, with a subtle hint in the UI that the model isn't loaded yet. Alternative: require the model to be loaded before auto-filling, and fall back to manual input otherwise. Open question.

**5. Docs.** Short update to `ONBOARDING.md`'s LM Studio section noting the auto-detection and the fallback.

## Open questions

1. **Handling not-yet-loaded models.** Pre-fill with `max_context_length`, or leave blank and make the user type? Pre-filling is friendlier but risks misleading users if LM Studio ultimately loads the model at a smaller context. Leaning pre-fill-with-max + small UI hint.
2. **Should auto-detected values be treated as authoritative at send time?** Alternative design: skip the input entirely and have the sidecar re-query `/api/v0/models` right before `session.setModel` so the live value is always used. More robust against "user loaded the model after spawning", but more coupling and an extra HTTP call on every spawn. Recommend sticking with spawn-time discovery + manual override for simplicity.
3. **Do we want to refresh auto-detected values across a session?** If the user reloads a model in LM Studio at a different context length mid-session, Monarch won't know. In scope for this issue, or a future polling/watch concern? Recommend out of scope (one-shot discovery at spawn).
4. **LM Studio API surface stability.** `/api/v0` implies a versioned, not-yet-stable endpoint. Worth a quick note on how to adapt if LM Studio ships `/api/v1`. Mostly a "don't bake the path as a constant in too many places" concern.

## Out of scope reminders

- Any change to the live-context meter numerator (MON-8).
- Any change to the session-lifetime billing readout (MON-8).
- Polling LM Studio for live context-length changes mid-session.
- Auto-loading models into LM Studio on Monarch's behalf.
- Non-LM-Studio providers.

## Dependencies

- **Blocked by MON-8.** This plan assumes the context-window input in `SpawnDialog.svelte`, the `contextWindow` field on the sidecar `CreateSessionCommand`, and the meter denominator wiring all already exist. MON-9 only adds a data source that populates the existing input.
