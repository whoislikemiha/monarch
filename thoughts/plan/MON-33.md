# MON-33 — Collapse ws_* duplication behind a shared service layer

## Summary

Monarch exposes most backend operations twice: once as a `#[tauri::command]` consumed by the Svelte UI over IPC, and once as a `ws_*` free function consumed by the WebSocket control channel via `ws::dispatch_command`. The two sides mirror each other line-for-line across `agent.rs` (agent lifecycle), `db.rs` (CRUD), and `persistence.rs` (prompt files), with ~500 lines of duplicated logic. Drift has already happened silently: `ws_spawn_agent` dropped `context_window` until MON-35, and `ws_get_agents` in `db.rs` still returns `context_window: None` while its Tauri sibling selects the real column. MON-33 collapses the duplication by pushing the business logic into shared methods/functions (on `AgentManager`, `Database`, or as persistence free functions) and reducing every Tauri command and every `ws::dispatch_command` arm to a thin parse-and-delegate adapter. No wire-contract changes, no frontend-visible behavior changes — the refactor is invisible to `bindings.ts` and to every existing call site.

## Relevant files and areas

- **`src-tauri/src/agent.rs`** — the bulk of the duplication.
  - Lines ~1400–1811: the Tauri commands `spawn_agent`, `send_command`, `kill_agent`, `load_session_context`, `new_agent_session`, `switch_agent_session`, `respond_extension_ui`. (Non-twins that stay put: `get_agent_state` ~1599, `rebuild_agent_state_from_session` ~1618.)
  - Lines ~1813–2100: the matching `ws_*` twins plus `ws_detect_project` and `ws_read_project_instructions` (which have no Tauri twin under those names — they are WS-only wrappers around `find_project_root` and `read_instructions_from_root`, so the equivalent Tauri logic lives inline inside `spawn_agent` as `resolve_project`).
  - `SpawnAgentRequest` / `ShadowSpec` at ~1390: already the shared request shape from MON-35, reused as-is.
  - `AgentManager::get_app_handle()` is already available and is how the WS side already acquires `AppHandle` today — confirmed usable as the single path for adapter-to-method handle handoff.

- **`src-tauri/src/db.rs`** — partial extraction exists but is inconsistent.
  - Lines ~20, 266–540: `impl Database` with `_internal` methods (`upsert_agent_internal`, `create_session_internal`, `save_message_internal`, `session_exists_internal`, `ensure_agent_exists_internal`, `get_agent_context_window_internal`, `update_session_internal`, `log_event_internal`, etc.). These are already shared between Tauri and WS for the write paths.
  - Lines ~540–960: Tauri commands `db_get_agents`, `db_delete_agent`, `db_get_sessions`, `db_get_messages`, `db_save_memory`, `db_get_memories`, `db_upsert_project`, `db_get_projects`, `db_rename_project`, `db_update_project_instructions`, `db_delete_project`, `db_list_agent_templates`, `db_save_agent_template`, `db_delete_agent_template`. Most of these inline SQL directly instead of calling an `_internal` method.
  - Lines ~964–1147: `ws_*` twins that re-implement the same SQL. `ws_get_agents` at line 978 pointedly sets `context_window: None` instead of selecting the column — this is a live latent drift bug the refactor should close, not copy.
  - Note: the DB extraction is *almost* already the target shape for writes. The gap is read paths and the fact that several pairs skipped the `_internal` helper and inlined SQL twice.

- **`src-tauri/src/persistence.rs`** — trivially thin.
  - Tauri commands `get_agent_prompt`, `save_agent_prompt`, `get_prompts_dir` and their `ws_save_agent_prompt` / `ws_get_prompts_dir` twins are byte-identical. The free functions `read_agent_prompt_file`, `prompts_dir`, `monarch_dir` are already the de-facto shared core.

