# MON-35 — Re-enable specta coverage for spawn_agent

- **Linear:** https://linear.app/monarch-commander/issue/MON-35
- **Parent:** MON-14 (Phase 1 Rust state ownership)
- **Wave:** 2, step 3 of 5 (MON-31 → MON-32 → **MON-35** → MON-33 → MON-34)
- **Base:** `markocvijanovic1998/mon-14-phase-1-rust-state-ownership` (tip `6f34ff6`)

## What was implemented

`spawn_agent` used to take 13 arguments (three state extractors + ten value params) and was the only Tauri command excluded from `specta_builder`'s `collect_commands!` list — specta's `SpectaFn` trait caps at 10. That made the single most critical command in the app the only one without a typed frontend wrapper, and `App.svelte` dispatched it via a stringly-typed `invoke("spawn_agent", {...})`. This change collapses the ten value params into a single `SpawnAgentRequest` struct (with a nested `ShadowSpec` for the shadow identity block), mirrors the same collapse onto `ws_spawn_agent` and the WS dispatch arm, and re-registers the command with specta. Regenerated `bindings.ts` now exports `commands.spawnAgent(req: SpawnAgentRequest)`, and both `App.svelte` call sites route through it.

This closes the last hole in the Phase 1 typed command surface for the agent lifecycle.

## Key decisions

- **Nested `ShadowSpec`, not flat.** The request struct exposes `shadow: Option<ShadowSpec>` where `ShadowSpec { shadow_name, shadow_title, shadow_grade }`. Nesting matches the frontend's existing `ShadowIdentity` shape — `config.shadow` passes through the typed call site as a single value, no flatten/unflatten dance. Kept the `shadow_*` field names (serializing to `shadowName / shadowTitle / shadowGrade`) so the TS type lines up structurally with `ShadowIdentity`.
- **New `ShadowSpec` type, not a reuse of `sidecar_protocol::ShadowConfig`.** `ShadowConfig` carries an `id` field the backend synthesizes from the agent id, so reusing it would force the frontend to pass a redundant identifier. `SpawnAgentRequest` handlers map `ShadowSpec` → `ShadowConfig` inside the command body and inject `id` there.
- **`Value = unknown; Vec<T> = T[]` patch kept.** Post-regeneration grep showed the hack is still referenced by `respondExtensionUi` (takes a `serde_json::Value`), `StreamingMessage.content`, and `ToolExecution.args/result` — none of which are spawn_agent's to fix. The plan already anticipated this; the decision is documented on the acceptance-criterion bullet in the tracker.
- **No shared helper between `spawn_agent` and `ws_spawn_agent`.** The plan explicitly reserved that extraction for MON-33. Both functions are now close copies with identical `SpawnAgentRequest` args — the cleanest possible starting shape for MON-33's service-layer collapse.
- **Drive-by fix on `ws_spawn_agent.context_window`.** The WS path previously hardcoded `context_window: None`, silently dropping the caller's value. Collapsing to `SpawnAgentRequest` surfaced the inconsistency; the WS path now mirrors the Tauri command's "use the request value, else fall back to the value persisted on the agent row" logic. Flagged in the PR body.

## Files touched

- `src-tauri/src/agent.rs` — added `SpawnAgentRequest` + `ShadowSpec` structs; rewrote `spawn_agent` and `ws_spawn_agent` to destructure a single `req` argument; `ws_spawn_agent` now honours the caller's `context_window`.
- `src-tauri/src/ws.rs` — `dispatch_command`'s `"spawn_agent"` arm collapsed to a single `serde_json::from_value::<SpawnAgentRequest>`.
- `src-tauri/src/lib.rs` — `agent::spawn_agent` re-added to `specta_builder`'s `collect_commands!`; stale "omitted, specta arg cap" doc comments rewritten.
- `src/lib/bindings.ts` — regenerated via `cargo run -- --export-bindings`. Adds `commands.spawnAgent`, `SpawnAgentRequest`, and `ShadowSpec`.
- `src/App.svelte` — imports `commands` from `./lib/bindings`; both call sites (`createAgent` and the lazy-spawn restart path) now go through `commands.spawnAgent({...})` with the nested `shadow` slot passed straight from the existing `ShadowIdentity` shape.
- `thoughts/plan/MON-35.md` — research plan (committed before implementation).
- `thoughts/impl/MON-14-cleanup.md` — Wave 2 tracker entry: MON-35 marked as "PR open", scope-deviation note on the hack bullet, checkpoint note recording the starting point MON-33 will inherit.

## What was left out

- **Deleting the `Value = unknown; Vec<T> = T[]` patch.** The ticket's acceptance bullet was "decide and document", not "delete unconditionally" — and the empirical audit confirmed the hack is still load-bearing for commands and types outside `spawn_agent`'s blast radius. Follow-up work (likely a specta upgrade or wrapping `serde_json::Value` fields in specta-aware newtypes) is parking-lot material for a separate ticket.
- **Shared service layer between `spawn_agent` and `ws_spawn_agent`.** Reserved for MON-33 per the Wave 2 plan. The two functions are left as close copies; the duplication is intentional for one more ticket.
- **Broader refactor of `App.svelte`'s agent creation flow.** Scope was "swap two `invoke` calls to typed wrapper calls, nothing else" — no restructuring of `createAgent`, the lazy-spawn path, or the restore flow.
- **`respond_extension_ui` or other commands that still reference `Value`.** Not this ticket.
