//! Load bge-small-en-v1.5 via ORT, embed a corpus, and optionally populate a
//! SQLite DB with real embeddings so build_index + bench_query can run the
//! recall-on-real-embeddings validation.
//!
//! Corpus source: walks a directory, reads every non-binary text file below
//! ~100 KiB, splits on lines, dedups, takes `--limit` lines. Using the repo
//! itself as a dogfood corpus keeps the spike self-contained.

use anyhow::Result;
use clap::Parser;
use mon91_storage_spike::{f32_to_blob, l2_normalize, open_db, rss_mib};
use ndarray::{s, Array2};
use ort::session::Session;
use ort::value::Value;
use rusqlite::params;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokenizers::Tokenizer;

#[derive(Parser)]
struct Args {
    /// Root dir to pull text lines from. Default: this repo.
    #[arg(long, default_value = "../..")]
    corpus: PathBuf,

    #[arg(long, default_value = "/tmp/mon91-real.db")]
    db: PathBuf,

    #[arg(long, default_value_t = 10_000)]
    limit: usize,

    #[arg(long, default_value = ".assets/model.onnx")]
    model: PathBuf,

    #[arg(long, default_value = ".assets/tokenizer.json")]
    tokenizer: PathBuf,

    /// Max tokens per input (truncates longer lines).
    #[arg(long, default_value_t = 128)]
    max_len: usize,

    /// Minimum printable chars per line to include.
    #[arg(long, default_value_t = 20)]
    min_chars: usize,

    /// Max bytes per file to scan (skip very large files).
    #[arg(long, default_value_t = 100_000)]
    max_file_bytes: u64,

    /// Skip DB insert — just validate the model loads + produces stable output.
    #[arg(long)]
    dry_run: bool,
}

fn gather_lines(root: &Path, limit: usize, min_chars: usize, max_file_bytes: u64) -> Result<Vec<String>> {
    use std::collections::HashSet;
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::with_capacity(limit);

    let ignore_dirs = ["node_modules", "target", ".git", "dist", ".assets"];

    fn walk(
        dir: &Path,
        ignore: &[&str],
        max_file_bytes: u64,
        min_chars: usize,
        seen: &mut std::collections::HashSet<String>,
        out: &mut Vec<String>,
        limit: usize,
    ) -> Result<()> {
        if out.len() >= limit {
            return Ok(());
        }
        let rd = match std::fs::read_dir(dir) {
            Ok(r) => r,
            Err(_) => return Ok(()),
        };
        for entry in rd {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = entry.path();
            let name = entry.file_name();
            let name_s = name.to_string_lossy();
            if ignore.contains(&name_s.as_ref()) {
                continue;
            }
            if path.is_dir() {
                walk(&path, ignore, max_file_bytes, min_chars, seen, out, limit)?;
                if out.len() >= limit {
                    return Ok(());
                }
            } else if path.is_file() {
                let meta = match entry.metadata() {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                if meta.len() > max_file_bytes {
                    continue;
                }
                // Accept only text extensions for speed.
                let ok_ext = matches!(
                    path.extension().and_then(|e| e.to_str()),
                    Some("md" | "rs" | "ts" | "tsx" | "svelte" | "js" | "json" | "toml" | "yaml" | "yml" | "txt")
                );
                if !ok_ext {
                    continue;
                }
                let content = match std::fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.chars().filter(|c| !c.is_whitespace()).count() < min_chars {
                        continue;
                    }
                    if seen.insert(trimmed.to_string()) {
                        out.push(trimmed.to_string());
                        if out.len() >= limit {
                            return Ok(());
                        }
                    }
                }
            }
        }
        Ok(())
    }

    walk(root, &ignore_dirs, max_file_bytes, min_chars, &mut seen, &mut out, limit)?;
    Ok(out)
}

fn embed_batch(
    session: &mut Session,
    tokenizer: &Tokenizer,
    texts: &[&str],
    max_len: usize,
) -> Result<Vec<Vec<f32>>> {
    let encodings = tokenizer
        .encode_batch(texts.to_vec(), true)
        .map_err(|e| anyhow::anyhow!("tokenizer encode_batch: {e}"))?;

    let batch = encodings.len();
    let seq = encodings
        .iter()
        .map(|e| e.get_ids().len().min(max_len))
        .max()
        .unwrap_or(1)
        .max(1);

    // Right-pad with 0s to a rectangular batch.
    let mut input_ids = Array2::<i64>::zeros((batch, seq));
    let mut attn_mask = Array2::<i64>::zeros((batch, seq));
    let mut type_ids = Array2::<i64>::zeros((batch, seq));
    for (i, enc) in encodings.iter().enumerate() {
        let ids = enc.get_ids();
        let am = enc.get_attention_mask();
        let ti = enc.get_type_ids();
        let n = ids.len().min(seq);
        for j in 0..n {
            input_ids[[i, j]] = ids[j] as i64;
            attn_mask[[i, j]] = am[j] as i64;
            type_ids[[i, j]] = ti[j] as i64;
        }
    }

    let input_ids_v = Value::from_array(input_ids)?;
    let attn_mask_v = Value::from_array(attn_mask)?;
    let type_ids_v = Value::from_array(type_ids)?;

    let outputs = session.run(ort::inputs![
        "input_ids" => input_ids_v,
        "attention_mask" => attn_mask_v,
        "token_type_ids" => type_ids_v,
    ])?;

    // bge-small-en-v1.5 returns `last_hidden_state` [batch, seq, 384]. CLS pooling.
    let (_shape, data) = outputs[0].try_extract_tensor::<f32>()?;
    let dim = data.len() / (batch * seq);
    let arr = ndarray::ArrayView3::from_shape((batch, seq, dim), data)?;

    let mut embeddings = Vec::with_capacity(batch);
    for i in 0..batch {
        let mut v: Vec<f32> = arr.slice(s![i, 0, ..]).to_vec();
        l2_normalize(&mut v);
        embeddings.push(v);
    }
    Ok(embeddings)
}