- **`src-tauri/src/ws.rs`** — the WS dispatcher.
  - Lines ~175–500+ (from what I read, starts at 177): `dispatch_command` match arms that decode args with `str_field` / `opt_str` / `serde_json::from_value` and call the `ws_*` functions. After refactor, each arm calls the shared method on `state.agent_mgr` / `state.db` directly and the `ws_*` free functions get deleted.
  - `spawn_agent` arm at ~180 already decodes via `serde_json::from_value::<SpawnAgentRequest>` — this is the target shape for every arm that can reasonably move to a typed request struct, but MON-33 explicitly leaves flat args flat.

- **`src-tauri/src/lib.rs`** — the specta command collection. Only touched if a Tauri command's signature changes. The `Value = unknown; Vec<T> = T[]` textual patch at ~110–123 is explicitly *not* MON-33's to remove.

- **`thoughts/impl/MON-14-cleanup.md`** — Wave 2 tracker. Needs a MON-33 Wave 2 entry when implementation lands (per the tracker convention).

## What needs to change

At the concept level, not the code level:

1. **Promote `AgentManager` to own the agent lifecycle logic.** Every current `spawn_agent` / `ws_spawn_agent` pair collapses into a single `impl AgentManager` method (one per operation: `spawn`, `send_command`, `kill`, `load_session_context`, `new_session`, `switch_session`, `respond_extension_ui`). Each method takes `&AppHandle`, `&Arc<Database>`, and a typed (for spawn) or flat (for the others) argument list. The method internally handles `ensure_sidecar`, DB writes, session map updates, sidecar command construction, and `send_with_recovery` / `send_to_sidecar` — exactly what the two twins do today, minus the duplication.

2. **Tauri command adapters become one-liners.** Each `#[tauri::command]` body shrinks to: extract state, call `state.method(&app, &db, args)?`, return. Under 10 lines including the signature. The `specta::specta` attribute stays; bindings.ts regeneration must be identical to the current output (zero wire-contract drift is an acceptance criterion).

3. **WS dispatch arms become one-liners.** Each `ws::dispatch_command` arm decodes args (flat `str_field`/`opt_str` or typed `from_value`), calls `state.agent_mgr.method(&state.agent_mgr.get_app_handle()?, &state.db, args)?`, and returns. The `ws_*` free functions in `agent.rs` are deleted entirely after the refactor — `ws_detect_project` and `ws_read_project_instructions` also collapse into methods (or free functions in a `project` helper module) shared with whatever inline path `spawn_agent` uses today.

4. **Finish the `Database` extraction that was started.** For every `db_*` / `ws_*` pair where SQL is still duplicated (notably the read paths: `get_agents`, `get_sessions`, `get_messages`, `get_memories`, `get_projects`, `list_agent_templates`, plus the `delete_*`, `rename_project`, `update_project_instructions`, `save_agent_template`, `save_memory` writes), introduce a single `impl Database` method (following the existing `_internal` suffix convention or renaming it away — see open questions). Both the Tauri command and the WS arm call it. The `ws_get_agents` context_window-drop bug is fixed incidentally by deletion: there is no longer a second implementation that can forget the column.

5. **Delete `persistence::ws_*` wrappers.** Both transports call `read_agent_prompt_file` / a new `write_agent_prompt_file` / `prompts_dir` directly. The Tauri commands themselves already do this for reads; writes just need the same treatment.

6. **Handle the `AppHandle` coupling consistently.** Pick one pattern and apply it everywhere: either shared methods take `&AppHandle` explicitly (and the WS adapter always calls `get_app_handle()?` before delegating), or shared methods pull it from `self` internally. Do not mix — the pre-refactor code already mixes these two styles and that is part of why the twins drifted. Recommendation: explicit `&AppHandle` parameter, because it keeps the method signatures honest about their IPC dependency and avoids a hidden `self.get_app_handle()?` that could silently fail.

7. **Keep `ensure_sidecar` inside the shared method, not the adapter.** The current `spawn_agent` calls it at the top of the command body; `ws_spawn_agent` also calls it. Moving it into the shared method means no adapter can forget, and future new adapters (if any) inherit the guarantee.

