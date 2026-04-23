# MON-82 — Classifier & Complexity Pill (Slice 1)

## Summary

Slice 1 of the Quest system, per `plans/quests.md`. Adds a sidecar-side Classifier that runs in parallel with every user turn, labeling the prompt as `chitchat` | `simple` | `decomposable` | `delegate`. Default model is Anthropic Haiku; LM Studio is a configurable fallback; the choice is swappable on the fly via a new config surface. The classifier emits a sidecar event that Rust persists into a new `classifications` table and rebroadcasts on `agent-classification-{agentId}`; the frontend renders a read-only pill next to each user message. Classification is best-effort — failures and timeouts log a "failed" pill state but never block the Pi turn. Monarch override is deferred to Slice 3 when the label gains a consumer (the Architect).

Design anchor: MON-83 (Slice 2) shipped `quest_nodes` / `quest_events` + a new toolbox tool the week before. This plan mirrors MON-83's patterns (post-launch migration, `emit_event` crate-wide, specta + WS-dispatch mirroring, reactive per-agent store) wherever they apply.

## Relevant files and areas

### Sidecar (Node, TypeScript)

- `sidecar/src/runtime-manager.ts` — `prompt()` at line 356 is the single entry point for user turns. Slice 1 forks the classifier here, in parallel with `session.prompt()` / `session.followUp()`. Model-resolution helpers already in place (`buildDynamicModel`, `resolveModel`, `ensureLmStudioProviderRegistered`, lines 84–185) are reused verbatim for LM Studio path.
- `sidecar/src/protocol.ts` — add a `ClassificationEvent` variant to the sidecar→Rust event union.
- `sidecar/src/classifier.ts` (new) — module exposing `classify(message, config) → { complexity, confidence, rationale, model, tokensIn, tokensOut, latencyMs }`. Owns prompt, model dispatch, JSON-schema output parsing, and timeout wrapping. Internally: Haiku via `pi-ai`'s existing Anthropic provider; LM Studio via the existing dynamic-model path.
- `sidecar/src/shadow-oath.ts` — untouched in Slice 1 (Quest Protocol additions live in Slice 3+).

### Rust (Tauri backend)

- `src-tauri/src/sidecar_protocol.rs` — add `SidecarEvent::Classification { … }` mirroring the TS type.
- `src-tauri/src/agent/event_handler.rs` — new match arm: enqueue classification persistence + rebroadcast on `agent-classification-{id}`. Reuses `emit_event` (promoted `pub(crate)` in MON-83).
- `src-tauri/src/agent/persist.rs` — extend `PersistCommand` with `SaveClassification` and `BackfillClassificationMessageId`. The existing user-role `MessageEnd` handler (lines 236–280) grows a post-save hook that enqueues the backfill.
- `src-tauri/src/db.rs` — add `classifications` table as an idempotent post-launch migration (mirrors the MON-83 block at lines 243–310). Add `ClassificationRow`, row mapper, `save_classification_internal`, `backfill_classification_message_id`, `list_classifications_for_agent`, `get_classification_for_message`. Writes funnel through the persist pipeline; reads go direct.
- `src-tauri/src/lib.rs` — register new Tauri commands in `generate_handler!` and specta type exports.
- `src-tauri/src/ws.rs` — dispatch arms for the new commands.
- `src-tauri/src/classifier_config.rs` (new) — loads/saves `~/.config/monarch/classifier.toml`, mirroring `thinking_config.rs` precedent. Exposes a Tauri command so the sidecar can fetch the active config at classify time (or the Rust side can pass it in on each `prompt` command — decision in impl).
- `src-tauri/src/models.rs` — untouched. Anthropic auth via `~/.pi/agent/auth.json` is reused. Local model discovery for the classifier picker reuses existing LM Studio model listing.

### Frontend (Svelte 5)

- `src/lib/MessageList.svelte` — render pill next to user messages. Per-message classification lives on the existing `DisplayItem` (extend the type with an optional `classification?: ClassificationInfo`).
- `src/lib/toolbox/liveAgentStore.svelte.ts` — subscribe to `agent-classification-{id}`; on event, attach to the most-recent user item matching the sidecar-provided `classification_id`.
- `src/lib/ClassificationPill.svelte` (new) — color coded by complexity, click reveals rationale/model/tokens/latency popover. Read-only in Slice 1. States: `pending`, `ok`, `failed`.
- `src/lib/toolbox/tools/ClassifierSettingsTool.svelte` (new) — toolbox tool for model picker (Haiku vs configured LM Studio model), on/off toggle, timeout slider. Reads/writes `classifier.toml` through the new Tauri commands.
- `src/lib/toolbox/registry.ts` — register the settings tool.
- `src/lib/api.ts` + `src/lib/bindings.ts` — bindings regenerated via `cargo run -- --export-bindings`.

