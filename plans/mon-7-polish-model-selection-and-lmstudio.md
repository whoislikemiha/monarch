# MON-7 — Polish model selection and add LM Studio provider support

- **Linear:** [MON-7](https://linear.app/monarch-commander/issue/MON-7/polish-model-selection-and-add-lm-studio-provider-support)
- **Branch:** `markocvijanovic1998/mon-7-polish-model-selection-and-add-lm-studio-provider-support`

## Summary

Add LM Studio as a first-class provider so users can spawn shadows backed by a locally running LM Studio server (OpenAI-compatible at `http://127.0.0.1:1234/v1`), and polish the existing SpawnDialog model picker so its loading / empty / error / auth-missing states are clear and its behavior across provider switches is solid.

Also introduce lightweight **agent templates** — reusable presets capturing provider + model + thinking level + shadow identity + cwd — so spawning multiple similar agents no longer requires filling in the dialog from scratch each time.

A related cleanup: `ONBOARDING.md:338` already documents LM Studio as if it were implemented via a `registerCustomProviders()` sidecar function — but grep shows neither the function nor the string `lmstudio` exists anywhere in the codebase. The docs are stale and need to be brought in line with what we actually ship.

## Relevant files and areas

- **`src-tauri/src/models.rs`** — owns the `get_models` Tauri command and model discovery. Today it hardcodes three provider arms (`anthropic`, `openai-codex`, `openrouter`) and caches OpenRouter results. It also exposes `get_provider_auth_status` for Pi-auth-backed providers. This is where LM Studio discovery (`/v1/models` fetch against the configured base URL) needs to live. Cache policy for LM Studio should be very short or nonexistent since the loaded-model set changes whenever the user loads/unloads.
- **`src/lib/SpawnDialog.svelte`** — the provider dropdown (`providers` array ~L28), `fetchModels` (~L93), fuzzy filter, keyboard nav, `$effect` that fires on provider change (~L132), and the fallback list for browser mode. This is the focus of the "polish" half. The `fixedModelId` pattern (used for `openai-codex`) is the precedent for provider-specific UI branches.
- **`sidecar/src/runtime-manager.ts`** — `buildDynamicModel` (~L81) currently only constructs a dynamic `Model<Api>` for `openrouter`. `resolveModel` (~L105) first tries `session.modelRegistry.find`, then falls back to the dynamic builder. This is where `lmstudio` needs a dynamic model shape. Used by both `createSession` and `setModel`.
- **`src/lib/types.ts`** — `AgentConfig.provider` is just an optional string, so no type changes are strictly required. A new `AgentTemplate` type will live here.
- **`src-tauri/src/db.rs`** — `agents` and `sessions` tables already persist `provider` / `model` as free-form text, so persistence needs no schema change for LM Studio. A new `agent_templates` table is needed for the templates feature.
- **`ONBOARDING.md`** (lines ~330–340) — "Providers" section has the stale claim about `registerCustomProviders()` and LM Studio. Needs to be rewritten after implementation to match reality.
- **`src/App.svelte`** — hosts the SpawnDialog and agent creation flow; will need a hook for "spawn from template".

## What needs to change

### LM Studio provider

1. **Backend model discovery.** Add an `lmstudio` arm to `get_models` in `src-tauri/src/models.rs` that fetches `GET {base_url}/models`, parses the OpenAI-style response, and maps it to `ModelInfo`. Base URL resolves from `LMSTUDIO_BASE_URL` env var, defaulting to `http://127.0.0.1:1234/v1`. On connection failure, return `Err(...)` with a clear message so the UI can render a "LM Studio not reachable" state distinct from "empty list".
   - If LM Studio's `/v1/models` response includes context window metadata per model, capture it into `ModelInfo` (may require extending the struct with an optional `context_window` field). Otherwise, leave it unset and let the sidecar default it.
2. **Provider auth status.** Make `get_provider_auth_status` a no-op for `lmstudio` (it has no auth). Reachability is signaled via `get_models` error, not via auth status — avoids duplicate network calls.
3. **Sidecar model resolution.** In `sidecar/src/runtime-manager.ts`, broaden `buildDynamicModel` so `provider === "lmstudio"` returns a `Model<Api>` with `api: "openai-completions"`, `baseUrl` from `process.env.LMSTUDIO_BASE_URL` (defaulting to `http://127.0.0.1:1234/v1`), zero cost struct, no reasoning, and `contextWindow` read from the frontend-supplied metadata if present, else defaulting to **32k**. Set a dummy API key of `"lm-studio"` if the Pi SDK's OpenAI-completions client requires one (it does for local custom baseUrl setups).
4. **Frontend wiring.** Add `{ label: "LM Studio", value: "lmstudio" }` to the `providers` array in SpawnDialog. No `fixedModelId`. When the discovery call errors, render a compact "LM Studio server not reachable at {baseUrl}" hint below the input with a retry button instead of the usual empty dropdown.

### Model-selection polish (conservative set)

5. **Refresh button.** Add an explicit refresh button next to the model input for providers whose lists are fetched remotely (OpenRouter, LM Studio). Hidden for static lists.
6. **State clarity.** Make loading / empty / error / auth-missing states visually distinct. Today empty and loading read almost identically.
7. **Provider-switch reset.** Audit the `$effect` that runs on provider change — ensure dropdown visibility, highlighted index, and any in-flight fetch promise are all reset cleanly so stale state from a previous provider never bleeds in.
8. **Docs.** Rewrite the "Providers" section of `ONBOARDING.md` to describe what actually exists after this change. Drop the `registerCustomProviders()` reference entirely.

### Agent templates (new feature)

9. **Data model.** Add an `agent_templates` table in `src-tauri/src/db.rs` with columns: `id`, `name`, `provider`, `model`, `thinking_level`, `cwd`, `shadow_name`, `shadow_title`, `shadow_grade`, `created_at`, `updated_at`. Write the corresponding Rust struct and persistence APIs alongside the existing `Agent` ones.
10. **Tauri commands.** Expose `list_agent_templates`, `save_agent_template`, `delete_agent_template` as Tauri commands. Register in `lib.rs`.
11. **Frontend type.** Add an `AgentTemplate` interface in `src/lib/types.ts` mirroring the DB row.
12. **SpawnDialog UX.** Add a "Save as template" button that captures the current form state and persists it. Add a template picker (dropdown or inline chip row) at the top of the dialog that, when clicked, prefills every field from the chosen template. Templates can be deleted from the picker. Keep the UI small — this is an accelerator, not the primary path.
13. **App-level shortcut (stretch).** Optionally add a "Spawn from template" entry point in the sidebar or a keybinding so power users can skip the dialog entirely when a template is already dialed in. Only include if it falls out cleanly from the SpawnDialog work; otherwise defer.

## Decisions locked in from open questions

- **Dummy API key:** sidecar injects `"lm-studio"` as the API key when building the dynamic LM Studio model.
- **Context window:** read from LM Studio `/v1/models` metadata when available, otherwise default to **32k**.
- **Not-reachable signal:** shape (a) — `get_models` returns `Err(...)` on failure; frontend distinguishes this from an empty list.
- **Polish scope:** conservative set (refresh button, cleaner states, provider-switch reset) — plus agent templates as a deliberate scope addition.
- **Browser-mode fallback:** `lmstudio` is hidden entirely when running outside Tauri (it can't reach the user's localhost from a hosted browser context reliably).

## Out of scope

- New built-in providers beyond LM Studio (Ollama, llama.cpp, vLLM, etc.)
- Persisting per-agent model preferences beyond what already exists
- Cost / context-window tracking redesign
- SpawnDialog layout redesign
- LM Studio install / server lifecycle management
- Template sharing / export / import
- Template versioning or history
