# MON-91 — Implementation notes

Linear: https://linear.app/monarch-commander/issue/MON-91
Plan: [`thoughts/plan/MON-91.md`](../plan/MON-91.md)
Measurements + recommendation: [`thoughts/spike/MON-91-storage.md`](../spike/MON-91-storage.md)

## What was done

Built a throwaway Rust crate under `spike/MON-91-storage/` to exercise the tentative shadow-memory storage stack — SQLite BLOB + `instant-distance` HNSW + `ort`-hosted `bge-small-en-v1.5` — end to end, at 100k and 1M vectors, on both synthetic Gaussian draws and 10k real embeddings pulled from the monarch repo itself.

Four small binaries (`generate_data`, `build_index`, `bench_query`, `embed`) and a shared `lib.rs` keep each measurement independently rerunnable. Raw JSON outputs are committed under `spike/MON-91-storage/results/` so future readers can diff numbers without rebuilding.

The spike confirmed every acceptance-criterion bar: p99 query latency 5.81 ms at 1M (target < 50 ms), recall@10 = 1.000 on real embeddings (target > 0.9), +24.6 MiB binary delta and +151 MiB total including the lazy-downloaded model (target < 200 MiB). No pivot to LanceDB / Kùzu / SurrealDB needed. Design doc assumption #12 in `substrate.md` is no longer tentative.

## Key decisions

- **HNSW crate: `instant-distance`** over `hnsw_rs`. Simpler API, matches design-doc-default, cleared bars on first pass — no bake-off needed.
- **Normalised cosine via L2 on unit vectors.** All vectors L2-normalised at write time; graph uses squared Euclidean distance, which is monotonic with cosine similarity on the unit sphere. Saves a normalization step per query and keeps the `Point` impl trivial.
- **Synthetic Gaussian for latency, real embeddings for recall.** Recall@10 on synthetic Gaussian vectors collapsed to 0.21 at 1M — the known curse-of-dimensionality artifact on isotropic draws, not an HNSW defect. Real embeddings (dogfood corpus of 10k monarch repo lines) measured recall@10 = 1.000. The notes file calls this split out prominently so future readers don't misread the synthetic number.
- **Bundle probe via temporary commit.** Added the storage deps to `src-tauri/Cargo.toml` + a dead-but-symbol-live `_spike_bundle_probe` module on commit `363d5fd`, measured baseline + with-deps binaries, then reverted on commit `f6feabc`. The measurement lives in the commit history + notes file; the production memory tickets (MON-95..97) will pull these deps in intentionally when they land.
- **ONNX Runtime statically linked via `ort`'s `download-binaries` feature.** No `libonnxruntime.so` to ship alongside — the `.a` from the download feature gets folded into the final binary at LTO time. Adds ~25 MiB to the monarch binary.
- **Model lazy-download recommendation.** 127 MiB ONNX model is an asset, not linked code. Recommended production path: lazy-fetch from HF Hub to `~/.config/monarch/models/` on first memory write. Keeps the installer at +25 MiB and defers the fetch to users who opt into shadow memory.

## Files touched

**New:**
- `spike/MON-91-storage/` — throwaway crate: `Cargo.toml`, `src/lib.rs`, `src/bin/{generate_data,build_index,bench_query,embed}.rs`, `fetch_model.sh`, `README.md`, `.gitignore`, raw JSON outputs under `results/`.
- `thoughts/spike/MON-91-storage.md` — measurements + go/pivot recommendation.

**Modified:**
- `thoughts/design/shadow-cognition/substrate.md` — dropped "tentative" framing on working assumption #12, pointed storage-stack references at the spike notes.
- `thoughts/design/shadow-cognition/distillation.md` — same treatment on the Implications-for-the-data-model section.

**Temporarily touched and reverted** (visible in commit history between `363d5fd` and `f6feabc`):
- `src-tauri/Cargo.toml` — storage deps added for the bundle probe, reverted.
- `src-tauri/src/lib.rs` — `_spike_bundle_probe` module mount, reverted.
- `src-tauri/src/_spike_bundle_probe.rs` — dead-but-symbol-live probe module, deleted in the revert.

## Follow-up tickets (created)

- **MON-93** — Design the hybrid BM25+vector reranker for memory retrieval.
- **MON-94** — Spike: memory retrieval eval harness (50 memories / 20 queries).
- **MON-95** — Wire up `memories_fts` (FTS5) alongside the HNSW vector path.
- **MON-96** — Background HNSW rebuild worker with atomic read swap.
- **MON-97** — Incremental HNSW insert path for per-memory writes.

All five are in the `Memory & context tools` project with `Related to` links back to MON-91.

## What was left out

- **macOS run.** Notes file reserves a "macOS row" that the user fills in after a local build + benchmark pass on their mac laptop. The Linux run is authoritative; mac is a cross-platform check. Not a blocker for closing MON-91 because `ort`'s `download-binaries` feature documents prebuilt mac x86_64 + aarch64 binaries and the rest of the stack is pure Rust.
- **Windows run.** Documentation-level risk review only; no hardware. Captured as a follow-up for whenever Windows packaging matters.
- **Quantized int8 model variant.** Per plan, only float32 measured. Quantized file size (~33 MiB) noted as a known alternative if bundle pressure ever forces the question.
- **Reranker / FTS5 / rebuild worker / incremental insert / eval harness** — all broken out into MON-93 / MON-95 / MON-96 / MON-97 / MON-94 respectively. Intentionally not done inside this spike.
- **Production schema migrations** — no changes to `src-tauri/src/db.rs`. The `memories` / `memories_fts` tables will land in MON-95 / a future ticket and will cite this spike.