### Plan / doc

- `thoughts/plan/MON-82.md` (this file) — committed before any code lands.
- `thoughts/impl/MON-82.md` — written at the end of implementation.
- `CLAUDE.md` — update Start Here table (new classifier files, new config file), post-launch schema gotcha (add `classifications`), new event channel `agent-classification-{id}`.
- `ONBOARDING.md` — data model + protocol sections.

## What needs to change

### Parallel classifier trigger

The sidecar `prompt()` today awaits `session.prompt()` / `session.followUp()` sequentially. Slice 1 splits the entry point into two concurrent tasks kicked off the instant a user message arrives: (a) the existing Pi turn, unchanged; (b) a new `classify()` call. The classifier task races independently and emits its event when ready; the Pi turn never blocks on it. A timeout (default 3s) wraps the classifier call — on timeout or throw, emit a `failed` classification event and log. Config toggle `enabled = false` skips the fork entirely.

### Classifier module

New `sidecar/src/classifier.ts` owns prompt, model dispatch, structured-JSON output parsing, and timeouts. Single entry point `classify(message, config)`. Two provider paths:

- **Haiku (default).** Uses `pi-ai`'s existing Anthropic provider for a one-shot completion. Reuses credentials already in place for the main agent runtime — no new auth plumbing. If `pi-ai` doesn't expose a lightweight one-shot helper, the fallback is to add `@anthropic-ai/sdk` as a sidecar dep; the cost is a small extra dep, bearable. Decision in impl after reading `sidecar/node_modules/@mariozechner/pi-ai`.
- **LM Studio (fallback / opt-in).** Reuses `buildDynamicModel` + `ensureLmStudioProviderRegistered`. Issues a one-shot completion against the user-selected local model. Enforces JSON-only output through the prompt.

Provider resolution is policy-driven: try the configured primary; on error or timeout, try the configured fallback; on double failure, emit a `failed` classification. The prompt is tuned to bias toward escalation — ambiguous cases route to `decomposable` rather than `simple`. Ship with a small fixtures file for hand-validation during impl.

### Config surface

New `classifier.toml` at `~/.config/monarch/`, mirroring `thinking.toml`. Shape (illustrative, not prescriptive):

- `enabled` — bool, default `true`
- `primary` — `{ provider: "anthropic" | "lmstudio", model: string }` — default `{ anthropic, "claude-haiku-4-5" }` (or whichever Haiku is current-stable)
- `fallback` — same shape, nullable — default `null`
- `timeout_ms` — number, default `3000`

A small `ClassifierSettingsTool.svelte` lets the Monarch pick primary/fallback models and toggle `enabled`. Model lists are populated from existing endpoints: Anthropic curated list from `models.rs`, LM Studio loaded-models from the existing discovery path. Changes take effect on the next turn — no live-reload complexity.

### Data model

New `classifications` table as a post-launch migration block in `db::init_schema`:

- `id TEXT PRIMARY KEY` — UUID v4, generated sidecar-side and threaded through the event.
- `message_id INTEGER REFERENCES messages(id)` — nullable; filled in by backfill.
- `agent_id TEXT NOT NULL`
- `session_id TEXT` — nullable (mirrors `persist.rs` guard at line 231 — classifier may fire before a session exists).
- `complexity TEXT NOT NULL CHECK (complexity IN ('chitchat','simple','decomposable','delegate'))`
- `confidence REAL`
- `rationale TEXT`
- `model TEXT`
- `tokens_in INTEGER`
- `tokens_out INTEGER`
- `latency_ms INTEGER`
- `created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))`

Indexes: `(agent_id, created_at)` and `(message_id)`.

Write path: classifier event → Rust event handler → `PersistCommand::SaveClassification` (via the MON-37 pipeline) → DB insert with `message_id = NULL`. User `MessageEnd` persistence emits a follow-up `PersistCommand::BackfillClassificationMessageId { classification_id, message_id }`. Linking key is the sidecar-generated `classification_id` threaded through both events so there is no "match most recent" race.

In the typical case Pi echoes the user-role `MessageEnd` before the classifier returns, so the classification row is inserted with an already-known message id (impl can short-circuit the two-phase write). Edge case where classification arrives first: insert with null, backfill on MessageEnd — same ordering still resolves cleanly because the key is exact, not "latest unlinked".

