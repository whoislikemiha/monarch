# MON-99 — P2 Slice A: Memory substrate + Inspector v0

## What was implemented

The full storage substrate for shadow memory plus a browse-only Memory Inspector. End-to-end: captain configures the Keeper model + downloads the embedding model from a new **Memory** tab in Settings; opens the **Memory** toolbox tool; runs a debug-only `memory_smoke_insert` Tauri command from devtools to insert a memory; the row is embedded via `bge-small-en-v1.5`, persisted to `memories` (which mirrors into `memories_fts` via FTS5 triggers), the in-process HNSW index is rebuilt, and the new memory renders in the Inspector with full provenance (id, created_at, embedding model, scope/kind/layer badges, content, summary, file refs, supersedes chain). No Keeper writes (Slice B / MON-100) and no retrieval injection (Slice C / MON-101) yet — those land in sibling tickets.

Verified end-to-end manually: Settings → Download model (~127 MiB) → Save config → Memory Inspector (empty) → `memory_smoke_insert` from devtools → memory id 1 appears under `self` with all provenance fields populated.

## Key decisions

- **D1 — Memory config lives in Settings dialog, not a toolbox tool.** Matches the classifier precedent (global config). The Inspector can later expose a "Configure Keeper…" shortcut button if useful.
- **D2 — Smoke command stays permanently, runtime-gated via `cfg!(debug_assertions)`.** The function signature is always compiled (so `bindings.ts` stays stable across debug/release); the body short-circuits in release with a `MonarchError::persistence` so it never touches the DB. Useful for repro after launch.
- **D4 — Memory tab gates Save on embedder being initialised.** Without a downloaded model we cannot embed memories at insert time, and a Keeper configured before the embedder is ready is a footgun. The "Download model" button is the only available action while `memory_index_status == false`.
- **D5 — Memory Inspector ships the full tree from day one.** Group by scope (`self` / `project` / `captain`), then chase `parent_id` chains within each scope, with depth-based indentation. A memory whose parent lives in a different scope is promoted to a top-level node within its own scope (no orphans).
- **Embedder pooling** — CLS pooling on `bge-small-en-v1.5` (last_hidden_state[:, 0, :]), then L2-normalised. L2 distance on normalised vectors is monotonic with cosine similarity, which is what the design calls for.
- **HNSW rebuild policy** — full rebuild after each insert (no background rebuild, no incremental insert). Brute-force at P2 volumes is fine; P3c/d will improve this when memory volumes warrant. The index lives in a `Mutex<Option<HnswIndex>>` and is rebuilt from the DB on cold start (lazy, only after embedder is initialised).
- **WS parity** — memory commands wired into both the Tauri runtime handler and the WebSocket dispatch in `ws.rs` (so the browser-fallback dev path works equivalently). `WsState` gained a `memory_index: Arc<MemoryIndex>` field.
- **SDK shim (`agentDir`)** — pi-coding-agent's `DefaultResourceLoaderOptions` added a required `agentDir` field. Monarch disables all agent-local resource discovery (`noExtensions: true`, `noSkills: true`, `noPromptTemplates: true`, `noThemes: true`), so the value is functionally unused — `cmd.cwd` is passed as a safe valid path.
- **SDK shim (`ExtensionUIContext`)** — pi-coding-agent added `setWorkingIndicator` and `addAutocompleteProvider` to the context interface. Both are TUI-only features Monarch doesn't expose; stubbed as no-ops.

## Files touched

