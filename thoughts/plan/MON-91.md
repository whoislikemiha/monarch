# MON-91 — Spike: storage stack viability for shadow memory

Research plan for the throwaway spike that validates the storage stack proposed in
[`thoughts/design/shadow-cognition/substrate.md`](../design/shadow-cognition/substrate.md) § L4 and
[`thoughts/design/shadow-cognition/distillation.md`](../design/shadow-cognition/distillation.md) § Implications for the data model.

Linear: https://linear.app/monarch-commander/issue/MON-91

## Summary

Before any `memories` / `memories_fts` / HNSW sidecar code commits in production, build a throwaway test crate that exercises the tentative storage stack — SQLite BLOBs for vectors + a pure-Rust HNSW index + `bge-small-en-v1.5` via the `ort` crate — at realistic scale (~1M vectors), measures the hot-path numbers the design implies (p99 latency, recall@10, RAM, disk, bundle size), and lands a measurement-backed go/pivot recommendation. The spike's load-bearing questions are all about the *combination* of these three technologies inside a Tauri-packaged binary; individually each one is known to work. The crate itself is throwaway — it does not touch `src-tauri/src/db.rs`, does not add tables to the live `monarch.db`, and will not merge into production code paths. The deliverables are a directory of self-contained Rust code under `spike/MON-91-storage/` (or equivalent), a notes file at `thoughts/spike/MON-91-storage.md` with the numbers and recommendation, and a decision captured in the recommendation: confirm the stack or name an alternative (LanceDB, Kùzu, SurrealDB, `candle`-based inference, etc.).

## Relevant files and areas

### Design docs the spike validates

- **`thoughts/design/shadow-cognition/substrate.md`** — § L4 "Search as access pattern" defines the read model (tree-walk + `memory_search` tool over hybrid BM25 + vector). § Implications for the data model names the tentative libraries: "Vector index: SQLite BLOBs + Rust-side HNSW sidecar file (`instant-distance` or equivalent). Storage choice locked here; implementation in a sibling spike." Working assumption #12 is the direction this spike either confirms or revises.
- **`thoughts/design/shadow-cognition/distillation.md`** — § Implications for the data model gives the rough `memories` row shape (`embedding BLOB`, `embedding_model_id TEXT`, `memory_index` HNSW sidecar rebuildable from BLOBs). § Open questions gives embedding model guidance: default to `bge-small-en-v1.5` or `nomic-embed-text`, shipped via ONNX Runtime. § Embedding scope pins down that the Keeper embeds the *summary* field, not raw content — informs how representative synthetic vectors need to be.

### Current Monarch state the spike intersects with

- **`src-tauri/Cargo.toml`** — already depends on `rusqlite 0.37 { features = ["bundled"] }` and `tokio-rusqlite 0.7 { features = ["bundled"] }`. The BLOB side of the stack is essentially pre-validated on the dev machine; the spike confirms BLOB insert/read performance at scale and the rebuild-from-BLOBs cold-start pattern.
- **`src-tauri/src/db.rs`** — pattern for the `Database` wrapper around `tokio_rusqlite::Connection`, async dispatch via `conn.call(|c| { ... }).await`, WAL + foreign keys, schema init via idempotent `CREATE TABLE IF NOT EXISTS`. The spike's SQLite access pattern should mirror this so any measurements generalize to the eventual production integration.
- **`src-tauri/tauri.conf.json`** — no `bundle` section currently. If bundle-size measurement requires a `bundle` block (resource whitelisting, sidecar binary listing), the spike documents the minimum additions.
- **`Cargo.toml` at repo root** — does *not* exist. `src-tauri/` is a standalone package, not a workspace member. The spike's test crate therefore stands fully outside `src-tauri/` and does not need to touch anything in the production tree to build.
- **`sidecar/`** — unrelated to this spike (Node runtime, not Rust). No changes.

### New files the spike creates

- **`spike/MON-91-storage/`** — throwaway Rust crate. Own `Cargo.toml`, own `src/`, own `README.md` describing how to reproduce each measurement. Not a workspace member of anything.
- **`thoughts/spike/MON-91-storage.md`** — the output notes file. Per the issue's acceptance criteria: methodology, raw numbers, anomalies, recommendation. Follows the same tone as existing `thoughts/impl/*.md` files (prose + bullets, no rigid template).
- **`thoughts/spike/`** — directory does not exist; gets created when the notes file lands.

### Docs that might update in the spike's eventual follow-up tickets (out of scope here but worth naming)

- `CLAUDE.md` "Start Here" — new rows for `src-tauri/src/memory/*` when the production implementation lands.
- `ONBOARDING.md` data model — new `memories` / `memories_fts` sections once the schema lands.
- This spike **does not** update either. The recommendation in the notes file is the hand-off.

