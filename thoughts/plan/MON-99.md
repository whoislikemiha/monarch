# MON-99 — P2: First memory, end-to-end

## Summary

P2 ships the full write → store → retrieve → inject loop for shadow memory. When a quest closes (status → `done`), a Keeper worker fires: it reads the quest's raw event stream, calls a configured LLM to extract atomic claims, embeds them with `bge-small-en-v1.5` (lazy-downloaded to `~/.config/monarch/models/`), and persists them in a new `memories` table backed by FTS5 (BM25) and an in-process HNSW index (`instant-distance`). On each user turn, hybrid retrieval pulls the top-K memories relevant to the incoming message and injects them into the prompt as a `## Relevant Memories` section. The captain can browse the tree in a new Memory Inspector toolbox tool. The Keeper model is configured via a new Memory tab in the Settings dialog; if unconfigured, memory formation is silently skipped. The storage stack (instant-distance + ort + bge-small-en-v1.5) was fully validated by MON-91.

---

## Relevant files and areas

### Spike validation
- `thoughts/impl/MON-91.md` — confirmed stack: `instant-distance` HNSW, `ort` with `download-binaries` (statically linked ORT), `bge-small-en-v1.5` lazy-downloaded to `~/.config/monarch/models/`. p99 query latency 5.81 ms at 1M vectors, recall@10 = 1.000 on real embeddings. Binary delta: +25 MiB (ORT) + 127 MiB model (lazy). No pivot needed.
- `thoughts/spike/MON-91-storage.md` — raw benchmark results.

### Database
- `src-tauri/src/db.rs` — all schema migrations live here as idempotent `CREATE TABLE IF NOT EXISTS` / `ALTER TABLE` blocks appended to `init_schema()`. Existing tables relevant: `quest_nodes` (has `status` column), `quest_events` (id, quest_id, event_type, actor, payload_json, created_at). New tables needed: `memories`, `memories_fts` (FTS5 virtual), `memory_keeper_runs`.

### Configuration pattern
- `src-tauri/src/classifier_config.rs` — the direct model for `memory_config.rs`. Reads from `~/.config/monarch/memory.toml`, exposes Tauri commands (`memory_get_config`, `memory_set_config`), has a `ResolvedMemoryConfig` type that the sidecar receives. Follow this pattern exactly.
- `src/lib/SettingsDialog.svelte` — has a `categories` array (currently: General, Appearance, Agent Defaults, Keybindings). Add a `{ id: "memory", label: "Memory" }` entry and a corresponding panel component.

### Persistence pipeline
- `src-tauri/src/agent/persist.rs` — MON-37 single-consumer bounded channel. `PersistCommand` enum is the extension point. New variants needed: `SaveMemory`, `SaveKeeperRun`, `RecordQuestEvent` (for `compaction_tick`). Follow the `SaveClassification` pattern.

### Sidecar
- `sidecar/src/runtime-manager.ts` — `ManagedSession` interface, `prompt()` method (fires per user turn — this is where retrieval injection goes). `destroySession()` is one candidate hook for quest-close, but the actual trigger should be quest status change, not session destruction.
- `sidecar/src/protocol.ts` — all command/event types. New commands needed: `keeper_run` (Rust → sidecar, fires at quest-close), `set_memory_config` (deliver config to sidecar on startup/change). New events: `keeper_result` (sidecar → Rust, structured JSON), `memory_search_result`.
- `sidecar/src/classifier.ts` — the structural model for the Keeper worker: a one-shot LLM call (not an interactive Pi session), fires on a trigger, returns structured JSON, fails gracefully without blocking the main turn.

### Quest system
- `src-tauri/src/agent/commands.rs` — Tauri command for quest status updates. The quest-done transition needs to enqueue a Keeper run.
- `src/lib/toolbox/questStore.svelte.ts` — per-agent quest state; `eventsByQuest` map already exists but event rendering is skeletal.
- `src/lib/toolbox/tools/QuestTimelineTool.svelte` — needs a `compaction_tick` event kind renderer. Currently renders quest nodes only.

### System prompt builder
- `sidecar/src/shadow-oath.ts` — `buildSystemPrompt()` already accepts identity payloads. Retrieval injection does **not** go here (system prompt is static per session). Retrieved memories are prepended to the user message text in `prompt()` instead, so they're contextually adjacent to the turn they're relevant to.

