# Backend restructure plan

Goal: break the monolithic backend files into domain-scoped modules so it's obvious where things live. This is a **pure code-motion refactor** — zero behavior change, zero signature change, zero new abstractions. Every work package must end with the build green.

## Ground rules (apply to every work package)

1. **Code motion only.** Move code verbatim. Do not rename functions, change signatures, "improve" logic, add traits, or reorder fields. Fixing a `use` path is fine; rewriting a body is not.
2. **Facade re-exports keep external paths stable.** When `foo.rs` becomes `foo/`, its `mod.rs` must `pub use` everything that was previously `pub`, so `crate::foo::Bar` keeps working everywhere else. Callers outside the module should need **no changes** (in particular `lib.rs`'s `generate_handler![]` must keep resolving `db::db_*` names).
3. **Verify before finishing:** `cargo check` from `src-tauri/` must pass with no errors (warnings about unused imports must also be cleaned up). For the sidecar package: `npm run build --prefix sidecar` must pass. Frontend untouched.
4. **Commit when green** with a conventional commit, e.g. `refactor(db): split db.rs into domain modules`.
5. Do not edit `src/lib/bindings.ts` or any frontend code. Do not regenerate bindings — types don't change, so bindings don't change.

## WP1 — `db.rs` (6,830 lines) → `src-tauri/src/db/`

The largest win. Current layout: schema init (~30–885), row types (~885–1553), `impl Database` internal methods (~1561–4616), helpers + row mappers (~4616–5247), Tauri commands (~5247–end).

Target layout — each domain file holds its **row types + mappers + `impl Database` internal methods + `#[tauri::command]` wrappers** together (Rust allows multiple `impl Database` blocks across files in the same crate):

- `db/mod.rs` — `Database` struct, `new()`, `new_in_memory()`, `db_path()`, `pub use` re-exports of every public item from the submodules.
- `db/schema.rs` — `init_schema` + all idempotent migration blocks (keep order intact!).
- `db/agents.rs` — agents CRUD, archive/unarchive, agent templates, agent stats + tool usage, `compute_specialization`.
- `db/sessions.rs` — sessions, messages, attachments, `get_messages_with_ancestry`, session message counts.
- `db/projects.rs` — projects CRUD + instructions.
- `db/quests.rs` — quest nodes, quest events, quest refs, working memory (`empty_working_memory`, `load/save_working_memory_tx`, `quest_path_tx`, `close_action_tx`), `QUEST_SELECT_SQL`.
- `db/plans.rs` — P4b (MON-111) plan items: validators, `*_plan_*_tx` helpers, plan commands.
- `db/reports.rs` — MON-119 quest reports.
- `db/memories.rs` — memories, FTS search, keeper runs, mark-accessed.
- `db/identity.rs` — captain bootstrap + captain/shadow identity versions.
- `db/classifications.rs` — MON-82 classifications.
- `db/misc.rs` — `ui_state`, event log (`db_log_event` / `log_event_internal`), anything that fits nowhere else.

Shared row types used by more than one domain (e.g. `AgentRow` used by sessions code) live in the domain that owns them and get re-exported via `mod.rs`. Preserve all existing doc comments and `// ---- section ----` comments alongside the code they describe.

## WP2 — `sidecar_protocol.rs` (2,096 lines) → `src-tauri/src/sidecar_protocol/`

- `sidecar_protocol/mod.rs` — re-exports.
- `sidecar_protocol/config.rs` — `ShadowConfig`, `LoadSessionMessage`, `ClassifierProvider`, `KeeperConfig`, `ClassifierInvocationConfig`, `ClassifierInvocation`.
- `sidecar_protocol/commands.rs` — `SidecarCommand`.
- `sidecar_protocol/events.rs` — `InnerEvent`, `KnownInnerEvent`, `SidecarEvent`, `KnownSidecarEvent`, `AtomicClaim`, and their (de)serialization logic.
- `sidecar_protocol/types.rs` — `Message`, `QuestReport*` and other shared payload structs.

Careful: the `Known*` enums are private deserialization helpers — keep them private to the module, in the same file as the public enums they back.

## WP3 — `src-tauri/src/agent/` internal split

- `agent/persist.rs` (1,517) → `agent/persist/`:
  - `persist/mod.rs` — `PersistCommand` enum + the dispatch entry point + re-exports.
  - `persist/messages.rs` — message/attachment persistence arms, `extract_image_attachments`, content helpers.
  - `persist/quests.rs` — quest/plan/report persistence arms, `emit_quest_notifications`.
  - `persist/util.rs` — `inner_event_tag`, `is_narration_tool`, small shared helpers.
  - Split the giant `impl PersistCommand` along match-arm/domain boundaries; helper methods move with their callers.
- `agent/manager.rs` (1,479): extract the free functions at the bottom (`rehydrate_user_content`, `render_keeper_slice`, `prompt_text`, `is_meaningful_quest_prompt`, `quest_title_from_prompt`, `quest_description_from_prompt`, `extract_text_from_stored_content`) into `agent/quest_prompt.rs` (quest-prompt heuristics) and `agent/keeper.rs` (keeper slice rendering). `AgentManager` itself stays in `manager.rs`.
- `agent/event_handler.rs` (905): move the keeper functions (`maybe_trigger_keeper`, `handle_keeper_result`) into `agent/keeper.rs` (same new file as above; WP3 owns it). Event dispatch stays put.
- Update `agent/mod.rs` re-exports so nothing outside `agent/` changes.

## WP4 — `src-tauri/src/ws.rs` (964 lines) → `src-tauri/src/ws/`

The bulk is one giant `handle_message` match (~lines 153–935) mirroring Tauri commands.

- `ws/mod.rs` — `WsState`, `start_ws_server`, `handle_connection`, `make_response`, re-exports.
- `ws/dispatch.rs` — `handle_message` reduced to a thin router that delegates to per-domain handler fns.
- `ws/handlers/` — `agents.rs`, `db.rs` (or split further: `quests.rs`, `plans.rs`, `memories.rs` if natural), `misc.rs` — each holding the match-arm bodies for its domain, plus the arg helpers (`str_field`, `opt_str`) in `ws/handlers/mod.rs` or `ws/util.rs`. `emit_plan_notifications` goes with the plan handlers.
- Group arms by the same domains as WP1 so the two structures mirror each other.

## WP5 — `sidecar/src/runtime-manager.ts` (936 lines)

Extract the free functions above the `RuntimeManager` class into focused modules:

- `sidecar/src/model-resolver.ts` — `buildDynamicModel`, `lmstudioBaseUrl`, `isValidThinkingLevel`, `ensureLmStudioProviderRegistered`, `resolveModel`.
- `sidecar/src/stored-content.ts` — `tryParseStoredContent`, `normalizeStoredUserContent`, `normalizeStoredAssistantContent`, `extractPromptText`, `oneLine`.
- `sidecar/src/memory-tools.ts` — `createSuggestMemoryTool`, `formatRelevantMemories`.
- `runtime-manager.ts` keeps only the `RuntimeManager` class, importing from the new modules.
- Verify with `npm run build --prefix sidecar`.

## WP6 — docs (after all WPs land)

Update CLAUDE.md "Start Here" table and ONBOARDING.md section 12 file references to the new paths. One commit: `docs: update file references after backend restructure`.

## Sequencing

WP1 → WP2 → WP3 → WP4 run **sequentially** (they share one Cargo crate; parallel runs would see each other's intermediate broken states in `cargo check`). WP5 (sidecar, separate package) runs in parallel with the Rust packages. WP6 last.
