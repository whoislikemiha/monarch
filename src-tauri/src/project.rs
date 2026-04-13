//! Project detection helpers shared between the `spawn_agent` path and the
//! `detect_project` / `read_project_instructions` commands. Prior to MON-33
//! these lived inline in `agent.rs` and were duplicated across the Tauri
//! command and its `ws_*` twin; both transports now funnel through the
//! free functions here.

use std::path::{Path, PathBuf};

use crate::db::{Database, ProjectRow};
use crate::error::MonarchError;
use crate::util::{chrono_now, uuid_v4_simple};

/// Walk up from `start` looking for a `.git` directory. Returns the directory
/// containing `.git`.
pub fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Read instruction files from a project root. Reads both AGENTS.md and
/// CLAUDE.md if present, concatenating them.
pub fn read_instructions_from_root(root: &Path) -> Option<String> {
    let mut parts = Vec::new();
    for name in &["AGENTS.md", "CLAUDE.md"] {
        let path = root.join(name);
        if path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                }
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

/// Detect project from cwd → find or create project row → return
/// `(project_id, instructions)`. DB instructions take precedence; on first
/// creation the DB row is populated from the on-disk instruction files.
pub async fn resolve_project(
    db: &Database,
    cwd: &str,
) -> Result<(Option<String>, Option<String>), MonarchError> {
    let cwd_path = Path::new(cwd);
    let root = match find_project_root(cwd_path) {
        Some(r) => r,
        None => return Ok((None, None)),
    };
    let root_str = root.to_string_lossy().to_string();

    let file_instructions = read_instructions_from_root(&root);

    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root_str.clone());
    let candidate_id = format!("project-{}", uuid_v4_simple());
    let now = chrono_now();
    let project_id = db
        .ensure_project_internal(&ProjectRow {
            id: candidate_id,
            name,
            root_path: root_str.clone(),
            instructions: file_instructions.clone(),
            created_at: now.clone(),
            updated_at: now,
        })
        .await?;

    let db_project = db.get_project_by_path_internal(&root_str).await?;
    let instructions = db_project
        .and_then(|p| p.instructions)
        .filter(|s| !s.trim().is_empty())
        .or(file_instructions);

    Ok((Some(project_id), instructions))
}

/// Lightweight project detection used by the UI's "did the user just open a
/// git repo" probe. Returns a JSON blob mirroring the historical shape
/// (`rootPath`, `name`, `projectId`, `hasInstructions`); typing this is
/// parked alongside the other `serde_json::Value`-emitting commands under
/// MON-14 Wave 2.
pub async fn detect_project(
    db: &Database,
    cwd: &str,
) -> Result<Option<serde_json::Value>, MonarchError> {
    let cwd_path = Path::new(cwd);
    let root = match find_project_root(cwd_path) {
        Some(r) => r,
        None => return Ok(None),
    };
    let root_str = root.to_string_lossy().to_string();
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root_str.clone());
    let existing = db.get_project_by_path_internal(&root_str).await?;
    let file_instructions = read_instructions_from_root(&root);
    Ok(Some(serde_json::json!({
        "rootPath": root_str,
        "name": existing.as_ref().map(|p| p.name.as_str()).unwrap_or(&name),
        "projectId": existing.as_ref().map(|p| p.id.as_str()),
        "hasInstructions": existing.as_ref()
            .and_then(|p| p.instructions.as_ref())
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
            || file_instructions.is_some(),
    })))
}

/// Return the concatenated on-disk instructions (AGENTS.md + CLAUDE.md) for
/// the project root above `cwd`, or `None` if the walk-up never finds a
/// `.git` directory.
pub fn read_project_instructions(cwd: &str) -> Option<String> {
    let cwd_path = Path::new(cwd);
    find_project_root(cwd_path).and_then(|root| read_instructions_from_root(&root))
}

