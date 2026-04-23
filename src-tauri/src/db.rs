use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio_rusqlite::Connection;

use crate::error::MonarchError;

/// MON-27: backed by `tokio_rusqlite::Connection`, which owns a single
/// `rusqlite::Connection` on a dedicated background thread. Every method is
/// `async` and dispatches work via `conn.call(|c| { ... }).await`; the
/// closure body is plain synchronous `rusqlite` code, so migrations, queries,
/// and transactions are unchanged from the pre-MON-27 shape.
///
/// `Connection` is `Clone` (cheap — internally `Arc`-ed), so `Arc<Database>`
/// in Tauri state works as before and worker tasks can keep their own clone.
pub struct Database {
    conn: Connection,
}

fn db_path() -> PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("monarch");
    std::fs::create_dir_all(&dir).ok();
    dir.join("monarch.db")
}

impl Database {
    pub async fn new() -> Result<Self, MonarchError> {
        let path = db_path();
        let conn = Connection::open(&path).await?;

        // Enable WAL mode for better concurrent access
        conn.call(|c| {
            c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
            Ok(())
        })
        .await?;

        let db = Self { conn };
        db.init_schema().await?;
        db.migrate_timestamps_to_rfc3339().await?;
        Ok(db)
    }

    /// In-memory SQLite instance for unit tests. Not WAL (in-memory databases
    /// don't support it) and schema is initialised the same way as the
    /// on-disk constructor.
    #[cfg(test)]
    pub async fn new_in_memory() -> Result<Self, MonarchError> {
        let conn = Connection::open_in_memory().await?;
        conn.call(|c| {
            c.execute_batch("PRAGMA foreign_keys=ON;")?;
            Ok(())
        })
        .await?;
        let db = Self { conn };
        db.init_schema().await?;
        db.migrate_timestamps_to_rfc3339().await?;
        Ok(db)
    }

    /// MON-39 item 4: one-shot conversion of pre-existing timestamp rows to
    /// the canonical `%Y-%m-%dT%H:%M:%SZ` RFC3339 shape.
    ///
    /// Before this migration the codebase wrote two incompatible formats to
    /// the same TEXT columns:
    /// * Rust's `chrono_now()` wrote Unix seconds as a numeric string
    /// * SQLite DEFAULT `datetime('now')` wrote `YYYY-MM-DD HH:MM:SS` (space
    ///   separator, no timezone)
    ///
    /// `parse_timestamp` in `agent_state.rs` only accepted the former, so
    /// sessions created by DEFAULT restored with `timestamp: None`. This
    /// migration normalises both shapes to RFC3339 per column. It is
    /// idempotent: already-RFC3339 rows match neither WHERE clause and are
    /// skipped on re-run.
    async fn migrate_timestamps_to_rfc3339(&self) -> Result<(), MonarchError> {
        self.conn
            .call(|conn| {
                let cols: &[(&str, &str)] = &[
                    ("projects", "created_at"),
                    ("projects", "updated_at"),
                    ("agents", "created_at"),
                    ("agents", "updated_at"),
                    ("sessions", "started_at"),
                    ("sessions", "ended_at"),
                    ("messages", "timestamp"),
                    ("memories", "created_at"),
                    ("memories", "last_accessed"),
                    ("events", "timestamp"),
                    ("agent_templates", "created_at"),
                    ("agent_templates", "updated_at"),
                ];
                let tx = conn.unchecked_transaction()?;
                for (tbl, col) in cols {
                    tx.execute(
                        &format!(
                            "UPDATE {t} SET {c} = strftime('%Y-%m-%dT%H:%M:%SZ', {c}, 'unixepoch') \
                             WHERE {c} IS NOT NULL AND {c} GLOB '[0-9]*' AND {c} NOT GLOB '*-*'",
                            t = tbl,
                            c = col
                        ),
                        [],
                    )?;
                    tx.execute(
                        &format!(
                            "UPDATE {t} SET {c} = strftime('%Y-%m-%dT%H:%M:%SZ', {c}) \
                             WHERE {c} IS NOT NULL AND {c} GLOB '*-*-* *:*:*' AND {c} NOT GLOB '*T*'",
                            t = tbl,
                            c = col
                        ),
                        [],
                    )?;
                }
                tx.commit()?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    async fn init_schema(&self) -> Result<(), MonarchError> {
        self.conn
            .call(|conn| {
                conn.execute_batch(
                    "
                    CREATE TABLE IF NOT EXISTS projects (
                        id TEXT PRIMARY KEY,
                        name TEXT NOT NULL,
                        root_path TEXT NOT NULL UNIQUE,
                        instructions TEXT,
                        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
                        updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
                    );

                    CREATE TABLE IF NOT EXISTS agents (
                        id TEXT PRIMARY KEY,
                        name TEXT NOT NULL,
                        project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
                        shadow_name TEXT,
                        shadow_title TEXT,
                        shadow_grade TEXT,
                        provider TEXT,
                        model TEXT,
                        thinking_level TEXT,
                        cwd TEXT,
                        custom_prompt TEXT,
                        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
                        updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
                    );

                    CREATE TABLE IF NOT EXISTS sessions (
                        id TEXT PRIMARY KEY,
                        agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
                        pi_session_file TEXT,
                        model TEXT,
                        provider TEXT,
                        started_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
                        ended_at TEXT,
                        message_count INTEGER DEFAULT 0,
                        total_tokens INTEGER DEFAULT 0,
                        total_cost REAL DEFAULT 0.0
                    );

                    CREATE TABLE IF NOT EXISTS messages (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                        role TEXT NOT NULL,
                        content TEXT NOT NULL,
                        model TEXT,
                        tokens INTEGER DEFAULT 0,
                        cost REAL DEFAULT 0.0,
                        timestamp TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
                    );

                    CREATE TABLE IF NOT EXISTS memories (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        agent_id TEXT REFERENCES agents(id) ON DELETE CASCADE,
                        layer TEXT NOT NULL DEFAULT 'hot',
                        category TEXT NOT NULL DEFAULT 'general',
                        content TEXT NOT NULL,
                        relevance REAL DEFAULT 1.0,
                        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
                        last_accessed TEXT,
                        access_count INTEGER DEFAULT 0
                    );

                    CREATE TABLE IF NOT EXISTS events (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        agent_id TEXT,
                        session_id TEXT,
                        event_type TEXT NOT NULL,
                        data TEXT,
                        timestamp TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
                    );

                    CREATE TABLE IF NOT EXISTS agent_templates (
                        id TEXT PRIMARY KEY,
                        name TEXT NOT NULL,
                        provider TEXT,
                        model TEXT,
                        thinking_level TEXT,
                        cwd TEXT,
                        shadow_name TEXT,
                        shadow_title TEXT,
                        shadow_grade TEXT,
                        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
                        updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
                    );

                    CREATE INDEX IF NOT EXISTS idx_sessions_agent ON sessions(agent_id);
                    CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);
                    CREATE INDEX IF NOT EXISTS idx_memories_agent ON memories(agent_id);
                    CREATE INDEX IF NOT EXISTS idx_memories_layer ON memories(layer);
                    CREATE INDEX IF NOT EXISTS idx_events_agent ON events(agent_id);
                    CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
                    CREATE INDEX IF NOT EXISTS idx_events_agent_session ON events(agent_id, session_id);
                    CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);

                    CREATE TABLE IF NOT EXISTS agent_stats (
                        agent_id TEXT PRIMARY KEY REFERENCES agents(id) ON DELETE CASCADE,
                        total_sessions INTEGER NOT NULL DEFAULT 0,
                        total_messages INTEGER NOT NULL DEFAULT 0,
                        total_turns INTEGER NOT NULL DEFAULT 0,
                        total_input_tokens INTEGER NOT NULL DEFAULT 0,
                        total_output_tokens INTEGER NOT NULL DEFAULT 0,
                        total_cost REAL NOT NULL DEFAULT 0.0,
                        updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
                    );

                    CREATE TABLE IF NOT EXISTS agent_tool_usage (
                        agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
                        tool_name TEXT NOT NULL,
                        call_count INTEGER NOT NULL DEFAULT 0,
                        error_count INTEGER NOT NULL DEFAULT 0,
                        PRIMARY KEY (agent_id, tool_name)
                    );
                    ",
                )?;

                // Migrations: ignore errors if columns/tables already exist
                let _ = conn.execute_batch(
                    "ALTER TABLE sessions ADD COLUMN parent_session_id TEXT REFERENCES sessions(id);",
                );
                let _ = conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS projects (
                        id TEXT PRIMARY KEY,
                        name TEXT NOT NULL,
                        root_path TEXT NOT NULL UNIQUE,
                        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
                        updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
                    );",
                );
                let _ = conn.execute_batch(
                    "ALTER TABLE agents ADD COLUMN project_id TEXT REFERENCES projects(id) ON DELETE SET NULL;",
                );
                let _ = conn.execute_batch(
                    "CREATE INDEX IF NOT EXISTS idx_agents_project ON agents(project_id);",
                );
                let _ = conn.execute_batch(
                    "ALTER TABLE projects ADD COLUMN instructions TEXT;",
                );
                let _ = conn.execute_batch(
                    "ALTER TABLE agents ADD COLUMN context_window INTEGER;",
                );

                let _ = conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS ui_state (
                        key TEXT PRIMARY KEY,
                        value TEXT NOT NULL
                    );",
                );

                // MON-66: archive lifecycle for shadows. NULL = active.
                let _ = conn.execute_batch(
                    "ALTER TABLE agents ADD COLUMN archived_at TEXT;",
                );

                // MON-71: per-turn wall-clock duration on assistant messages.
                // Nullable — old rows stay NULL (no backfill); pre-MON-71
                // assistant messages simply render without a duration chip.
                let _ = conn.execute_batch(
                    "ALTER TABLE messages ADD COLUMN duration_ms INTEGER;",
                );

                // MON-73: agent avatar type ("rive" | "image") and path.
                let _ = conn.execute_batch("ALTER TABLE agents ADD COLUMN avatar_type TEXT;");
                let _ = conn.execute_batch("ALTER TABLE agents ADD COLUMN avatar_path TEXT;");

                // MON-75: per-message image attachments. Bytes live under
                // ~/.config/monarch/attachments/{uuid}.{ext}; this table
                // just keeps an ordered reference so rebuilt snapshots and
                // session replays can find them. `position` is the ordinal
                // within a single user message (0-based).
                let _ = conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS message_attachments (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
                        path TEXT NOT NULL,
                        mime_type TEXT NOT NULL,
                        position INTEGER NOT NULL DEFAULT 0
                    );
                    CREATE INDEX IF NOT EXISTS idx_message_attachments_message
                        ON message_attachments(message_id);",
                );

                // MON-49: the events table is forensic, not operational.
                // Prune rows older than 30 days on startup so the table does
                // not grow unbounded. Errors are swallowed — a failed prune
                // must not block app boot.
                let _ = conn.execute(
                    "DELETE FROM events WHERE timestamp < datetime('now', '-30 days')",
                    [],
                );

                // MON-83: Quest system Slice 2 — fractal unit of work.
                // Design: plans/quests.md. Quests are orthogonal to sessions —
                // a quest can span sessions, a session can span quests.
                // CHECK constraints pin the finite enums (status/grade/
                // exec_hint/created_by) at the storage layer; Rust mirrors
                // the same values in quest::types.
                let _ = conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS quest_nodes (
                        id TEXT PRIMARY KEY,
                        root_id TEXT NOT NULL,
                        parent_id TEXT REFERENCES quest_nodes(id) ON DELETE CASCADE,
                        title TEXT NOT NULL,
                        description TEXT,
                        status TEXT NOT NULL CHECK (status IN (
                            'pending','in_progress','claimed_done',
                            'verified','disputed','ambiguous',
                            'done','abandoned','superseded'
                        )),
                        grade TEXT CHECK (grade IN ('E','D','C','B','A','S')),
                        exec_hint TEXT CHECK (exec_hint IN ('in_context','delegate','explore')),
                        explore_fork_count INTEGER,
                        assignee_shadow_id TEXT REFERENCES agents(id) ON DELETE SET NULL,
                        worktree_path TEXT,
                        branch_name TEXT,
                        base_branch TEXT,
                        branched_from_id TEXT REFERENCES quest_nodes(id) ON DELETE SET NULL,
                        superseded_by_id TEXT REFERENCES quest_nodes(id) ON DELETE SET NULL,
                        created_by TEXT NOT NULL CHECK (created_by IN (
                            'architect','steward','orchestrator','monarch'
                        )),
                        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
                        started_at TEXT,
                        completed_at TEXT,
                        abandoned_at TEXT,
                        estimated_tokens INTEGER,
                        actual_tokens INTEGER,
                        estimated_duration_ms INTEGER,
                        actual_duration_ms INTEGER,
                        summary TEXT
                    );
                    CREATE INDEX IF NOT EXISTS idx_quest_nodes_root ON quest_nodes(root_id);
                    CREATE INDEX IF NOT EXISTS idx_quest_nodes_parent ON quest_nodes(parent_id);
                    CREATE INDEX IF NOT EXISTS idx_quest_nodes_assignee_status
                        ON quest_nodes(assignee_shadow_id, status);
                    CREATE INDEX IF NOT EXISTS idx_quest_nodes_created_at
                        ON quest_nodes(created_at);",
                );
                let _ = conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS quest_events (
                        id TEXT PRIMARY KEY,
                        quest_id TEXT NOT NULL REFERENCES quest_nodes(id) ON DELETE CASCADE,
                        event_type TEXT NOT NULL,
                        actor TEXT,
                        payload_json TEXT,
                        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
                    );
                    CREATE INDEX IF NOT EXISTS idx_quest_events_quest
                        ON quest_events(quest_id, created_at);",
                );
                // messages.quest_id: nullable FK. Slice 2 leaves this NULL
                // everywhere; Slice 3 (Architect) is the first writer.
                let _ = conn.execute_batch(
                    "ALTER TABLE messages ADD COLUMN quest_id TEXT REFERENCES quest_nodes(id) ON DELETE SET NULL;",
                );
                let _ = conn.execute_batch(
                    "CREATE INDEX IF NOT EXISTS idx_messages_quest ON messages(quest_id);",
                );
                // agents.current_quest_id: nullable pointer into the tree.
                // Slice 2 adds the column; Slice 3+ populate it.
                let _ = conn.execute_batch(
                    "ALTER TABLE agents ADD COLUMN current_quest_id TEXT REFERENCES quest_nodes(id) ON DELETE SET NULL;",
                );

                // MON-82: Quest system Slice 1 — per-turn prompt classifier.
                // Design: plans/quests.md. Every user turn is tagged with a
                // complexity label before/in-parallel-with the Pi session.
                // `message_id` is nullable because the classifier emits before
                // the user message row exists (MessageEnd from Pi is the
                // source of truth for user-message persistence); it is
                // backfilled when that row lands. `complexity` is nullable to
                // allow an error row (error column populated, complexity
                // null) so calibration still sees failures.
                let _ = conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS classifications (
                        id TEXT PRIMARY KEY,
                        message_id INTEGER REFERENCES messages(id) ON DELETE CASCADE,
                        agent_id TEXT NOT NULL,
                        session_id TEXT,
                        complexity TEXT CHECK (complexity IN (
                            'chitchat','simple','decomposable','delegate'
                        )),
                        confidence REAL,
                        rationale TEXT,
                        model TEXT,
                        tokens_in INTEGER,
                        tokens_out INTEGER,
                        latency_ms INTEGER,
                        error TEXT,
                        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
                    );
                    CREATE INDEX IF NOT EXISTS idx_classifications_agent_created
                        ON classifications(agent_id, created_at);
                    CREATE INDEX IF NOT EXISTS idx_classifications_message
                        ON classifications(message_id);",
                );

                Ok(())
            })
            .await?;
        Ok(())
    }
}

