# MON-35 — Re-enable specta coverage for spawn_agent

## Summary

`spawn_agent` is the most important command in the app but it is the one command that specta does not generate a typed wrapper for. `src-tauri/src/lib.rs` intentionally omits it from `specta_builder` because the current signature takes 13 arguments (three state extractors + ten value params) and specta's `SpectaFn` trait caps at 10. As a result `bindings.ts` has no `spawnAgent()` export and `App.svelte` dispatches it via a raw `invoke("spawn_agent", { ... })` string call. This plan collapses the value params into a single `SpawnAgentRequest` struct, mirrors the change onto `ws_spawn_agent`, re-registers the command with specta, and switches the two frontend call sites to the generated wrapper — closing the last typed-surface hole in MON-14 Phase 1.

## Relevant files and areas

- `src-tauri/src/lib.rs`
  - Lines 26-43: doc comment explaining the omission and the `specta_builder()` command collection. `agent::spawn_agent` needs to re-appear in the `collect_commands![]` list, and the doc comment needs to drop the "omitted" note (or replace it with a one-line pointer to the request struct).
  - Lines 102-138: `export_bindings()` — contains the post-processing patch that injects `type Value = unknown; type Vec<T> = T[];` into `bindings.ts`. We need to audit whether this hack is still required after this refactor (see Open Questions).
  - Lines 160-211: runtime `tauri::Builder::default()...invoke_handler!` still registers `agent::spawn_agent` via `tauri::generate_handler!` and is unaffected structurally, but the doc comment on lines 160-163 should be updated to reflect the new signature.

