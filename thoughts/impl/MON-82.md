# MON-82 — Classifier & Complexity Pill (Slice 1)

## What was implemented

Slice 1 of the Quest system now ships a best-effort classifier on every user turn and surfaces the result inline in the transcript.

Concretely:

- The sidecar now forks a one-shot classification request alongside every `prompt` when the feature is enabled. The Pi turn remains the primary path; classification resolves independently and never blocks `session.prompt()` / `session.followUp()`.
- `sidecar/src/classifier.ts` owns the classifier prompt, structured-JSON parsing, timeout handling, and primary → fallback provider policy. Supported providers in this slice are Anthropic and LM Studio.
- Rust added a global `~/.config/monarch/classifier.toml` config surface via `classifier_config.rs`. The config is resolved on demand per prompt and shipped to the sidecar inside the prompt command payload, so there is no sidecar-side file watching or cache invalidation story to maintain.
- The sidecar now mints one `classification_id` per user turn and keeps a per-agent FIFO queue so the next user-role `message_end` can be annotated with that id. Rust uses the exact id to backfill `classifications.message_id` after the user message row lands.
- SQLite gained a new `classifications` table plus read-only commands for listing classifications by agent and looking one up by `message_id`. Successful rows store complexity / confidence / rationale / usage metadata; failed rows store `error` with `complexity = NULL`.
- Rust's sidecar event handler persists classification events through the existing MON-37 single-consumer pipeline and rebroadcasts them on a dedicated `agent-classification-{id}` channel.
- The frontend now renders a `ClassificationPill` beside each user message, with a popover showing rationale, model, token counts, latency, or failure details.
- A new `classifierStore` keeps per-agent classification results in a lightweight map keyed by user-message ordinal. On restore, `AgentView` hydrates the map from SQLite so historical conversations show their pills too.
- The toolbox gained `ClassifierSettingsTool.svelte`, exposing the global toggle, primary/fallback provider+model ids, timeout, config file path, and the read-only classifier system prompt.
- `CLAUDE.md` and `ONBOARDING.md` were updated with the new files, schema table, event channel, and config file.

## Key decisions

- **Use `pi-ai`'s `complete()` but resolve auth through the session's `modelRegistry`.** The initial implementation implicitly relied on environment-variable fallback for credentials. That was wrong for the normal Monarch flow, where Anthropic auth commonly comes from Pi's stored auth file. The shipped fix explicitly calls `session.modelRegistry.getApiKeyAndHeaders(model)` before `complete()`, so classifier requests honor the same credential source as the main Pi runtime.
- **Rust owns classifier config IO.** The sidecar does not read `classifier.toml` directly. Rust already owns config-dir conventions and Tauri command exposure, so the resolved config is attached to each `prompt` payload instead.
- **Failures are persisted, not dropped.** Rather than faking a `failed` complexity enum, the DB row stores `error` and leaves `complexity` null. That keeps the success enum clean while still preserving observability and UI history.
- **Exact id pairing for DB linkage; ordinal pairing for UI rendering.** Persistence uses the sidecar-minted `classification_id` so the `message_id` FK backfill is exact. The frontend deliberately avoids threading that id through `LiveAgentState`; instead it maps live classification events to user-message ordinals in FIFO order and rehydrates historical rows by `message_id` when a session is rebound.
- **No separate `classification_pending` event in Slice 1.** The original plan left room for a pending pill state. The shipped implementation skips that extra event and only renders once a success/failure event arrives, which kept the protocol smaller and avoided more UI state plumbing.
- **Settings UI is global and intentionally simple.** Slice 1 stops at raw provider/model inputs plus timeout/toggle controls. It does not yet reuse the spawn dialog's richer provider-aware model picker.

## Files touched

- `sidecar/src/classifier.ts`
- `sidecar/src/runtime-manager.ts`
- `sidecar/src/protocol.ts`
- `sidecar/src/index.ts`
- `src-tauri/src/classifier_config.rs`
- `src-tauri/src/sidecar_protocol.rs`
- `src-tauri/src/agent/manager.rs`
- `src-tauri/src/agent/event_handler.rs`
- `src-tauri/src/agent/persist.rs`
- `src-tauri/src/agent/mod.rs`
- `src-tauri/src/db.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/src/ws.rs`
- `src/lib/AgentView.svelte`
- `src/lib/MessageList.svelte`
- `src/lib/ClassificationPill.svelte`
- `src/lib/classifier-types.ts`
- `src/lib/classifierStore.svelte.ts`
- `src/lib/toolbox/tools/ClassifierSettingsTool.svelte`
- `src/lib/toolbox/registry.ts`
- `src/lib/bindings.ts`
- `CLAUDE.md`
- `ONBOARDING.md`

## What was left out

- **Architect consumption of the label.** The classifier is advisory only in this slice. Nothing in the runtime reacts to `decomposable` / `delegate` yet.
- **Monarch override controls on the pill.** The pill is read-only; override UX is deferred until a later slice has a consumer for the label.
- **Pending-pill UX.** There is no immediate shimmer / pending state before the classifier event resolves.
- **Provider-aware pickers in the settings tool.** Primary/fallback provider and model are currently free-form text fields, not auth-aware dropdowns backed by `get_models`.
- **Per-agent classifier config.** Configuration is global for the whole app.
- **Calibration / shadow-model recording.** The plan's optional dual-run agreement hook (`MONARCH_CLASSIFIER_SHADOW_MODEL`) was not implemented.

## Verification

- `npm test`
- `cd src-tauri && cargo check`
