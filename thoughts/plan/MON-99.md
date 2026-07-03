# MON-99 — P2 Slice A: Memory substrate + Inspector v0

> **Sibling slices:** [MON-100](https://linear.app/monarch-commander/issue/MON-100) (Slice B — Keeper write path) and [MON-101](https://linear.app/monarch-commander/issue/MON-101) (Slice C — retrieval read path) together complete the P2 phase from `thoughts/design/shadow-cognition/roadmap.md`. This slice (A) lays the substrate and the browse-only Inspector; B writes memories at quest-close; C surfaces them on user turns. Each slice is independently testable per the roadmap's phase rule.

## Summary

Slice A ships everything required for memories to **exist and be inspected**, with nothing yet writing or reading them on the agent loop. That means: the SQLite schema (`memories`, `memories_fts`, `memory_keeper_runs`), the embedding pipeline (`bge-small-en-v1.5` via ONNX, lazy-downloaded), the in-process HNSW index (`instant-distance`), the `memory.toml` configuration loader, a Memory tab in the Settings dialog, a browse-only Memory Inspector toolbox tool, and a debug-only smoke-test command that lets the captain insert a memory by hand to verify the substrate end-to-end. The substrate has been validated by the MON-91 spike (recall@10 = 1.000, p99 query latency < 6 ms at 1 M vectors).

## Status (already committed on this branch)

The substrate is in flight. The two feature commits already on `mon-99-p2-first-memory-end-to-end` are:

- `c87eeb8 feat(mon-99): memories schema, keeper_runs, FTS5 triggers, DB internals` — `db.rs` schema + DB internals (`insert_memory_internal`, `list_memories_for_agent_internal`, `get_memory_internal`, `insert_keeper_run_internal`, `fts_search_memories_internal`, `update_memory_access_internal`).
- `be39295 feat(mon-99): memory_config.rs, memory_index.rs, Cargo deps` — global config loader + Tauri commands, HNSW + ONNX embedder + Tauri commands, `Cargo.toml` deps (`instant-distance`, `ort` with `download-binaries`, `ndarray`, `tokenizers`).

Wiring already in `lib.rs`:

- `MemoryIndex` constructed in `run()` (reads `models_dir` from `memory_config::resolved()`) and registered with `.manage(memory_index)`.
- All five new Tauri commands registered in both `specta_builder()` and the runtime `tauri::generate_handler!`: `memory_get_config`, `memory_set_config`, `memory_get_config_path`, `memory_index_status`, `memory_download_and_init`.
- `db::db_list_memories_for_agent` and `db::db_get_memory` already registered for the Inspector to consume.

So the back-end half of Slice A is essentially done. What remains is the front-end (Memory tab + Inspector tool), one debug command, and verification.

## Relevant files and areas

### Already authored
- `src-tauri/src/db.rs` — schema migrations + DB internals for `memories`, `memories_fts`, `memory_keeper_runs`. New table columns include `parent_id`, `scope`, `kind`, `summary`, `content`, `embedding`, `embedding_model_id`, `supersedes_id`, `archived_at`, `source_quest_id`, `source_events`, `file_refs`, `access_count`, `last_accessed_at`, `created_at`. FTS5 mirror has insert/update/delete triggers.
- `src-tauri/src/memory_config.rs` — `MemoryConfig` (raw) + `ResolvedMemoryConfig`, TOML at `~/.config/monarch/memory.toml`. Tauri commands `memory_get_config`, `memory_set_config`, `memory_get_config_path`. `enabled` flag derived from `keeper.is_some()`.
- `src-tauri/src/memory_index.rs` — `MemoryIndex` owning `Mutex<Option<Embedder>>` and `Mutex<Option<IndexState>>`. Methods: `ensure_model_downloaded`, `init_embedder`, `embed_text`, `embed_to_blob`, `rebuild`, `query`. Tauri commands `memory_index_status`, `memory_download_and_init`. CLS-pooled, L2-normalised embeddings.
- `src-tauri/Cargo.toml` — deps added.

### To touch in this slice
- `src/lib/SettingsDialog.svelte` — has a `categories` array (currently General, Appearance, Agent Defaults, Keybindings). Add `{ id: "memory", label: "Memory" }` and route to a new panel component.
- `src/lib/MemorySettings.svelte` (new) — settings panel. Pattern reference: `src/lib/toolbox/tools/ClassifierSettingsTool.svelte` (load on mount, dirty tracking, save on submit). Form fields: Keeper provider+model (dropdown sourced from `models::get_models`), embedding model ID (read-only display + status), download-and-init button (calls `memory_download_and_init`), top-K input.
- `src/lib/toolbox/tools/MemoryInspectorTool.svelte` (new) — toolbox tool. Reference: `src/lib/toolbox/tools/IdentityTool.svelte` for shape; `src/lib/toolbox/tools/ContextInspectorTool.svelte` for live-state read patterns. Two-pane layout (tree + detail).
- `src/lib/toolbox/registry.ts` — register the Inspector with `id: "memory-inspector"`, `order: 6` (between Identity at 5 and Context at 10).
- `src-tauri/src/lib.rs` — register the new debug smoke-test command (and gate its body behind `#[cfg(debug_assertions)]` if we want it stripped in release).
- `src/lib/bindings.ts` — regenerated automatically by `cargo run -- --export-bindings`.

### Reference (do not modify in this slice)
- `src-tauri/src/agent/persist.rs` — `PersistCommand` enum extension point. Memory persist variants land in MON-100, not here.
- `sidecar/src/runtime-manager.ts`, `sidecar/src/keeper.ts` (does not exist yet) — Slice B / MON-100 territory.
- `sidecar/src/protocol.ts` — `keeper_run` / `keeper_result` types land in MON-100.
- `src/lib/toolbox/tools/QuestTimelineTool.svelte` — `compaction_tick` renderer is MON-100.

## What needs to change

### 1. Settings: Memory tab (frontend)
- Add the `{ id: "memory", label: "Memory" }` entry to the `categories` array in `SettingsDialog.svelte`.
- Create `src/lib/MemorySettings.svelte`. Mounts call `memory_get_config` to populate state. Form supports: Keeper provider dropdown + model ID (text or autocomplete), embedding model status (a check vs `memory_index_status` plus a "Download model" button calling `memory_download_and_init` — show progress / disable while pending), top-K input (default 5).
- Save button serializes back into the `MemoryConfig` shape and calls `memory_set_config`. After save, reflect the resolved view (`enabled` flag, etc.) in the UI.
- Empty / unconfigured state: clearly communicate that no Keeper model means memory formation is disabled (relevant to MON-100, but worth surfacing now so the captain isn't surprised later).

### 2. Memory Inspector toolbox tool (frontend)
- New component at `src/lib/toolbox/tools/MemoryInspectorTool.svelte`, accepts `{ agentContext }: ToolProps`.
- Register in `src/lib/toolbox/registry.ts` with stable `id` (`memory-inspector`), title (`Memory`), an SVG icon, `order: 6`.
- Two-pane layout: left pane is a tree of memories grouped by `parent_id` → `scope` (`self` / `project` / `captain`) → `kind`. Right pane shows the selected memory's detail: title, summary, content, kind/scope badges, provenance (source quest id link, keeper run id), `file_refs` list, `supersedes_id` chain (display the chain by chasing `supersedes_id` across rows already loaded), `created_at`.
- Data source: `db_list_memories_for_agent(agentId)` for the tree, `db_get_memory(memoryId)` for the detail when needed (or just read from the listed rows if the list returns full bodies — pick at implementation time based on shape).
- Read-only — no edit / archive / promote affordances (those are P12).
- Empty state: helpful copy explaining that memories appear after the Keeper runs (forward-reference MON-100), and mentioning the smoke-test command for hand-testing.

### 3. Smoke-test Tauri command (backend)
- New command in either `memory_config.rs` or a new tiny module (`memory_smoke.rs`?). Signature roughly `memory_smoke_insert(agent_id: String, title: String, content: String) -> Result<i64>` (returns the memory id).
- Implementation: ensure embedder initialised, `embed_to_blob(content)`, `db::insert_memory_internal(...)` with sensible defaults (`scope = "self"`, `kind = "claim"`, `parent_id = None`, etc.), then call `memory_index.rebuild` with the freshly fetched `(id, blob)` set.
- Gate the body behind `#[cfg(debug_assertions)]` so the release binary does not expose it. The function signature can stay always-compiled so bindings emit consistently.
- Register in `lib.rs` (both `specta_builder` and runtime handler).

### 4. Bindings regeneration
- After the smoke command lands, run `cargo run -- --export-bindings` from `src-tauri/`. Verify `src/lib/bindings.ts` includes the new command and reroutes through `./api`.

### 5. Verification pass
- `cargo check` from `src-tauri/` — clean.
- `npx svelte-check` from repo root — clean.
- `npm run build:sidecar && npm run tauri dev`. Open Settings → Memory. Configure a Keeper model (any provider you have credentials for; the model is not exercised in this slice). Click Download model — observe `~/.config/monarch/models/bge-small-en-v1.5.onnx` (+ tokenizer json) appear and `memory_index_status` flip to `true`.
- Open Memory Inspector. Empty state visible.
- Run `memory_smoke_insert` (via the dev shell, devtools console, or a temporary debug button — pick at impl time). Inspector refreshes (or re-open it) and shows the memory with provenance.
- Restart the app cold. Memory still present (DB-persisted). HNSW rebuilds on cold start once embedder is initialised.

## Decisions locked

- **D1 (was Q1) — Memory config lives in the Settings dialog, not a toolbox tool.** Matches the classifier precedent. The Inspector can later expose a "Configure Keeper…" shortcut button if useful, but no separate toolbox config tool.
- **D2 (was Q2) — Smoke command stays permanently behind `#[cfg(debug_assertions)]`.** Stripped from release builds; always available in dev for repro. The function signature stays always-compiled so the bindings emit consistently.
- **D3 (was Q3) — Memory-to-agent ownership.** Verified at impl time by reading `db.rs` and the existing `db_list_memories_for_agent` internal — informs the smoke command's insert shape and the Inspector's load query.
- **D4 (was Q4) — Memory tab gates save on the embedder being initialised.** No save button until the embedding model is downloaded and the embedder reports `memory_index_status == true`. Without an embedder we cannot embed memories at insert time, and the captain configuring the Keeper before the embedder is ready is a footgun. The Download Model button is the only available action while `status == false`.
- **D5 (was Q5) — Memory Inspector ships the full tree from day one.** Group by `parent_id` → `scope` → `kind`. No interim flat list — the schema supports the tree and the additional component complexity is small.

## Out of scope for this slice

- Quest-close trigger, sidecar Keeper worker, structured-JSON claim extraction (MON-100 / Slice B).
- `compaction_tick` event kind in `quest_events` and its `QuestTimelineTool` renderer (MON-100).
- `suggest_memory` executor tool (MON-100).
- Hybrid retrieval (FTS5 + HNSW) on user turn, IPC round-trip Rust ⇄ sidecar, `## Relevant Memories` injection (MON-101 / Slice C).
- Project / captain scoping for retrieval (P9).
- Eval harness (P3a / MON-94), reranker (P3b / MON-93), background HNSW rebuild + atomic swap (P3c / MON-96), incremental HNSW insert (P3d / MON-97).
- L2 working memory (P4), chat-shadow (P7), forking (P10), stale-flagging via `file_refs.anchor_sha` (P11).
- Captain edit / archive / promote / supersede in Memory Inspector (P12).
- Inner-node summary regeneration (P12).
- Continuous and idle compaction triggers (post-P2).