- `src-tauri/src/agent.rs`
  - Lines 1382-1508: `spawn_agent` Tauri command. The ten value params (`id`, `session_id`, `provider`, `model`, `thinking_level`, `cwd`, `shadow_name`, `shadow_title`, `shadow_grade`, `context_window`) all land in `SidecarCommand::CreateSession` plus the DB upsert. This is the primary site to collapse.
  - Lines 1466-1488: already constructs a `ShadowConfig` from the three shadow fields — the request struct can either mirror the flat `shadow_name/title/grade` triple or host a nested `ShadowSpec` that matches it. The existing `ShadowConfig` in `sidecar_protocol.rs` is the canonical name and uses camelCase serde; the request struct's shadow shape should match that so the mapping is trivial.
  - Lines 1785-1880: `ws_spawn_agent` — same shape minus `context_window` (it's currently hardcoded to `None` at line 1863 — the request struct fix naturally surfaces `context_window` to the WS path too, matching the Tauri command).

- `src-tauri/src/ws.rs`
  - Lines 177-197: `dispatch_command` → `"spawn_agent"` arm currently extracts each field via `str_field` / `opt_str`. After the refactor, this arm should decode the `args: Value` as `SpawnAgentRequest` in a single `serde_json::from_value` call and hand it to `ws_spawn_agent`.

- `src-tauri/src/sidecar_protocol.rs`
  - Lines 29-36: `ShadowConfig` camelCase serde shape. The request struct's shadow field can reuse this type directly or a sibling `ShadowSpec` with the same fields — the translation into the outbound `CreateSession` command is already constrained by this shape.
  - Lines 61-115: `SidecarCommand::CreateSession` variant. This is the natural inner representation that `SpawnAgentRequest` maps into. MON-32 already fully typed it; this ticket does not touch it.

- `src/App.svelte`
  - Line 304: primary `invoke("spawn_agent", { ... })` call in `createAgent`. Payload mirrors the ten params exactly.
  - Line 442: lazy-spawn path for stopped agents — same ten-field payload.
  - Both sites currently do `await invoke(...)` / `.then/.catch` with untyped shapes; they should switch to `import { commands } from "$lib/bindings"` (or whatever the existing wrapper import style is in this repo — see Open Questions on shape naming) and call the generated function.

- `src/lib/bindings.ts`
  - Currently auto-generated; `spawnAgent` is absent. Regeneration after the Rust change via `cargo run -- --export-bindings` (per `lib.rs:102-108`) should produce a typed wrapper. Not edited by hand.

- `src/lib/api.ts` — the invoke shim that routes typed commands through WS or the Tauri bridge depending on environment; already referenced by `lib.rs:129-132` as the rewritten import target. No changes expected but worth sanity-checking that it works when a command takes a struct argument (same serde round-trip as existing typed commands — should just work).

- MON-32 plan/impl notes in `thoughts/plan/MON-32.md` and `thoughts/impl/MON-32.md` — useful context for how the typed-command migration has been framed so far; MON-35 is the last bullet of that line of work.

- `thoughts/plan/MON-14-phase-1.md` — notes the "spawn_agent omitted, fix in Phase 2" decision that this ticket is closing out.

## What needs to change

1. **Define `SpawnAgentRequest`** in `agent.rs` (or a new `agent::commands` submodule if preferred — suggest agent.rs for proximity to the command itself). `#[derive(Debug, Deserialize, specta::Type)]` with `#[serde(rename_all = "camelCase")]` so it round-trips with the existing JS payload field names. Carry every current value param: `id`, `session_id`, `provider: Option<String>`, `model: Option<String>`, `thinking_level: Option<String>`, `cwd: Option<String>`, the three shadow fields (flat or nested — see Open Questions), `context_window: Option<i32>`. Do not reuse `SidecarCommand::CreateSession` as the argument type: it's the outbound protocol shape, not the inbound request shape, and fields like `id` / `session_id` and the `Option`-ful fallbacks don't exist in it.

2. **Rewrite `agent::spawn_agent`** to take `(app, state, db, req: SpawnAgentRequest)`. Move the field-by-field destructure to the top of the body so the rest of the function (DB upsert, session exists check, session map insert, `SidecarCommand::CreateSession` build, `AgentState` insert) reads almost identically. The effective-context-window fallback at lines 1415-1421 stays as-is, just keyed off `req.context_window`.

3. **Rewrite `agent::ws_spawn_agent`** in parallel with the same `SpawnAgentRequest` arg. Do NOT factor out a shared helper — the MON-32 handoff explicitly reserved a shared service layer for MON-33. The two functions should stay as close copies, just with the arg collapsed. Also fix the inconsistency where the WS path hardcodes `context_window: None`; with the struct in place it naturally flows through.

4. **Re-register `agent::spawn_agent`** inside `specta_builder()` in `lib.rs` (add it to the `collect_commands![]` list, remove the `// NOTE: agent::spawn_agent omitted` line, and update the doc comment on lines 26-34 to no longer reference the 10-arg cap issue). Runtime `tauri::generate_handler!` registration is unchanged — the command is still a regular Tauri command, just with a different arg shape.

5. **Update `ws::dispatch_command`'s `"spawn_agent"` arm** to do a single `serde_json::from_value::<SpawnAgentRequest>(args)?` decode and pass the value to `ws_spawn_agent`. This removes the per-field `str_field`/`opt_str` extraction block and makes the wire contract single-source-of-truth.

6. **Regenerate `src/lib/bindings.ts`** via `cargo run -- --export-bindings`. Confirm `spawnAgent` appears as a typed wrapper and that `SpawnAgentRequest` is exported as a type.

7. **Swap the two `App.svelte` call sites** (lines 304 and 442) to use the generated typed wrapper. This should be a mechanical rename from `invoke("spawn_agent", { ... })` to `commands.spawnAgent({ ... })` (or whatever the generated symbol name ends up being). The object shape is already camelCase, so no field-name translation should be needed. Double-check that the lazy-spawn site also passes `contextWindow` — it currently does via `agent.contextWindow ?? null` on line 446, so it stays.

8. **Audit the `Value = unknown` / `Vec<T> = T[]` post-processing hack** in `lib.rs:110-123`. Grep regenerated `bindings.ts` for bare `Value` / `Vec<` references after the refactor. Per the MON-32 handoff, `LiveAgentState` and the typed sidecar event protocol still reference `serde_json::Value`, so the hack likely still has at least one consumer. Document the finding in the PR description and in the impl notes; **do not delete the hack unilaterally**. If it turns out to be genuinely dead weight post-change, the reviewer can be asked to bless removing it in the same PR; otherwise punt to a follow-up ticket. The handoff already flagged this as a negotiation point.

## Resolved decisions

1. **Shadow shape: nested.** `SpawnAgentRequest` carries `shadow: Option<ShadowSpec>`. The frontend already builds a nested `config.shadow` object in `App.svelte`, so nesting on the wire matches the native shape and removes the flatten/unflatten dance.

2. **`ShadowSpec` is a new type, not a reuse of `sidecar_protocol::ShadowConfig`.** Define `ShadowSpec { name, title, grade }` (camelCase serde, `Deserialize + specta::Type`) alongside `SpawnAgentRequest` in `agent.rs`. `ShadowConfig` stays the sidecar-facing type; `spawn_agent` maps `ShadowSpec` → `ShadowConfig` inside the command body and injects the synthesized `id` there.

3. **`Value = unknown` / `Vec<T> = T[]` patch: keep it, verify empirically.** Expected to still be required because `LiveAgentState` and the typed MON-32 sidecar protocol still reference `serde_json::Value`. Regenerate `bindings.ts`, grep for bare `Value` / `Vec<` references, document the finding in the PR. Deletion is a follow-up ticket if it turns out to be dead.

## Open questions

1. **Generated wrapper symbol name.** tauri-specta emits either a flat `spawnAgent` function or a `commands.spawnAgent` namespace depending on version/config. Read one line of the currently-generated `bindings.ts` before editing `App.svelte` to match the established import style. Not a design decision — mechanical lookup during impl.

## Out of scope reminders

- Extracting a shared spawn service layer shared between `spawn_agent` and `ws_spawn_agent` — reserved for MON-33. The two call sites stay as close duplicates.
- Fixing `respond_extension_ui` or any other command that also contributes to the `Value = unknown` workaround.
- Any sidecar-side protocol changes. `SidecarCommand::CreateSession` was fully typed in MON-32 and is the canonical inner shape; nothing about it changes here.
- Broader refactor of `App.svelte`'s agent creation flow. The scope is: swap two `invoke(...)` calls to typed wrapper calls, nothing else.
- Removing the `Value = unknown` hack unconditionally. The acceptance bullet is "decide and document"; deletion only lands in this PR if the empirical audit proves it is genuinely unused now.