// ---- Data types ----

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRow {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub instructions: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentRow {
    pub id: String,
    pub name: String,
    pub project_id: Option<String>,
    pub shadow_name: Option<String>,
    pub shadow_title: Option<String>,
    pub shadow_grade: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking_level: Option<String>,
    pub cwd: Option<String>,
    pub custom_prompt: Option<String>,
    /// User-supplied context window (tokens). Currently only used for lmstudio.
    pub context_window: Option<i32>,
    pub created_at: String,
    pub updated_at: String,
    /// MON-66: ISO timestamp when the agent was archived, or None if active.
    /// Archive preserves the DB row (history, sessions, stats) but removes
    /// the shadow from the default active roster. See `archive_agent_internal`.
    pub archived_at: Option<String>,
    /// MON-73: "rive" | "image" | null (null = default rive preset).
    pub avatar_type: Option<String>,
    /// MON-73: For "rive": path to .riv file (null = default). For "image":
    /// built-in web path ("/avatars/foo.png") or absolute filesystem path.
    pub avatar_path: Option<String>,
}

/// MON-73: Payload for updating editable agent fields post-creation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentUpdatePayload {
    pub id: String,
    pub name: String,
    pub shadow_name: Option<String>,
    pub shadow_title: Option<String>,
    pub shadow_grade: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking_level: Option<String>,
    pub cwd: Option<String>,
    pub avatar_type: Option<String>,
    pub avatar_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionRow {
    pub id: String,
    pub agent_id: String,
    // Deprecated compatibility field kept in the schema for older databases.
    pub pi_session_file: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub message_count: i32,
    pub total_tokens: i32,
    pub total_cost: f64,
    pub parent_session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MessageRow {
    pub id: i64,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub model: Option<String>,
    pub tokens: i32,
    pub cost: f64,
    pub timestamp: String,
    /// MON-71: wall-clock duration of the assistant turn that produced this
    /// message, in milliseconds. NULL for rows written before MON-71, and for
    /// roles other than `assistant` (user and toolResult carry their own
    /// timing — tool durations live inside the toolResult JSON blob).
    #[serde(default)]
    pub duration_ms: Option<i64>,
    /// MON-75: image attachments persisted alongside user messages. Empty
    /// for assistant/tool rows and for user messages without images.
    /// Populated by `get_messages_with_ancestry`; ignored by writers
    /// (`save_message_internal` never inserts attachments — that goes
    /// through `save_message_attachment_internal`).
    #[serde(default)]
    pub attachments: Vec<MessageAttachmentRow>,
}

/// MON-75: one row in `message_attachments`. Exposed to the frontend so
/// the display snapshot can surface attachment paths to MessageList for
/// rendering; the webview resolves each path through the
/// `read_attachment_data_url` command.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MessageAttachmentRow {
    pub path: String,
    pub mime_type: String,
    pub position: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentTemplateRow {
    pub id: String,
    pub name: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking_level: Option<String>,
    pub cwd: Option<String>,
    pub shadow_name: Option<String>,
    pub shadow_title: Option<String>,
    pub shadow_grade: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRow {
    pub id: i64,
    pub agent_id: Option<String>,
    pub layer: String,
    pub category: String,
    pub content: String,
    pub relevance: f64,
    pub created_at: String,
    pub last_accessed: Option<String>,
    pub access_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolUsageEntry {
    pub tool_name: String,
    pub call_count: i32,
    pub error_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SpecializationScores {
    pub coding: f64,
    pub research: f64,
    pub testing: f64,
    pub debugging: f64,
    pub devops: f64,
    pub documentation: f64,
    pub database: f64,
    pub configuration: f64,
    pub design: f64,
    pub communication: f64,
    pub refactoring: f64,
    pub security: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentStats {
    pub agent_id: String,
    pub total_sessions: i32,
    pub total_messages: i32,
    pub total_turns: i32,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cost: f64,
    /// Normalized experience level 0-100, derived from total tokens (log scale).
    pub experience: f64,
    pub tool_usage: Vec<ToolUsageEntry>,
    pub specialization: SpecializationScores,
    pub updated_at: String,
}

// MON-83: Quest system row types. Enums (status/grade/exec_hint/created_by)
// are stored as strings matching the CHECK constraints in the schema. See
// plans/quests.md for the full design.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct QuestRow {
    pub id: String,
    pub root_id: String,
    pub parent_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub grade: Option<String>,
    pub exec_hint: Option<String>,
    pub explore_fork_count: Option<i32>,
    pub assignee_shadow_id: Option<String>,
    pub worktree_path: Option<String>,
    pub branch_name: Option<String>,
    pub base_branch: Option<String>,
    pub branched_from_id: Option<String>,
    pub superseded_by_id: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub abandoned_at: Option<String>,
    pub estimated_tokens: Option<i32>,
    pub actual_tokens: Option<i32>,
    pub estimated_duration_ms: Option<i64>,
    pub actual_duration_ms: Option<i64>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct QuestEventRow {
    pub id: String,
    pub quest_id: String,
    pub event_type: String,
    pub actor: Option<String>,
    pub payload_json: Option<String>,
    pub created_at: String,
}

/// Payload for `db_create_quest`. `id` is optional — server generates a
/// UUID if omitted. Defaults: `status='pending'`, `grade='C'`,
/// `exec_hint='in_context'`, `created_by='monarch'`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateQuestPayload {
    pub id: Option<String>,
    pub parent_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub status: Option<String>,
    pub grade: Option<String>,
    pub exec_hint: Option<String>,
    pub assignee_shadow_id: Option<String>,
    pub created_by: Option<String>,
}

/// Payload for `db_update_quest`. Only non-`None` fields are written.
/// Lifecycle timestamps (`started_at` / `completed_at` / `abandoned_at`)
/// can be set explicitly by the caller; the Steward owns this in Slice 4+.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateQuestPayload {
    pub id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub grade: Option<String>,
    pub exec_hint: Option<String>,
    pub assignee_shadow_id: Option<String>,
    pub summary: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub abandoned_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RecordQuestEventPayload {
    pub quest_id: String,
    pub event_type: String,
    pub actor: Option<String>,
    pub payload_json: Option<String>,
}

// ---- MON-82: Classifications ----

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClassificationRow {
    pub id: String,
    pub message_id: Option<i64>,
    pub agent_id: String,
    pub session_id: Option<String>,
    pub complexity: Option<String>,
    pub confidence: Option<f64>,
    pub rationale: Option<String>,
    pub model: Option<String>,
    pub tokens_in: Option<i32>,
    pub tokens_out: Option<i32>,
    pub latency_ms: Option<i32>,
    pub error: Option<String>,
    pub created_at: String,
}

/// Payload written when the sidecar emits a classification event. The
/// sidecar mints `id` so the frontend can reconcile a pending pill with the
/// resolved row (and so backfill can link by exact id, not "latest
/// unlinked"). `message_id` is always `None` at insert time — the user
/// message row doesn't exist yet; `backfill_classification_message_id`
/// fills it when Pi emits user `MessageEnd`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveClassificationPayload {
    pub id: String,
    pub agent_id: String,
    pub session_id: Option<String>,
    pub complexity: Option<String>,
    pub confidence: Option<f64>,
    pub rationale: Option<String>,
    pub model: Option<String>,
    pub tokens_in: Option<i32>,
    pub tokens_out: Option<i32>,
    pub latency_ms: Option<i32>,
    pub error: Option<String>,
}

// ---- Persistence API ----
//
// MON-27: every method is `async`; the body moves into a synchronous closure
// passed to `Connection::call`, which runs it on the connection's dedicated
// background thread. Callers `.await` the result. Captures inside each
// closure must be `Send + 'static`, so borrowed `&str` / `&[T]` arguments are
// cloned up-front into owned values.

impl Database {
    /// Insert a project if the root_path doesn't already exist, then return the winning row's id.
    /// Safe under concurrent inserts: losers get the existing row's id back.
    pub async fn ensure_project_internal(&self, project: &ProjectRow) -> Result<String, MonarchError> {
        let project = project.clone();
        Ok(self
            .conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO projects (id, name, root_path, instructions, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(root_path) DO UPDATE SET updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')",
                    params![project.id, project.name, project.root_path, project.instructions, project.created_at, project.updated_at],
                )?;
                let id: String = conn.query_row(
                    "SELECT id FROM projects WHERE root_path = ?1",
                    params![project.root_path],
                    |row| row.get(0),
                )?;
                Ok(id)
            })
            .await?)
    }

    pub async fn get_project_by_path_internal(
        &self,
        root_path: &str,
    ) -> Result<Option<ProjectRow>, MonarchError> {
        let root_path = root_path.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let result = conn.query_row(
                    "SELECT id, name, root_path, instructions, created_at, updated_at FROM projects WHERE root_path = ?1",
                    params![root_path],
                    map_project,
                );
                match result {
                    Ok(row) => Ok(Some(row)),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                    Err(e) => Err(e),
                }
            })
            .await?)
    }

    pub async fn session_exists_internal(&self, session_id: &str) -> Result<bool, MonarchError> {
        let session_id = session_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let exists: i64 = conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM sessions WHERE id = ?1)",
                    params![session_id],
                    |row| row.get(0),
                )?;
                Ok(exists != 0)
            })
            .await?)
    }

    pub async fn ensure_agent_exists_internal(&self, agent: &AgentRow) -> Result<(), MonarchError> {
        let agent = agent.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO agents (id, name, project_id, shadow_name, shadow_title, shadow_grade, provider, model, thinking_level, cwd, custom_prompt, context_window, avatar_type, avatar_path, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
                     ON CONFLICT(id) DO NOTHING",
                    params![
                        agent.id, agent.name, agent.project_id, agent.shadow_name, agent.shadow_title,
                        agent.shadow_grade, agent.provider, agent.model, agent.thinking_level,
                        agent.cwd, agent.custom_prompt, agent.context_window, agent.avatar_type, agent.avatar_path,
                        agent.created_at, agent.updated_at,
                    ],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn get_agent_context_window_internal(
        &self,
        agent_id: &str,
    ) -> Result<Option<i32>, MonarchError> {
        let agent_id = agent_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let result: Option<i32> = conn
                    .query_row(
                        "SELECT context_window FROM agents WHERE id = ?1",
                        params![agent_id],
                        |row| row.get(0),
                    )
                    .ok()
                    .flatten();
                Ok(result)
            })
            .await?)
    }

    pub async fn upsert_agent_internal(&self, agent: &AgentRow) -> Result<(), MonarchError> {
        let agent = agent.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO agents (id, name, project_id, shadow_name, shadow_title, shadow_grade, provider, model, thinking_level, cwd, custom_prompt, context_window, avatar_type, avatar_path, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
                     ON CONFLICT(id) DO UPDATE SET
                       name=excluded.name, project_id=excluded.project_id,
                       shadow_name=excluded.shadow_name, shadow_title=excluded.shadow_title,
                       shadow_grade=excluded.shadow_grade, provider=excluded.provider, model=excluded.model,
                       thinking_level=excluded.thinking_level, cwd=excluded.cwd, custom_prompt=excluded.custom_prompt,
                       context_window=excluded.context_window, avatar_type=excluded.avatar_type,
                       avatar_path=excluded.avatar_path,
                       updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')",
                    params![
                        agent.id, agent.name, agent.project_id, agent.shadow_name, agent.shadow_title,
                        agent.shadow_grade, agent.provider, agent.model, agent.thinking_level,
                        agent.cwd, agent.custom_prompt, agent.context_window, agent.avatar_type, agent.avatar_path,
                        agent.created_at, agent.updated_at,
                    ],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    /// MON-73: Update all user-editable agent fields post-creation.
    pub async fn update_agent_internal(&self, payload: &AgentUpdatePayload) -> Result<(), MonarchError> {
        let payload = payload.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE agents SET
                       name=?2, shadow_name=?3, shadow_title=?4, shadow_grade=?5,
                       provider=?6, model=?7, thinking_level=?8, cwd=?9,
                       avatar_type=?10, avatar_path=?11,
                       updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')
                     WHERE id=?1",
                    params![
                        payload.id, payload.name, payload.shadow_name, payload.shadow_title,
                        payload.shadow_grade, payload.provider, payload.model, payload.thinking_level,
                        payload.cwd, payload.avatar_type, payload.avatar_path,
                    ],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn save_message_internal(&self, message: &MessageRow) -> Result<i64, MonarchError> {
        let message = message.clone();
        Ok(self
            .conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO messages (session_id, role, content, model, tokens, cost, timestamp, duration_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        message.session_id, message.role, message.content,
                        message.model, message.tokens, message.cost, message.timestamp,
                        message.duration_ms,
                    ],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await?)
    }

    /// MON-75: link one persisted image blob to its parent user message.
    /// `path` is the absolute filesystem path returned by
    /// `persistence::write_attachment_bytes`; `position` is the ordinal
    /// within the message so order matches what the user sent.
    pub async fn save_message_attachment_internal(
        &self,
        message_id: i64,
        path: &str,
        mime_type: &str,
        position: i64,
    ) -> Result<(), MonarchError> {
        let path = path.to_string();
        let mime_type = mime_type.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO message_attachments (message_id, path, mime_type, position)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![message_id, path, mime_type, position],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn update_session_internal(
        &self,
        session_id: &str,
        message_count: Option<i32>,
        total_tokens: Option<i32>,
        total_cost: Option<f64>,
        ended_at: Option<&str>,
    ) -> Result<(), MonarchError> {
        let session_id = session_id.to_string();
        let ended_at = ended_at.map(|s| s.to_string());
        self.conn
            .call(move |conn| {
                if let Some(mc) = message_count {
                    conn.execute(
                        "UPDATE sessions SET message_count = ?1 WHERE id = ?2",
                        params![mc, session_id],
                    )?;
                }
                if let Some(tt) = total_tokens {
                    conn.execute(
                        "UPDATE sessions SET total_tokens = ?1 WHERE id = ?2",
                        params![tt, session_id],
                    )?;
                }
                if let Some(tc) = total_cost {
                    conn.execute(
                        "UPDATE sessions SET total_cost = ?1 WHERE id = ?2",
                        params![tc, session_id],
                    )?;
                }
                if let Some(ea) = ended_at.as_deref() {
                    conn.execute(
                        "UPDATE sessions SET ended_at = ?1 WHERE id = ?2",
                        params![ea, session_id],
                    )?;
                }
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn log_event_internal(
        &self,
        agent_id: Option<&str>,
        session_id: Option<&str>,
        event_type: &str,
        data: Option<&str>,
    ) -> Result<(), MonarchError> {
        let agent_id = agent_id.map(|s| s.to_string());
        let session_id = session_id.map(|s| s.to_string());
        let event_type = event_type.to_string();
        let data = data.map(|s| s.to_string());
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO events (agent_id, session_id, event_type, data, timestamp) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![agent_id, session_id, event_type, data, crate::util::chrono_now()],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    /// Atomically increment message_count, total_tokens, and total_cost for a session
    pub async fn increment_session_message_count(
        &self,
        session_id: &str,
        tokens: i32,
        cost: f64,
    ) -> Result<(), MonarchError> {
        let session_id = session_id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE sessions SET message_count = message_count + 1, total_tokens = total_tokens + ?1, total_cost = total_cost + ?2 WHERE id = ?3",
                    params![tokens, cost, session_id],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn create_session_internal(&self, session: &SessionRow) -> Result<(), MonarchError> {
        let session = session.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO sessions (id, agent_id, pi_session_file, model, provider, started_at, ended_at, message_count, total_tokens, total_cost, parent_session_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                     ON CONFLICT(id) DO UPDATE SET
                       agent_id=excluded.agent_id,
                       model=excluded.model,
                       provider=excluded.provider,
                       ended_at=excluded.ended_at,
                       message_count=excluded.message_count,
                       total_tokens=excluded.total_tokens,
                       total_cost=excluded.total_cost,
                       parent_session_id=excluded.parent_session_id",
                    params![
                        session.id, session.agent_id, session.pi_session_file, session.model,
                        session.provider, session.started_at, session.ended_at,
                        session.message_count, session.total_tokens, session.total_cost,
                        session.parent_session_id,
                    ],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    /// Get the full message chain for a session, following parent_session_id links.
    /// Returns messages from oldest ancestor to current, in chronological order.
    pub async fn get_messages_with_ancestry(
        &self,
        session_id: &str,
    ) -> Result<Vec<MessageRow>, MonarchError> {
        let session_id = session_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                // Walk the parent chain to collect all session IDs (oldest first)
                let mut chain: Vec<String> = vec![session_id.clone()];
                let mut current = session_id;
                loop {
                    let parent: Option<String> = conn
                        .query_row(
                            "SELECT parent_session_id FROM sessions WHERE id = ?1",
                            params![current],
                            |row| row.get(0),
                        )
                        .ok()
                        .flatten();
                    match parent {
                        Some(pid) => {
                            chain.push(pid.clone());
                            current = pid;
                        }
                        None => break,
                    }
                }
                chain.reverse(); // oldest first

                let mut all_messages: Vec<MessageRow> = Vec::new();
                for sid in &chain {
                    let mut stmt = conn.prepare(
                        "SELECT id, session_id, role, content, model, tokens, cost, timestamp, duration_ms FROM messages WHERE session_id = ?1 ORDER BY id ASC",
                    )?;
                    let rows = stmt.query_map(params![sid], map_message)?;
                    for row in rows {
                        all_messages.push(row?);
                    }
                }

                // MON-75: hydrate attachments per message in one pass. Only
                // user rows are expected to have any, so scoping the query
                // to `role = 'user'` keeps it cheap; positions come back
                // sorted so the UI renders them in send order.
                let mut att_stmt = conn.prepare(
                    "SELECT path, mime_type, position
                     FROM message_attachments
                     WHERE message_id = ?1
                     ORDER BY position ASC",
                )?;
                for msg in all_messages.iter_mut() {
                    if msg.role != "user" {
                        continue;
                    }
                    let rows = att_stmt.query_map(params![msg.id], |row| {
                        Ok(MessageAttachmentRow {
                            path: row.get(0)?,
                            mime_type: row.get(1)?,
                            position: row.get(2)?,
                        })
                    })?;
                    for att in rows {
                        msg.attachments.push(att?);
                    }
                }

                Ok(all_messages)
            })
            .await?)
    }

    pub async fn get_agents_internal(
        &self,
        include_archived: bool,
    ) -> Result<Vec<AgentRow>, MonarchError> {
        // Active rows first (archived_at IS NULL), then archived ones ordered by
        // most-recently-archived. Within each group, fall back to updated_at DESC
        // so the default view matches prior behavior.
        Ok(self
            .conn
            .call(move |conn| {
                let sql = if include_archived {
                    "SELECT id, name, project_id, shadow_name, shadow_title, shadow_grade, provider, model, thinking_level, cwd, custom_prompt, context_window, created_at, updated_at, archived_at, avatar_type, avatar_path FROM agents ORDER BY (archived_at IS NOT NULL) ASC, archived_at DESC, updated_at DESC"
                } else {
                    "SELECT id, name, project_id, shadow_name, shadow_title, shadow_grade, provider, model, thinking_level, cwd, custom_prompt, context_window, created_at, updated_at, archived_at, avatar_type, avatar_path FROM agents WHERE archived_at IS NULL ORDER BY updated_at DESC"
                };
                let mut stmt = conn.prepare(sql)?;
                let rows = stmt.query_map([], map_agent)?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
            })
            .await?)
    }

    /// MON-66: stamp the agent as archived. Idempotent — re-archiving just
    /// refreshes the timestamp. Does not touch anything else.
    pub async fn archive_agent_internal(&self, agent_id: &str) -> Result<(), MonarchError> {
        let agent_id = agent_id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE agents SET archived_at = strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE id = ?1",
                    params![agent_id],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    /// MON-66: clear the archive stamp. Restores the agent to the active roster.
    pub async fn unarchive_agent_internal(&self, agent_id: &str) -> Result<(), MonarchError> {
        let agent_id = agent_id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE agents SET archived_at = NULL WHERE id = ?1",
                    params![agent_id],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn delete_agent_internal(&self, agent_id: &str) -> Result<(), MonarchError> {
        let agent_id = agent_id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute("DELETE FROM agents WHERE id = ?1", params![agent_id])?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn get_sessions_internal(
        &self,
        agent_id: &str,
    ) -> Result<Vec<SessionRow>, MonarchError> {
        let agent_id = agent_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, agent_id, pi_session_file, model, provider, started_at, ended_at, message_count, total_tokens, total_cost, parent_session_id FROM sessions WHERE agent_id = ?1 ORDER BY started_at DESC",
                )?;
                let rows = stmt.query_map(params![agent_id], map_session)?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
            })
            .await?)
    }

    pub async fn get_messages_internal(
        &self,
        session_id: &str,
    ) -> Result<Vec<MessageRow>, MonarchError> {
        let session_id = session_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, session_id, role, content, model, tokens, cost, timestamp, duration_ms FROM messages WHERE session_id = ?1 ORDER BY id ASC",
                )?;
                let rows = stmt.query_map(params![session_id], map_message)?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
            })
            .await?)
    }

    pub async fn save_memory_internal(&self, memory: &MemoryRow) -> Result<i64, MonarchError> {
        let memory = memory.clone();
        Ok(self
            .conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO memories (agent_id, layer, category, content, relevance, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        memory.agent_id, memory.layer, memory.category,
                        memory.content, memory.relevance, memory.created_at,
                    ],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await?)
    }

    pub async fn get_memories_internal(
        &self,
        agent_id: Option<&str>,
        layer: Option<&str>,
    ) -> Result<Vec<MemoryRow>, MonarchError> {
        let agent_id = agent_id.map(|s| s.to_string());
        let layer = layer.map(|s| s.to_string());
        Ok(self
            .conn
            .call(move |conn| {
                let sql = match (&agent_id, &layer) {
                    (Some(_), Some(_)) => "SELECT id, agent_id, layer, category, content, relevance, created_at, last_accessed, access_count FROM memories WHERE (agent_id = ?1 OR agent_id IS NULL) AND layer = ?2 ORDER BY relevance DESC, created_at DESC",
                    (Some(_), None) => "SELECT id, agent_id, layer, category, content, relevance, created_at, last_accessed, access_count FROM memories WHERE (agent_id = ?1 OR agent_id IS NULL) ORDER BY relevance DESC, created_at DESC",
                    (None, Some(_)) => "SELECT id, agent_id, layer, category, content, relevance, created_at, last_accessed, access_count FROM memories WHERE layer = ?1 ORDER BY relevance DESC, created_at DESC",
                    (None, None) => "SELECT id, agent_id, layer, category, content, relevance, created_at, last_accessed, access_count FROM memories ORDER BY relevance DESC, created_at DESC",
                };
                let mut stmt = conn.prepare(sql)?;
                let rows = match (agent_id, layer) {
                    (Some(a), Some(l)) => stmt.query_map(params![a, l], map_memory)?.collect::<rusqlite::Result<Vec<_>>>()?,
                    (Some(a), None) => stmt.query_map(params![a], map_memory)?.collect::<rusqlite::Result<Vec<_>>>()?,
                    (None, Some(l)) => stmt.query_map(params![l], map_memory)?.collect::<rusqlite::Result<Vec<_>>>()?,
                    (None, None) => stmt.query_map([], map_memory)?.collect::<rusqlite::Result<Vec<_>>>()?,
                };
                Ok(rows)
            })
            .await?)
    }

    pub async fn upsert_project_internal(&self, project: &ProjectRow) -> Result<(), MonarchError> {
        let project = project.clone();
        self.conn
            .call(move |conn| {
                let existing: Option<String> = conn
                    .query_row(
                        "SELECT id FROM projects WHERE root_path = ?1",
                        params![project.root_path],
                        |row| row.get(0),
                    )
                    .ok();
                if let Some(existing_id) = existing {
                    conn.execute(
                        "UPDATE projects SET name = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE id = ?2",
                        params![project.name, existing_id],
                    )?;
                } else {
                    conn.execute(
                        "INSERT INTO projects (id, name, root_path, instructions, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                         ON CONFLICT(id) DO UPDATE SET
                           name=excluded.name, root_path=excluded.root_path, instructions=excluded.instructions, updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')",
                        params![project.id, project.name, project.root_path, project.instructions, project.created_at, project.updated_at],
                    )?;
                }
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn get_projects_internal(&self) -> Result<Vec<ProjectRow>, MonarchError> {
        Ok(self
            .conn
            .call(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, name, root_path, instructions, created_at, updated_at FROM projects ORDER BY updated_at DESC",
                )?;
                let rows = stmt.query_map([], map_project)?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
            })
            .await?)
    }

    pub async fn rename_project_internal(
        &self,
        project_id: &str,
        name: &str,
    ) -> Result<(), MonarchError> {
        let project_id = project_id.to_string();
        let name = name.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE projects SET name = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE id = ?2",
                    params![name, project_id],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn update_project_instructions_internal(
        &self,
        project_id: &str,
        instructions: Option<&str>,
    ) -> Result<(), MonarchError> {
        let project_id = project_id.to_string();
        let instructions = instructions.map(|s| s.to_string());
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE projects SET instructions = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE id = ?2",
                    params![instructions, project_id],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn delete_project_internal(&self, project_id: &str) -> Result<(), MonarchError> {
        let project_id = project_id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute("DELETE FROM projects WHERE id = ?1", params![project_id])?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn list_agent_templates_internal(
        &self,
    ) -> Result<Vec<AgentTemplateRow>, MonarchError> {
        Ok(self
            .conn
            .call(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, name, provider, model, thinking_level, cwd, shadow_name, shadow_title, shadow_grade, created_at, updated_at
                     FROM agent_templates ORDER BY updated_at DESC",
                )?;
                let rows = stmt.query_map([], map_agent_template)?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
            })
            .await?)
    }

    pub async fn save_agent_template_internal(
        &self,
        template: &AgentTemplateRow,
    ) -> Result<(), MonarchError> {
        let template = template.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO agent_templates (id, name, provider, model, thinking_level, cwd, shadow_name, shadow_title, shadow_grade, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                     ON CONFLICT(id) DO UPDATE SET
                       name=excluded.name,
                       provider=excluded.provider,
                       model=excluded.model,
                       thinking_level=excluded.thinking_level,
                       cwd=excluded.cwd,
                       shadow_name=excluded.shadow_name,
                       shadow_title=excluded.shadow_title,
                       shadow_grade=excluded.shadow_grade,
                       updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')",
                    params![
                        template.id, template.name, template.provider, template.model,
                        template.thinking_level, template.cwd, template.shadow_name,
                        template.shadow_title, template.shadow_grade,
                        template.created_at, template.updated_at,
                    ],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn delete_agent_template_internal(
        &self,
        template_id: &str,
    ) -> Result<(), MonarchError> {
        let template_id = template_id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "DELETE FROM agent_templates WHERE id = ?1",
                    params![template_id],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn get_ui_state_internal(&self, key: &str) -> Result<Option<String>, MonarchError> {
        let key = key.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let result = conn.query_row(
                    "SELECT value FROM ui_state WHERE key = ?1",
                    params![key],
                    |row| row.get::<_, String>(0),
                );
                match result {
                    Ok(value) => Ok(Some(value)),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                    Err(e) => Err(e),
                }
            })
            .await?)
    }

    pub async fn set_ui_state_internal(
        &self,
        key: &str,
        value: &str,
    ) -> Result<(), MonarchError> {
        let key = key.to_string();
        let value = value.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO ui_state (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![key, value],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }
    // ---- Agent Stats ----

    /// Increment token/cost/message counters for an agent. Called from the
    /// persistence pipeline alongside SaveAssistantMessage.
    pub async fn increment_agent_stats(
        &self,
        agent_id: &str,
        input_tokens: i64,
        output_tokens: i64,
        cost: f64,
    ) -> Result<(), MonarchError> {
        let agent_id = agent_id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO agent_stats (agent_id, total_messages, total_input_tokens, total_output_tokens, total_cost, updated_at)
                     VALUES (?1, 1, ?2, ?3, ?4, strftime('%Y-%m-%dT%H:%M:%SZ','now'))
                     ON CONFLICT(agent_id) DO UPDATE SET
                       total_messages = total_messages + 1,
                       total_input_tokens = total_input_tokens + ?2,
                       total_output_tokens = total_output_tokens + ?3,
                       total_cost = total_cost + ?4,
                       updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')",
                    params![agent_id, input_tokens, output_tokens, cost],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    /// Increment the turn counter for an agent. Called on TurnEnd events.
    pub async fn increment_agent_turns(&self, agent_id: &str) -> Result<(), MonarchError> {
        let agent_id = agent_id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO agent_stats (agent_id, total_turns, updated_at)
                     VALUES (?1, 1, strftime('%Y-%m-%dT%H:%M:%SZ','now'))
                     ON CONFLICT(agent_id) DO UPDATE SET
                       total_turns = total_turns + 1,
                       updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')",
                    params![agent_id],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    /// Increment the session counter for an agent. Called when a new session is created.
    pub async fn increment_agent_sessions(&self, agent_id: &str) -> Result<(), MonarchError> {
        let agent_id = agent_id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO agent_stats (agent_id, total_sessions, updated_at)
                     VALUES (?1, 1, strftime('%Y-%m-%dT%H:%M:%SZ','now'))
                     ON CONFLICT(agent_id) DO UPDATE SET
                       total_sessions = total_sessions + 1,
                       updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')",
                    params![agent_id],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    /// Record a tool execution for an agent (upsert call_count / error_count).
    pub async fn record_tool_usage(
        &self,
        agent_id: &str,
        tool_name: &str,
        is_error: bool,
    ) -> Result<(), MonarchError> {
        let agent_id = agent_id.to_string();
        let tool_name = tool_name.to_string();
        let error_delta: i32 = if is_error { 1 } else { 0 };
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO agent_tool_usage (agent_id, tool_name, call_count, error_count)
                     VALUES (?1, ?2, 1, ?3)
                     ON CONFLICT(agent_id, tool_name) DO UPDATE SET
                       call_count = call_count + 1,
                       error_count = error_count + ?3",
                    params![agent_id, tool_name, error_delta],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    /// Get the full stats picture for an agent, including tool usage and
    /// derived specialization scores.
    pub async fn get_agent_stats_internal(
        &self,
        agent_id: &str,
    ) -> Result<AgentStats, MonarchError> {
        let agent_id = agent_id.to_string();
        self.conn
            .call(move |conn| {
                // Get or create base stats
                let (total_sessions, total_messages, total_turns, total_input_tokens, total_output_tokens, total_cost, updated_at) = conn
                    .query_row(
                        "SELECT total_sessions, total_messages, total_turns, total_input_tokens, total_output_tokens, total_cost, updated_at
                         FROM agent_stats WHERE agent_id = ?1",
                        params![agent_id],
                        |row| Ok((
                            row.get::<_, i32>(0)?,
                            row.get::<_, i32>(1)?,
                            row.get::<_, i32>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, f64>(5)?,
                            row.get::<_, String>(6)?,
                        )),
                    )
                    .unwrap_or((0, 0, 0, 0, 0, 0.0, String::new()));

                // Get tool usage
                let mut stmt = conn.prepare(
                    "SELECT tool_name, call_count, error_count FROM agent_tool_usage WHERE agent_id = ?1 ORDER BY call_count DESC",
                )?;
                let tool_usage: Vec<ToolUsageEntry> = stmt
                    .query_map(params![agent_id], |row| {
                        Ok(ToolUsageEntry {
                            tool_name: row.get(0)?,
                            call_count: row.get(1)?,
                            error_count: row.get(2)?,
                        })
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;

                // Derive specialization from tool usage
                let specialization = compute_specialization(&tool_usage);

                // Compute experience from total tokens (log scale)
                let total_tokens = total_input_tokens + total_output_tokens;
                let experience = if total_tokens <= 0 {
                    0.0
                } else {
                    ((total_tokens as f64).log10() * 15.0).min(100.0)
                };

                Ok(AgentStats {
                    agent_id,
                    total_sessions,
                    total_messages,
                    total_turns,
                    total_input_tokens,
                    total_output_tokens,
                    total_cost,
                    experience,
                    tool_usage,
                    specialization,
                    updated_at,
                })
            })
            .await
            .map_err(MonarchError::from)
    }

    // ---- MON-83: Quests ----

    /// Insert a quest node. Uses the payload id if present, otherwise mints a
    /// fresh UUID. `root_id` is resolved from the parent: root quests have
    /// `root_id = id`; sub-quests inherit the parent's `root_id`. A
    /// `status_change: null → <status>` event is seeded in the same
    /// transaction so the event log always has a creation entry (Slice 2
    /// read-only UI relies on this for its success criterion).
    pub async fn create_quest_internal(
        &self,
        payload: &CreateQuestPayload,
    ) -> Result<String, MonarchError> {
        let payload = payload.clone();
        let id = payload.id.clone().unwrap_or_else(crate::util::uuid_v4_simple);
        let status = payload.status.clone().unwrap_or_else(|| "pending".to_string());
        let grade = payload.grade.clone().unwrap_or_else(|| "C".to_string());
        let exec_hint = payload
            .exec_hint
            .clone()
            .unwrap_or_else(|| "in_context".to_string());
        let created_by = payload
            .created_by
            .clone()
            .unwrap_or_else(|| "monarch".to_string());
        let now = crate::util::chrono_now();
        let event_id = crate::util::uuid_v4_simple();

        let id_for_return = id.clone();
        self.conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                // Resolve root_id: if parent present, inherit its root; else self.
                let root_id: String = if let Some(pid) = payload.parent_id.as_ref() {
                    tx.query_row(
                        "SELECT root_id FROM quest_nodes WHERE id = ?1",
                        params![pid],
                        |row| row.get::<_, String>(0),
                    )?
                } else {
                    id.clone()
                };
                tx.execute(
                    "INSERT INTO quest_nodes (
                        id, root_id, parent_id, title, description,
                        status, grade, exec_hint, assignee_shadow_id,
                        created_by, created_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        id,
                        root_id,
                        payload.parent_id,
                        payload.title,
                        payload.description,
                        status,
                        grade,
                        exec_hint,
                        payload.assignee_shadow_id,
                        created_by,
                        now,
                    ],
                )?;
                // Seed the creation event so the event log is never empty.
                let event_payload = serde_json::json!({
                    "from": null,
                    "to": status,
                })
                .to_string();
                tx.execute(
                    "INSERT INTO quest_events (id, quest_id, event_type, actor, payload_json, created_at)
                     VALUES (?1, ?2, 'status_change', ?3, ?4, ?5)",
                    params![event_id, id, created_by, event_payload, now],
                )?;
                tx.commit()?;
                Ok(())
            })
            .await?;
        Ok(id_for_return)
    }

    /// Partial update — only `Some` fields are written. Status / timestamp
    /// changes that carry semantic weight (e.g. status→done) should ALSO
    /// record a `quest_events` row via `record_quest_event_internal`; this
    /// method does not mirror them automatically so the caller keeps full
    /// control over the audit trail.
    pub async fn update_quest_internal(
        &self,
        payload: &UpdateQuestPayload,
    ) -> Result<(), MonarchError> {
        let payload = payload.clone();
        self.conn
            .call(move |conn| {
                // Build SET clause dynamically. `rusqlite` does not support
                // array-of-params with named columns, so we stringify and
                // push each present field.
                let mut sets: Vec<&str> = Vec::new();
                let mut args: Vec<rusqlite::types::Value> = Vec::new();
                macro_rules! push {
                    ($field:expr, $col:literal) => {
                        if let Some(v) = $field.as_ref() {
                            sets.push(concat!($col, " = ?"));
                            args.push(rusqlite::types::Value::Text(v.clone()));
                        }
                    };
                }
                push!(payload.title, "title");
                push!(payload.description, "description");
                push!(payload.status, "status");
                push!(payload.grade, "grade");
                push!(payload.exec_hint, "exec_hint");
                push!(payload.assignee_shadow_id, "assignee_shadow_id");
                push!(payload.summary, "summary");
                push!(payload.started_at, "started_at");
                push!(payload.completed_at, "completed_at");
                push!(payload.abandoned_at, "abandoned_at");
                if sets.is_empty() {
                    return Ok(());
                }
                let sql = format!(
                    "UPDATE quest_nodes SET {} WHERE id = ?",
                    sets.join(", ")
                );
                args.push(rusqlite::types::Value::Text(payload.id.clone()));
                let params_slice: Vec<&dyn rusqlite::ToSql> =
                    args.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
                conn.execute(&sql, params_slice.as_slice())?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn get_quest_internal(
        &self,
        quest_id: &str,
    ) -> Result<Option<QuestRow>, MonarchError> {
        let quest_id = quest_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(QUEST_SELECT_SQL)?;
                let mut rows = stmt.query(params![quest_id])?;
                if let Some(row) = rows.next()? {
                    Ok(Some(map_quest(row)?))
                } else {
                    Ok(None)
                }
            })
            .await?)
    }

    /// Every quest where this agent is the assignee, ordered newest-first.
    /// Filter is assignee-only — `agents.current_quest_id` is a pointer into
    /// the tree, not a list key.
    pub async fn list_quests_for_agent_internal(
        &self,
        agent_id: &str,
    ) -> Result<Vec<QuestRow>, MonarchError> {
        let agent_id = agent_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(&format!(
                    "{} WHERE assignee_shadow_id = ?1 ORDER BY created_at DESC",
                    QUEST_BASE_SELECT
                ))?;
                let rows = stmt
                    .query_map(params![agent_id], map_quest)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await?)
    }

    /// Full tree under `root_id`, ordered by created_at so a depth-first
    /// reconstruction on the frontend (using parent_id) produces a stable
    /// visual order.
    pub async fn get_quest_tree_for_root_internal(
        &self,
        root_id: &str,
    ) -> Result<Vec<QuestRow>, MonarchError> {
        let root_id = root_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(&format!(
                    "{} WHERE root_id = ?1 ORDER BY created_at ASC",
                    QUEST_BASE_SELECT
                ))?;
                let rows = stmt
                    .query_map(params![root_id], map_quest)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await?)
    }

    pub async fn record_quest_event_internal(
        &self,
        payload: &RecordQuestEventPayload,
    ) -> Result<String, MonarchError> {
        let payload = payload.clone();
        let id = crate::util::uuid_v4_simple();
        let now = crate::util::chrono_now();
        let id_for_return = id.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO quest_events (id, quest_id, event_type, actor, payload_json, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        id,
                        payload.quest_id,
                        payload.event_type,
                        payload.actor,
                        payload.payload_json,
                        now,
                    ],
                )?;
                Ok(())
            })
            .await?;
        Ok(id_for_return)
    }

    pub async fn list_quest_events_internal(
        &self,
        quest_id: &str,
    ) -> Result<Vec<QuestEventRow>, MonarchError> {
        let quest_id = quest_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, quest_id, event_type, actor, payload_json, created_at
                     FROM quest_events WHERE quest_id = ?1 ORDER BY created_at ASC",
                )?;
                let rows = stmt
                    .query_map(params![quest_id], map_quest_event)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await?)
    }

    // ---- MON-82: Classifications ----

    /// Insert a classification row. `message_id` is always null at insert
    /// time (see `SaveClassificationPayload`); `backfill_classification_message_id`
    /// sets it later when the user message row lands.
    pub async fn save_classification_internal(
        &self,
        payload: &SaveClassificationPayload,
    ) -> Result<(), MonarchError> {
        let payload = payload.clone();
        let now = crate::util::chrono_now();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO classifications (
                        id, message_id, agent_id, session_id, complexity,
                        confidence, rationale, model, tokens_in, tokens_out,
                        latency_ms, error, created_at
                    ) VALUES (
                        ?1, NULL, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12
                    )",
                    params![
                        payload.id,
                        payload.agent_id,
                        payload.session_id,
                        payload.complexity,
                        payload.confidence,
                        payload.rationale,
                        payload.model,
                        payload.tokens_in,
                        payload.tokens_out,
                        payload.latency_ms,
                        payload.error,
                        now,
                    ],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    /// Link an existing classification row to a persisted user message.
    /// Returns the number of rows updated — `0` means the classification
    /// row isn't in the table yet (classifier still in flight); the caller
    /// should stash the mapping and re-apply on `SaveClassification`.
    pub async fn backfill_classification_message_id(
        &self,
        classification_id: &str,
        message_id: i64,
    ) -> Result<usize, MonarchError> {
        let classification_id = classification_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let rows = conn.execute(
                    "UPDATE classifications SET message_id = ?1 WHERE id = ?2",
                    params![message_id, classification_id],
                )?;
                Ok(rows)
            })
            .await?)
    }

    pub async fn list_classifications_for_agent_internal(
        &self,
        agent_id: &str,
        limit: Option<i64>,
    ) -> Result<Vec<ClassificationRow>, MonarchError> {
        let agent_id = agent_id.to_string();
        let limit = limit.unwrap_or(200);
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(&format!(
                    "{} WHERE agent_id = ?1 ORDER BY created_at DESC LIMIT ?2",
                    CLASSIFICATION_BASE_SELECT
                ))?;
                let rows = stmt
                    .query_map(params![agent_id, limit], map_classification)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await?)
    }

    pub async fn get_classification_for_message_internal(
        &self,
        message_id: i64,
    ) -> Result<Option<ClassificationRow>, MonarchError> {
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(&format!(
                    "{} WHERE message_id = ?1 ORDER BY created_at DESC LIMIT 1",
                    CLASSIFICATION_BASE_SELECT
                ))?;
                let mut rows = stmt.query(params![message_id])?;
                if let Some(row) = rows.next()? {
                    Ok(Some(map_classification(row)?))
                } else {
                    Ok(None)
                }
            })
            .await?)
    }
}

