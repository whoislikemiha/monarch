//! MON-99: In-process HNSW index + bge-small-en-v1.5 ONNX embedder.
//!
//! Validated by MON-91 spike (instant-distance + ort, recall@10 = 1.000 on
//! 10k real embeddings, p99 query latency 1.26 ms). The index is rebuilt
//! from DB embeddings on cold start and after each Keeper run. No background
//! rebuild worker (P3c) or incremental insert (P3d) — P2 volumes are small
//! enough for brute full-rebuild.
//!
//! Model files are lazy-downloaded to `~/.config/monarch/models/` on first
//! request. The ONNX Runtime itself is statically linked via ort's
//! `download-binaries` feature (+25 MiB binary); the model is +127 MiB on
//! first use.

use instant_distance::{Builder, HnswMap, Point, Search};
use ndarray::{s, Array2};
use ort::session::Session;
use ort::value::Value;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokenizers::Tokenizer;

use crate::error::MonarchError;

const MAX_SEQ_LEN: usize = 128;
const MODEL_FILENAME: &str = "bge-small-en-v1.5.onnx";
const TOKENIZER_FILENAME: &str = "bge-small-en-v1.5-tokenizer.json";
const MODEL_URL: &str =
    "https://huggingface.co/Xenova/bge-small-en-v1.5/resolve/main/onnx/model.onnx";
const TOKENIZER_URL: &str =
    "https://huggingface.co/Xenova/bge-small-en-v1.5/resolve/main/tokenizer.json";

/// Unit-normalised embedding vector. L2 distance on normalised vectors is
/// monotonic with cosine similarity, which is what the design calls for.
#[derive(Clone, Serialize, Deserialize)]
struct Embedding(Vec<f32>);

impl Point for Embedding {
    fn distance(&self, other: &Self) -> f32 {
        self.0
            .iter()
            .zip(other.0.iter())
            .map(|(a, b)| (a - b) * (a - b))
            .sum()
    }
}

struct Embedder {
    session: Session,
    tokenizer: Tokenizer,
}

struct IndexState {
    hnsw: HnswMap<Embedding, i64>,
}

pub struct MemoryIndex {
    embedder: Arc<Mutex<Option<Embedder>>>,
    index: Arc<Mutex<Option<IndexState>>>,
    models_dir: PathBuf,
}

impl MemoryIndex {
    pub fn new(models_dir: impl Into<PathBuf>) -> Self {
        Self {
            embedder: Arc::new(Mutex::new(None)),
            index: Arc::new(Mutex::new(None)),
            models_dir: models_dir.into(),
        }
    }

    pub fn model_path(&self) -> PathBuf {
        self.models_dir.join(MODEL_FILENAME)
    }

    pub fn tokenizer_path(&self) -> PathBuf {
        self.models_dir.join(TOKENIZER_FILENAME)
    }

    pub fn model_files_present(&self) -> bool {
        self.model_path().exists() && self.tokenizer_path().exists()
    }

    pub fn is_initialized(&self) -> bool {
        self.embedder.lock().is_some()
    }

    /// Download model + tokenizer from HuggingFace if not already present.
    pub async fn ensure_model_downloaded(&self) -> Result<(), MonarchError> {
        tokio::fs::create_dir_all(&self.models_dir)
            .await
            .map_err(MonarchError::from)?;
        if !self.model_path().exists() {
            download_file(MODEL_URL, &self.model_path()).await?;
        }
        if !self.tokenizer_path().exists() {
            download_file(TOKENIZER_URL, &self.tokenizer_path()).await?;
        }
        Ok(())
    }

    /// Load the ONNX session and tokenizer into memory. Idempotent.
    pub async fn init_embedder(&self) -> Result<(), MonarchError> {
        if self.embedder.lock().is_some() {
            return Ok(());
        }
        if !self.model_files_present() {
            return Err(MonarchError::persistence(
                "bge-small-en-v1.5 model files not found — download them first",
            ));
        }
        let model_path = self.model_path();
        let tok_path = self.tokenizer_path();
        let embedder = self.embedder.clone();
        tokio::task::spawn_blocking(move || {
            let session = Session::builder()
                .map_err(|e| MonarchError::persistence(e.to_string()))?
                .commit_from_file(&model_path)
                .map_err(|e| MonarchError::persistence(e.to_string()))?;
            let tokenizer = Tokenizer::from_file(&tok_path)
                .map_err(|e| MonarchError::persistence(e.to_string()))?;
            *embedder.lock() = Some(Embedder { session, tokenizer });
            Ok::<_, MonarchError>(())
        })
        .await
        .map_err(|e| MonarchError::persistence(e.to_string()))??;
        Ok(())
    }