### Event channel

New sidecar event `ClassificationEvent` mirrors between `sidecar/src/protocol.ts` and `sidecar_protocol.rs`. New frontend channel `agent-classification-{agentId}` emitted from the Rust event handler through the existing dual Tauri+WS `emit_event(app, ws_tx, ...)` helper, consistent with `agent-state-{id}` / `agent-event-{id}`. Payload carries the full row plus a `classification_id` for the frontend to reconcile pending → resolved transitions.

### Frontend pill & store wiring

`ClassificationPill.svelte` — small color-coded chip (one color per complexity) showing label + rounded confidence %. Click expands a popover with rationale / model / tokens / latency. Three states: `pending` (shimmer while the turn is live and no event has landed yet), `ok` (full data), `failed` (greyed, tooltip explains). Read-only.

`MessageList.svelte` mounts the pill next to each user message, keyed by the `classification_id` attached to the live-store `DisplayItem`. The live store's user-item shape gains an optional `classification?: ClassificationInfo` field; the `agent-classification-{id}` listener updates the matching item.

Sidecar emits a second, earlier event at turn-start — `classification_pending` with the `classification_id` — so the pill can render in `pending` state immediately rather than waiting for resolution. This is a tiny addition that makes the UX feel instant; if it's redundant in impl (e.g. `classification_id` can be stapled to the user `MessageEnd`), skip it.

### Failure modes (all observable, never blocking)

- Both providers unreachable → emit `failed` classification event. Persist an audit row with `complexity = 'failed'`? Or skip persistence and only show the UI state? Lean: persist with a new `error` column rather than stuffing `complexity`. To decide in impl.
- Timeout (3s default) → same as failure.
- Model returns malformed JSON → same as failure.
- Config disabled → classifier task not spawned; no pill rendered at all.

### Calibration hook (placeholder only)

Env flag `MONARCH_CLASSIFIER_SHADOW_MODEL` runs a second model in parallel purely to record a second `classifications` row for later agreement analysis. Not wired in Slice 1 unless impl finds it a 30-line addition. Callout exists so future calibration work has a clean seam.

## Open questions

1. **pi-ai one-shot helper.** Does `pi-ai` expose a lightweight one-shot completion separate from `session.prompt`? If not, reusing a session-like object may be cheap enough; else add `@anthropic-ai/sdk` direct in the sidecar (small, well-maintained dep). Resolve during impl by reading `sidecar/node_modules/@mariozechner/pi-ai`.

2. **Config location — global vs per-agent.** `thinking.toml` precedent is global. The classifier arguably has per-agent value (some agents are code-heavy, others chitchat). Lean global for Slice 1; per-agent override is a cheap follow-up if usage demands.

3. **Config-plumbing flow.** Two shapes: sidecar reads `classifier.toml` itself (simpler, sidecar tails the file) vs Rust reads and passes the resolved config on every `prompt` command payload (centralizes I/O in Rust, matches existing patterns). Lean Rust-passes-on-each-prompt — cheaper plumbing, consistent with how `thinking_level` already flows.

4. **Failure persistence.** Persist failures as rows (with an `error` column, null `complexity`) or drop entirely? Persisting is useful for calibration; dropping is simpler. Lean persist with an error column.

5. **Rationale length.** Classifier can ramble. Cap at ~200 chars? Lean uncap for Slice 1; the volume is small.

6. **Pill visual language.** Borrow palette from existing status/grade badges in `QuestTimelineTool.svelte` or introduce a dedicated complexity palette? Defer to impl; cosmetic.

7. **Slice 2 bleed.** MON-83's `questStore` pattern is the freshest example of per-agent reactive state. The classifier doesn't need its own store (pill lives on existing user-message items), but the settings tool may warrant a small module. Settle in impl.

## Out of scope

- **Architect invocation on `decomposable` / `delegate`.** Slice 3 / MON-84.
- **Steward**, always-on observer, drift detection, `claim_complete`. Slice 4.
- **Monarch override dropdown** on the pill. Deferred to Slice 3 when the label has a consumer.
- **Quest-tree writes or reads.** Untouched here.
- **Orchestrator context injection**, `request_replan` tool. Later slices.
- **Embedding-distance "new topic" gate** to reduce classifier firings. Plan doc defers this; every turn fires.
- **Parallel-run-both-models calibration** as a shipping feature. Dev-flag placeholder only.
- **Shadow Oath additions** (Quest Protocol in `shadow-oath.ts`). First needed in Slice 3.