fn main() -> Result<()> {
    let args = Args::parse();
    let rss_start = rss_mib();

    eprintln!("gathering lines from {}", args.corpus.display());
    let t_gather = Instant::now();
    let lines = gather_lines(&args.corpus, args.limit, args.min_chars, args.max_file_bytes)?;
    eprintln!(
        "collected {} unique lines in {:.2}s",
        lines.len(),
        t_gather.elapsed().as_secs_f64()
    );
    if lines.is_empty() {
        anyhow::bail!("no lines found; check --corpus and --min-chars");
    }

    eprintln!("loading tokenizer from {}", args.tokenizer.display());
    let tokenizer =
        Tokenizer::from_file(&args.tokenizer).map_err(|e| anyhow::anyhow!("tokenizer: {e}"))?;

    eprintln!("loading ONNX model from {}", args.model.display());
    let t_model = Instant::now();
    let mut session = Session::builder()?.commit_from_file(&args.model)?;
    let model_load_secs = t_model.elapsed().as_secs_f64();
    eprintln!("model loaded in {model_load_secs:.2}s, RSS {:.0} MiB", rss_mib());

    // Stability check — embed the first sentence twice, verify match.
    let test_a = embed_batch(&mut session, &tokenizer, &[lines[0].as_str()], args.max_len)?;
    let test_b = embed_batch(&mut session, &tokenizer, &[lines[0].as_str()], args.max_len)?;
    let max_diff = test_a[0]
        .iter()
        .zip(test_b[0].iter())
        .map(|(a, b)| (a - b).abs())
        .fold(0f32, f32::max);
    eprintln!("stability check max abs diff: {max_diff:e}");
    let dim = test_a[0].len();
    eprintln!("embedding dim: {dim}");

    if args.dry_run {
        println!("{{");
        println!("  \"binary\": \"embed\",");
        println!("  \"mode\": \"dry_run\",");
        println!("  \"dim\": {dim},");
        println!("  \"model_load_secs\": {model_load_secs:.3},");
        println!("  \"stability_max_abs_diff\": {max_diff:e},");
        println!("  \"rss_start_mib\": {rss_start:.1},");
        println!("  \"rss_end_mib\": {:.1}", rss_mib());
        println!("}}");
        return Ok(());
    }

    if args.db.exists() {
        std::fs::remove_file(&args.db).ok();
    }
    let mut conn = open_db(&args.db)?;

    let batch_size = 32;
    let mut total_infer_secs = 0f64;
    let t0 = Instant::now();
    for (chunk_idx, chunk) in lines.chunks(batch_size).enumerate() {
        let refs: Vec<&str> = chunk.iter().map(|s| s.as_str()).collect();
        let t = Instant::now();
        let embs = embed_batch(&mut session, &tokenizer, &refs, args.max_len)?;
        total_infer_secs += t.elapsed().as_secs_f64();

        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare("INSERT INTO vectors (id, label, embedding) VALUES (?, ?, ?)")?;
            for (j, emb) in embs.iter().enumerate() {
                let id = (chunk_idx * batch_size + j) as i64;
                stmt.execute(params![id, &chunk[j], f32_to_blob(emb)])?;
            }
        }
        tx.commit()?;

        if chunk_idx % 20 == 0 {
            eprintln!(
                "chunk {chunk_idx}, total rows {}, elapsed {:.1}s",
                (chunk_idx + 1) * batch_size,
                t0.elapsed().as_secs_f64()
            );
        }
    }
    let wall_secs = t0.elapsed().as_secs_f64();
    let db_size = std::fs::metadata(&args.db).map(|m| m.len()).unwrap_or(0);

    println!("{{");
    println!("  \"binary\": \"embed\",");
    println!("  \"mode\": \"populate_db\",");
    println!("  \"lines\": {},", lines.len());
    println!("  \"dim\": {dim},");
    println!("  \"batch_size\": {batch_size},");
    println!("  \"model_load_secs\": {model_load_secs:.3},");
    println!("  \"inference_secs\": {total_infer_secs:.3},");
    println!("  \"wall_secs\": {wall_secs:.3},");
    println!(
        "  \"sentences_per_sec\": {:.1},",
        lines.len() as f64 / total_infer_secs.max(1e-9)
    );
    println!("  \"db_bytes\": {db_size},");
    println!("  \"db_mib\": {:.2},", db_size as f64 / 1024.0 / 1024.0);
    println!("  \"stability_max_abs_diff\": {max_diff:e},");
    println!("  \"rss_end_mib\": {:.1}", rss_mib());
    println!("}}");
    Ok(())
}
