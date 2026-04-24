# MON-91 — Storage stack viability spike

Throwaway Rust crate validating the shadow-memory storage direction from
[`thoughts/design/shadow-cognition/substrate.md`](../../thoughts/design/shadow-cognition/substrate.md) § L4 and working assumption #12:

- **SQLite BLOBs** for raw vectors, via `rusqlite` (bundled build, matches `src-tauri`).
- **HNSW index** via `instant-distance` (pure-Rust, rebuildable from BLOBs).
- **ONNX embedding model** (`bge-small-en-v1.5`, 384-dim, float32) via `ort`.

Not production code. Does not touch `~/.config/monarch/monarch.db`. Does not link against
the main `monarch` crate. Lives here so the bench results + notes travel in one PR;
the directory can be deleted once the production implementation lands.

## Binaries

| Binary | What it does |
|---|---|
| `generate_data` | Inserts N synthetic Gaussian vectors as BLOB rows into a temp SQLite DB. |
| `build_index` | Reads BLOBs, builds the HNSW graph, measures build time + RAM + disk. |
| `bench_query` | Runs M random queries; measures p50/p95/p99 latency and recall@10 vs brute-force. |
| `embed` | Loads the `bge-small-en-v1.5` ONNX model, embeds a corpus, optionally populates a real-vector DB for the recall validation run. |

## Prerequisites

```bash
./fetch_model.sh    # downloads bge-small-en-v1.5 ONNX + tokenizer into .assets/
```

## Typical run

```bash
# From spike/MON-91-storage/
cargo build --release

# Synthetic 1M benchmark
./target/release/generate_data --n 1000000 --dim 384 --db /tmp/mon91-synth.db
./target/release/build_index --db /tmp/mon91-synth.db --out /tmp/mon91-synth.hnsw
./target/release/bench_query --db /tmp/mon91-synth.db --hnsw /tmp/mon91-synth.hnsw \
    --queries 1000 --recall-sample 100

# Real embeddings recall check (10k code snippets from the repo)
./target/release/embed --corpus ../../ --db /tmp/mon91-real.db --limit 10000
./target/release/build_index --db /tmp/mon91-real.db --out /tmp/mon91-real.hnsw
./target/release/bench_query --db /tmp/mon91-real.db --hnsw /tmp/mon91-real.hnsw \
    --queries 100 --recall-sample 100
```

Numbers land in [`thoughts/spike/MON-91-storage.md`](../../thoughts/spike/MON-91-storage.md).
