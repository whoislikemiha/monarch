//! File/folder path suggestions for the `@`-mention autocomplete (MON-76).
//!
//! One Tauri command — `list_paths` — takes the shadow's cwd and the text the
//! user has typed after `@`. Leading `../` segments shift the search anchor
//! upward; the remainder is a fuzzy-match needle over every path the walker
//! surfaces beneath that anchor.
//!
//! Walking uses the `ignore` crate so `.gitignore`, `.git/`, `node_modules/`,
//! and the usual hidden-dir skip list come for free. Scoring uses
//! `nucleo-matcher` (the matcher behind helix/zed-style fuzzy pickers).
//!
//! Returned paths are always relative to the *original* `cwd` — even when the
//! user climbs out with `../`. That keeps the inserted `@<path>` token
//! predictable no matter what the query shape was.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use ignore::WalkBuilder;
use nucleo_matcher::{
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
    Matcher,
};
use serde::Serialize;

use crate::error::MonarchError;

/// Hard cap on returned suggestions. The frontend renders a scrollable list,
/// but returning thousands of rows for an empty query serves nobody.
const MAX_RESULTS: usize = 150;
/// Hard cap on walked entries, regardless of matches. Protects the Tauri call
/// from pathological trees (e.g. `/` with gitignore disabled somewhere weird).
const MAX_WALKED: usize = 20_000;

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PathSuggestion {
    /// Path relative to the original `cwd`, always using forward slashes so
    /// the inserted token looks the same on every platform.
    pub path: String,
    /// True if the entry is a directory, false if it's a file.
    pub is_dir: bool,
}

/// Split the query into (anchor shift, remainder).
///
/// Every leading `../` chunk peels one level off the cwd. What's left is the
/// fuzzy-match needle. Mid-query `../` segments are **not** treated as
/// anchor shifts — only the prefix. Keeps things easy to reason about and
/// lines up with how shells resolve relative paths.
fn split_query(query: &str) -> (usize, &str) {
    let mut rest = query;
    let mut ups = 0;
    loop {
        if let Some(stripped) = rest.strip_prefix("../") {
            ups += 1;
            rest = stripped;
        } else if rest == ".." {
            ups += 1;
            rest = "";
            break;
        } else {
            break;
        }
    }
    (ups, rest)
}

/// Climb `ups` levels out of `start`. Returns the deepest ancestor we reach;
/// bottoms out at the filesystem root rather than erroring, so a user mashing
/// `../../../` doesn't get a hard failure.
fn anchor_for(start: &Path, ups: usize) -> PathBuf {
    let mut anchor = start.to_path_buf();
    for _ in 0..ups {
        if !anchor.pop() {
            break;
        }
    }
    anchor
}

/// Normalize a path for display: forward slashes, no leading `./`, `..`
/// components preserved. We deliberately do **not** canonicalize (would
/// resolve symlinks and strip `..`), since we want the inserted token to
/// match what the user expects based on the query they typed.
fn display_relative(anchor: &Path, entry: &Path, cwd: &Path, ups: usize) -> Option<String> {
    // `entry` is absolute; strip the anchor prefix to get the tail we walked
    // into. Then prepend `../` * ups so the path is relative to `cwd`.
    let tail = entry.strip_prefix(anchor).ok()?;
    let mut out = String::new();
    for _ in 0..ups {
        out.push_str("../");
    }
    let mut first = true;
    for component in tail.components() {
        match component {
            Component::Normal(s) => {
                if !first {
                    out.push('/');
                }
                out.push_str(&s.to_string_lossy());
                first = false;
            }
            // The walker emits normal components only — anything else
            // (prefix, root dir, curdir) would be weird here. Skip.
            _ => continue,
        }
    }
    // Empty tail means `entry == anchor`. We don't want to suggest the
    // anchor itself ("@" or "@../"), so drop it — but only if the query was
    // empty. A query matches nothing here anyway.
    let _ = cwd; // kept in signature for future symmetry / debugging
    if out.is_empty() {
        return None;
    }
    Some(out)
}