## What needs to change

### 1. Shape the throwaway crate

Stand up `spike/MON-91-storage/` as a self-contained Rust project with its own `Cargo.toml`. Dependencies the spike pulls in (for research, not final picks):

- **SQLite** — `rusqlite` + `tokio-rusqlite` with `bundled` feature, matching `src-tauri/`.
- **HNSW (pick one per benchmark)** — `instant-distance` and `hnsw_rs` are both pure-Rust and publicly documented with HNSW param knobs (`M`, `ef_construction`, `ef_search`). Build against both if the first one has ergonomic or performance issues; the point of the spike is to pick.
- **ONNX Runtime** — `ort` crate, currently the standard Rust binding to ONNX Runtime native. Research its packaging story on the three Tauri targets (Linux / macOS / Windows); note whether it ships prebuilt native libs or requires system install. If `ort` proves unviable for any reason (bundle size, cross-platform install surface), `candle` is the backup — pure-Rust inference, at the cost of re-implementing the model's compute graph.
- **Tokenizer** — `tokenizers` crate for loading the `bge-small-en-v1.5` tokenizer JSON. Required to produce real embeddings for the small validation subset.
- **Bench tooling** — `criterion` (or hand-rolled timing) for latency measurements. `rand` / `rand_distr` for synthetic vectors.

The crate builds as one or more binary targets:
- `generate_data` — populate a temp SQLite DB with N synthetic vectors as BLOBs.
- `build_index` — read BLOBs, build the HNSW index, persist to file.
- `bench_query` — load the HNSW index, run K random queries, measure latency distribution, compute recall@10 vs brute-force.
- `embed` — run `bge-small-en-v1.5` via `ort` on a small corpus of sentences, verify stable embeddings, and emit real vectors for a secondary recall check.

Keep each binary small; orthogonal binaries are easier to iterate than a mega-binary with subcommands.

### 2. The 1M-vector SQLite BLOB test

- Decide vector dimensionality from the embedding model's native output (`bge-small-en-v1.5` → 384). The exact number isn't important for HNSW performance characteristics; pick one and be explicit.
- Insert N rows (target ~1M; start at 100k to iterate, scale up once the plumbing works). Each row: `id INTEGER PRIMARY KEY, embedding BLOB, metadata TEXT` or similar — the exact schema is unimportant, only that the BLOB column stores the raw little-endian float32 array.
- Measure: wall-clock insert time in batches of ~10k, on-disk DB size post-insert, DB size after `VACUUM`.
- Cold read: reopen the DB, read all `embedding` BLOBs back, deserialize to `Vec<f32>`s, measure total read time. This is the HNSW cold-start cost — the index rebuild path per `substrate.md`'s "rebuildable from SQLite BLOBs on load" invariant.

### 3. HNSW build, persist, reload, query