8. **Refresh `thoughts/impl/MON-14-cleanup.md`** with a Wave 2 entry documenting the service-layer collapse, the drift bugs closed (context_window on `ws_get_agents`, any others found during implementation), and a confirmation that `bindings.ts` is byte-identical before/after (ideally shown by diffing the generated file).

Shape after the refactor, at a glance:

```
#[tauri::command]
pub fn spawn_agent(app, state, db, req) -> Result<...> {
    state.spawn(&app, &db, req)
}

// in ws::dispatch_command
"spawn_agent" => {
    let req = serde_json::from_value(args)?;
    let app = state.agent_mgr.get_app_handle()?;
    state.agent_mgr.spawn(&app, &state.db, req)?;
    Ok(Value::Null)
}

impl AgentManager {
    pub fn spawn(&self, app: &AppHandle, db: &Arc<Database>, req: SpawnAgentRequest) -> Result<(), MonarchError> {
        // all the logic currently duplicated between spawn_agent and ws_spawn_agent
    }
}
```

## Resolved decisions

1. **`AppHandle` handoff**: explicit `&AppHandle` parameter on every shared method. The WS adapter calls `state.agent_mgr.get_app_handle()?` and passes it through. No implicit `self`-pulls — uniform across the refactor.

2. **`_internal` suffix convention**: keep as-is. Existing `_internal` methods stay named; newly extracted shared methods follow the same convention. Minimizes diff churn; naming cleanup is not this ticket's scope.

3. **`ws_detect_project` / `ws_read_project_instructions` placement**: extract into a small `agent::project` helper module (free functions) shared by the `spawn` path and the WS arms. `resolve_project` moves there too so both sides call the same helper.

4. **Testing**: add at least one round-trip integration test that exercises a representative operation (likely `spawn`) through both the Tauri command and the WS dispatch, proving both adapters funnel into the same shared method. Plus compile-green, specta bindings byte-diff clean, and manual smoke test of agent spawn on both transports.

5. **`respond_extension_ui` typed arg — fold in**: introduce a typed `ExtensionUiResponseRequest` (or similar) struct carrying `agent_id`, `request_id`, and a typed `value`. Replaces the raw `serde_json::Value` parameter on both transports. This expands MON-33's scope slightly but is cheap to land alongside the dedup. Note: whether this also unblocks removing the `Value = unknown` textual patch in `lib.rs` depends on the other holdouts (`LiveAgentState`, `StreamingMessage`, `ToolExecution`) — the patch itself still stays, still parked under MON-14 Wave 2.

6. **`ws_get_agents` context_window drift**: fix silently as part of the dedup (only one implementation will remain, so it's unavoidable). Call out explicitly in the PR description and in `thoughts/impl/MON-14-cleanup.md`'s Wave 2 entry so the reviewer sees it.

## Out of scope

- Reshaping flat `String` args on `send_command`, `new_agent_session`, `switch_agent_session`, or `load_session_context` into typed request structs. MON-35 did this for `spawn_agent`; MON-33 now adds `respond_extension_ui` per resolved decision #5 but leaves the other four flat.
- Removing the `Value = unknown; Vec<T> = T[]` textual patch in `src-tauri/src/lib.rs`. Parked under MON-14 Wave 2; still blocked on `LiveAgentState` / `StreamingMessage` / `ToolExecution` even after `respond_extension_ui` is typed.
- Any sidecar-side changes. `SidecarCommand` is already typed and stable.
- Frontend changes. `bindings.ts` must remain byte-identical after the refactor *except* for the new typed request struct added for `respond_extension_ui` (resolved decision #5); `App.svelte` / `AgentView.svelte` are only touched if the `respond_extension_ui` call site needs to pass the new struct shape.
- Renaming the `_internal` suffix convention.
- Expanding test coverage beyond the one round-trip integration test and the existing smoke-test path.