### Frontend toolbox
- `src/lib/toolbox/registry.ts` — add Memory Inspector entry (order 6, between Identity at 5 and Context at 10).
- `src/lib/toolbox/tools/IdentityTool.svelte` — reference for toolbox tool structure.

---

## What needs to change

### 1. Rust: `src-tauri/Cargo.toml`
Add `instant-distance`, `ort` (with `download-binaries` feature), and `ndarray` (for embedding vector math). These were temporarily probed in MON-91 and reverted — now they land for real.

### 2. Rust: `src-tauri/src/db.rs`
Add three schema blocks to `init_schema()`:
- `memories` table — full tree-structured schema (parent_id, scope, kind, title, summary, content, embedding BLOB, embedding_model_id, supersedes_id, archived_at, source_quest_id, source_events, file_refs, access_count, last_accessed_at, created_at)
- `memories_fts` — FTS5 virtual table on (title, summary, content), with `content='memories'` and triggers to keep it in sync on insert/update/delete
- `memory_keeper_runs` — provenance table (shadow_id, trigger, started_at, completed_at, raw event range, tokens, model_id, output_summary, outcome)

Add DB internal functions: `insert_memory_internal`, `list_memories_for_agent_internal`, `get_memory_internal`, `insert_keeper_run_internal`, `fts_search_memories_internal`, `update_memory_access_internal`.

### 3. Rust: `src-tauri/src/memory_config.rs` (new file)
Mirror of `classifier_config.rs`. Reads `~/.config/monarch/memory.toml`. Config shape:
- Keeper model: provider + model ID (same provider enum as classifier)
- Embedding model ID (default: `bge-small-en-v1.5`)
- Model download path (default: `~/.config/monarch/models/`)
- Top-K for retrieval (default: 5)
- Keeper enabled flag (derived: true only if keeper model is configured)

Expose Tauri commands: `memory_get_config`, `memory_set_config`, `memory_get_config_path`.

### 4. Rust: `src-tauri/src/memory_index.rs` (new file)
Owns the in-process HNSW index as a `Mutex<Option<HnswIndex>>`. Responsibilities:
- Model download: check `~/.config/monarch/models/bge-small-en-v1.5.onnx`, lazy-fetch from HF Hub if missing
- Embed: accept text, return `Vec<f32>` (normalized)
- Build: load all `memories.embedding` BLOBs from DB, build HNSW index, store in memory
- Query: given a query string, embed it, search HNSW for top-K, return memory IDs
- Rebuild: full rebuild from DB (called on startup and after each Keeper run for P2)

The index is ephemeral (in-process), rebuilt from the DB. P3c adds background rebuild + atomic swap.

### 5. Rust: `src-tauri/src/agent/persist.rs`
Add `PersistCommand::SaveMemory(MemoryPayload)` and `PersistCommand::SaveKeeperRun(KeeperRunPayload)` variants. Add `PersistCommand::RecordCompactionTick { quest_id, keeper_run_id }` to write the `compaction_tick` quest event. Route all through the existing single-consumer loop — no new channels.

### 6. Sidecar: `sidecar/src/protocol.ts`
New commands (Rust → sidecar):
- `keeper_run` — fires at quest-close: `{ agentId, questId, eventSlice: QuestEvent[], memoryConfig: KeeperModelConfig }`
- `set_memory_config` — delivers config to sidecar on startup or config change

New events (sidecar → Rust):
- `keeper_result` — `{ agentId, questId, keeperRunId, claims: AtomicClaim[], compactionSummary, error? }`

### 7. Sidecar: `sidecar/src/keeper.ts` (new file)
Mirrors `classifier.ts` structurally: one-shot LLM call, returns structured JSON, never blocks the user turn. Responsibilities:
- Receive the event slice + relevant tree slice (passed in from Rust)
- Build the Keeper prompt (event context + existing memories + extraction instructions)
- Call the configured model via Pi's `complete()` API
- Parse structured JSON output: `{ claims: AtomicClaim[], compaction_summary: string }`
- Return result; error → emit `keeper_result` with `error` field, never throw

