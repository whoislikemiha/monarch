//! Read all vector BLOBs from SQLite, build an HNSW graph, measure it, and
//! optionally persist the graph to a sidecar file.

use anyhow::{Context, Result};
use clap::Parser;
use instant_distance::{Builder, HnswMap, Point, Search};
use mon91_storage_spike::{blob_to_f32, rss_mib};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Instant;

/// Unit-norm vectors → L2 distance is monotonic with cosine similarity.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Embedding(pub Vec<f32>);

impl Point for Embedding {
    fn distance(&self, other: &Self) -> f32 {
        let mut sum = 0.0f32;
        for (a, b) in self.0.iter().zip(other.0.iter()) {
            let d = a - b;
            sum += d * d;
        }
        sum
    }
}

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "/tmp/mon91-synth.db")]
    db: PathBuf,

    #[arg(long, default_value = "/tmp/mon91-synth.hnsw")]
    out: PathBuf,

    /// HNSW build-time fanout. Higher = better graph, slower build.
    #[arg(long, default_value_t = 16)]
    m: usize,

    /// Candidate list size at build. Higher = better graph, slower.
    #[arg(long, default_value_t = 200)]
    ef_construction: usize,

    /// Skip writing the sidecar file (still measures build + rebuild cost).
    #[arg(long)]
    no_persist: bool,
}

fn load_all(conn: &Connection) -> Result<Vec<(u32, Embedding)>> {
    let mut stmt = conn.prepare("SELECT id, embedding FROM vectors ORDER BY id")?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let blob: Vec<u8> = row.get(1)?;
        let v = blob_to_f32(&blob)?;
        out.push((id as u32, Embedding(v)));
    }
    Ok(out)
}

fn main() -> Result<()> {
    let args = Args::parse();
    let rss_start = rss_mib();

    let conn = Connection::open(&args.db)
        .with_context(|| format!("open sqlite at {}", args.db.display()))?;

    let t0 = Instant::now();
    let loaded = load_all(&conn)?;
    let load_secs = t0.elapsed().as_secs_f64();
    let n = loaded.len();
    let dim = loaded.first().map(|(_, e)| e.0.len()).unwrap_or(0);
    eprintln!("loaded {n} vectors (dim {dim}) from SQLite in {load_secs:.2}s");

    let rss_after_load = rss_mib();

    let (ids, points): (Vec<u32>, Vec<Embedding>) = loaded.into_iter().unzip();

    let t_build = Instant::now();
    let hnsw: HnswMap<Embedding, u32> = Builder::default()
        .ef_construction(args.ef_construction)
        .build(points, ids);
    let build_secs = t_build.elapsed().as_secs_f64();
    let rss_after_build = rss_mib();

    // Sanity query — a zero vector resolves to *some* neighbour without panicking.
    let mut search = Search::default();
    let first = hnsw
        .search(&Embedding(vec![0.0; dim]), &mut search)
        .next()
        .map(|item| *item.value);
    eprintln!("sanity: zero-query nearest id = {first:?}");

    let mut persist_secs: Option<f64> = None;
    let mut persist_bytes: Option<u64> = None;
    if !args.no_persist {
        let t_persist = Instant::now();
        let bytes = bincode::serialize(&hnsw).context("bincode serialize hnsw")?;
        std::fs::write(&args.out, &bytes)?;
        persist_secs = Some(t_persist.elapsed().as_secs_f64());
        persist_bytes = Some(bytes.len() as u64);

        // Round-trip sanity — reload and compare size.
        let t_reload = Instant::now();
        let raw = std::fs::read(&args.out)?;
        let _reloaded: HnswMap<Embedding, u32> =
            bincode::deserialize(&raw).context("bincode deserialize hnsw")?;
        eprintln!(
            "reload-from-sidecar took {:.2}s (bytes {})",
            t_reload.elapsed().as_secs_f64(),
            raw.len()
        );
    }

    println!("{{");
    println!("  \"binary\": \"build_index\",");
    println!("  \"n\": {n},");
    println!("  \"dim\": {dim},");
    println!("  \"m\": {},", args.m);
    println!("  \"ef_construction\": {},", args.ef_construction);
    println!("  \"load_from_sqlite_secs\": {load_secs:.3},");
    println!("  \"build_secs\": {build_secs:.3},");
    if let (Some(ps), Some(pb)) = (persist_secs, persist_bytes) {
        println!("  \"persist_secs\": {ps:.3},");
        println!("  \"persist_bytes\": {pb},");
        println!("  \"persist_mib\": {:.2},", pb as f64 / 1024.0 / 1024.0);
    }
    println!("  \"rss_start_mib\": {rss_start:.1},");
    println!("  \"rss_after_load_mib\": {rss_after_load:.1},");
    println!("  \"rss_after_build_mib\": {rss_after_build:.1},");
    println!(
        "  \"hnsw_resident_mib_approx\": {:.1}",
        (rss_after_build - rss_after_load).max(0.0)
    );
    println!("}}");
    Ok(())
}