- Pick one HNSW crate first (start with whichever has the better-maintained docs at the time of spike; both are candidates). Build the index over the 1M in-memory vectors.
- Measure: build time, in-memory RAM footprint (via `memory_stats` or `/proc/self/status` reads), persisted-file size.
- Persist the index to a sidecar file (`hnsw.bin` in the spike's temp dir), reload it, verify a deterministic query returns the same neighbors before and after persist/reload.
- For the query benchmark: sample ~1000 random query vectors, run each, collect timings. Report p50/p95/p99.
- For recall@10: pick ~100 of those queries, compute ground-truth top-10 via brute-force (full scan of the 1M BLOBs with cosine), compare against HNSW's top-10, report mean recall.
- Start with commonly-cited default params (`M=16`, `ef_construction=200`, `ef_search=64`). If latency or recall fails the bar, tune — but record which params were needed.

### 4. ONNX embedding model, end to end

- Download `bge-small-en-v1.5` (or an equivalent confirmed ONNX export) to the spike's assets dir. Record the exact HF model URL and revision in the notes — reproducibility matters.
- Load via `ort`, run inference on a tiny fixture (10–20 sentences), verify outputs are stable across runs, and verify shape matches the expected dimensionality (384).
- Measure: model file size, `ort` runtime dependency footprint, first-call cold-start latency, steady-state inference latency per sentence.
- Spot-check the HNSW recall story with real embeddings: embed the 20 fixture sentences, insert alongside the 1M synthetic vectors, query near-neighbors, confirm semantic structure surfaces (a sentence's own embedding should be in its own top-10). This is a sanity check, not a retrieval eval.

### 5. Tauri bundle integration check

- Separately from the benchmark crate, add the three deps (`rusqlite`, chosen HNSW crate, `ort`) plus the ONNX model as a bundled resource to `src-tauri/Cargo.toml` temporarily on this branch, run `npm run tauri build`, and compare bundle size (installer + app payload) against a baseline build off the same commit without the deps. This is the only part of the spike that touches production files; the dep additions are reverted before the branch is closed — they stay in the commit history but do not merge.
- Research step (does not require running the build on mac/win): read `ort` docs on Linux / macOS / Windows packaging. Does it statically link, dynamically link, require a system library? Are prebuilt binaries shipped per target? Capture the findings — they drive the cross-platform risk assessment in the recommendation.

### 6. Write the notes file

`thoughts/spike/MON-91-storage.md` contains:
- Methodology — exact crates + versions, exact model + revision, exact machine specs, exact command sequence to reproduce.
- Raw numbers — one subsection per acceptance-criterion metric.
- Anomalies — anything surprising (e.g., HNSW rebuild slower than expected, ort cold-start dominant, bundle size explosion).
- Recommendation — explicit go/pivot. If go, name the locked choices (HNSW crate, ONNX loader, embedding model). If pivot, name the alternative and why.
- Open follow-ups — things the production implementation needs to confirm or decide that the spike intentionally didn't (reranking approach, quantized vs float weights, section-precision stale flagging, etc.).

### 7. Clean up

Spike crate stays on this branch as forensic context — do not delete in the same PR. The production ticket that implements `memories` / HNSW sidecar references this plan + notes file, and the spike crate can be archived or deleted once the production implementation lands. Branch merges to `master` to capture the measurements and recommendation; the `spike/` directory is understood as non-production.

## Resolved decisions

1. **Spike crate location.** `spike/MON-91-storage/` at repo root, outside `src-tauri/`'s build, with its own `Cargo.toml`. Crate + notes file land in one PR.

2. **Tauri bundle measurement.** Add deps to `src-tauri/Cargo.toml` temporarily on this branch, run `npm run tauri build`, compare to a baseline build. Dep additions get reverted before merge (captured in commit history for forensic context, not in the final tree).

3. **Synthetic vs real vectors.** Split the load: headline 1M-scale latency + p99 benchmark runs on synthetic Gaussian vectors (fast to generate, stresses HNSW graph structure). Recall@10 validation runs on real `bge-small-en-v1.5` embeddings over a smaller subset (~10k). Notes file makes the split explicit.

4. **HNSW crate.** Single crate. Pick whichever of `instant-distance` / `hnsw_rs` has fresher maintenance + clearer docs at spike start. Fall back to the other only if the first fails p99 or recall bars.

5. **Embedding model precision.** Float32 `bge-small-en-v1.5` for all recall + latency measurements. Quantized variant not benchmarked in this spike.

6. **macOS / Windows verification.** Linux end-to-end (primary) + macOS laptop end-to-end (confirms `ort` packaging, HNSW build, SQLite BLOB path on Apple silicon). Windows remains documentation-review-only; captured as a follow-up risk in the notes rather than blocking the spike.

7. **Timebox.** 1 day, single session. Notes file drafted continuously so the stop state is always "here is what we measured so far." If the 1M-scale run doesn't fit, scale down to the largest N that fits and flag the gap in the recommendation.

8. **Benchmark hardware.** Linux desktop (primary) + macOS laptop (cross-platform check). Notes file records exact CPU / disk / RAM for each machine and reports numbers per-machine. p99 claims flagged "valid on this hardware" rather than universal.

## Out of scope reminders

- **Production implementation** — the `memories`, `memories_fts`, `memory_keeper_runs`, `quest_reports` tables and the keeper pipeline are separate tickets. This plan only validates the storage plumbing.
- **Schema migrations into live `monarch.db`** — every measurement runs against an isolated temp DB. Nothing the spike writes touches the user's real database.
- **Keeper / distillation logic** — the spike does not run any LLM-driven extraction. It only validates that given vectors (synthetic or pre-computed), the storage + index + search layer behaves within the design envelope.
- **Memory tree structure, inner nodes, top-level taxonomy** — orthogonal to storage. Handled by the implementation tickets downstream.
- **Retrieval quality eval (50 memories / 20 queries harness)** — `distillation.md` § Open questions flags this as non-negotiable but lives in its own spike, separate from storage viability.
- **Reranker design** — also a distinct concern per `distillation.md` § Open questions.
- **Full macOS / Windows bundle verification** — spike captures research-level risk assessment only; real cross-platform CI / manual verification is a follow-up ticket.
- **BM25 / FTS5 side of the hybrid search** — SQLite FTS5 is well-trodden territory and not in dispute. This spike only looks at the vector half where the risk lives.
- **UI for the Memory Inspector** — captured in feature tickets. Not relevant to storage viability.
