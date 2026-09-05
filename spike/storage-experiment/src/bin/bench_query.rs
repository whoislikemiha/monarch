//! Run M random queries against a persisted HNSW index, report latency
//! percentiles, and compute recall@10 against brute-force ground truth on a
//! sample of queries.

use anyhow::{Context, Result};
use clap::Parser;
use instant_distance::{HnswMap, Point, Search};
use mon91_storage_spike::{blob_to_f32, brute_top_k, l2_normalize, percentile, rss_mib};
use rand::SeedableRng;
use rand_distr::{Distribution, Normal};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Instant;

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
    hnsw: PathBuf,

    /// Number of timed queries for the latency distribution.
    #[arg(long, default_value_t = 1000)]
    queries: usize,

    /// Number of queries to also run brute-force for recall@10 computation.
    #[arg(long, default_value_t = 100)]
    recall_sample: usize,

    /// k for recall@k and returned neighbour count.
    #[arg(long, default_value_t = 10)]
    k: usize,

    /// If set, generate query vectors from this Gaussian seed. Otherwise draw
    /// random vectors already in the DB (in-distribution, harder for HNSW).
    #[arg(long)]
    gaussian_queries: bool,

    #[arg(long, default_value_t = 17)]
    seed: u64,
}

fn sample_query_vectors_from_db(
    conn: &Connection,
    n: usize,
    dim: usize,
    seed: u64,
) -> Result<Vec<(u32, Vec<f32>)>> {
    use rand::seq::SliceRandom;
    let mut stmt = conn.prepare("SELECT id FROM vectors")?;
    let all: Vec<i64> = stmt
        .query_map([], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let mut picked = all.clone();
    picked.shuffle(&mut rng);
    picked.truncate(n);

    let mut fetch =
        conn.prepare("SELECT embedding FROM vectors WHERE id = ?")?;
    let mut out = Vec::with_capacity(n);
    for id in picked {
        let blob: Vec<u8> = fetch.query_row([id], |r| r.get(0))?;
        let v = blob_to_f32(&blob)?;
        assert_eq!(v.len(), dim, "dim mismatch");
        out.push((id as u32, v));
    }
    Ok(out)
}

fn gaussian_queries(n: usize, dim: usize, seed: u64) -> Vec<Vec<f32>> {
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let dist = Normal::new(0.0f32, 1.0).unwrap();
    (0..n)
        .map(|_| {
            let mut v: Vec<f32> = (0..dim).map(|_| dist.sample(&mut rng)).collect();
            l2_normalize(&mut v);
            v
        })
        .collect()
}

fn load_all_for_brute(conn: &Connection) -> Result<Vec<(u32, Vec<f32>)>> {
    let mut stmt = conn.prepare("SELECT id, embedding FROM vectors ORDER BY id")?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let blob: Vec<u8> = row.get(1)?;
        out.push((id as u32, blob_to_f32(&blob)?));
    }
    Ok(out)
}

fn main() -> Result<()> {
    let args = Args::parse();
    let rss_start = rss_mib();

    eprintln!("loading HNSW from {}", args.hnsw.display());
    let t_load = Instant::now();
    let raw = std::fs::read(&args.hnsw)
        .with_context(|| format!("read {}", args.hnsw.display()))?;
    let hnsw: HnswMap<Embedding, u32> =
        bincode::deserialize(&raw).context("bincode deserialize hnsw")?;
    let load_secs = t_load.elapsed().as_secs_f64();
    let rss_after_load = rss_mib();
    eprintln!("hnsw load took {load_secs:.2}s, RSS now {rss_after_load:.0} MiB");

    let conn = Connection::open(&args.db)?;
    let n_total: i64 = conn.query_row("SELECT COUNT(*) FROM vectors", [], |r| r.get(0))?;
    let dim: usize = {
        let blob: Vec<u8> = conn.query_row(
            "SELECT embedding FROM vectors LIMIT 1",
            [],
            |r| r.get(0),
        )?;
        blob.len() / 4
    };
    eprintln!("db has {n_total} rows at dim {dim}");

    // Build query set.
    let query_vectors: Vec<Vec<f32>> = if args.gaussian_queries {
        gaussian_queries(args.queries, dim, args.seed)
    } else {
        sample_query_vectors_from_db(&conn, args.queries, dim, args.seed)?
            .into_iter()
            .map(|(_, v)| v)
            .collect()
    };

    // Timing loop.
    let mut latencies: Vec<u128> = Vec::with_capacity(query_vectors.len());
    let mut search = Search::default();
    let mut last_neighbours: Vec<Vec<u32>> = Vec::with_capacity(query_vectors.len());
    for q in &query_vectors {
        let point = Embedding(q.clone());
        let t = Instant::now();
        let hits: Vec<u32> = hnsw
            .search(&point, &mut search)
            .take(args.k)
            .map(|item| *item.value)
            .collect();
        latencies.push(t.elapsed().as_micros());
        last_neighbours.push(hits);
    }
    latencies.sort_unstable();

    // Recall@k against brute force over a sample.
    let mut recall_mean: Option<f64> = None;
    if args.recall_sample > 0 {
        eprintln!("loading all vectors for brute-force ground truth...");
        let t_bload = Instant::now();
        let all = load_all_for_brute(&conn)?;
        eprintln!("loaded {} rows in {:.1}s", all.len(), t_bload.elapsed().as_secs_f64());

        let sample = args.recall_sample.min(query_vectors.len());
        let mut recalls = Vec::with_capacity(sample);
        for i in 0..sample {
            let truth: HashSet<u32> =
                brute_top_k(&query_vectors[i], &all, args.k).into_iter().collect();
            let hnsw_set: HashSet<u32> = last_neighbours[i].iter().copied().collect();
            let hit = truth.intersection(&hnsw_set).count();
            recalls.push(hit as f64 / args.k as f64);
        }
        recall_mean = Some(recalls.iter().sum::<f64>() / recalls.len() as f64);
    }

    let total: u128 = latencies.iter().sum();
    let rss_end = rss_mib();

    println!("{{");
    println!("  \"binary\": \"bench_query\",");
    println!("  \"n_in_index\": {n_total},");
    println!("  \"dim\": {dim},");
    println!("  \"queries\": {},", args.queries);
    println!("  \"k\": {},", args.k);
    println!("  \"gaussian_queries\": {},", args.gaussian_queries);
    println!("  \"hnsw_load_secs\": {load_secs:.3},");
    println!("  \"p50_us\": {},", percentile(&latencies, 0.50));
    println!("  \"p95_us\": {},", percentile(&latencies, 0.95));
    println!("  \"p99_us\": {},", percentile(&latencies, 0.99));
    println!(
        "  \"mean_us\": {:.1},",
        total as f64 / latencies.len() as f64
    );
    println!(
        "  \"p50_ms\": {:.3},",
        percentile(&latencies, 0.50) as f64 / 1000.0
    );
    println!(
        "  \"p99_ms\": {:.3},",
        percentile(&latencies, 0.99) as f64 / 1000.0
    );
    if let Some(r) = recall_mean {
        println!("  \"recall_at_{}_mean\": {:.4},", args.k, r);
        println!("  \"recall_sample_size\": {},", args.recall_sample);
    }
    println!("  \"rss_start_mib\": {rss_start:.1},");
    println!("  \"rss_after_load_mib\": {rss_after_load:.1},");
    println!("  \"rss_end_mib\": {rss_end:.1}");
    println!("}}");
    Ok(())
}
