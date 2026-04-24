//! MON-91 bundle-probe (temporary; reverted before merge).
//!
//! References each MON-91 candidate dep so `npm run tauri build` cannot
//! tree-shake them out of the final binary. The function below is never
//! called; its sole job is to keep symbols live so bundle-size numbers
//! reflect the real cost of shipping the shadow-memory storage stack.
//!
//! Remove this module and the matching entries in Cargo.toml before merging
//! MON-91. See `thoughts/spike/MON-91-storage.md`.

#![allow(dead_code)]

use instant_distance::{Builder, HnswMap, Point};
use ndarray::Array2;
use ort::session::Session;
use serde::{Deserialize, Serialize};
use tokenizers::Tokenizer;

#[derive(Clone, Serialize, Deserialize)]
struct ProbePoint(Vec<f32>);

impl Point for ProbePoint {
    fn distance(&self, other: &Self) -> f32 {
        self.0
            .iter()
            .zip(other.0.iter())
            .map(|(a, b)| (a - b).powi(2))
            .sum()
    }
}

pub fn _force_link() {
    let pts = vec![ProbePoint(vec![0.0; 4]); 2];
    let ids = vec![0u32, 1];
    let hnsw: HnswMap<ProbePoint, u32> = Builder::default().build(pts, ids);
    let _bytes = bincode::serialize(&hnsw).ok();
    let _arr: Array2<f32> = Array2::zeros((1, 4));
    let _slice: &[u8] = bytemuck::cast_slice::<f32, u8>(&[0.0f32; 4]);
    let _session: Result<Session, _> = Session::builder().and_then(|mut b| b.commit_from_file("/dev/null"));
    let _tok: Result<Tokenizer, _> = Tokenizer::from_file("/dev/null");
}