    /// Embed a single text string. Returns a unit-normalised 384-dim vector.
    pub async fn embed_text(&self, text: &str) -> Result<Vec<f32>, MonarchError> {
        let text = text.to_string();
        let embedder = self.embedder.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = embedder.lock();
            let emb = guard
                .as_mut()
                .ok_or_else(|| MonarchError::persistence("embedder not initialised"))?;
            let mut vecs = embed_batch(&mut emb.session, &emb.tokenizer, &[text.as_str()])?;
            Ok(vecs.remove(0))
        })
        .await
        .map_err(|e| MonarchError::persistence(e.to_string()))?
    }

    /// Convert a text string to a raw f32 BLOB for storage.
    pub async fn embed_to_blob(&self, text: &str) -> Result<Vec<u8>, MonarchError> {
        let v = self.embed_text(text).await?;
        Ok(bytemuck::cast_slice::<f32, u8>(&v).to_vec())
    }

    /// Rebuild the in-process HNSW index from raw (memory_id, embedding_blob) pairs.
    /// Called at startup (if embedder is ready) and after each Keeper run.
    pub async fn rebuild(&self, data: Vec<(i64, Vec<u8>)>) -> Result<(), MonarchError> {
        let index = self.index.clone();
        tokio::task::spawn_blocking(move || {
            let mut points: Vec<Embedding> = Vec::with_capacity(data.len());
            let mut ids: Vec<i64> = Vec::with_capacity(data.len());
            for (id, blob) in data {
                if blob.len() % 4 != 0 {
                    continue;
                }
                let v: Vec<f32> = bytemuck::cast_slice::<u8, f32>(&blob).to_vec();
                points.push(Embedding(v));
                ids.push(id);
            }
            if points.is_empty() {
                *index.lock() = None;
                return Ok(());
            }
            let hnsw = Builder::default().build(points, ids);
            *index.lock() = Some(IndexState { hnsw });
            Ok::<_, MonarchError>(())
        })
        .await
        .map_err(|e| MonarchError::persistence(e.to_string()))?
    }

    /// Query the HNSW index with a text query. Returns top-k memory IDs.
    /// Returns empty vec if the index is not built yet.
    pub async fn query(&self, query_text: &str, k: usize) -> Result<Vec<i64>, MonarchError> {
        let v = match self.embed_text(query_text).await {
            Ok(v) => v,
            Err(_) => return Ok(vec![]),
        };
        let index = self.index.clone();
        tokio::task::spawn_blocking(move || {
            let guard = index.lock();
            let idx = match guard.as_ref() {
                Some(i) => i,
                None => return Ok(vec![]),
            };
            let point = Embedding(v);
            let mut search = Search::default();
            let hits: Vec<i64> = idx
                .hnsw
                .search(&point, &mut search)
                .take(k)
                .map(|item| *item.value)
                .collect();
            Ok(hits)
        })
        .await
        .map_err(|e| MonarchError::persistence(e.to_string()))?
    }
}

fn l2_normalize(v: &mut [f32]) {
    let norm_sq: f32 = v.iter().map(|x| x * x).sum();
    if norm_sq > 0.0 {
        let inv = 1.0 / norm_sq.sqrt();
        for x in v.iter_mut() {
            *x *= inv;
        }
    }
}

fn embed_batch(
    session: &mut Session,
    tokenizer: &Tokenizer,
    texts: &[&str],
) -> Result<Vec<Vec<f32>>, MonarchError> {
    let encodings = tokenizer
        .encode_batch(texts.to_vec(), true)
        .map_err(|e| MonarchError::persistence(format!("tokenizer encode: {e}")))?;

    let batch = encodings.len();
    let seq = encodings
        .iter()
        .map(|e| e.get_ids().len().min(MAX_SEQ_LEN))
        .max()
        .unwrap_or(1)
        .max(1);

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

    let iids =
        Value::from_array(input_ids).map_err(|e| MonarchError::persistence(e.to_string()))?;
    let amask =
        Value::from_array(attn_mask).map_err(|e| MonarchError::persistence(e.to_string()))?;
    let tids = Value::from_array(type_ids).map_err(|e| MonarchError::persistence(e.to_string()))?;

    let outputs = session
        .run(ort::inputs![
            "input_ids" => iids,
            "attention_mask" => amask,
            "token_type_ids" => tids
        ])
        .map_err(|e| MonarchError::persistence(e.to_string()))?;

    // bge-small-en-v1.5 returns last_hidden_state [batch, seq, 384]. CLS pooling.
    let (_, data) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| MonarchError::persistence(e.to_string()))?;
    let dim = data.len() / (batch * seq);
    let arr = ndarray::ArrayView3::from_shape((batch, seq, dim), data)
        .map_err(|e| MonarchError::persistence(e.to_string()))?;

    let mut embeddings = Vec::with_capacity(batch);
    for i in 0..batch {
        let mut v: Vec<f32> = arr.slice(s![i, 0, ..]).to_vec();
        l2_normalize(&mut v);
        embeddings.push(v);
    }
    Ok(embeddings)
}

async fn download_file(url: &str, dest: &Path) -> Result<(), MonarchError> {
    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| MonarchError::persistence(format!("download {url}: {e}")))?;
    if !resp.status().is_success() {
        return Err(MonarchError::persistence(format!(
            "download {url}: HTTP {}",
            resp.status()
        )));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| MonarchError::persistence(e.to_string()))?;
    tokio::fs::write(dest, bytes)
        .await
        .map_err(MonarchError::from)?;
    Ok(())
}

/// Tauri command: check whether the embedding model files are present.
#[tauri::command]
#[specta::specta]
pub async fn memory_index_status(
    index: tauri::State<'_, Arc<MemoryIndex>>,
) -> Result<bool, MonarchError> {
    Ok(index.is_initialized())
}

/// Tauri command: download model files + initialise the embedder.
/// Called from the Settings Memory tab.
#[tauri::command]
#[specta::specta]
pub async fn memory_download_and_init(
    index: tauri::State<'_, Arc<MemoryIndex>>,
) -> Result<(), MonarchError> {
    index.ensure_model_downloaded().await?;
    index.init_embedder().await
}
