//! Populate a temp SQLite DB with N synthetic Gaussian vectors as BLOB rows.
//!
//! Gaussian draws stress HNSW graph connectivity fairly even though their
//! neighbourhood density isn't representative of real embeddings — real
//! embeddings are covered by `embed` + `build_index` on the dogfood corpus.

use anyhow::{Context, Result};
use clap::Parser;
use mon91_storage_spike::{f32_to_blob, l2_normalize, open_db, rss_mib};
use rand::SeedableRng;
use rand_distr::{Distribution, Normal};
use std::path::PathBuf;
use std::time::Instant;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "/tmp/mon91-synth.db")]
    db: PathBuf,

    #[arg(long, default_value_t = 1_000_000)]
    n: usize,

    #[arg(long, default_value_t = 384)]
    dim: usize,

    #[arg(long, default_value_t = 10_000)]
    batch: usize,

    #[arg(long, default_value_t = 42)]
    seed: u64,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let rss_before = rss_mib();

    if args.db.exists() {
        std::fs::remove_file(&args.db).ok();
    }
    let mut conn = open_db(&args.db)?;
    let mut rng = rand::rngs::StdRng::seed_from_u64(args.seed);
    let dist = Normal::new(0.0f32, 1.0).unwrap();

    let start = Instant::now();
    let mut inserted = 0usize;
    while inserted < args.n {
        let batch_size = args.batch.min(args.n - inserted);
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare("INSERT INTO vectors (id, embedding) VALUES (?, ?)")?;
            for i in 0..batch_size {
                let id = (inserted + i) as i64;
                let mut v: Vec<f32> = (0..args.dim).map(|_| dist.sample(&mut rng)).collect();
                l2_normalize(&mut v);
                stmt.execute(rusqlite::params![id, f32_to_blob(&v)])?;
            }
        }
        tx.commit()?;
        inserted += batch_size;
        if inserted % (args.batch * 10) == 0 || inserted == args.n {
            let elapsed = start.elapsed().as_secs_f64();
            let rate = inserted as f64 / elapsed;
            eprintln!("inserted {inserted} / {} ({rate:.0} rows/s)", args.n);
        }
    }

    let elapsed = start.elapsed();
    let size = std::fs::metadata(&args.db)
        .map(|m| m.len())
        .context("stat db file")?;
    let rss_after = rss_mib();

    println!("{{");
    println!("  \"binary\": \"generate_data\",");
    println!("  \"n\": {},", args.n);
    println!("  \"dim\": {},", args.dim);
    println!("  \"insert_secs\": {:.3},", elapsed.as_secs_f64());
    println!(
        "  \"insert_rate_rows_per_sec\": {:.0},",
        args.n as f64 / elapsed.as_secs_f64()
    );
    println!("  \"db_bytes\": {size},");
    println!("  \"db_mib\": {:.2},", size as f64 / 1024.0 / 1024.0);
    println!("  \"rss_before_mib\": {rss_before:.1},");
    println!("  \"rss_after_mib\": {rss_after:.1}");
    println!("}}");
    Ok(())
}