**Rust (substrate, already on branch from earlier commits):**
- `src-tauri/Cargo.toml` — `instant-distance`, `ort` (with `download-binaries`), `ndarray`, `tokenizers` deps.
- `src-tauri/src/db.rs` — `memories` schema (post-launch ALTER blocks: `scope`, `project_id`, `parent_id`, `kind`, `title`, `summary`, `manual_override`, `source_quest_id`, `source_session_id`, `source_events`, `file_refs`, `embedding`, `embedding_model_id`, `supersedes_id`, `archived_at`, `last_accessed_at`), `memory_keeper_runs` table, `memories_fts` FTS5 virtual table with insert/update/delete triggers, plus DB internals (`insert_memory_internal`, `list_memories_for_agent_internal`, `get_memory_internal`, `fts_search_memories_internal`, `load_embeddings_for_agent_internal`, `insert_keeper_run_internal`, `complete_keeper_run_internal`, `update_memory_access_internal`).
- `src-tauri/src/memory_config.rs` — new module. `MemoryConfig` (raw) + `ResolvedMemoryConfig`, TOML at `~/Library/Application Support/monarch/memory.toml` (macOS) / `~/.config/monarch/memory.toml` (Linux). Tauri commands `memory_get_config`, `memory_set_config`, `memory_get_config_path`. `enabled` flag derived from `keeper.is_some()`.
- `src-tauri/src/memory_index.rs` — new module. `MemoryIndex` owns `Mutex<Option<Embedder>>` (ONNX session + tokenizer) and `Mutex<Option<IndexState>>` (HNSW). Methods: `ensure_model_downloaded` (lazy fetch from HF Hub), `init_embedder`, `embed_text`, `embed_to_blob`, `rebuild`, `query`. Tauri commands `memory_index_status`, `memory_download_and_init`.

**Rust (Slice A close-out, this push):**
- `src-tauri/src/memory_smoke.rs` — new module. Debug-gated `memory_smoke_insert(agent_id, title, content)` Tauri command.
- `src-tauri/src/lib.rs` — `mod memory_smoke`, command registration in `specta_builder` + runtime handler, `memory_index` cloned into `WsState`.
- `src-tauri/src/ws.rs` — `WsState.memory_index` field; dispatch arms for `memory_index_status`, `memory_download_and_init`, `memory_smoke_insert`.

**Frontend:**
- `src/lib/SettingsDialog.svelte` — `{ id: "memory", label: "Memory" }` category added between Agent Defaults and Keybindings; routes to `<MemorySettings />`.
- `src/lib/MemorySettings.svelte` — new panel. Embedding model status + Download button, Keeper provider/model inputs, top-K input, save gated on embedder ready (D4). Mirrors `ClassifierSettingsTool.svelte` ergonomics.
- `src/lib/toolbox/tools/MemoryInspectorTool.svelte` — new toolbox tool (order 6, between Identity and Context). Two-pane: scope-bucketed tree on left, detail with full provenance on right. Read-only. Snippet-based recursive node rendering for tree depth.
- `src/lib/toolbox/registry.ts` — registered.
- `src/lib/bindings.ts` — regenerated.

**Sidecar (SDK shim, unrelated to MON-99 conceptually):**
- `sidecar/src/runtime-manager.ts` — pass `agentDir: cmd.cwd` to `DefaultResourceLoader`.
- `sidecar/src/ui-bridge.ts` — stub `setWorkingIndicator` and `addAutocompleteProvider` as no-ops.

**Plan:**
- `thoughts/plan/MON-99.md` — rewritten from full-P2 to Slice A scope; decisions D1–D5 locked.

## What was left out

- **Keeper sidecar worker, quest-close trigger, structured-JSON claim extraction** — MON-100 (Slice B).
- **`compaction_tick` event in `quest_events` + `QuestTimelineTool` renderer** — MON-100.
- **`suggest_memory` executor tool plumbing** — MON-100.
- **Hybrid retrieval (FTS5 + HNSW) on user turn, IPC round-trip Rust ⇄ sidecar, `## Relevant Memories` prompt injection** — MON-101 (Slice C).
- **`access_count` / `last_accessed_at` updates** — DB internal exists (`update_memory_access_internal`) but isn't called yet; first writer is Slice C's retrieval path.
- **Captain edit / archive / promote / supersede affordances in the Inspector** — P12.
- **Inner-node summary regeneration** — P12.
- **Embedding model download progress UI** — current Download button shows "Downloading…" but no byte progress. Acceptable for one-time fetch; can polish later.
- **HNSW background rebuild + atomic swap** — P3c (MON-96).
- **Incremental HNSW insert** — P3d (MON-97).
- **Eval harness for retrieval recall** — P3a (MON-94).
- **Reranker pass** — P3b (MON-93).
- **Project + captain scoping for retrieval / writes** — P9.
- **Stale-flagging via `file_refs.anchor_sha`** — P11.

## Aside

CLAUDE.md still lists POSIX-style paths (`~/.config/monarch/...`) for the SQLite DB, prompts, attachments, avatars, models. On macOS these all resolve under `~/Library/Application Support/monarch/` via `dirs::config_dir()`. Not fixed in this PR — separate doc cleanup.