// Shared column list for quest_nodes SELECTs. `QUEST_SELECT_SQL` is the
// single-row lookup by id; `QUEST_BASE_SELECT` is the prefix for filtered
// list queries (no WHERE clause).
const QUEST_BASE_SELECT: &str = "SELECT \
    id, root_id, parent_id, title, description, status, grade, exec_hint, \
    explore_fork_count, assignee_shadow_id, worktree_path, branch_name, \
    base_branch, branched_from_id, superseded_by_id, created_by, created_at, \
    started_at, completed_at, abandoned_at, estimated_tokens, actual_tokens, \
    estimated_duration_ms, actual_duration_ms, summary FROM quest_nodes";

const QUEST_SELECT_SQL: &str = "SELECT \
    id, root_id, parent_id, title, description, status, grade, exec_hint, \
    explore_fork_count, assignee_shadow_id, worktree_path, branch_name, \
    base_branch, branched_from_id, superseded_by_id, created_by, created_at, \
    started_at, completed_at, abandoned_at, estimated_tokens, actual_tokens, \
    estimated_duration_ms, actual_duration_ms, summary \
    FROM quest_nodes WHERE id = ?1";

// MON-82: Classifications. Shared column list; callers append the WHERE
// clause they need.
const CLASSIFICATION_BASE_SELECT: &str = "SELECT \
    id, message_id, agent_id, session_id, complexity, confidence, rationale, \
    model, tokens_in, tokens_out, latency_ms, error, created_at \
    FROM classifications";