### 8. Sidecar: `sidecar/src/runtime-manager.ts`
- On `prompt()`: before forwarding the user message to Pi, run hybrid retrieval (FTS5 via Rust IPC + HNSW via the new `memory_search` command). Prepend a `## Relevant Memories` section to the user message text if results are non-empty.
- Add `keeper_run` command dispatch: receive command, call `keeper.ts`, emit `keeper_result`.
- Add `suggest_memory` as an available Pi tool in the executor session (tool schema injected into system prompt). Tool calls to `suggest_memory` are forwarded to Rust as a sidecar event for queuing to the Keeper.

### 9. Rust: `src-tauri/src/agent/commands.rs` + `manager.rs`
- When quest status is set to `done`, send a `keeper_run` command to the sidecar with the quest's event slice (fetched from `quest_events`).
- Handle `keeper_result` event: write claims to `memories` via persist pipeline, write `memory_keeper_runs` row, emit `compaction_tick` quest event, trigger HNSW rebuild.

### 10. Frontend: `src/lib/SettingsDialog.svelte` + new `MemorySettings.svelte`
Add `{ id: "memory", label: "Memory" }` to the `categories` array. Create `src/lib/MemorySettings.svelte`: form for Keeper model (provider dropdown + model ID text field), embedding model path display, top-K slider, save button. Follows the same pattern as `ClassifierSettingsTool.svelte` (load on mount, dirty tracking, save on submit).

### 11. Frontend: `src/lib/toolbox/tools/MemoryInspectorTool.svelte` (new file)
Toolbox tool (order 6). Per-agent view. Two-pane layout:
- Left: topic tree (grouped by `parent_id` → scope → kind). Clicking a node selects it.
- Right: memory detail panel — title, summary, content, kind badge, scope badge, provenance (source quest link, keeper run ID), file_refs list, supersedes chain, created_at.
- Load: `memory_list_for_agent(agentId)` Tauri command.
- Read-only for P2 (no edit/archive/promote).

### 12. Frontend: `src/lib/toolbox/tools/QuestTimelineTool.svelte`
Add a renderer for `compaction_tick` event type: shows the compaction summary from the Keeper run, styled distinctly (e.g., with a memory icon and muted border).

---

## Open questions

1. **Memory retrieval on `prompt()`**: Retrieval currently envisioned as a sidecar-side call — but the HNSW index lives in Rust (in-process). The sidecar needs to ask Rust for relevant memories via a new IPC round-trip before forwarding the user message. This adds latency. Alternative: the sidecar caches the top-K from the last retrieval and refreshes it on each turn. Needs a decision before implementation: **round-trip IPC per turn, or cached?**

2. **suggest_memory tool**: The executor proposes a memory via a Pi tool call. The tool call lands as a sidecar event. Does the sidecar queue it directly to the Keeper (all in-sidecar), or does it forward to Rust which re-sends a `keeper_run` command? The all-sidecar path is simpler but means the Keeper runs independently of the Rust persistence pipeline. The Rust-round-trip path preserves the single-writer guarantee cleanly.

3. **Memory config in sidecar**: The `memory.toml` config lives in Rust. The sidecar needs the Keeper model config to make LLM calls. How is it delivered: on `create_session`, via a separate `set_memory_config` command, or via the existing `set_custom_prompt` extension pattern? Recommend a dedicated `set_memory_config` sidecar command sent on startup and on config change.

4. **Retrieval scope for P2**: Retrieved memories are keyed by `agentId` (shadow's own memories only). Project-scoped and captain-scoped memories are deferred to P9. This should be confirmed so the retrieval query is scoped correctly from day one without over-fetching.

---

## Out of scope reminders

- Eval harness / recall@5 (P3a — MON-94)
- Reranker (P3b — MON-93)
- Background HNSW rebuild worker (P3c — MON-96)
- Incremental HNSW insert (P3d — MON-97)
- FTS5 wiring as a standalone ticket (MON-95 — subsumed into P2 since retrieval needs it)
- L2 working memory (P4)
- Chat-shadow (P7)
- Project-scoped memory sharing (P9)
- First-person quest reports as Keeper input (P6)
- Captain edit / archive / promote in Memory Inspector (P12)
- Continuous and idle compaction triggers
- Inner-node summary regeneration
- Stale-flagging via `file_refs` anchor_sha (P11)
- Merge/supersede logic at cosine threshold (deferred — P2 Keeper always inserts; merge/supersede calibrated after eval in P3a)