/// Walk `anchor` with ignore-crate defaults and collect up to `MAX_WALKED`
/// (path-relative-to-cwd, is_dir) tuples. The walker skips its own root, so
/// the caller doesn't need to filter it.
fn collect_entries(anchor: &Path, cwd: &Path, ups: usize) -> Vec<(String, bool)> {
    let mut out: Vec<(String, bool)> = Vec::with_capacity(1024);
    // Defaults: respect .gitignore, .ignore, global gitignore, skip hidden
    // files. Those defaults are exactly what we want for a code-oriented
    // mention list. Disabling hidden would flood the list with .DS_Store
    // and friends; leaving it on also hides the `.git` directory.
    let walker = WalkBuilder::new(anchor)
        .max_depth(None)
        .follow_links(false)
        .build();
    for result in walker {
        if out.len() >= MAX_WALKED {
            break;
        }
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };
        // Skip the anchor itself.
        if entry.depth() == 0 {
            continue;
        }
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let Some(rel) = display_relative(anchor, entry.path(), cwd, ups) else {
            continue;
        };
        out.push((rel, is_dir));
    }
    out
}

/// Score and sort `entries` against `needle`. Empty needle means "show
/// everything in breadth-first-ish order" — we sort alphabetically so the
/// list doesn't jitter.
fn rank(entries: Vec<(String, bool)>, needle: &str) -> Vec<PathSuggestion> {
    if needle.is_empty() {
        let mut sorted = entries;
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        return sorted
            .into_iter()
            .take(MAX_RESULTS)
            .map(|(path, is_dir)| PathSuggestion { path, is_dir })
            .collect();
    }

    let mut matcher = Matcher::new(nucleo_matcher::Config::DEFAULT.match_paths());
    let pattern = Pattern::new(
        needle,
        CaseMatching::Smart,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );
    let mut scored: Vec<(u32, String, bool)> = Vec::with_capacity(entries.len());
    for (path, is_dir) in entries {
        let haystack_str = path.clone();
        // nucleo's Pattern::score takes a Utf32Str; we build it per-entry.
        let mut buf = Vec::new();
        let haystack = nucleo_matcher::Utf32Str::new(&haystack_str, &mut buf);
        if let Some(score) = pattern.score(haystack, &mut matcher) {
            scored.push((score, path, is_dir));
        }
    }
    // Higher score first; alpha tiebreak keeps results stable under identical scores.
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    scored
        .into_iter()
        .take(MAX_RESULTS)
        .map(|(_, path, is_dir)| PathSuggestion { path, is_dir })
        .collect()
}

/// Core entry point shared by the Tauri and WS transports.
pub fn list_paths_inner(cwd: &str, query: &str) -> Result<Vec<PathSuggestion>, MonarchError> {
    let cwd_path = Path::new(cwd);
    if cwd.trim().is_empty() || !cwd_path.is_dir() {
        // No workspace → no suggestions. Frontend treats this as "feature disabled".
        return Ok(Vec::new());
    }
    let (ups, needle) = split_query(query);
    let anchor = anchor_for(cwd_path, ups);
    if !anchor.is_dir() {
        return Ok(Vec::new());
    }
    let entries = collect_entries(&anchor, cwd_path, ups);
    Ok(rank(entries, needle))
}

/// Tauri command wrapper. Walking is synchronous under the hood but the entry
/// count is bounded by `MAX_WALKED`, so running it on a blocking thread via
/// `spawn_blocking` keeps the async runtime unclogged on huge trees.
#[tauri::command]
#[specta::specta]
pub async fn list_paths(
    cwd: String,
    query: String,
) -> Result<Vec<PathSuggestion>, MonarchError> {
    let cwd = Arc::new(cwd);
    let query = Arc::new(query);
    let c = cwd.clone();
    let q = query.clone();
    tokio::task::spawn_blocking(move || list_paths_inner(&c, &q))
        .await
        .map_err(|e| MonarchError::persistence(format!("list_paths join error: {e}")))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_query_handles_leading_parents() {
        assert_eq!(split_query(""), (0, ""));
        assert_eq!(split_query("foo"), (0, "foo"));
        assert_eq!(split_query("../foo"), (1, "foo"));
        assert_eq!(split_query("../../bar"), (2, "bar"));
        assert_eq!(split_query(".."), (1, ""));
        // Only the prefix shifts the anchor.
        assert_eq!(split_query("foo/../bar"), (0, "foo/../bar"));
    }

    #[test]
    fn anchor_for_bottoms_out_at_root() {
        let p = Path::new("/home/u/proj");
        assert_eq!(anchor_for(p, 1), Path::new("/home/u"));
        assert_eq!(anchor_for(p, 2), Path::new("/home"));
        assert_eq!(anchor_for(p, 3), Path::new("/"));
        // Going past root is clamped, not errored.
        assert_eq!(anchor_for(p, 99), Path::new("/"));
    }
}