/// Map tool names to specialization categories and compute normalized scores.
fn compute_specialization(tool_usage: &[ToolUsageEntry]) -> SpecializationScores {
    let mut scores = [0.0f64; 12]; // indexed by category
    // Categories: 0=coding, 1=research, 2=testing, 3=debugging, 4=devops,
    //   5=documentation, 6=database, 7=configuration, 8=design, 9=communication,
    //   10=refactoring, 11=security

    for entry in tool_usage {
        let count = entry.call_count as f64;
        let name = entry.tool_name.as_str();
        match name {
            // Coding tools
            "Edit" | "Write" | "NotebookEdit" => scores[0] += count,
            // Research tools
            "Read" | "Grep" | "Glob" | "LS" | "ListDir" | "Search"
            | "WebSearch" | "WebFetch" | "NotebookRead" => scores[1] += count,
            // Devops tools
            "Bash" => {
                // Bash is ambiguous — split across coding/devops
                scores[0] += count * 0.5;
                scores[4] += count * 0.5;
            }
            // Agent/communication tools
            "Agent" | "SendMessage" | "AskUser" | "AskUserQuestion" => scores[9] += count,
            // Task/planning tools
            "TaskCreate" | "TaskUpdate" | "TaskList" | "TaskGet"
            | "TodoWrite" | "TodoRead" | "EnterPlanMode" | "ExitPlanMode" => scores[0] += count * 0.5,
            // Everything else — distribute lightly to coding
            _ => scores[0] += count * 0.3,
        }
    }

    let total: f64 = scores.iter().sum();
    if total > 0.0 {
        for s in &mut scores {
            *s /= total;
        }
    }

    SpecializationScores {
        coding: scores[0],
        research: scores[1],
        testing: scores[2],
        debugging: scores[3],
        devops: scores[4],
        documentation: scores[5],
        database: scores[6],
        configuration: scores[7],
        design: scores[8],
        communication: scores[9],
        refactoring: scores[10],
        security: scores[11],
    }
}

