//! Small crate-root helpers used across modules.
//!
//! Lives here (instead of under `agent/`) because the consumers span modules
//! that have nothing to do with the agent loop — `db.rs` timestamps rows,
//! `project.rs` mints project ids, and every `agent/*` submodule still reads
//! both. Keeping them at the crate root removes a backwards dependency from
//! `db`/`project` into `agent`.

/// RFC3339 UTC timestamp with second precision, matching the schema
/// DEFAULT `strftime('%Y-%m-%dT%H:%M:%SZ','now')`. MON-39 item 4: before
/// the migration, Rust wrote Unix-seconds strings while SQLite DEFAULTs
/// wrote `datetime('now')` (space-separated, no timezone), and
/// `parse_timestamp` only handled the former. Now both sides produce the
/// same RFC3339 shape.
pub(crate) fn chrono_now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

pub(crate) fn uuid_v4_simple() -> String {
    uuid::Uuid::new_v4().to_string()
}
