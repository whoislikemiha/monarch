//! Shared helpers for the MON-91 storage spike.
//!
//! Not wired into production. Keep this small; each binary stays independently readable.

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;

pub const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS vectors (
    id       INTEGER PRIMARY KEY,
    label    TEXT,
    embedding BLOB NOT NULL
);
"#;

pub fn open_db(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)
        .with_context(|| format!("open sqlite at {}", path.display()))?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
    conn.execute_batch(SCHEMA_SQL)?;
    Ok(conn)
}

pub fn f32_to_blob(v: &[f32]) -> Vec<u8> {
    bytemuck::cast_slice::<f32, u8>(v).to_vec()
}

pub fn blob_to_f32(b: &[u8]) -> Result<Vec<f32>> {
    if b.len() % 4 != 0 {
        anyhow::bail!("blob length {} is not a multiple of 4", b.len());
    }
    Ok(bytemuck::cast_slice::<u8, f32>(b).to_vec())
}

/// Unit-normalise in place. Lets us treat L2 distance on the HNSW graph as a
/// monotonic proxy for cosine similarity, which is what the design calls for.
pub fn l2_normalize(v: &mut [f32]) {
    let norm_sq: f32 = v.iter().map(|x| x * x).sum();
    if norm_sq > 0.0 {
        let inv = 1.0 / norm_sq.sqrt();
        for x in v.iter_mut() {
            *x *= inv;
        }
    }
}

/// Process resident set size in MiB, Linux only. Returns 0.0 on other platforms
/// so call sites stay terse — we only benchmark on Linux in this spike.
pub fn rss_mib() -> f64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(s) = std::fs::read_to_string("/proc/self/status") {
            for line in s.lines() {
                if let Some(rest) = line.strip_prefix("VmRSS:") {
                    let kb: f64 = rest
                        .split_whitespace()
                        .next()
                        .and_then(|n| n.parse().ok())
                        .unwrap_or(0.0);
                    return kb / 1024.0;
                }
            }
        }
    }
    0.0
}

pub fn percentile(sorted_micros: &[u128], p: f64) -> u128 {
    if sorted_micros.is_empty() {
        return 0;
    }
    let idx = ((sorted_micros.len() - 1) as f64 * p).round() as usize;
    sorted_micros[idx]
}

/// L2 distance squared — cheaper than the rooted form, monotonic for ranking.
pub fn l2_sq(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| {
            let d = x - y;
            d * d
        })
        .sum()
}

/// Brute-force top-K over a slice of vectors. O(N*D) per query. Used as recall
/// ground truth — intentionally not optimised.
pub fn brute_top_k(query: &[f32], vectors: &[(u32, Vec<f32>)], k: usize) -> Vec<u32> {
    let mut scored: Vec<(u32, f32)> = vectors
        .iter()
        .map(|(id, v)| (*id, l2_sq(query, v)))
        .collect();
    scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().take(k).map(|(id, _)| id).collect()
}