// ---- Row mappers ----

fn map_project(row: &Row<'_>) -> rusqlite::Result<ProjectRow> {
    Ok(ProjectRow {
        id: row.get(0)?,
        name: row.get(1)?,
        root_path: row.get(2)?,
        instructions: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn map_agent(row: &Row<'_>) -> rusqlite::Result<AgentRow> {
    Ok(AgentRow {
        id: row.get(0)?,
        name: row.get(1)?,
        project_id: row.get(2)?,
        shadow_name: row.get(3)?,
        shadow_title: row.get(4)?,
        shadow_grade: row.get(5)?,
        provider: row.get(6)?,
        model: row.get(7)?,
        thinking_level: row.get(8)?,
        cwd: row.get(9)?,
        custom_prompt: row.get(10)?,
        context_window: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
        archived_at: row.get(14)?,
        avatar_type: row.get(15)?,
        avatar_path: row.get(16)?,
    })
}

fn map_session(row: &Row<'_>) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        id: row.get(0)?,
        agent_id: row.get(1)?,
        pi_session_file: row.get(2)?,
        model: row.get(3)?,
        provider: row.get(4)?,
        started_at: row.get(5)?,
        ended_at: row.get(6)?,
        message_count: row.get(7)?,
        total_tokens: row.get(8)?,
        total_cost: row.get(9)?,
        parent_session_id: row.get(10)?,
    })
}

fn map_message(row: &Row<'_>) -> rusqlite::Result<MessageRow> {
    Ok(MessageRow {
        id: row.get(0)?,
        session_id: row.get(1)?,
        role: row.get(2)?,
        content: row.get(3)?,
        model: row.get(4)?,
        tokens: row.get(5)?,
        cost: row.get(6)?,
        timestamp: row.get(7)?,
        duration_ms: row.get(8).ok(),
        // Filled in by `get_messages_with_ancestry` via a second query —
        // the row returned from a single messages SELECT cannot join the
        // attachments table because rusqlite row handlers are sync and
        // sibling queries are easier to reason about than window joins.
        attachments: Vec::new(),
    })
}

fn map_memory(row: &Row<'_>) -> rusqlite::Result<MemoryRow> {
    Ok(MemoryRow {
        id: row.get(0)?,
        agent_id: row.get(1)?,
        layer: row.get(2)?,
        category: row.get(3)?,
        content: row.get(4)?,
        relevance: row.get(5)?,
        created_at: row.get(6)?,
        last_accessed: row.get(7)?,
        access_count: row.get(8)?,
    })
}

fn map_agent_template(row: &Row<'_>) -> rusqlite::Result<AgentTemplateRow> {
    Ok(AgentTemplateRow {
        id: row.get(0)?,
        name: row.get(1)?,
        provider: row.get(2)?,
        model: row.get(3)?,
        thinking_level: row.get(4)?,
        cwd: row.get(5)?,
        shadow_name: row.get(6)?,
        shadow_title: row.get(7)?,
        shadow_grade: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn map_quest(row: &Row<'_>) -> rusqlite::Result<QuestRow> {
    Ok(QuestRow {
        id: row.get(0)?,
        root_id: row.get(1)?,
        parent_id: row.get(2)?,
        title: row.get(3)?,
        description: row.get(4)?,
        status: row.get(5)?,
        grade: row.get(6)?,
        exec_hint: row.get(7)?,
        explore_fork_count: row.get(8)?,
        assignee_shadow_id: row.get(9)?,
        worktree_path: row.get(10)?,
        branch_name: row.get(11)?,
        base_branch: row.get(12)?,
        branched_from_id: row.get(13)?,
        superseded_by_id: row.get(14)?,
        created_by: row.get(15)?,
        created_at: row.get(16)?,
        started_at: row.get(17)?,
        completed_at: row.get(18)?,
        abandoned_at: row.get(19)?,
        estimated_tokens: row.get(20)?,
        actual_tokens: row.get(21)?,
        estimated_duration_ms: row.get(22)?,
        actual_duration_ms: row.get(23)?,
        summary: row.get(24)?,
    })
}

fn map_quest_event(row: &Row<'_>) -> rusqlite::Result<QuestEventRow> {
    Ok(QuestEventRow {
        id: row.get(0)?,
        quest_id: row.get(1)?,
        event_type: row.get(2)?,
        actor: row.get(3)?,
        payload_json: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn map_classification(row: &Row<'_>) -> rusqlite::Result<ClassificationRow> {
    Ok(ClassificationRow {
        id: row.get(0)?,
        message_id: row.get(1)?,
        agent_id: row.get(2)?,
        session_id: row.get(3)?,
        complexity: row.get(4)?,
        confidence: row.get(5)?,
        rationale: row.get(6)?,
        model: row.get(7)?,
        tokens_in: row.get(8)?,
        tokens_out: row.get(9)?,
        latency_ms: row.get(10)?,
        error: row.get(11)?,
        created_at: row.get(12)?,
    })
}

// ---- Tauri Commands: Agents ----

#[tauri::command]
#[specta::specta]
pub async fn db_upsert_agent(
    db: tauri::State<'_, Arc<Database>>,
    agent: AgentRow,
) -> Result<(), MonarchError> {
    db.upsert_agent_internal(&agent).await
}

#[tauri::command]
#[specta::specta]
/// MON-73: Update user-editable agent fields without touching spawn-time fields.
pub async fn db_update_agent(
    db: tauri::State<'_, Arc<Database>>,
    payload: AgentUpdatePayload,
) -> Result<(), MonarchError> {
    db.update_agent_internal(&payload).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_get_agents(
    db: tauri::State<'_, Arc<Database>>,
    include_archived: Option<bool>,
) -> Result<Vec<AgentRow>, MonarchError> {
    db.get_agents_internal(include_archived.unwrap_or(false)).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_archive_agent(
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
) -> Result<(), MonarchError> {
    db.archive_agent_internal(&agent_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_unarchive_agent(
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
) -> Result<(), MonarchError> {
    db.unarchive_agent_internal(&agent_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_delete_agent(
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
) -> Result<(), MonarchError> {
    db.delete_agent_internal(&agent_id).await
}

// ---- Tauri Commands: Sessions ----

#[tauri::command]
#[specta::specta]
pub async fn db_create_session(
    db: tauri::State<'_, Arc<Database>>,
    session: SessionRow,
) -> Result<(), MonarchError> {
    db.create_session_internal(&session).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_get_sessions(
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
) -> Result<Vec<SessionRow>, MonarchError> {
    db.get_sessions_internal(&agent_id).await
}

// ---- Tauri Commands: Messages ----

#[tauri::command]
#[specta::specta]
pub async fn db_save_message(
    db: tauri::State<'_, Arc<Database>>,
    message: MessageRow,
) -> Result<i64, MonarchError> {
    db.save_message_internal(&message).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_get_messages(
    db: tauri::State<'_, Arc<Database>>,
    session_id: String,
) -> Result<Vec<MessageRow>, MonarchError> {
    db.get_messages_internal(&session_id).await
}

/// Get messages for a session, including all ancestor sessions (for continued sessions)
#[tauri::command]
#[specta::specta]
pub async fn db_get_messages_with_ancestry(
    db: tauri::State<'_, Arc<Database>>,
    session_id: String,
) -> Result<Vec<MessageRow>, MonarchError> {
    db.get_messages_with_ancestry(&session_id).await
}

// ---- Tauri Commands: Memories ----

#[tauri::command]
#[specta::specta]
pub async fn db_save_memory(
    db: tauri::State<'_, Arc<Database>>,
    memory: MemoryRow,
) -> Result<i64, MonarchError> {
    db.save_memory_internal(&memory).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_get_memories(
    db: tauri::State<'_, Arc<Database>>,
    agent_id: Option<String>,
    layer: Option<String>,
) -> Result<Vec<MemoryRow>, MonarchError> {
    db.get_memories_internal(agent_id.as_deref(), layer.as_deref())
        .await
}

// ---- Tauri Commands: Projects ----

#[tauri::command]
#[specta::specta]
pub async fn db_upsert_project(
    db: tauri::State<'_, Arc<Database>>,
    project: ProjectRow,
) -> Result<(), MonarchError> {
    db.upsert_project_internal(&project).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_get_projects(
    db: tauri::State<'_, Arc<Database>>,
) -> Result<Vec<ProjectRow>, MonarchError> {
    db.get_projects_internal().await
}

#[tauri::command]
#[specta::specta]
pub async fn db_get_project_by_path(
    db: tauri::State<'_, Arc<Database>>,
    root_path: String,
) -> Result<Option<ProjectRow>, MonarchError> {
    db.get_project_by_path_internal(&root_path).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_rename_project(
    db: tauri::State<'_, Arc<Database>>,
    project_id: String,
    name: String,
) -> Result<(), MonarchError> {
    db.rename_project_internal(&project_id, &name).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_update_project_instructions(
    db: tauri::State<'_, Arc<Database>>,
    project_id: String,
    instructions: Option<String>,
) -> Result<(), MonarchError> {
    db.update_project_instructions_internal(&project_id, instructions.as_deref())
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn db_delete_project(
    db: tauri::State<'_, Arc<Database>>,
    project_id: String,
) -> Result<(), MonarchError> {
    db.delete_project_internal(&project_id).await
}

// ---- Tauri Commands: Agent Templates ----

#[tauri::command]
#[specta::specta]
pub async fn db_list_agent_templates(
    db: tauri::State<'_, Arc<Database>>,
) -> Result<Vec<AgentTemplateRow>, MonarchError> {
    db.list_agent_templates_internal().await
}

#[tauri::command]
#[specta::specta]
pub async fn db_save_agent_template(
    db: tauri::State<'_, Arc<Database>>,
    template: AgentTemplateRow,
) -> Result<(), MonarchError> {
    db.save_agent_template_internal(&template).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_delete_agent_template(
    db: tauri::State<'_, Arc<Database>>,
    template_id: String,
) -> Result<(), MonarchError> {
    db.delete_agent_template_internal(&template_id).await
}

// ---- Tauri Commands: Events ----

#[tauri::command]
#[specta::specta]
pub async fn db_log_event(
    db: tauri::State<'_, Arc<Database>>,
    agent_id: Option<String>,
    session_id: Option<String>,
    event_type: String,
    data: Option<String>,
) -> Result<(), MonarchError> {
    db.log_event_internal(
        agent_id.as_deref(),
        session_id.as_deref(),
        &event_type,
        data.as_deref(),
    )
    .await
}

// ---- Tauri Commands: UI State ----

#[tauri::command]
#[specta::specta]
pub async fn db_get_ui_state(
    db: tauri::State<'_, Arc<Database>>,
    key: String,
) -> Result<Option<String>, MonarchError> {
    db.get_ui_state_internal(&key).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_set_ui_state(
    db: tauri::State<'_, Arc<Database>>,
    key: String,
    value: String,
) -> Result<(), MonarchError> {
    db.set_ui_state_internal(&key, &value).await
}

// ---- Tauri Commands: Agent Stats ----

#[tauri::command]
#[specta::specta]
pub async fn db_get_agent_stats(
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
) -> Result<AgentStats, MonarchError> {
    db.get_agent_stats_internal(&agent_id).await
}

// ---- Tauri Commands: Quests (MON-83) ----
//
// Write commands take the `AgentManager` state so they can broadcast event
// channels (`quest-created-{id}` / `quest-updated-{id}` /
// `quest-event-{questId}`) via the shared `ws_broadcast` sender. Slice 2
// payloads are small — the event is the quest id and minimal metadata so
// subscribers can re-fetch with `db_get_quest` / `db_list_quest_events`.

#[tauri::command]
#[specta::specta]
pub async fn db_create_quest(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    payload: CreateQuestPayload,
) -> Result<String, MonarchError> {
    let id = db.create_quest_internal(&payload).await?;
    crate::agent::emit_event(
        &app,
        &agent_mgr.ws_broadcast,
        &format!("quest-created-{}", id),
        &serde_json::json!({ "id": id }).to_string(),
    );
    Ok(id)
}

#[tauri::command]
#[specta::specta]
pub async fn db_update_quest(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    payload: UpdateQuestPayload,
) -> Result<(), MonarchError> {
    let id = payload.id.clone();
    db.update_quest_internal(&payload).await?;
    crate::agent::emit_event(
        &app,
        &agent_mgr.ws_broadcast,
        &format!("quest-updated-{}", id),
        &serde_json::json!({ "id": id }).to_string(),
    );
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn db_get_quest(
    db: tauri::State<'_, Arc<Database>>,
    quest_id: String,
) -> Result<Option<QuestRow>, MonarchError> {
    db.get_quest_internal(&quest_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_list_quests_for_agent(
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
) -> Result<Vec<QuestRow>, MonarchError> {
    db.list_quests_for_agent_internal(&agent_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_get_quest_tree_for_root(
    db: tauri::State<'_, Arc<Database>>,
    root_id: String,
) -> Result<Vec<QuestRow>, MonarchError> {
    db.get_quest_tree_for_root_internal(&root_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_record_quest_event(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    payload: RecordQuestEventPayload,
) -> Result<String, MonarchError> {
    let quest_id = payload.quest_id.clone();
    let event_type = payload.event_type.clone();
    let id = db.record_quest_event_internal(&payload).await?;
    crate::agent::emit_event(
        &app,
        &agent_mgr.ws_broadcast,
        &format!("quest-event-{}", quest_id),
        &serde_json::json!({ "id": id, "eventType": event_type }).to_string(),
    );
    Ok(id)
}

#[tauri::command]
#[specta::specta]
pub async fn db_list_quest_events(
    db: tauri::State<'_, Arc<Database>>,
    quest_id: String,
) -> Result<Vec<QuestEventRow>, MonarchError> {
    db.list_quest_events_internal(&quest_id).await
}

// ---- MON-82: Classifications ----
//
// Writes are sidecar-originated and flow through the MON-37 persistence
// pipeline (see `agent/persist.rs`); no Tauri `db_save_classification`
// command is exposed. The commands below are read-only helpers for the
// frontend (e.g. the classifier settings tool / future analytics).

#[tauri::command]
#[specta::specta]
pub async fn db_list_classifications_for_agent(
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
    limit: Option<i64>,
) -> Result<Vec<ClassificationRow>, MonarchError> {
    db.list_classifications_for_agent_internal(&agent_id, limit)
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn db_get_classification_for_message(
    db: tauri::State<'_, Arc<Database>>,
    message_id: i64,
) -> Result<Option<ClassificationRow>, MonarchError> {
    db.get_classification_for_message_internal(message_id).await
}
