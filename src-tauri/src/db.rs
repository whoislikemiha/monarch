use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
        db.ensure_captain_bootstrap().await?;
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
                // P4: nested execution narrative. `actor` remains the
                // concrete writer id/name; `author` is the semantic source
                // (executor/chat_shadow/captain/keeper/system). Existing
                // rows keep NULLs and render through legacy fallbacks.
                let _ = conn.execute_batch(
                    "ALTER TABLE quest_events ADD COLUMN parent_event_id TEXT REFERENCES quest_events(id) ON DELETE CASCADE;",
                );
                let _ = conn.execute_batch(
                    "ALTER TABLE quest_events ADD COLUMN author TEXT;",
                );
                let _ = conn.execute_batch(
                    "ALTER TABLE quest_events ADD COLUMN surface_override TEXT;",
                );
                let _ = conn.execute_batch(
                    "ALTER TABLE quest_events ADD COLUMN payload_schema_version INTEGER NOT NULL DEFAULT 1;",
                );
                let _ = conn.execute_batch(
                    "CREATE INDEX IF NOT EXISTS idx_quest_events_parent
                        ON quest_events(parent_event_id);",
                );
                let _ = conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS agent_working_memory (
                        agent_id TEXT PRIMARY KEY REFERENCES agents(id) ON DELETE CASCADE,
                        payload_json TEXT NOT NULL,
                        updated_at TEXT NOT NULL
                    );",
                );
                // P4b (MON-111): durable per-quest execution plan items.
                // Plan items are the *intended route* — distinct from the
                // recorded coherent-action timeline. Status is a finite
                // lifecycle pinned at the storage layer; Rust mirrors the
                // values in `PlanItemStatus`. `parent_id` exists so future
                // grouping is possible without migration; V0 UI is flat.
                let _ = conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS quest_plan_items (
                        id TEXT PRIMARY KEY,
                        quest_id TEXT NOT NULL REFERENCES quest_nodes(id) ON DELETE CASCADE,
                        parent_id TEXT REFERENCES quest_plan_items(id) ON DELETE CASCADE,
                        title TEXT NOT NULL,
                        status TEXT NOT NULL CHECK (status IN (
                            'pending','active','completed','skipped','blocked'
                        )),
                        order_index INTEGER NOT NULL,
                        created_by TEXT NOT NULL CHECK (created_by IN (
                            'executor','chat_shadow','captain','architect','monarch'
                        )),
                        rationale TEXT,
                        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
                        updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
                        completed_at TEXT
                    );
                    CREATE INDEX IF NOT EXISTS idx_quest_plan_items_quest_order
                        ON quest_plan_items(quest_id, order_index);
                    CREATE INDEX IF NOT EXISTS idx_quest_plan_items_quest_status
                        ON quest_plan_items(quest_id, status);",
                );
                // P4b (MON-111): coherent_action events stamp the plan_item_id
                // active in L2 at INSERT time so timeline rendering can group
                // actions under their plan item without a join through L2.
                // Nullable — actions emitted while no item is active stay NULL.
                let _ = conn.execute_batch(
                    "ALTER TABLE quest_events ADD COLUMN plan_item_id TEXT REFERENCES quest_plan_items(id) ON DELETE SET NULL;",
                );
                let _ = conn.execute_batch(
                    "CREATE INDEX IF NOT EXISTS idx_quest_events_plan_item
                        ON quest_events(plan_item_id);",
                );
                // P5 (MON-116): rich quest metadata. Older MON-83 columns
                // already include status, grade, worktree_path, and summary;
                // these ALTERs add only the missing what/why fields.
                let _ = conn.execute_batch("ALTER TABLE quest_nodes ADD COLUMN scope TEXT;");
                let _ = conn.execute_batch(
                    "ALTER TABLE quest_nodes ADD COLUMN current_direction TEXT;",
                );
                let _ = conn.execute_batch("ALTER TABLE quest_nodes ADD COLUMN rationale TEXT;");
                let _ = conn.execute_batch(
                    "ALTER TABLE quest_nodes ADD COLUMN fork_parent_id TEXT REFERENCES quest_nodes(id) ON DELETE SET NULL;",
                );
                let _ = conn.execute_batch(
                    "CREATE INDEX IF NOT EXISTS idx_quest_nodes_fork_parent
                        ON quest_nodes(fork_parent_id);",
                );
                let _ = conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS quest_refs (
                        id TEXT PRIMARY KEY,
                        quest_id TEXT NOT NULL REFERENCES quest_nodes(id) ON DELETE CASCADE,
                        ref_type TEXT NOT NULL,
                        label TEXT,
                        target TEXT NOT NULL,
                        metadata_json TEXT,
                        created_by TEXT NOT NULL,
                        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
                        updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
                    );
                    CREATE INDEX IF NOT EXISTS idx_quest_refs_quest
                        ON quest_refs(quest_id, created_at);
                    CREATE INDEX IF NOT EXISTS idx_quest_refs_type
                        ON quest_refs(ref_type);",
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

                // MON-98: Captain identity (L1a) and shadow identity (L1b).
                // `captain` is a singleton (CHECK id = 1). `current_version`
                // is an unguarded integer pointer (no FK) to sidestep the
                // circular-reference bootstrapping problem; integrity is
                // enforced in `ensure_captain_bootstrap`. Shadow versions are
                // append-only rows keyed by agent; `agents.identity_version_id`
                // is the live pointer.
                let _ = conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS captain_identity_versions (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        payload TEXT NOT NULL DEFAULT '',
                        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
                        supersedes_id INTEGER,
                        edit_note TEXT
                    );
                    CREATE INDEX IF NOT EXISTS idx_captain_identity_versions_created
                        ON captain_identity_versions(created_at);
                    CREATE TABLE IF NOT EXISTS captain (
                        id INTEGER PRIMARY KEY CHECK (id = 1),
                        name TEXT NOT NULL DEFAULT 'Captain',
                        current_version INTEGER
                    );
                    CREATE TABLE IF NOT EXISTS shadow_identity_versions (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
                        payload TEXT NOT NULL DEFAULT '',
                        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
                        supersedes_id INTEGER,
                        edit_note TEXT
                    );
                    CREATE INDEX IF NOT EXISTS idx_shadow_identity_versions_agent
                        ON shadow_identity_versions(agent_id, created_at);",
                );
                let _ = conn.execute_batch(
                    "ALTER TABLE agents ADD COLUMN identity_version_id INTEGER;",
                );

                // MON-99: P2 — shadow memory (L3 knowledge tree).
                // `memories` already exists (initial schema); extend it with
                // P2 columns via ALTER TABLE (idempotent — errors are swallowed).
                // `memory_keeper_runs` is provenance per Keeper invocation.
                // `memories_fts` mirrors title+summary+content for BM25 retrieval.
                for col_stmt in &[
                    "ALTER TABLE memories ADD COLUMN scope TEXT NOT NULL DEFAULT 'self'",
                    "ALTER TABLE memories ADD COLUMN project_id TEXT",
                    "ALTER TABLE memories ADD COLUMN parent_id INTEGER",
                    "ALTER TABLE memories ADD COLUMN kind TEXT",
                    "ALTER TABLE memories ADD COLUMN title TEXT NOT NULL DEFAULT ''",
                    "ALTER TABLE memories ADD COLUMN summary TEXT NOT NULL DEFAULT ''",
                    "ALTER TABLE memories ADD COLUMN manual_override INTEGER NOT NULL DEFAULT 0",
                    "ALTER TABLE memories ADD COLUMN source_quest_id TEXT",
                    "ALTER TABLE memories ADD COLUMN source_session_id TEXT",
                    "ALTER TABLE memories ADD COLUMN source_events TEXT",
                    "ALTER TABLE memories ADD COLUMN file_refs TEXT",
                    "ALTER TABLE memories ADD COLUMN embedding BLOB",
                    "ALTER TABLE memories ADD COLUMN embedding_model_id TEXT",
                    "ALTER TABLE memories ADD COLUMN supersedes_id INTEGER",
                    "ALTER TABLE memories ADD COLUMN archived_at TEXT",
                    "ALTER TABLE memories ADD COLUMN last_accessed_at TEXT",
                ] {
                    let _ = conn.execute(col_stmt, []);
                }
                let _ = conn.execute_batch(
                    "CREATE INDEX IF NOT EXISTS idx_memories_agent_scope
                        ON memories(agent_id, scope, archived_at);
                    CREATE INDEX IF NOT EXISTS idx_memories_quest
                        ON memories(source_quest_id);
                    CREATE INDEX IF NOT EXISTS idx_memories_parent
                        ON memories(parent_id);",
                );
                let _ = conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS memory_keeper_runs (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        agent_id TEXT NOT NULL,
                        trigger TEXT NOT NULL,
                        quest_id TEXT REFERENCES quest_nodes(id) ON DELETE SET NULL,
                        started_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
                        completed_at TEXT,
                        tokens_input INTEGER,
                        tokens_output INTEGER,
                        model_id TEXT,
                        output_summary TEXT,
                        outcome TEXT NOT NULL DEFAULT 'pending'
                    );
                    CREATE INDEX IF NOT EXISTS idx_keeper_runs_agent
                        ON memory_keeper_runs(agent_id, started_at);",
                );

                // MON-119: P6 Slice A — first-person quest reports. One row per
                // quest (enforced by UNIQUE(quest_id)); revisions upsert.
                // `agent_id` is denormalized from `quest_nodes.assignee_shadow_id`
                // at write time so per-agent and (later) per-project listings
                // are one table instead of a JOIN. `payload` is opaque JSON in
                // Slice A; the structured shape (summary/outcome/decisions/
                // learned/artifacts/open_threads/reflection/grade) lands with
                // the sidecar tool in Slice B. `distilled_by_keeper_run_id`
                // is populated by Slice D when the Keeper consumes the report.
                let _ = conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS quest_reports (
                        id TEXT PRIMARY KEY,
                        quest_id TEXT NOT NULL UNIQUE REFERENCES quest_nodes(id) ON DELETE CASCADE,
                        agent_id TEXT REFERENCES agents(id) ON DELETE SET NULL,
                        payload TEXT NOT NULL,
                        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
                        updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
                        distilled_by_keeper_run_id INTEGER REFERENCES memory_keeper_runs(id) ON DELETE SET NULL
                    );
                    CREATE INDEX IF NOT EXISTS idx_quest_reports_agent
                        ON quest_reports(agent_id, created_at);",
                );

                // FTS5 virtual table — separate batch because some SQLite
                // builds don't have FTS5; we log the failure and continue.
                let _ = conn.execute_batch(
                    "CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                        title, summary, content,
                        content='memories', content_rowid='id'
                    );
                    CREATE TRIGGER IF NOT EXISTS memories_fts_insert
                        AFTER INSERT ON memories BEGIN
                            INSERT INTO memories_fts(rowid, title, summary, content)
                            VALUES (new.id, new.title, new.summary, COALESCE(new.content,''));
                        END;
                    CREATE TRIGGER IF NOT EXISTS memories_fts_update
                        AFTER UPDATE ON memories BEGIN
                            INSERT INTO memories_fts(memories_fts, rowid, title, summary, content)
                            VALUES ('delete', old.id, old.title, old.summary, COALESCE(old.content,''));
                            INSERT INTO memories_fts(rowid, title, summary, content)
                            VALUES (new.id, new.title, new.summary, COALESCE(new.content,''));
                        END;
                    CREATE TRIGGER IF NOT EXISTS memories_fts_delete
                        BEFORE DELETE ON memories BEGIN
                            INSERT INTO memories_fts(memories_fts, rowid, title, summary, content)
                            VALUES ('delete', old.id, old.title, old.summary, COALESCE(old.content,''));
                        END;",
                );

                Ok(())
            })
            .await?;
        Ok(())
    }

    /// MON-98: Ensure the captain singleton row exists with at least one
    /// identity version. Called once from `Database::new` after `init_schema`.
    pub async fn ensure_captain_bootstrap(&self) -> Result<(), MonarchError> {
        self.conn
            .call(|conn| {
                conn.execute(
                    "INSERT OR IGNORE INTO captain (id, name, current_version) VALUES (1, 'Captain', NULL)",
                    [],
                )?;
                let needs_seed: bool = conn
                    .query_row(
                        "SELECT current_version IS NULL FROM captain WHERE id = 1",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap_or(true);
                if needs_seed {
                    conn.execute(
                        "INSERT INTO captain_identity_versions (payload, created_at) \
                         VALUES ('', strftime('%Y-%m-%dT%H:%M:%SZ','now'))",
                        [],
                    )?;
                    let version_id = conn.last_insert_rowid();
                    conn.execute(
                        "UPDATE captain SET current_version = ?1 WHERE id = 1",
                        params![version_id],
                    )?;
                }
                Ok(())
            })
            .await?;
        Ok(())
    }

    // ---- Captain identity (L1a) ----

    pub async fn get_captain_identity_internal(&self) -> Result<CaptainIdentityRow, MonarchError> {
        self.conn
            .call(|conn| {
                conn.query_row(
                    "SELECT c.name, c.current_version, COALESCE(v.payload, ''), \
                     v.created_at, v.edit_note \
                     FROM captain c \
                     LEFT JOIN captain_identity_versions v ON v.id = c.current_version \
                     WHERE c.id = 1",
                    [],
                    |row| {
                        Ok(CaptainIdentityRow {
                            name: row.get(0)?,
                            current_version_id: row.get(1)?,
                            payload: row.get(2)?,
                            created_at: row.get(3)?,
                            edit_note: row.get(4)?,
                        })
                    },
                )
            })
            .await
            .map_err(MonarchError::from)
    }

    pub async fn upsert_captain_identity_internal(
        &self,
        name: &str,
        payload: &str,
        edit_note: Option<&str>,
    ) -> Result<(), MonarchError> {
        let name = name.to_string();
        let payload = payload.to_string();
        let edit_note = edit_note.map(|s| s.to_string());
        self.conn
            .call(move |conn| {
                let current_version_id: Option<i64> = conn
                    .query_row(
                        "SELECT current_version FROM captain WHERE id = 1",
                        [],
                        |row| row.get(0),
                    )
                    .ok()
                    .flatten();
                conn.execute(
                    "INSERT INTO captain_identity_versions \
                     (payload, created_at, supersedes_id, edit_note) \
                     VALUES (?1, strftime('%Y-%m-%dT%H:%M:%SZ','now'), ?2, ?3)",
                    params![payload, current_version_id, edit_note],
                )?;
                let new_version_id = conn.last_insert_rowid();
                conn.execute(
                    "UPDATE captain SET name = ?1, current_version = ?2 WHERE id = 1",
                    params![name, new_version_id],
                )?;
                Ok(())
            })
            .await
            .map_err(MonarchError::from)
    }

    pub async fn get_captain_identity_payload_internal(
        &self,
    ) -> Result<Option<String>, MonarchError> {
        self.conn
            .call(|conn| {
                let result = conn.query_row(
                    "SELECT v.payload FROM captain c \
                     LEFT JOIN captain_identity_versions v ON v.id = c.current_version \
                     WHERE c.id = 1",
                    [],
                    |row| row.get::<_, Option<String>>(0),
                );
                match result {
                    Ok(p) => Ok(p.filter(|s| !s.is_empty())),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                    Err(e) => Err(e),
                }
            })
            .await
            .map_err(MonarchError::from)
    }

    // ---- Shadow identity (L1b) ----

    pub async fn get_shadow_identity_internal(
        &self,
        agent_id: &str,
    ) -> Result<Option<ShadowIdentityRow>, MonarchError> {
        let agent_id = agent_id.to_string();
        self.conn
            .call(move |conn| {
                let result = conn.query_row(
                    "SELECT v.id, v.agent_id, v.payload, v.created_at, v.supersedes_id, v.edit_note \
                     FROM shadow_identity_versions v \
                     INNER JOIN agents a ON a.identity_version_id = v.id \
                     WHERE a.id = ?1",
                    params![agent_id],
                    |row| {
                        Ok(ShadowIdentityRow {
                            id: row.get(0)?,
                            agent_id: row.get(1)?,
                            payload: row.get(2)?,
                            created_at: row.get(3)?,
                            supersedes_id: row.get(4)?,
                            edit_note: row.get(5)?,
                        })
                    },
                );
                match result {
                    Ok(row) => Ok(Some(row)),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                    Err(e) => Err(e),
                }
            })
            .await
            .map_err(MonarchError::from)
    }

    pub async fn upsert_shadow_identity_internal(
        &self,
        agent_id: &str,
        payload: &str,
        edit_note: Option<&str>,
    ) -> Result<(), MonarchError> {
        let agent_id = agent_id.to_string();
        let payload = payload.to_string();
        let edit_note = edit_note.map(|s| s.to_string());
        self.conn
            .call(move |conn| {
                let current_version_id: Option<i64> = conn
                    .query_row(
                        "SELECT identity_version_id FROM agents WHERE id = ?1",
                        params![agent_id],
                        |row| row.get(0),
                    )
                    .ok()
                    .flatten();
                conn.execute(
                    "INSERT INTO shadow_identity_versions \
                     (agent_id, payload, created_at, supersedes_id, edit_note) \
                     VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%SZ','now'), ?3, ?4)",
                    params![agent_id, payload, current_version_id, edit_note],
                )?;
                let new_version_id = conn.last_insert_rowid();
                conn.execute(
                    "UPDATE agents SET identity_version_id = ?1, \
                     updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now') \
                     WHERE id = ?2",
                    params![new_version_id, agent_id],
                )?;
                Ok(())
            })
            .await
            .map_err(MonarchError::from)
    }

    pub async fn get_shadow_identity_payload_internal(
        &self,
        agent_id: &str,
    ) -> Result<Option<String>, MonarchError> {
        let agent_id = agent_id.to_string();
        self.conn
            .call(move |conn| {
                let result = conn.query_row(
                    "SELECT v.payload FROM shadow_identity_versions v \
                     INNER JOIN agents a ON a.identity_version_id = v.id \
                     WHERE a.id = ?1",
                    params![agent_id],
                    |row| row.get::<_, String>(0),
                );
                match result {
                    Ok(p) => Ok(if p.is_empty() { None } else { Some(p) }),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                    Err(e) => Err(e),
                }
            })
            .await
            .map_err(MonarchError::from)
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
/// MON-99: P2 memory row returned to frontend / used for retrieval.
pub struct MemoryRow {
    pub id: i64,
    pub agent_id: Option<String>,
    pub scope: String,
    pub project_id: Option<String>,
    pub parent_id: Option<i64>,
    pub layer: String,
    pub kind: Option<String>,
    pub title: String,
    pub summary: String,
    pub content: Option<String>,
    pub manual_override: bool,
    pub source_quest_id: Option<String>,
    pub source_session_id: Option<String>,
    pub source_events: Option<String>,
    pub file_refs: Option<String>,
    /// Embedding stored as raw little-endian f32 bytes (not serialized to frontend).
    #[serde(skip)]
    pub embedding: Option<Vec<u8>>,
    pub embedding_model_id: Option<String>,
    pub supersedes_id: Option<i64>,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub last_accessed_at: Option<String>,
    pub access_count: i32,
}

/// Payload for inserting a new memory. Does not include id, created_at, access_count.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct InsertMemoryPayload {
    pub agent_id: Option<String>,
    pub scope: String,
    pub project_id: Option<String>,
    pub parent_id: Option<i64>,
    pub layer: String,
    pub kind: Option<String>,
    pub title: String,
    pub summary: String,
    pub content: Option<String>,
    pub source_quest_id: Option<String>,
    pub source_session_id: Option<String>,
    pub source_events: Option<String>,
    pub file_refs: Option<String>,
    pub supersedes_id: Option<i64>,
}

/// MON-99: Row returned from `memory_keeper_runs`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KeeperRunRow {
    pub id: i64,
    pub agent_id: String,
    pub trigger: String,
    pub quest_id: Option<String>,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub tokens_input: Option<i64>,
    pub tokens_output: Option<i64>,
    pub model_id: Option<String>,
    pub output_summary: Option<String>,
    pub outcome: String,
}

/// MON-99: FTS5 search result — memory id + snippet.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FtsMemoryResult {
    pub memory_id: i64,
    pub rank: f64,
}

/// MON-98: Current captain identity (L1a). Returned by `get_captain_identity`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CaptainIdentityRow {
    pub name: String,
    pub current_version_id: Option<i64>,
    pub payload: String,
    pub created_at: Option<String>,
    pub edit_note: Option<String>,
}

/// MON-98: Current shadow identity version (L1b). Returned by `get_shadow_identity`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ShadowIdentityRow {
    pub id: i64,
    pub agent_id: String,
    pub payload: String,
    pub created_at: String,
    pub supersedes_id: Option<i64>,
    pub edit_note: Option<String>,
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
    pub scope: Option<String>,
    pub current_direction: Option<String>,
    pub rationale: Option<String>,
    pub status: String,
    pub grade: Option<String>,
    pub exec_hint: Option<String>,
    pub explore_fork_count: Option<i32>,
    pub assignee_shadow_id: Option<String>,
    pub fork_parent_id: Option<String>,
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
    pub parent_event_id: Option<String>,
    pub author: Option<String>,
    pub surface_override: Option<String>,
    pub payload_schema_version: i32,
    pub plan_item_id: Option<String>,
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
    pub scope: Option<String>,
    pub current_direction: Option<String>,
    pub rationale: Option<String>,
    pub fork_parent_id: Option<String>,
    pub status: Option<String>,
    pub grade: Option<String>,
    pub exec_hint: Option<String>,
    pub assignee_shadow_id: Option<String>,
    pub summary: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub abandoned_at: Option<String>,
}

/// P5 manual editor payload. This narrower path records semantic timeline
/// events for quest-level changes; generic `db_update_quest` remains available
/// for older callers that only need a direct row patch.
#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ManualQuestUpdatePayload {
    pub id: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub current_direction: Option<String>,
    #[serde(default)]
    pub rationale: Option<String>,
    #[serde(default)]
    pub grade: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub change_rationale: Option<String>,
    #[serde(default)]
    pub actor: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct QuestRefRow {
    pub id: String,
    pub quest_id: String,
    pub ref_type: String,
    pub label: Option<String>,
    pub target: String,
    pub metadata_json: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateQuestRefPayload {
    #[serde(default)]
    pub id: Option<String>,
    pub quest_id: String,
    pub ref_type: String,
    #[serde(default)]
    pub label: Option<String>,
    pub target: String,
    #[serde(default)]
    pub metadata_json: Option<String>,
    #[serde(default)]
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateQuestRefPayload {
    pub id: String,
    #[serde(default)]
    pub ref_type: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub metadata_json: Option<String>,
}

/// MON-119: one first-person quest report per quest. `agent_id` is
/// denormalized from `quest_nodes.assignee_shadow_id` at write time and
/// becomes NULL only when the source agent is deleted (`ON DELETE SET NULL`).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct QuestReportRow {
    pub id: String,
    pub quest_id: String,
    pub agent_id: Option<String>,
    pub payload: String,
    pub created_at: String,
    pub updated_at: String,
    pub distilled_by_keeper_run_id: Option<i64>,
}

/// MON-119: payload for upserting a quest report. `agent_id` is omitted —
/// the write helper resolves it from `quest_nodes.assignee_shadow_id`.
/// `payload` is opaque JSON in Slice A; Slice B's sidecar tool defines the
/// structured shape.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WriteQuestReportPayload {
    #[serde(default)]
    pub id: Option<String>,
    pub quest_id: String,
    pub payload: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ManualQuestEventPayload {
    pub quest_id: String,
    pub event_type: String,
    pub text: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub metadata_json: Option<String>,
    #[serde(default)]
    pub actor: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub surface_override: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RecordQuestEventPayload {
    pub quest_id: String,
    pub event_type: String,
    pub actor: Option<String>,
    pub payload_json: Option<String>,
    #[serde(default)]
    pub parent_event_id: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub surface_override: Option<String>,
    #[serde(default)]
    pub payload_schema_version: Option<i32>,
}

// P4b (MON-111): execution-plan item row + input shapes. Status mirrors
// the CHECK constraint in the schema. `created_by` covers both executor
// and human/chat-shadow authoring so manual edits in the UI carry their
// origin without schema churn later.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanItemRow {
    pub id: String,
    pub quest_id: String,
    pub parent_id: Option<String>,
    pub title: String,
    pub status: String,
    pub order_index: i32,
    pub created_by: String,
    pub rationale: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

/// Input row for `db_set_plan` / executor `set_plan`. `id` is optional —
/// server generates a UUID if omitted, which is the common case for newly
/// proposed items. Status defaults to `pending` when omitted; the only
/// reason a caller would supply it is when the new plan inherits a
/// previously active item without restarting it.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanItemInput {
    #[serde(default)]
    pub id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub rationale: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
}

/// Bulk replace payload — `db_set_plan(quest_id, items, created_by)`.
/// `created_by` defaults to `'captain'` when called from the manual UI
/// path; sidecar pass-through sets it to `'executor'`. The whole list is
/// authoritative — items not present (matched by id) are deleted.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SetPlanPayload {
    pub quest_id: String,
    pub items: Vec<PlanItemInput>,
    #[serde(default)]
    pub created_by: Option<String>,
    #[serde(default)]
    pub rationale: Option<String>,
}

/// Per-item edit payload. Only non-`None` fields are written. `id` is the
/// row's primary key.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePlanItemPayload {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub rationale: Option<String>,
    #[serde(default)]
    pub order_index: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AddPlanItemPayload {
    pub quest_id: String,
    pub title: String,
    #[serde(default)]
    pub rationale: Option<String>,
    /// Insert this item after the named item id, or at the end when
    /// omitted. Insertion shifts subsequent `order_index` values forward.
    #[serde(default)]
    pub after_item_id: Option<String>,
    #[serde(default)]
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkingMemoryCurrentAction {
    pub event_id: String,
    pub quest_id: String,
    pub intent: String,
    pub started_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkingMemoryRecentAction {
    pub event_id: String,
    pub quest_id: String,
    pub intent: String,
    pub outcome: String,
    pub completed_at: String,
    #[serde(default)]
    pub auto_closed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkingMemoryPayload {
    pub schema_version: i32,
    pub current_quest_id: Option<String>,
    pub current_quest_path: Vec<String>,
    pub current_action: Option<WorkingMemoryCurrentAction>,
    pub recent_actions: Vec<WorkingMemoryRecentAction>,
    pub updated_at: String,
    // P4b: plan slice. Pointers into `quest_plan_items` for the active
    // quest. Defaults preserve forward compatibility with v1 rows — old
    // payloads deserialize cleanly with both fields empty.
    #[serde(default)]
    pub active_plan_item_id: Option<String>,
    #[serde(default)]
    pub next_plan_item_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct QuestEventNotification {
    pub quest_id: String,
    pub event_id: String,
    pub event_type: String,
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
    pub async fn ensure_project_internal(
        &self,
        project: &ProjectRow,
    ) -> Result<String, MonarchError> {
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
    pub async fn update_agent_internal(
        &self,
        payload: &AgentUpdatePayload,
    ) -> Result<(), MonarchError> {
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
                        payload.id,
                        payload.name,
                        payload.shadow_name,
                        payload.shadow_title,
                        payload.shadow_grade,
                        payload.provider,
                        payload.model,
                        payload.thinking_level,
                        payload.cwd,
                        payload.avatar_type,
                        payload.avatar_path,
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
                    params![
                        agent_id,
                        session_id,
                        event_type,
                        data,
                        crate::util::chrono_now()
                    ],
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

    /// MON-99: Insert a new memory claim. Returns the new row id.
    pub async fn insert_memory_internal(
        &self,
        payload: InsertMemoryPayload,
        embedding: Option<Vec<u8>>,
        embedding_model_id: Option<String>,
    ) -> Result<i64, MonarchError> {
        Ok(self
            .conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO memories (
                        agent_id, scope, project_id, parent_id, layer, kind,
                        title, summary, content, source_quest_id, source_session_id,
                        source_events, file_refs, embedding, embedding_model_id,
                        supersedes_id
                    ) VALUES (
                        ?1, ?2, ?3, ?4, ?5, ?6,
                        ?7, ?8, ?9, ?10, ?11,
                        ?12, ?13, ?14, ?15,
                        ?16
                    )",
                    params![
                        payload.agent_id,
                        payload.scope,
                        payload.project_id,
                        payload.parent_id,
                        payload.layer,
                        payload.kind,
                        payload.title,
                        payload.summary,
                        payload.content,
                        payload.source_quest_id,
                        payload.source_session_id,
                        payload.source_events,
                        payload.file_refs,
                        embedding,
                        embedding_model_id,
                        payload.supersedes_id,
                    ],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await?)
    }

    /// MON-99: List memories for an agent (shadow-scoped, non-archived), ordered newest-first.
    pub async fn list_memories_for_agent_internal(
        &self,
        agent_id: &str,
    ) -> Result<Vec<MemoryRow>, MonarchError> {
        let agent_id = agent_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, agent_id, scope, project_id, parent_id, layer, kind,
                        title, summary, content, manual_override, source_quest_id,
                        source_session_id, source_events, file_refs, embedding_model_id,
                        supersedes_id, archived_at, created_at, last_accessed_at, access_count
                     FROM memories
                     WHERE agent_id = ?1 AND archived_at IS NULL
                     ORDER BY created_at DESC",
                )?;
                let rows = stmt
                    .query_map(params![agent_id], map_memory)?
                    .collect::<rusqlite::Result<Vec<_>>>();
                rows
            })
            .await?)
    }

    /// MON-99: Get a single memory by id.
    pub async fn get_memory_internal(&self, id: i64) -> Result<Option<MemoryRow>, MonarchError> {
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, agent_id, scope, project_id, parent_id, layer, kind,
                        title, summary, content, manual_override, source_quest_id,
                        source_session_id, source_events, file_refs, embedding_model_id,
                        supersedes_id, archived_at, created_at, last_accessed_at, access_count
                     FROM memories WHERE id = ?1",
                )?;
                let mut rows = stmt.query_map(params![id], map_memory)?;
                if let Some(row) = rows.next() {
                    Ok(Some(row?))
                } else {
                    Ok(None)
                }
            })
            .await?)
    }

    /// MON-99: FTS5 search — returns (memory_id, rank) ordered by relevance.
    pub async fn fts_search_memories_internal(
        &self,
        agent_id: &str,
        query: &str,
        limit: i64,
    ) -> Result<Vec<FtsMemoryResult>, MonarchError> {
        let agent_id = agent_id.to_string();
        let query = query.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                // FTS5 may not be available on all builds; return empty on error.
                let mut stmt = match conn.prepare(
                    "SELECT m.id, f.rank FROM memories_fts f
                     JOIN memories m ON m.id = f.rowid
                     WHERE memories_fts MATCH ?1 AND m.agent_id = ?2 AND m.archived_at IS NULL
                     ORDER BY f.rank LIMIT ?3",
                ) {
                    Ok(s) => s,
                    Err(_) => return Ok(vec![]),
                };
                let rows = stmt
                    .query_map(params![query, agent_id, limit], |row| {
                        Ok(FtsMemoryResult {
                            memory_id: row.get(0)?,
                            rank: row.get(1)?,
                        })
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>();
                rows
            })
            .await?)
    }

    /// MON-101: mark retrieved memories as accessed. Best-effort callers may
    /// pass an empty list; archived/missing rows are naturally ignored.
    pub async fn mark_memories_accessed_internal(
        &self,
        memory_ids: Vec<i64>,
    ) -> Result<(), MonarchError> {
        if memory_ids.is_empty() {
            return Ok(());
        }
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.transaction()?;
                for id in memory_ids {
                    tx.execute(
                        "UPDATE memories
                         SET access_count = access_count + 1,
                             last_accessed_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
                         WHERE id = ?1 AND archived_at IS NULL",
                        params![id],
                    )?;
                }
                tx.commit()?;
                Ok(())
            })
            .await?)
    }

    /// MON-99: Load embedding BLOBs for all non-archived memories of an agent.
    /// Returns (id, embedding_bytes) pairs for HNSW index rebuild.
    pub async fn load_embeddings_for_agent_internal(
        &self,
        agent_id: &str,
    ) -> Result<Vec<(i64, Vec<u8>)>, MonarchError> {
        let agent_id = agent_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, embedding FROM memories
                     WHERE agent_id = ?1 AND archived_at IS NULL AND embedding IS NOT NULL",
                )?;
                let rows = stmt
                    .query_map(params![agent_id], |row| {
                        Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>();
                rows
            })
            .await?)
    }

    /// MON-99: Insert a Keeper run provenance row. Returns the new id.
    pub async fn insert_keeper_run_internal(
        &self,
        agent_id: &str,
        trigger: &str,
        quest_id: Option<&str>,
        model_id: &str,
    ) -> Result<i64, MonarchError> {
        let agent_id = agent_id.to_string();
        let trigger = trigger.to_string();
        let quest_id = quest_id.map(|s| s.to_string());
        let model_id = model_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO memory_keeper_runs (agent_id, trigger, quest_id, model_id, outcome)
                     VALUES (?1, ?2, ?3, ?4, 'pending')",
                    params![agent_id, trigger, quest_id, model_id],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await?)
    }

    /// MON-100: lookup `agents.current_quest_id`. Returns None when the
    /// agent has no current quest, when the row is missing, or on read
    /// error (caller treats those identically — record no quest event).
    pub async fn get_agent_current_quest_id_internal(
        &self,
        agent_id: &str,
    ) -> Result<Option<String>, MonarchError> {
        let agent_id = agent_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let v: Option<String> = conn
                    .query_row(
                        "SELECT current_quest_id FROM agents WHERE id = ?1",
                        params![agent_id],
                        |row| row.get(0),
                    )
                    .ok()
                    .flatten();
                Ok(v)
            })
            .await?)
    }

    /// MON-105: create a root quest for a meaningful user turn and set it as
    /// the agent's current quest, but only if there is no active current
    /// quest. Returns `Some(new_id)` when a quest was created.
    pub async fn auto_create_current_quest_internal(
        &self,
        agent_id: &str,
        title: &str,
        description: Option<&str>,
    ) -> Result<Option<String>, MonarchError> {
        let agent_id = agent_id.to_string();
        let title = title.to_string();
        let description = description.map(|s| s.to_string());
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let existing: Option<(String, Option<String>)> = tx
                    .query_row(
                        "SELECT q.id, q.status
                         FROM agents a
                         LEFT JOIN quest_nodes q ON q.id = a.current_quest_id
                         WHERE a.id = ?1",
                        params![agent_id],
                        |row| {
                            Ok((
                                row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                                row.get::<_, Option<String>>(1)?,
                            ))
                        },
                    )
                    .ok();
                if let Some((id, status)) = existing {
                    let terminal = matches!(
                        status.as_deref(),
                        Some("done" | "verified" | "abandoned" | "superseded")
                    );
                    if !id.is_empty() && !terminal {
                        tx.commit()?;
                        return Ok(None);
                    }
                }

                let id = crate::util::uuid_v4_simple();
                let event_id = crate::util::uuid_v4_simple();
                let now = crate::util::chrono_now();
                tx.execute(
                    "INSERT INTO quest_nodes (
                        id, root_id, parent_id, title, description,
                        status, grade, exec_hint, assignee_shadow_id,
                        created_by, created_at, started_at
                    ) VALUES (?1, ?1, NULL, ?2, ?3, 'in_progress', 'C', 'in_context', ?4, 'monarch', ?5, ?5)",
                    params![id, title, description, agent_id, now],
                )?;
                let event_payload = serde_json::json!({
                    "from": null,
                    "to": "in_progress",
                    "autoCreated": true,
                })
                .to_string();
                tx.execute(
                    "INSERT INTO quest_events (id, quest_id, event_type, actor, payload_json, created_at)
                     VALUES (?1, ?2, 'status_change', 'monarch', ?3, ?4)",
                    params![event_id, id, event_payload, now],
                )?;
                tx.execute(
                    "UPDATE agents SET current_quest_id = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE id = ?2",
                    params![id, agent_id],
                )?;
                tx.commit()?;
                Ok(Some(id))
            })
            .await?)
    }

    /// MON-100: Most recent successful Keeper run for an agent, or None.
    /// Drives slice anchoring (we replay messages newer than this row's
    /// `completed_at`) and the synthesized scaffold's prior summary (its
    /// `output_summary`).
    pub async fn last_successful_keeper_run_internal(
        &self,
        agent_id: &str,
    ) -> Result<Option<KeeperRunRow>, MonarchError> {
        let agent_id = agent_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, agent_id, trigger, quest_id, started_at, completed_at,
                            tokens_input, tokens_output, model_id, output_summary, outcome
                     FROM memory_keeper_runs
                     WHERE agent_id = ?1 AND outcome = 'ok' AND completed_at IS NOT NULL
                     ORDER BY completed_at DESC LIMIT 1",
                )?;
                let mut rows = stmt.query_map(params![agent_id], map_keeper_run)?;
                if let Some(row) = rows.next() {
                    Ok(Some(row?))
                } else {
                    Ok(None)
                }
            })
            .await?)
    }

    /// MON-103: load one Keeper run by id so result persistence can use the
    /// run row's trigger / quest provenance instead of whatever quest happens
    /// to be current when the async model call returns.
    pub async fn get_keeper_run_internal(
        &self,
        run_id: i64,
    ) -> Result<Option<KeeperRunRow>, MonarchError> {
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, agent_id, trigger, quest_id, started_at, completed_at,
                            tokens_input, tokens_output, model_id, output_summary, outcome
                     FROM memory_keeper_runs
                     WHERE id = ?1",
                )?;
                let mut rows = stmt.query_map(params![run_id], map_keeper_run)?;
                if let Some(row) = rows.next() {
                    Ok(Some(row?))
                } else {
                    Ok(None)
                }
            })
            .await?)
    }

    /// MON-100: Messages across all of an agent's sessions newer than the
    /// supplied timestamp (NULL = all). Ordered ascending by timestamp so
    /// the rendered slice reads chronologically. Excludes the synthetic
    /// `toolResult` rows? — no: the Keeper benefits from seeing tool output
    /// inline so it can claim things like "Tool X returned Y", so we keep
    /// every role.
    pub async fn list_agent_messages_since_internal(
        &self,
        agent_id: &str,
        since: Option<&str>,
    ) -> Result<Vec<MessageRow>, MonarchError> {
        let agent_id = agent_id.to_string();
        let since = since.map(|s| s.to_string());
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT m.id, m.session_id, m.role, m.content, m.model, m.tokens, m.cost,
                            m.timestamp, m.duration_ms
                     FROM messages m JOIN sessions s ON m.session_id = s.id
                     WHERE s.agent_id = ?1 AND (?2 IS NULL OR m.timestamp > ?2)
                     ORDER BY m.timestamp ASC, m.id ASC",
                )?;
                let rows = stmt.query_map(params![agent_id, since], map_message)?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
            })
            .await?)
    }

    /// MON-100: Sum of `messages.tokens` across the agent's sessions newer
    /// than the last successful Keeper run. Used to seed
    /// `LiveAgentState.tokens_since_last_compaction` on Monarch restart so
    /// the trigger keeps working without requiring an in-memory counter to
    /// survive process death.
    pub async fn tokens_since_last_keeper_run_internal(
        &self,
        agent_id: &str,
    ) -> Result<i64, MonarchError> {
        let agent_id = agent_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let last: Option<String> = conn
                    .query_row(
                        "SELECT completed_at FROM memory_keeper_runs
                         WHERE agent_id = ?1 AND outcome = 'ok' AND completed_at IS NOT NULL
                         ORDER BY completed_at DESC LIMIT 1",
                        params![agent_id],
                        |row| row.get(0),
                    )
                    .ok();
                let sum: i64 = conn.query_row(
                    "SELECT COALESCE(SUM(m.tokens), 0) FROM messages m
                     JOIN sessions s ON m.session_id = s.id
                     WHERE s.agent_id = ?1 AND (?2 IS NULL OR m.timestamp > ?2)",
                    params![agent_id, last],
                    |row| row.get(0),
                )?;
                Ok(sum)
            })
            .await?)
    }

    /// MON-99: Mark a Keeper run as completed (ok | failed | partial).
    pub async fn complete_keeper_run_internal(
        &self,
        run_id: i64,
        outcome: &str,
        output_summary: Option<String>,
        tokens_input: Option<i64>,
        tokens_output: Option<i64>,
    ) -> Result<(), MonarchError> {
        let outcome = outcome.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE memory_keeper_runs SET
                        outcome = ?1, output_summary = ?2,
                        tokens_input = ?3, tokens_output = ?4,
                        completed_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
                     WHERE id = ?5",
                    params![outcome, output_summary, tokens_input, tokens_output, run_id],
                )?;
                Ok(())
            })
            .await?)
    }

    // Legacy wrapper kept so the old Tauri command still compiles.
    pub async fn get_memories_internal(
        &self,
        agent_id: Option<&str>,
        _layer: Option<&str>,
    ) -> Result<Vec<MemoryRow>, MonarchError> {
        if let Some(id) = agent_id {
            self.list_memories_for_agent_internal(id).await
        } else {
            Ok(vec![])
        }
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

    pub async fn set_ui_state_internal(&self, key: &str, value: &str) -> Result<(), MonarchError> {
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
        let id = payload
            .id
            .clone()
            .unwrap_or_else(crate::util::uuid_v4_simple);
        let status = payload
            .status
            .clone()
            .unwrap_or_else(|| "pending".to_string());
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
                push!(payload.scope, "scope");
                push!(payload.current_direction, "current_direction");
                push!(payload.rationale, "rationale");
                push!(payload.fork_parent_id, "fork_parent_id");
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
                let sql = format!("UPDATE quest_nodes SET {} WHERE id = ?", sets.join(", "));
                args.push(rusqlite::types::Value::Text(payload.id.clone()));
                let params_slice: Vec<&dyn rusqlite::ToSql> =
                    args.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
                conn.execute(&sql, params_slice.as_slice())?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    /// MON-103: when a quest closes, clear the agent pointer only if it
    /// still points at that quest. This lets the next meaningful prompt
    /// auto-create a fresh current quest without disturbing newer work.
    pub async fn clear_agent_current_quest_if_matches_internal(
        &self,
        agent_id: &str,
        quest_id: &str,
    ) -> Result<(), MonarchError> {
        let agent_id = agent_id.to_string();
        let quest_id = quest_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE agents
                     SET current_quest_id = NULL,
                         updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
                     WHERE id = ?1 AND current_quest_id = ?2",
                    params![agent_id, quest_id],
                )?;
                Ok(())
            })
            .await?)
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
                    "INSERT INTO quest_events (
                        id, quest_id, event_type, actor, payload_json, created_at,
                        parent_event_id, author, surface_override, payload_schema_version
                     )
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        id,
                        payload.quest_id,
                        payload.event_type,
                        payload.actor,
                        payload.payload_json,
                        now,
                        payload.parent_event_id,
                        payload.author,
                        payload.surface_override,
                        payload.payload_schema_version.unwrap_or(1),
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
                    "SELECT id, quest_id, event_type, actor, payload_json, created_at,
                            parent_event_id, author, surface_override, payload_schema_version,
                            plan_item_id
                     FROM quest_events WHERE quest_id = ?1 ORDER BY created_at ASC",
                )?;
                let rows = stmt
                    .query_map(params![quest_id], map_quest_event)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await?)
    }

    pub async fn update_quest_manual_internal(
        &self,
        payload: &ManualQuestUpdatePayload,
    ) -> Result<Vec<QuestEventNotification>, MonarchError> {
        let payload = payload.clone();
        self.conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let mut stmt = tx.prepare(QUEST_SELECT_SQL)?;
                let before = stmt.query_row(params![payload.id], map_quest)?;
                drop(stmt);

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
                push!(payload.status, "status");
                push!(payload.scope, "scope");
                push!(payload.current_direction, "current_direction");
                push!(payload.rationale, "rationale");
                push!(payload.grade, "grade");
                push!(payload.summary, "summary");
                if !sets.is_empty() {
                    let sql = format!("UPDATE quest_nodes SET {} WHERE id = ?", sets.join(", "));
                    args.push(rusqlite::types::Value::Text(payload.id.clone()));
                    let params_slice: Vec<&dyn rusqlite::ToSql> =
                        args.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
                    tx.execute(&sql, params_slice.as_slice())?;
                }

                let mut stmt = tx.prepare(QUEST_SELECT_SQL)?;
                let after = stmt.query_row(params![payload.id], map_quest)?;
                drop(stmt);

                let actor = payload.actor.unwrap_or_else(|| "monarch".to_string());
                let author = payload.author.unwrap_or_else(|| "captain".to_string());
                let change_rationale = payload.change_rationale;
                let now = crate::util::chrono_now();
                let mut notes = Vec::new();

                macro_rules! emit_change {
                    ($event_type:literal, $before:expr, $after:expr) => {
                        if $before != $after {
                            let event_id = crate::util::uuid_v4_simple();
                            let event_payload = serde_json::json!({
                                "from": $before,
                                "to": $after,
                                "rationale": change_rationale.clone(),
                            })
                            .to_string();
                            tx.execute(
                                "INSERT INTO quest_events (
                                    id, quest_id, event_type, actor, payload_json, created_at,
                                    parent_event_id, author, surface_override, payload_schema_version
                                 )
                                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, NULL, 1)",
                                params![
                                    event_id,
                                    after.id.clone(),
                                    $event_type,
                                    actor.clone(),
                                    event_payload,
                                    now,
                                    author.clone()
                                ],
                            )?;
                            notes.push(QuestEventNotification {
                                quest_id: after.id.clone(),
                                event_id,
                                event_type: $event_type.to_string(),
                            });
                        }
                    };
                }

                emit_change!("scope_change", before.scope, after.scope);
                emit_change!(
                    "direction_change",
                    before.current_direction,
                    after.current_direction
                );
                emit_change!("quest_rationale_change", before.rationale, after.rationale);
                emit_change!("grade_change", before.grade, after.grade);
                emit_change!("quest_summary_change", before.summary, after.summary);

                tx.commit()?;
                Ok(notes)
            })
            .await
            .map_err(MonarchError::from)
    }

    pub async fn record_manual_quest_event_internal(
        &self,
        payload: &ManualQuestEventPayload,
    ) -> Result<String, MonarchError> {
        let event_type = payload.event_type.as_str();
        if !matches!(
            event_type,
            "note" | "blocker" | "blocker_resolved" | "question" | "answer"
        ) {
            return Err(MonarchError::invalid_input(format!(
                "Unsupported manual quest event type: {}",
                event_type
            )));
        }
        let payload_json = serde_json::json!({
            "text": payload.text,
            "title": payload.title,
            "metadata": payload
                .metadata_json
                .as_deref()
                .and_then(|raw| serde_json::from_str::<Value>(raw).ok()),
        })
        .to_string();
        self.record_quest_event_internal(&RecordQuestEventPayload {
            quest_id: payload.quest_id.clone(),
            event_type: payload.event_type.clone(),
            actor: Some(
                payload
                    .actor
                    .clone()
                    .unwrap_or_else(|| "monarch".to_string()),
            ),
            payload_json: Some(payload_json),
            author: Some(
                payload
                    .author
                    .clone()
                    .unwrap_or_else(|| "captain".to_string()),
            ),
            surface_override: payload.surface_override.clone(),
            ..Default::default()
        })
        .await
    }

    pub async fn list_quest_refs_internal(
        &self,
        quest_id: &str,
    ) -> Result<Vec<QuestRefRow>, MonarchError> {
        let quest_id = quest_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, quest_id, ref_type, label, target, metadata_json,
                            created_by, created_at, updated_at
                     FROM quest_refs WHERE quest_id = ?1 ORDER BY created_at ASC",
                )?;
                let rows = stmt
                    .query_map(params![quest_id], map_quest_ref)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await?)
    }

    pub async fn get_quest_ref_internal(
        &self,
        id: &str,
    ) -> Result<Option<QuestRefRow>, MonarchError> {
        let id = id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, quest_id, ref_type, label, target, metadata_json,
                            created_by, created_at, updated_at
                     FROM quest_refs WHERE id = ?1",
                )?;
                let mut rows = stmt.query(params![id])?;
                if let Some(row) = rows.next()? {
                    Ok(Some(map_quest_ref(row)?))
                } else {
                    Ok(None)
                }
            })
            .await?)
    }

    pub async fn create_quest_ref_internal(
        &self,
        payload: &CreateQuestRefPayload,
    ) -> Result<String, MonarchError> {
        if payload.ref_type.trim().is_empty() {
            return Err(MonarchError::invalid_input("refType required"));
        }
        if payload.target.trim().is_empty() {
            return Err(MonarchError::invalid_input("target required"));
        }
        let payload = payload.clone();
        let id = payload
            .id
            .clone()
            .unwrap_or_else(crate::util::uuid_v4_simple);
        let id_for_return = id.clone();
        let created_by = payload.created_by.unwrap_or_else(|| "captain".to_string());
        let now = crate::util::chrono_now();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO quest_refs (
                        id, quest_id, ref_type, label, target, metadata_json,
                        created_by, created_at, updated_at
                     )
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                    params![
                        id,
                        payload.quest_id,
                        payload.ref_type,
                        payload.label,
                        payload.target,
                        payload.metadata_json,
                        created_by,
                        now
                    ],
                )?;
                Ok(())
            })
            .await?;
        Ok(id_for_return)
    }

    pub async fn update_quest_ref_internal(
        &self,
        payload: &UpdateQuestRefPayload,
    ) -> Result<(), MonarchError> {
        let payload = payload.clone();
        self.conn
            .call(move |conn| {
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
                push!(payload.ref_type, "ref_type");
                push!(payload.label, "label");
                push!(payload.target, "target");
                push!(payload.metadata_json, "metadata_json");
                if sets.is_empty() {
                    return Ok(());
                }
                sets.push("updated_at = ?");
                args.push(rusqlite::types::Value::Text(crate::util::chrono_now()));
                let sql = format!("UPDATE quest_refs SET {} WHERE id = ?", sets.join(", "));
                args.push(rusqlite::types::Value::Text(payload.id.clone()));
                let params_slice: Vec<&dyn rusqlite::ToSql> =
                    args.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
                conn.execute(&sql, params_slice.as_slice())?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn delete_quest_ref_internal(&self, id: &str) -> Result<(), MonarchError> {
        let id = id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute("DELETE FROM quest_refs WHERE id = ?1", params![id])?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    /// MON-119: upsert a first-person quest report. `UNIQUE(quest_id)` means
    /// each call either inserts a fresh row or replaces the existing payload
    /// (revisions). `agent_id` is resolved server-side from the quest's
    /// `assignee_shadow_id` so callers never have to pass it; this keeps the
    /// denormalized column honest. Returns the row id (existing id on
    /// conflict).
    pub async fn upsert_quest_report_internal(
        &self,
        payload: &WriteQuestReportPayload,
    ) -> Result<String, MonarchError> {
        if payload.quest_id.trim().is_empty() {
            return Err(MonarchError::invalid_input("questId required"));
        }
        if payload.payload.trim().is_empty() {
            return Err(MonarchError::invalid_input("payload required"));
        }
        let payload = payload.clone();
        let provided_id = payload
            .id
            .clone()
            .unwrap_or_else(crate::util::uuid_v4_simple);
        let now = crate::util::chrono_now();
        Ok(self
            .conn
            .call(move |conn| {
                let agent_id: Option<String> = conn
                    .query_row(
                        "SELECT assignee_shadow_id FROM quest_nodes WHERE id = ?1",
                        params![payload.quest_id],
                        |row| row.get(0),
                    )
                    .map_err(|e| match e {
                        rusqlite::Error::QueryReturnedNoRows => {
                            rusqlite::Error::SqliteFailure(
                                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CONSTRAINT),
                                Some(format!("quest_nodes row not found for {}", payload.quest_id)),
                            )
                        }
                        other => other,
                    })?;
                // Try INSERT first; on UNIQUE(quest_id) conflict, update the
                // existing row's payload/agent_id/updated_at and return its id.
                let inserted = conn.execute(
                    "INSERT INTO quest_reports (
                        id, quest_id, agent_id, payload, created_at, updated_at
                     )
                     VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                     ON CONFLICT(quest_id) DO UPDATE SET
                        payload = excluded.payload,
                        agent_id = excluded.agent_id,
                        updated_at = excluded.updated_at",
                    params![
                        provided_id,
                        payload.quest_id,
                        agent_id,
                        payload.payload,
                        now,
                    ],
                )?;
                debug_assert!(inserted == 1);
                let id: String = conn.query_row(
                    "SELECT id FROM quest_reports WHERE quest_id = ?1",
                    params![payload.quest_id],
                    |row| row.get(0),
                )?;
                Ok(id)
            })
            .await?)
    }

    /// MON-119: fetch the single report for a quest (or None).
    pub async fn get_quest_report_by_quest_internal(
        &self,
        quest_id: &str,
    ) -> Result<Option<QuestReportRow>, MonarchError> {
        let quest_id = quest_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, quest_id, agent_id, payload, created_at, updated_at,
                            distilled_by_keeper_run_id
                     FROM quest_reports WHERE quest_id = ?1",
                )?;
                let mut rows = stmt.query(params![quest_id])?;
                if let Some(row) = rows.next()? {
                    Ok(Some(map_quest_report(row)?))
                } else {
                    Ok(None)
                }
            })
            .await?)
    }

    /// MON-119: list every report written by a specific agent, newest first.
    /// Justifies the denormalized `agent_id` column — a JOIN through
    /// `quest_nodes` would be slower and stop working after agent archive.
    pub async fn list_quest_reports_for_agent_internal(
        &self,
        agent_id: &str,
    ) -> Result<Vec<QuestReportRow>, MonarchError> {
        let agent_id = agent_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, quest_id, agent_id, payload, created_at, updated_at,
                            distilled_by_keeper_run_id
                     FROM quest_reports WHERE agent_id = ?1
                     ORDER BY created_at DESC",
                )?;
                let rows = stmt
                    .query_map(params![agent_id], map_quest_report)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await?)
    }

    pub async fn get_working_memory_internal(
        &self,
        agent_id: &str,
    ) -> Result<Option<WorkingMemoryPayload>, MonarchError> {
        let agent_id = agent_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let payload: Option<String> = conn
                    .query_row(
                        "SELECT payload_json FROM agent_working_memory WHERE agent_id = ?1",
                        params![agent_id],
                        |row| row.get(0),
                    )
                    .ok();
                Ok(payload.and_then(|p| serde_json::from_str(&p).ok()))
            })
            .await?)
    }

    pub async fn record_action_transition_internal(
        &self,
        agent_id: &str,
        quest_id: &str,
        intent: &str,
        previous_outcome: Option<&str>,
    ) -> Result<Vec<QuestEventNotification>, MonarchError> {
        let agent_id = agent_id.to_string();
        let quest_id = quest_id.to_string();
        let intent = intent.to_string();
        let previous_outcome = previous_outcome.map(str::to_string);
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let now = crate::util::chrono_now();
                let mut notes = Vec::new();
                let mut wm = load_working_memory_tx(&tx, &agent_id)
                    .unwrap_or_else(|| empty_working_memory(&quest_id, &now));

                if let Some(current) = wm.current_action.clone() {
                    let (outcome, auto_closed, reason) = match previous_outcome.as_deref() {
                        Some(o) if !o.trim().is_empty() => (o.trim().to_string(), false, None),
                        _ => (
                            "Moved on before recording an outcome.".to_string(),
                            true,
                            Some("new_action_started".to_string()),
                        ),
                    };
                    close_action_tx(
                        &tx,
                        &agent_id,
                        &mut wm,
                        &current,
                        &outcome,
                        auto_closed,
                        reason,
                        &now,
                        &mut notes,
                    )?;
                }

                let event_id = crate::util::uuid_v4_simple();
                let payload = serde_json::json!({
                    "intent": intent,
                    "status": "active",
                    "started_at": now,
                })
                .to_string();
                // P4b: stamp plan_item_id from the L2 plan slice so timeline
                // rendering can group consecutive actions under their plan
                // item without a join through L2. The slice may have shifted
                // since the previous action — recompute against the live
                // table here, not the loaded L2 snapshot.
                let plan_item_id = recompute_plan_slice_tx(&tx, &quest_id)
                    .ok()
                    .and_then(|(active, _)| active);
                tx.execute(
                    "INSERT INTO quest_events (
                        id, quest_id, event_type, actor, payload_json, created_at,
                        parent_event_id, author, surface_override, payload_schema_version,
                        plan_item_id
                     )
                     VALUES (?1, ?2, 'coherent_action', ?3, ?4, ?5, NULL, 'executor', NULL, 1, ?6)",
                    params![event_id, quest_id, agent_id, payload, now, plan_item_id],
                )?;
                wm.current_quest_id = Some(quest_id.clone());
                wm.current_quest_path = quest_path_tx(&tx, &quest_id);
                wm.current_action = Some(WorkingMemoryCurrentAction {
                    event_id: event_id.clone(),
                    quest_id: quest_id.clone(),
                    intent,
                    started_at: now.clone(),
                });
                wm.updated_at = now.clone();
                save_working_memory_tx(&tx, &agent_id, &wm)?;
                notes.push(QuestEventNotification {
                    quest_id,
                    event_id,
                    event_type: "coherent_action".to_string(),
                });
                tx.commit()?;
                Ok(notes)
            })
            .await?)
    }

    pub async fn complete_action_internal(
        &self,
        agent_id: &str,
        outcome: &str,
    ) -> Result<Vec<QuestEventNotification>, MonarchError> {
        let agent_id = agent_id.to_string();
        let outcome = outcome.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let now = crate::util::chrono_now();
                let mut notes = Vec::new();
                let Some(mut wm) = load_working_memory_tx(&tx, &agent_id) else {
                    tx.commit()?;
                    return Ok(notes);
                };
                let Some(current) = wm.current_action.clone() else {
                    tx.commit()?;
                    return Ok(notes);
                };
                close_action_tx(
                    &tx,
                    &agent_id,
                    &mut wm,
                    &current,
                    outcome.trim(),
                    false,
                    None,
                    &now,
                    &mut notes,
                )?;
                wm.current_action = None;
                wm.updated_at = now;
                save_working_memory_tx(&tx, &agent_id, &wm)?;
                tx.commit()?;
                Ok(notes)
            })
            .await?)
    }

    pub async fn record_executor_decision_internal(
        &self,
        agent_id: &str,
        quest_id: &str,
        decision: &str,
        rationale: Option<&str>,
    ) -> Result<Vec<QuestEventNotification>, MonarchError> {
        let agent_id = agent_id.to_string();
        let quest_id = quest_id.to_string();
        let decision = decision.to_string();
        let rationale = rationale.map(str::to_string);
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let now = crate::util::chrono_now();
                let parent = load_working_memory_tx(&tx, &agent_id)
                    .and_then(|wm| wm.current_action.map(|a| a.event_id));
                let event_id = crate::util::uuid_v4_simple();
                let payload = serde_json::json!({
                    "decision": decision,
                    "rationale": rationale,
                })
                .to_string();
                tx.execute(
                    "INSERT INTO quest_events (
                        id, quest_id, event_type, actor, payload_json, created_at,
                        parent_event_id, author, surface_override, payload_schema_version
                     )
                     VALUES (?1, ?2, 'executor_decision', ?3, ?4, ?5, ?6, 'executor', NULL, 1)",
                    params![event_id, quest_id, agent_id, payload, now, parent],
                )?;
                tx.commit()?;
                Ok(vec![QuestEventNotification {
                    quest_id,
                    event_id,
                    event_type: "executor_decision".to_string(),
                }])
            })
            .await?)
    }

    pub async fn record_tool_call_start_internal(
        &self,
        agent_id: &str,
        quest_id: &str,
        tool_call_id: &str,
        tool_name: &str,
        args: Option<Value>,
    ) -> Result<Vec<QuestEventNotification>, MonarchError> {
        let agent_id = agent_id.to_string();
        let quest_id = quest_id.to_string();
        let tool_call_id = tool_call_id.to_string();
        let tool_name = tool_name.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let Some(wm) = load_working_memory_tx(&tx, &agent_id) else {
                    tx.commit()?;
                    return Ok(Vec::new());
                };
                let Some(parent) = wm.current_action.map(|a| a.event_id) else {
                    tx.commit()?;
                    return Ok(Vec::new());
                };
                let now = crate::util::chrono_now();
                let event_id = crate::util::uuid_v4_simple();
                let payload = serde_json::json!({
                    "tool_call_id": tool_call_id,
                    "tool_name": tool_name,
                    "args_preview": preview_value(args.as_ref()),
                    "status": "running",
                    "started_at": now,
                })
                .to_string();
                tx.execute(
                    "INSERT INTO quest_events (
                        id, quest_id, event_type, actor, payload_json, created_at,
                        parent_event_id, author, surface_override, payload_schema_version
                     )
                     VALUES (?1, ?2, 'tool_call', ?3, ?4, ?5, ?6, 'executor', NULL, 1)",
                    params![event_id, quest_id, agent_id, payload, now, parent],
                )?;
                tx.commit()?;
                Ok(vec![QuestEventNotification {
                    quest_id,
                    event_id,
                    event_type: "tool_call".to_string(),
                }])
            })
            .await?)
    }

    pub async fn record_tool_call_end_internal(
        &self,
        tool_call_id: &str,
        result: Option<Value>,
        is_error: bool,
        duration_ms: Option<i64>,
    ) -> Result<Vec<QuestEventNotification>, MonarchError> {
        let tool_call_id = tool_call_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let found: Option<(String, String, String)> = tx
                    .query_row(
                        "SELECT id, quest_id, payload_json
                         FROM quest_events
                         WHERE event_type = 'tool_call'
                           AND json_extract(payload_json, '$.tool_call_id') = ?1
                         ORDER BY created_at DESC
                         LIMIT 1",
                        params![tool_call_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .ok();
                let Some((event_id, quest_id, raw_payload)) = found else {
                    tx.commit()?;
                    return Ok(Vec::new());
                };
                let now = crate::util::chrono_now();
                let mut payload: Value =
                    serde_json::from_str(&raw_payload).unwrap_or_else(|_| serde_json::json!({}));
                if let Some(obj) = payload.as_object_mut() {
                    obj.insert(
                        "status".to_string(),
                        Value::String(if is_error { "error" } else { "done" }.to_string()),
                    );
                    obj.insert("is_error".to_string(), Value::Bool(is_error));
                    obj.insert(
                        "result_preview".to_string(),
                        Value::String(preview_value(result.as_ref())),
                    );
                    obj.insert("completed_at".to_string(), Value::String(now));
                    if let Some(duration_ms) = duration_ms {
                        obj.insert(
                            "duration_ms".to_string(),
                            Value::Number(serde_json::Number::from(duration_ms)),
                        );
                    }
                }
                tx.execute(
                    "UPDATE quest_events SET payload_json = ?1 WHERE id = ?2",
                    params![payload.to_string(), event_id],
                )?;
                tx.commit()?;
                Ok(vec![QuestEventNotification {
                    quest_id,
                    event_id,
                    event_type: "tool_call".to_string(),
                }])
            })
            .await?)
    }

    // ---- P4b (MON-111): Quest plan items ----

    /// Read all plan items for a quest, ordered by `order_index`. The
    /// frontend store keeps this list per quest and refreshes when
    /// `quest-event-{quest_id}` carries a `plan_*` event type.
    pub async fn list_plan_items_internal(
        &self,
        quest_id: &str,
    ) -> Result<Vec<PlanItemRow>, MonarchError> {
        let quest_id = quest_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, quest_id, parent_id, title, status, order_index,
                            created_by, rationale, created_at, updated_at, completed_at
                     FROM quest_plan_items
                     WHERE quest_id = ?1
                     ORDER BY order_index ASC",
                )?;
                let rows = stmt
                    .query_map(params![quest_id], map_plan_item)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await?)
    }

    pub async fn get_plan_item_internal(
        &self,
        item_id: &str,
    ) -> Result<Option<PlanItemRow>, MonarchError> {
        let item_id = item_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let row = conn
                    .query_row(
                        "SELECT id, quest_id, parent_id, title, status, order_index,
                                created_by, rationale, created_at, updated_at, completed_at
                         FROM quest_plan_items WHERE id = ?1",
                        params![item_id],
                        map_plan_item,
                    )
                    .ok();
                Ok(row)
            })
            .await?)
    }

    /// Bulk replace a quest's plan. Existing rows whose ids are in the
    /// payload are preserved (status untouched); rows missing from the
    /// payload are deleted; new rows arrive with `status='pending'`.
    /// Emits `plan_created` when the quest had no prior plan, otherwise
    /// `plan_changed`. The active assignee's L2 plan slice is recomputed
    /// and saved when the agent's `current_quest_id` matches.
    pub async fn set_plan_internal(
        &self,
        payload: &SetPlanPayload,
    ) -> Result<Vec<QuestEventNotification>, MonarchError> {
        let payload = payload.clone();
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let now = crate::util::chrono_now();
                let quest_id = payload.quest_id.clone();
                let created_by = payload
                    .created_by
                    .clone()
                    .unwrap_or_else(|| "captain".to_string());
                validate_plan_created_by(&created_by)?;

                let prior_count: i64 = tx
                    .query_row(
                        "SELECT COUNT(*) FROM quest_plan_items WHERE quest_id = ?1",
                        params![quest_id],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);

                // Resolve final ids upfront so we can delete any existing
                // row not in the new list (one statement, FK-friendly).
                let mut final_ids: Vec<String> = Vec::with_capacity(payload.items.len());
                for input in &payload.items {
                    let id = input.id.clone().unwrap_or_else(crate::util::uuid_v4_simple);
                    final_ids.push(id);
                }

                if !final_ids.is_empty() {
                    let placeholders = std::iter::repeat("?")
                        .take(final_ids.len())
                        .collect::<Vec<_>>()
                        .join(",");
                    let sql = format!(
                        "DELETE FROM quest_plan_items WHERE quest_id = ? AND id NOT IN ({})",
                        placeholders
                    );
                    let mut stmt = tx.prepare(&sql)?;
                    let mut bound: Vec<&dyn rusqlite::ToSql> =
                        Vec::with_capacity(1 + final_ids.len());
                    bound.push(&quest_id);
                    for id in &final_ids {
                        bound.push(id);
                    }
                    stmt.execute(rusqlite::params_from_iter(bound))?;
                } else {
                    tx.execute(
                        "DELETE FROM quest_plan_items WHERE quest_id = ?1",
                        params![quest_id],
                    )?;
                }

                for (idx, input) in payload.items.iter().enumerate() {
                    let id = &final_ids[idx];
                    let order_index = idx as i32;
                    let status = input
                        .status
                        .clone()
                        .unwrap_or_else(|| "pending".to_string());
                    validate_plan_status(&status)?;
                    tx.execute(
                        "INSERT INTO quest_plan_items
                            (id, quest_id, parent_id, title, status, order_index,
                             created_by, rationale, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
                         ON CONFLICT(id) DO UPDATE SET
                            title = excluded.title,
                            parent_id = excluded.parent_id,
                            rationale = excluded.rationale,
                            order_index = excluded.order_index,
                            updated_at = excluded.updated_at",
                        params![
                            id,
                            quest_id,
                            input.parent_id,
                            input.title.trim(),
                            status,
                            order_index,
                            created_by,
                            input.rationale,
                            now,
                        ],
                    )?;
                }

                let event_type = if prior_count == 0 {
                    "plan_created"
                } else {
                    "plan_changed"
                };
                let payload_json = serde_json::json!({
                    "item_ids": final_ids,
                    "rationale": payload.rationale,
                    "created_by": created_by,
                })
                .to_string();
                let event_id =
                    insert_plan_event_tx(&tx, &quest_id, event_type, None, &payload_json, &now)?;
                sync_plan_l2_tx(&tx, &quest_id, &now)?;
                tx.commit()?;
                Ok(vec![QuestEventNotification {
                    quest_id,
                    event_id,
                    event_type: event_type.to_string(),
                }])
            })
            .await?)
    }

    /// Append (or insert after a named item) a single new plan item. Emits
    /// `plan_changed`. Newly added items always start as `pending`.
    pub async fn add_plan_item_internal(
        &self,
        payload: &AddPlanItemPayload,
    ) -> Result<(String, Vec<QuestEventNotification>), MonarchError> {
        let payload = payload.clone();
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let now = crate::util::chrono_now();
                let id = crate::util::uuid_v4_simple();
                let created_by = payload
                    .created_by
                    .clone()
                    .unwrap_or_else(|| "captain".to_string());
                validate_plan_created_by(&created_by)?;
                let quest_id = payload.quest_id.clone();

                let order_index = if let Some(after_id) = payload.after_item_id.as_deref() {
                    let after_order: Option<i32> = tx
                        .query_row(
                            "SELECT order_index FROM quest_plan_items
                             WHERE id = ?1 AND quest_id = ?2",
                            params![after_id, quest_id],
                            |row| row.get(0),
                        )
                        .ok();
                    match after_order {
                        Some(o) => {
                            tx.execute(
                                "UPDATE quest_plan_items
                                 SET order_index = order_index + 1,
                                     updated_at = ?2
                                 WHERE quest_id = ?1 AND order_index > ?3",
                                params![quest_id, now, o],
                            )?;
                            o + 1
                        }
                        None => {
                            let max: Option<i32> = tx
                                .query_row(
                                    "SELECT MAX(order_index) FROM quest_plan_items
                                     WHERE quest_id = ?1",
                                    params![quest_id],
                                    |row| row.get(0),
                                )
                                .ok()
                                .flatten();
                            max.map(|m| m + 1).unwrap_or(0)
                        }
                    }
                } else {
                    let max: Option<i32> = tx
                        .query_row(
                            "SELECT MAX(order_index) FROM quest_plan_items WHERE quest_id = ?1",
                            params![quest_id],
                            |row| row.get(0),
                        )
                        .ok()
                        .flatten();
                    max.map(|m| m + 1).unwrap_or(0)
                };

                tx.execute(
                    "INSERT INTO quest_plan_items
                        (id, quest_id, parent_id, title, status, order_index,
                         created_by, rationale, created_at, updated_at)
                     VALUES (?1, ?2, NULL, ?3, 'pending', ?4, ?5, ?6, ?7, ?7)",
                    params![
                        id,
                        quest_id,
                        payload.title.trim(),
                        order_index,
                        created_by,
                        payload.rationale,
                        now,
                    ],
                )?;

                let payload_json = serde_json::json!({
                    "item_id": id,
                    "title": payload.title,
                    "after_item_id": payload.after_item_id,
                    "created_by": created_by,
                })
                .to_string();
                let event_id = insert_plan_event_tx(
                    &tx,
                    &quest_id,
                    "plan_changed",
                    None,
                    &payload_json,
                    &now,
                )?;
                sync_plan_l2_tx(&tx, &quest_id, &now)?;
                tx.commit()?;
                Ok((
                    id,
                    vec![QuestEventNotification {
                        quest_id,
                        event_id,
                        event_type: "plan_changed".to_string(),
                    }],
                ))
            })
            .await?)
    }

    /// Edit a single plan item's title / rationale / order_index. Emits
    /// `plan_changed` when something actually changed; no-op (empty
    /// notification list) otherwise.
    pub async fn update_plan_item_internal(
        &self,
        payload: &UpdatePlanItemPayload,
    ) -> Result<Vec<QuestEventNotification>, MonarchError> {
        let payload = payload.clone();
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let now = crate::util::chrono_now();
                let Some((quest_id, _, _)) = lookup_plan_item_tx(&tx, &payload.id)? else {
                    tx.commit()?;
                    return Ok(Vec::new());
                };
                let mut changed = false;
                if let Some(title) = payload.title.as_deref() {
                    tx.execute(
                        "UPDATE quest_plan_items SET title = ?1, updated_at = ?2 WHERE id = ?3",
                        params![title.trim(), now, payload.id],
                    )?;
                    changed = true;
                }
                if let Some(rationale) = &payload.rationale {
                    tx.execute(
                        "UPDATE quest_plan_items SET rationale = ?1, updated_at = ?2 WHERE id = ?3",
                        params![rationale, now, payload.id],
                    )?;
                    changed = true;
                }
                if let Some(new_order) = payload.order_index {
                    tx.execute(
                        "UPDATE quest_plan_items SET order_index = ?1, updated_at = ?2 WHERE id = ?3",
                        params![new_order, now, payload.id],
                    )?;
                    changed = true;
                }
                if !changed {
                    tx.commit()?;
                    return Ok(Vec::new());
                }
                let payload_json = serde_json::json!({
                    "item_id": payload.id,
                    "fields": {
                        "title": payload.title,
                        "rationale": payload.rationale,
                        "order_index": payload.order_index,
                    },
                })
                .to_string();
                let event_id = insert_plan_event_tx(
                    &tx,
                    &quest_id,
                    "plan_changed",
                    None,
                    &payload_json,
                    &now,
                )?;
                sync_plan_l2_tx(&tx, &quest_id, &now)?;
                tx.commit()?;
                Ok(vec![QuestEventNotification {
                    quest_id,
                    event_id,
                    event_type: "plan_changed".to_string(),
                }])
            })
            .await?)
    }

    pub async fn delete_plan_item_internal(
        &self,
        item_id: &str,
    ) -> Result<Vec<QuestEventNotification>, MonarchError> {
        let item_id = item_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let now = crate::util::chrono_now();
                let Some((quest_id, _, _)) = lookup_plan_item_tx(&tx, &item_id)? else {
                    tx.commit()?;
                    return Ok(Vec::new());
                };
                tx.execute(
                    "DELETE FROM quest_plan_items WHERE id = ?1",
                    params![item_id],
                )?;
                let payload_json = serde_json::json!({
                    "deleted_item_id": item_id,
                })
                .to_string();
                let event_id = insert_plan_event_tx(
                    &tx,
                    &quest_id,
                    "plan_changed",
                    None,
                    &payload_json,
                    &now,
                )?;
                sync_plan_l2_tx(&tx, &quest_id, &now)?;
                tx.commit()?;
                Ok(vec![QuestEventNotification {
                    quest_id,
                    event_id,
                    event_type: "plan_changed".to_string(),
                }])
            })
            .await?)
    }

    /// Mark a plan item active. At most one item per quest may be active —
    /// any sibling currently in `active` is silently reset to `pending`
    /// (the caller owns explicit completion / skip / block; the reset is
    /// a defensive invariant, not a status transition the captain sees).
    pub async fn start_plan_item_internal(
        &self,
        item_id: &str,
    ) -> Result<Vec<QuestEventNotification>, MonarchError> {
        let item_id = item_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let now = crate::util::chrono_now();
                let Some((quest_id, _, _)) = lookup_plan_item_tx(&tx, &item_id)? else {
                    tx.commit()?;
                    return Ok(Vec::new());
                };
                tx.execute(
                    "UPDATE quest_plan_items
                     SET status = 'pending', updated_at = ?2
                     WHERE quest_id = ?1 AND status = 'active' AND id <> ?3",
                    params![quest_id, now, item_id],
                )?;
                tx.execute(
                    "UPDATE quest_plan_items
                     SET status = 'active', updated_at = ?2, completed_at = NULL
                     WHERE id = ?1",
                    params![item_id, now],
                )?;
                let payload_json = serde_json::json!({ "item_id": item_id }).to_string();
                let event_id = insert_plan_event_tx(
                    &tx,
                    &quest_id,
                    "plan_item_started",
                    Some(&item_id),
                    &payload_json,
                    &now,
                )?;
                sync_plan_l2_tx(&tx, &quest_id, &now)?;
                tx.commit()?;
                Ok(vec![QuestEventNotification {
                    quest_id,
                    event_id,
                    event_type: "plan_item_started".to_string(),
                }])
            })
            .await?)
    }

    pub async fn complete_plan_item_internal(
        &self,
        item_id: &str,
        outcome: Option<&str>,
    ) -> Result<Vec<QuestEventNotification>, MonarchError> {
        let item_id = item_id.to_string();
        let outcome = outcome.map(str::to_string);
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let now = crate::util::chrono_now();
                let Some((quest_id, _, _)) = lookup_plan_item_tx(&tx, &item_id)? else {
                    tx.commit()?;
                    return Ok(Vec::new());
                };
                tx.execute(
                    "UPDATE quest_plan_items
                     SET status = 'completed', updated_at = ?2, completed_at = ?2
                     WHERE id = ?1",
                    params![item_id, now],
                )?;
                let payload_json = serde_json::json!({
                    "item_id": item_id,
                    "outcome": outcome,
                })
                .to_string();
                let event_id = insert_plan_event_tx(
                    &tx,
                    &quest_id,
                    "plan_item_completed",
                    Some(&item_id),
                    &payload_json,
                    &now,
                )?;
                sync_plan_l2_tx(&tx, &quest_id, &now)?;
                tx.commit()?;
                Ok(vec![QuestEventNotification {
                    quest_id,
                    event_id,
                    event_type: "plan_item_completed".to_string(),
                }])
            })
            .await?)
    }

    pub async fn skip_plan_item_internal(
        &self,
        item_id: &str,
        reason: Option<&str>,
    ) -> Result<Vec<QuestEventNotification>, MonarchError> {
        let item_id = item_id.to_string();
        let reason = reason.map(str::to_string);
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let now = crate::util::chrono_now();
                let Some((quest_id, _, _)) = lookup_plan_item_tx(&tx, &item_id)? else {
                    tx.commit()?;
                    return Ok(Vec::new());
                };
                tx.execute(
                    "UPDATE quest_plan_items
                     SET status = 'skipped', updated_at = ?2, completed_at = ?2
                     WHERE id = ?1",
                    params![item_id, now],
                )?;
                let payload_json = serde_json::json!({
                    "item_id": item_id,
                    "reason": reason,
                })
                .to_string();
                let event_id = insert_plan_event_tx(
                    &tx,
                    &quest_id,
                    "plan_item_skipped",
                    Some(&item_id),
                    &payload_json,
                    &now,
                )?;
                sync_plan_l2_tx(&tx, &quest_id, &now)?;
                tx.commit()?;
                Ok(vec![QuestEventNotification {
                    quest_id,
                    event_id,
                    event_type: "plan_item_skipped".to_string(),
                }])
            })
            .await?)
    }

    /// Resolve the agent's current active plan item — the row whose
    /// `status = 'active'` on the agent's L2 `currentQuestId`. Used by the
    /// persist pipeline when a sidecar plan-lifecycle event arrives
    /// without an explicit item id (the executor tool implicitly targets
    /// the active item). Returns `None` if the agent's L2 has no current
    /// quest, or if no item is active on it.
    pub async fn get_active_plan_item_for_agent_internal(
        &self,
        agent_id: &str,
    ) -> Result<Option<String>, MonarchError> {
        let agent_id = agent_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let item: Option<String> = conn
                    .query_row(
                        "SELECT pi.id FROM quest_plan_items pi
                         INNER JOIN agent_working_memory w
                            ON json_extract(w.payload_json, '$.currentQuestId') = pi.quest_id
                         WHERE w.agent_id = ?1 AND pi.status = 'active'
                         ORDER BY pi.order_index ASC
                         LIMIT 1",
                        params![agent_id],
                        |row| row.get(0),
                    )
                    .ok();
                Ok(item)
            })
            .await?)
    }

    pub async fn block_plan_item_internal(
        &self,
        item_id: &str,
        reason: &str,
    ) -> Result<Vec<QuestEventNotification>, MonarchError> {
        let item_id = item_id.to_string();
        let reason = reason.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let now = crate::util::chrono_now();
                let Some((quest_id, _, _)) = lookup_plan_item_tx(&tx, &item_id)? else {
                    tx.commit()?;
                    return Ok(Vec::new());
                };
                tx.execute(
                    "UPDATE quest_plan_items
                     SET status = 'blocked', updated_at = ?2
                     WHERE id = ?1",
                    params![item_id, now],
                )?;
                let payload_json = serde_json::json!({
                    "item_id": item_id,
                    "reason": reason,
                })
                .to_string();
                let event_id = insert_plan_event_tx(
                    &tx,
                    &quest_id,
                    "plan_item_blocked",
                    Some(&item_id),
                    &payload_json,
                    &now,
                )?;
                sync_plan_l2_tx(&tx, &quest_id, &now)?;
                tx.commit()?;
                Ok(vec![QuestEventNotification {
                    quest_id,
                    event_id,
                    event_type: "plan_item_blocked".to_string(),
                }])
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
    id, root_id, parent_id, title, description, scope, current_direction, \
    rationale, status, grade, exec_hint, explore_fork_count, assignee_shadow_id, \
    fork_parent_id, worktree_path, branch_name, \
    base_branch, branched_from_id, superseded_by_id, created_by, created_at, \
    started_at, completed_at, abandoned_at, estimated_tokens, actual_tokens, \
    estimated_duration_ms, actual_duration_ms, summary FROM quest_nodes";

const QUEST_SELECT_SQL: &str = "SELECT \
    id, root_id, parent_id, title, description, scope, current_direction, \
    rationale, status, grade, exec_hint, explore_fork_count, assignee_shadow_id, \
    fork_parent_id, worktree_path, branch_name, \
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
            "Read" | "Grep" | "Glob" | "LS" | "ListDir" | "Search" | "WebSearch" | "WebFetch"
            | "NotebookRead" => scores[1] += count,
            // Devops tools
            "Bash" => {
                // Bash is ambiguous — split across coding/devops
                scores[0] += count * 0.5;
                scores[4] += count * 0.5;
            }
            // Agent/communication tools
            "Agent" | "SendMessage" | "AskUser" | "AskUserQuestion" => scores[9] += count,
            // Task/planning tools
            "TaskCreate" | "TaskUpdate" | "TaskList" | "TaskGet" | "TodoWrite" | "TodoRead"
            | "EnterPlanMode" | "ExitPlanMode" => scores[0] += count * 0.5,
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
        scope: row
            .get::<_, Option<String>>(2)?
            .unwrap_or_else(|| "self".into()),
        project_id: row.get(3)?,
        parent_id: row.get(4)?,
        layer: row
            .get::<_, Option<String>>(5)?
            .unwrap_or_else(|| "leaf".into()),
        kind: row.get(6)?,
        title: row.get::<_, Option<String>>(7)?.unwrap_or_default(),
        summary: row.get::<_, Option<String>>(8)?.unwrap_or_default(),
        content: row.get(9)?,
        manual_override: row.get::<_, i64>(10).unwrap_or(0) != 0,
        source_quest_id: row.get(11)?,
        source_session_id: row.get(12)?,
        source_events: row.get(13)?,
        file_refs: row.get(14)?,
        embedding: None,
        embedding_model_id: row.get(15)?,
        supersedes_id: row.get(16)?,
        archived_at: row.get(17)?,
        created_at: row.get(18)?,
        last_accessed_at: row.get(19)?,
        access_count: row.get(20)?,
    })
}

fn map_keeper_run(row: &Row<'_>) -> rusqlite::Result<KeeperRunRow> {
    Ok(KeeperRunRow {
        id: row.get(0)?,
        agent_id: row.get(1)?,
        trigger: row.get(2)?,
        quest_id: row.get(3)?,
        started_at: row.get(4)?,
        completed_at: row.get(5)?,
        tokens_input: row.get(6)?,
        tokens_output: row.get(7)?,
        model_id: row.get(8)?,
        output_summary: row.get(9)?,
        outcome: row.get(10)?,
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
        scope: row.get(5)?,
        current_direction: row.get(6)?,
        rationale: row.get(7)?,
        status: row.get(8)?,
        grade: row.get(9)?,
        exec_hint: row.get(10)?,
        explore_fork_count: row.get(11)?,
        assignee_shadow_id: row.get(12)?,
        fork_parent_id: row.get(13)?,
        worktree_path: row.get(14)?,
        branch_name: row.get(15)?,
        base_branch: row.get(16)?,
        branched_from_id: row.get(17)?,
        superseded_by_id: row.get(18)?,
        created_by: row.get(19)?,
        created_at: row.get(20)?,
        started_at: row.get(21)?,
        completed_at: row.get(22)?,
        abandoned_at: row.get(23)?,
        estimated_tokens: row.get(24)?,
        actual_tokens: row.get(25)?,
        estimated_duration_ms: row.get(26)?,
        actual_duration_ms: row.get(27)?,
        summary: row.get(28)?,
    })
}

fn map_quest_ref(row: &Row<'_>) -> rusqlite::Result<QuestRefRow> {
    Ok(QuestRefRow {
        id: row.get(0)?,
        quest_id: row.get(1)?,
        ref_type: row.get(2)?,
        label: row.get(3)?,
        target: row.get(4)?,
        metadata_json: row.get(5)?,
        created_by: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn map_quest_report(row: &Row<'_>) -> rusqlite::Result<QuestReportRow> {
    Ok(QuestReportRow {
        id: row.get(0)?,
        quest_id: row.get(1)?,
        agent_id: row.get(2)?,
        payload: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        distilled_by_keeper_run_id: row.get(6)?,
    })
}

fn empty_working_memory(current_quest_id: &str, now: &str) -> WorkingMemoryPayload {
    WorkingMemoryPayload {
        schema_version: 2,
        current_quest_id: Some(current_quest_id.to_string()),
        current_quest_path: Vec::new(),
        current_action: None,
        recent_actions: Vec::new(),
        updated_at: now.to_string(),
        active_plan_item_id: None,
        next_plan_item_ids: Vec::new(),
    }
}

fn load_working_memory_tx(
    tx: &rusqlite::Transaction<'_>,
    agent_id: &str,
) -> Option<WorkingMemoryPayload> {
    let payload: String = tx
        .query_row(
            "SELECT payload_json FROM agent_working_memory WHERE agent_id = ?1",
            params![agent_id],
            |row| row.get(0),
        )
        .ok()?;
    serde_json::from_str(&payload).ok()
}

fn save_working_memory_tx(
    tx: &rusqlite::Transaction<'_>,
    agent_id: &str,
    wm: &WorkingMemoryPayload,
) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO agent_working_memory (agent_id, payload_json, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(agent_id) DO UPDATE SET
            payload_json = excluded.payload_json,
            updated_at = excluded.updated_at",
        params![
            agent_id,
            serde_json::to_string(wm).unwrap_or_default(),
            wm.updated_at
        ],
    )?;
    Ok(())
}

fn quest_path_tx(tx: &rusqlite::Transaction<'_>, quest_id: &str) -> Vec<String> {
    let mut path = Vec::new();
    let mut current = Some(quest_id.to_string());
    while let Some(id) = current {
        let row: Option<(String, Option<String>)> = tx
            .query_row(
                "SELECT title, parent_id FROM quest_nodes WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();
        let Some((title, parent_id)) = row else {
            break;
        };
        path.push(title);
        current = parent_id;
    }
    path.reverse();
    path
}

#[allow(clippy::too_many_arguments)]
fn close_action_tx(
    tx: &rusqlite::Transaction<'_>,
    agent_id: &str,
    wm: &mut WorkingMemoryPayload,
    current: &WorkingMemoryCurrentAction,
    outcome: &str,
    auto_closed: bool,
    auto_closed_reason: Option<String>,
    completed_at: &str,
    notes: &mut Vec<QuestEventNotification>,
) -> rusqlite::Result<()> {
    let status = if auto_closed {
        "auto_closed"
    } else {
        "completed"
    };
    let mut parent_payload = serde_json::json!({
        "intent": current.intent,
        "status": status,
        "started_at": current.started_at,
        "completed_at": completed_at,
        "outcome": outcome,
    });
    if let Some(obj) = parent_payload.as_object_mut() {
        if auto_closed {
            obj.insert("auto_closed".to_string(), Value::Bool(true));
        }
        if let Some(reason) = auto_closed_reason.clone() {
            obj.insert("auto_closed_reason".to_string(), Value::String(reason));
        }
    }
    tx.execute(
        "UPDATE quest_events SET payload_json = ?1 WHERE id = ?2",
        params![parent_payload.to_string(), current.event_id],
    )?;

    let outcome_event_id = crate::util::uuid_v4_simple();
    let outcome_payload = serde_json::json!({
        "outcome": outcome,
        "auto_closed": auto_closed,
        "auto_closed_reason": auto_closed_reason,
    })
    .to_string();
    tx.execute(
        "INSERT INTO quest_events (
            id, quest_id, event_type, actor, payload_json, created_at,
            parent_event_id, author, surface_override, payload_schema_version
         )
         VALUES (?1, ?2, 'action_outcome', ?3, ?4, ?5, ?6, 'executor', NULL, 1)",
        params![
            outcome_event_id,
            current.quest_id,
            agent_id,
            outcome_payload,
            completed_at,
            current.event_id
        ],
    )?;

    wm.recent_actions.push(WorkingMemoryRecentAction {
        event_id: current.event_id.clone(),
        quest_id: current.quest_id.clone(),
        intent: current.intent.clone(),
        outcome: outcome.to_string(),
        completed_at: completed_at.to_string(),
        auto_closed: auto_closed.then_some(true),
    });
    if wm.recent_actions.len() > 10 {
        let overflow = wm.recent_actions.len() - 10;
        wm.recent_actions.drain(0..overflow);
    }
    notes.push(QuestEventNotification {
        quest_id: current.quest_id.clone(),
        event_id: current.event_id.clone(),
        event_type: "coherent_action".to_string(),
    });
    notes.push(QuestEventNotification {
        quest_id: current.quest_id.clone(),
        event_id: outcome_event_id,
        event_type: "action_outcome".to_string(),
    });
    Ok(())
}

// ---- P4b plan helpers ----

fn validate_plan_status(status: &str) -> rusqlite::Result<()> {
    match status {
        "pending" | "active" | "completed" | "skipped" | "blocked" => Ok(()),
        other => Err(rusqlite::Error::ToSqlConversionFailure(
            format!("invalid plan status: {}", other).into(),
        )),
    }
}

fn validate_plan_created_by(created_by: &str) -> rusqlite::Result<()> {
    match created_by {
        "executor" | "chat_shadow" | "captain" | "architect" | "monarch" => Ok(()),
        other => Err(rusqlite::Error::ToSqlConversionFailure(
            format!("invalid plan created_by: {}", other).into(),
        )),
    }
}

/// Look up the row's `(quest_id, status, agent_id_of_quest_assignee)` for
/// a plan item id. Returns `None` if the item has been deleted.
fn lookup_plan_item_tx(
    tx: &rusqlite::Transaction<'_>,
    item_id: &str,
) -> rusqlite::Result<Option<(String, String, Option<String>)>> {
    let row = tx
        .query_row(
            "SELECT pi.quest_id, pi.status, q.assignee_shadow_id
             FROM quest_plan_items pi
             INNER JOIN quest_nodes q ON q.id = pi.quest_id
             WHERE pi.id = ?1",
            params![item_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .ok();
    Ok(row)
}

fn insert_plan_event_tx(
    tx: &rusqlite::Transaction<'_>,
    quest_id: &str,
    event_type: &str,
    plan_item_id: Option<&str>,
    payload_json: &str,
    now: &str,
) -> rusqlite::Result<String> {
    let event_id = crate::util::uuid_v4_simple();
    tx.execute(
        "INSERT INTO quest_events (
            id, quest_id, event_type, actor, payload_json, created_at,
            parent_event_id, author, surface_override, payload_schema_version,
            plan_item_id
         )
         VALUES (?1, ?2, ?3, NULL, ?4, ?5, NULL, 'executor', NULL, 1, ?6)",
        params![
            event_id,
            quest_id,
            event_type,
            payload_json,
            now,
            plan_item_id
        ],
    )?;
    Ok(event_id)
}

/// Recompute the plan slice for a quest: the active item id (if any) and
/// up to three pending items in order. Used by `sync_plan_l2_tx` and by
/// the read path that surfaces the slice into Agent View.
fn recompute_plan_slice_tx(
    tx: &rusqlite::Transaction<'_>,
    quest_id: &str,
) -> rusqlite::Result<(Option<String>, Vec<String>)> {
    let active: Option<String> = tx
        .query_row(
            "SELECT id FROM quest_plan_items
             WHERE quest_id = ?1 AND status = 'active'
             ORDER BY order_index ASC
             LIMIT 1",
            params![quest_id],
            |row| row.get(0),
        )
        .ok();
    let mut next = Vec::with_capacity(3);
    let mut stmt = tx.prepare(
        "SELECT id FROM quest_plan_items
         WHERE quest_id = ?1 AND status = 'pending'
         ORDER BY order_index ASC
         LIMIT 3",
    )?;
    let mut rows = stmt.query(params![quest_id])?;
    while let Some(row) = rows.next()? {
        next.push(row.get(0)?);
    }
    Ok((active, next))
}

/// If any agent's L2 currently points at this quest, recompute its plan
/// slice and write it back. We filter by the L2 payload's own
/// `currentQuestId` (not `agents.current_quest_id`) because action
/// transitions update L2 directly and the column-side pointer can lag —
/// L2 is the authoritative live state for which quest a shadow is on.
fn sync_plan_l2_tx(
    tx: &rusqlite::Transaction<'_>,
    quest_id: &str,
    now: &str,
) -> rusqlite::Result<()> {
    let agent_ids: Vec<String> = {
        let mut stmt = tx.prepare(
            "SELECT agent_id FROM agent_working_memory
             WHERE json_extract(payload_json, '$.currentQuestId') = ?1",
        )?;
        let rows = stmt
            .query_map(params![quest_id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    if agent_ids.is_empty() {
        return Ok(());
    }
    let (active, next) = recompute_plan_slice_tx(tx, quest_id)?;
    for agent_id in agent_ids {
        let Some(mut wm) = load_working_memory_tx(tx, &agent_id) else {
            continue;
        };
        if wm.current_quest_id.as_deref() != Some(quest_id) {
            continue;
        }
        wm.active_plan_item_id = active.clone();
        wm.next_plan_item_ids = next.clone();
        wm.updated_at = now.to_string();
        save_working_memory_tx(tx, &agent_id, &wm)?;
    }
    Ok(())
}

fn map_plan_item(row: &Row<'_>) -> rusqlite::Result<PlanItemRow> {
    Ok(PlanItemRow {
        id: row.get(0)?,
        quest_id: row.get(1)?,
        parent_id: row.get(2)?,
        title: row.get(3)?,
        status: row.get(4)?,
        order_index: row.get(5)?,
        created_by: row.get(6)?,
        rationale: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        completed_at: row.get(10)?,
    })
}

fn preview_value(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    let raw = match value {
        Value::String(s) => s.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
    };
    let compact = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.len() <= 500 {
        compact
    } else {
        format!("{}...", compact.chars().take(497).collect::<String>())
    }
}

fn map_quest_event(row: &Row<'_>) -> rusqlite::Result<QuestEventRow> {
    Ok(QuestEventRow {
        id: row.get(0)?,
        quest_id: row.get(1)?,
        event_type: row.get(2)?,
        actor: row.get(3)?,
        payload_json: row.get(4)?,
        created_at: row.get(5)?,
        parent_event_id: row.get(6)?,
        author: row.get(7)?,
        surface_override: row.get(8)?,
        payload_schema_version: row.get(9)?,
        plan_item_id: row.get(10)?,
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
    db.get_agents_internal(include_archived.unwrap_or(false))
        .await
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

/// MON-99: List all non-archived memories for an agent (Memory Inspector v0).
#[tauri::command]
#[specta::specta]
pub async fn db_list_memories_for_agent(
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
) -> Result<Vec<MemoryRow>, MonarchError> {
    db.list_memories_for_agent_internal(&agent_id).await
}

/// MON-99: Get a single memory by id.
#[tauri::command]
#[specta::specta]
pub async fn db_get_memory(
    db: tauri::State<'_, Arc<Database>>,
    id: i64,
) -> Result<Option<MemoryRow>, MonarchError> {
    db.get_memory_internal(id).await
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
    let assignee = payload.assignee_shadow_id.clone();
    let id = db.create_quest_internal(&payload).await?;
    crate::agent::emit_event(
        &app,
        &agent_mgr.ws_broadcast,
        &format!("quest-created-{}", id),
        &serde_json::json!({ "id": id.clone() }).to_string(),
    );
    if let Some(agent_id) = assignee {
        crate::agent::emit_event(
            &app,
            &agent_mgr.ws_broadcast,
            &format!("quest-created-for-agent-{}", agent_id),
            &serde_json::json!({ "id": id, "agentId": agent_id }).to_string(),
        );
    }
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
    let before = db.get_quest_internal(&id).await?;
    db.update_quest_internal(&payload).await?;
    let after = db.get_quest_internal(&id).await?;
    crate::agent::emit_event(
        &app,
        &agent_mgr.ws_broadcast,
        &format!("quest-updated-{}", id),
        &serde_json::json!({ "id": id }).to_string(),
    );
    if let Some(after_quest) = after.as_ref() {
        if after_quest.root_id != after_quest.id {
            crate::agent::emit_event(
                &app,
                &agent_mgr.ws_broadcast,
                &format!("quest-updated-{}", after_quest.root_id),
                &serde_json::json!({ "id": after_quest.id, "rootId": after_quest.root_id })
                    .to_string(),
            );
        }
    }
    handle_quest_update_side_effects(&app, db.inner(), agent_mgr.inner(), before, after).await?;
    Ok(())
}

pub(crate) async fn handle_quest_update_side_effects(
    app: &tauri::AppHandle,
    db: &Arc<Database>,
    agent_mgr: &Arc<crate::agent::AgentManager>,
    before: Option<QuestRow>,
    after: Option<QuestRow>,
) -> Result<(), MonarchError> {
    let Some(after) = after else {
        return Ok(());
    };
    let before_status = before.as_ref().map(|q| q.status.as_str());
    if before_status == Some(after.status.as_str()) {
        return Ok(());
    }

    let event_payload = serde_json::json!({
        "from": before_status,
        "to": after.status.as_str(),
    })
    .to_string();
    let event_id = db
        .record_quest_event_internal(&RecordQuestEventPayload {
            quest_id: after.id.clone(),
            event_type: "status_change".to_string(),
            actor: Some("monarch".to_string()),
            payload_json: Some(event_payload),
            ..Default::default()
        })
        .await?;
    crate::agent::emit_event(
        app,
        &agent_mgr.ws_broadcast,
        &format!("quest-event-{}", after.id),
        &serde_json::json!({ "id": event_id, "eventType": "status_change" }).to_string(),
    );

    let transitioned_to_done = before_status != Some("done") && after.status == "done";
    if !transitioned_to_done {
        return Ok(());
    }

    let Some(agent_id) = after.assignee_shadow_id.clone() else {
        return Ok(());
    };
    db.clear_agent_current_quest_if_matches_internal(&agent_id, &after.id)
        .await?;
    let since = after
        .started_at
        .clone()
        .or_else(|| Some(after.created_at.clone()));
    if let Err(e) = agent_mgr
        .dispatch_keeper_run(
            db,
            &agent_id,
            crate::agent::KeeperRunTrigger::QuestClose {
                quest_id: after.id.clone(),
                since,
            },
        )
        .await
    {
        eprintln!(
            "[monarch] quest-close keeper dispatch failed for {} quest {}: {:?}",
            agent_id, after.id, e
        );
    }
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

#[tauri::command]
#[specta::specta]
pub async fn db_update_quest_manual(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    payload: ManualQuestUpdatePayload,
) -> Result<(), MonarchError> {
    let id = payload.id.clone();
    let before = db.get_quest_internal(&id).await?;
    let notes = db.update_quest_manual_internal(&payload).await?;
    let after = db.get_quest_internal(&id).await?;
    emit_quest_updated_notifications(&app, &agent_mgr.ws_broadcast, &id, after.as_ref());
    emit_plan_notifications(&app, &agent_mgr.ws_broadcast, notes);
    handle_quest_update_side_effects(&app, db.inner(), agent_mgr.inner(), before, after).await?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn db_record_manual_quest_event(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    payload: ManualQuestEventPayload,
) -> Result<String, MonarchError> {
    let quest_id = payload.quest_id.clone();
    let event_type = payload.event_type.clone();
    let id = db.record_manual_quest_event_internal(&payload).await?;
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
pub async fn db_list_quest_refs(
    db: tauri::State<'_, Arc<Database>>,
    quest_id: String,
) -> Result<Vec<QuestRefRow>, MonarchError> {
    db.list_quest_refs_internal(&quest_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_create_quest_ref(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    payload: CreateQuestRefPayload,
) -> Result<String, MonarchError> {
    let quest_id = payload.quest_id.clone();
    let id = db.create_quest_ref_internal(&payload).await?;
    emit_quest_ref_notification(&app, &agent_mgr.ws_broadcast, &quest_id, "created", &id);
    Ok(id)
}

#[tauri::command]
#[specta::specta]
pub async fn db_update_quest_ref(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    payload: UpdateQuestRefPayload,
) -> Result<(), MonarchError> {
    let id = payload.id.clone();
    let before = db.get_quest_ref_internal(&id).await?;
    db.update_quest_ref_internal(&payload).await?;
    if let Some(row) = before {
        emit_quest_ref_notification(&app, &agent_mgr.ws_broadcast, &row.quest_id, "updated", &id);
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn db_delete_quest_ref(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    ref_id: String,
) -> Result<(), MonarchError> {
    let before = db.get_quest_ref_internal(&ref_id).await?;
    db.delete_quest_ref_internal(&ref_id).await?;
    if let Some(row) = before {
        emit_quest_ref_notification(
            &app,
            &agent_mgr.ws_broadcast,
            &row.quest_id,
            "deleted",
            &ref_id,
        );
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn db_get_working_memory(
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
) -> Result<Option<WorkingMemoryPayload>, MonarchError> {
    db.get_working_memory_internal(&agent_id).await
}

// ---- P4b (MON-111): Quest plan items ----
//
// Read-only commands and manual-edit write commands. The executor's
// plan-lifecycle path goes through the sidecar (Slice B) → InnerEvent →
// PersistCommand pipeline; the captain UI talks to these commands
// directly. Both end up calling the same `*_internal` methods, so plan
// state stays consistent across origins.

#[tauri::command]
#[specta::specta]
pub async fn db_list_plan_items(
    db: tauri::State<'_, Arc<Database>>,
    quest_id: String,
) -> Result<Vec<PlanItemRow>, MonarchError> {
    db.list_plan_items_internal(&quest_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_get_plan_item(
    db: tauri::State<'_, Arc<Database>>,
    item_id: String,
) -> Result<Option<PlanItemRow>, MonarchError> {
    db.get_plan_item_internal(&item_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_set_plan(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    payload: SetPlanPayload,
) -> Result<(), MonarchError> {
    let notes = db.set_plan_internal(&payload).await?;
    emit_plan_notifications(&app, &agent_mgr.ws_broadcast, notes);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn db_add_plan_item(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    payload: AddPlanItemPayload,
) -> Result<String, MonarchError> {
    let (id, notes) = db.add_plan_item_internal(&payload).await?;
    emit_plan_notifications(&app, &agent_mgr.ws_broadcast, notes);
    Ok(id)
}

#[tauri::command]
#[specta::specta]
pub async fn db_update_plan_item(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    payload: UpdatePlanItemPayload,
) -> Result<(), MonarchError> {
    let notes = db.update_plan_item_internal(&payload).await?;
    emit_plan_notifications(&app, &agent_mgr.ws_broadcast, notes);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn db_delete_plan_item(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    item_id: String,
) -> Result<(), MonarchError> {
    let notes = db.delete_plan_item_internal(&item_id).await?;
    emit_plan_notifications(&app, &agent_mgr.ws_broadcast, notes);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn db_start_plan_item(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    item_id: String,
) -> Result<(), MonarchError> {
    let notes = db.start_plan_item_internal(&item_id).await?;
    emit_plan_notifications(&app, &agent_mgr.ws_broadcast, notes);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn db_complete_plan_item(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    item_id: String,
    outcome: Option<String>,
) -> Result<(), MonarchError> {
    let notes = db
        .complete_plan_item_internal(&item_id, outcome.as_deref())
        .await?;
    emit_plan_notifications(&app, &agent_mgr.ws_broadcast, notes);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn db_skip_plan_item(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    item_id: String,
    reason: Option<String>,
) -> Result<(), MonarchError> {
    let notes = db
        .skip_plan_item_internal(&item_id, reason.as_deref())
        .await?;
    emit_plan_notifications(&app, &agent_mgr.ws_broadcast, notes);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn db_block_plan_item(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    item_id: String,
    reason: String,
) -> Result<(), MonarchError> {
    let notes = db.block_plan_item_internal(&item_id, &reason).await?;
    emit_plan_notifications(&app, &agent_mgr.ws_broadcast, notes);
    Ok(())
}

pub(crate) fn emit_plan_notifications(
    app: &tauri::AppHandle,
    ws_tx: &tokio::sync::broadcast::Sender<crate::agent::WsBroadcast>,
    notes: Vec<QuestEventNotification>,
) {
    for note in notes {
        crate::agent::emit_event(
            app,
            ws_tx,
            &format!("quest-event-{}", note.quest_id),
            &serde_json::json!({ "id": note.event_id, "eventType": note.event_type }).to_string(),
        );
    }
}

pub(crate) fn emit_quest_updated_notifications(
    app: &tauri::AppHandle,
    ws_tx: &tokio::sync::broadcast::Sender<crate::agent::WsBroadcast>,
    id: &str,
    after: Option<&QuestRow>,
) {
    crate::agent::emit_event(
        app,
        ws_tx,
        &format!("quest-updated-{}", id),
        &serde_json::json!({ "id": id }).to_string(),
    );
    if let Some(after_quest) = after {
        if after_quest.root_id != after_quest.id {
            crate::agent::emit_event(
                app,
                ws_tx,
                &format!("quest-updated-{}", after_quest.root_id),
                &serde_json::json!({ "id": after_quest.id, "rootId": after_quest.root_id })
                    .to_string(),
            );
        }
    }
}

pub(crate) fn emit_quest_ref_notification(
    app: &tauri::AppHandle,
    ws_tx: &tokio::sync::broadcast::Sender<crate::agent::WsBroadcast>,
    quest_id: &str,
    action: &str,
    ref_id: &str,
) {
    crate::agent::emit_event(
        app,
        ws_tx,
        &format!("quest-refs-{}", quest_id),
        &serde_json::json!({ "id": ref_id, "questId": quest_id, "action": action }).to_string(),
    );
}

// ---- MON-119: P6 Slice A — first-person quest reports ----
//
// Captain-initiated saves go through `db_save_quest_report` and write
// directly via `upsert_quest_report_internal` (matching the
// `db_create_quest_ref` precedent). Sidecar-originated writes (Slice B)
// flow through `PersistCommand::WriteQuestReport` instead so they preserve
// ordering against surrounding quest events. Both paths emit on
// `quest-report-{quest_id}` so the captain UI (Slice C) can subscribe
// once and see writes regardless of origin.

#[tauri::command]
#[specta::specta]
pub async fn db_save_quest_report(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    agent_mgr: tauri::State<'_, Arc<crate::agent::AgentManager>>,
    payload: WriteQuestReportPayload,
) -> Result<String, MonarchError> {
    let quest_id = payload.quest_id.clone();
    let id = db.upsert_quest_report_internal(&payload).await?;
    emit_quest_report_notification(&app, &agent_mgr.ws_broadcast, &quest_id, "saved", &id);
    Ok(id)
}

#[tauri::command]
#[specta::specta]
pub async fn db_get_quest_report(
    db: tauri::State<'_, Arc<Database>>,
    quest_id: String,
) -> Result<Option<QuestReportRow>, MonarchError> {
    db.get_quest_report_by_quest_internal(&quest_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_list_quest_reports_for_agent(
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
) -> Result<Vec<QuestReportRow>, MonarchError> {
    db.list_quest_reports_for_agent_internal(&agent_id).await
}

pub(crate) fn emit_quest_report_notification(
    app: &tauri::AppHandle,
    ws_tx: &tokio::sync::broadcast::Sender<crate::agent::WsBroadcast>,
    quest_id: &str,
    action: &str,
    report_id: &str,
) {
    crate::agent::emit_event(
        app,
        ws_tx,
        &format!("quest-report-{}", quest_id),
        &serde_json::json!({ "id": report_id, "questId": quest_id, "action": action }).to_string(),
    );
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_agent(id: &str) -> AgentRow {
        let now = crate::util::chrono_now();
        AgentRow {
            id: id.to_string(),
            name: id.to_string(),
            project_id: None,
            shadow_name: Some("Igris".to_string()),
            shadow_title: Some("Test Shadow".to_string()),
            shadow_grade: Some("Knight".to_string()),
            provider: Some("lmstudio".to_string()),
            model: Some("test-model".to_string()),
            thinking_level: Some("off".to_string()),
            cwd: Some("/tmp".to_string()),
            custom_prompt: None,
            context_window: None,
            created_at: now.clone(),
            updated_at: now,
            archived_at: None,
            avatar_type: None,
            avatar_path: None,
        }
    }

    async fn seed_agent_and_quest(db: &Database) -> (String, String) {
        let agent_id = "agent-p4".to_string();
        db.ensure_agent_exists_internal(&test_agent(&agent_id))
            .await
            .expect("agent");
        let quest_id = db
            .create_quest_internal(&CreateQuestPayload {
                id: None,
                parent_id: None,
                title: "Test quest".to_string(),
                description: None,
                status: Some("in_progress".to_string()),
                grade: Some("C".to_string()),
                exec_hint: Some("in_context".to_string()),
                assignee_shadow_id: Some(agent_id.clone()),
                created_by: Some("monarch".to_string()),
            })
            .await
            .expect("quest");
        (agent_id, quest_id)
    }

    #[tokio::test]
    async fn action_transition_sets_current_action() {
        let db = Database::new_in_memory().await.expect("db");
        let (agent_id, quest_id) = seed_agent_and_quest(&db).await;

        db.record_action_transition_internal(
            &agent_id,
            &quest_id,
            "Understand the failing authentication test",
            None,
        )
        .await
        .expect("transition");

        let wm = db
            .get_working_memory_internal(&agent_id)
            .await
            .expect("wm")
            .expect("wm row");
        let current = wm.current_action.expect("current action");
        assert_eq!(current.intent, "Understand the failing authentication test");
        assert_eq!(current.quest_id, quest_id);
        assert!(wm.recent_actions.is_empty());
    }

    #[tokio::test]
    async fn action_transition_closes_previous_action_with_outcome() {
        let db = Database::new_in_memory().await.expect("db");
        let (agent_id, quest_id) = seed_agent_and_quest(&db).await;

        db.record_action_transition_internal(&agent_id, &quest_id, "Map auth flow", None)
            .await
            .expect("first");
        db.record_action_transition_internal(
            &agent_id,
            &quest_id,
            "Patch expiry handler",
            Some("Found expired sessions return 401 instead of redirecting."),
        )
        .await
        .expect("second");

        let wm = db
            .get_working_memory_internal(&agent_id)
            .await
            .expect("wm")
            .expect("wm row");
        assert_eq!(
            wm.current_action.expect("current").intent,
            "Patch expiry handler"
        );
        assert_eq!(wm.recent_actions.len(), 1);
        assert_eq!(wm.recent_actions[0].intent, "Map auth flow");
        assert_eq!(
            wm.recent_actions[0].outcome,
            "Found expired sessions return 401 instead of redirecting."
        );
        assert_eq!(wm.recent_actions[0].auto_closed, None);
    }

    #[tokio::test]
    async fn complete_action_clears_current_action_and_records_outcome_child() {
        let db = Database::new_in_memory().await.expect("db");
        let (agent_id, quest_id) = seed_agent_and_quest(&db).await;

        db.record_action_transition_internal(&agent_id, &quest_id, "Edit session restore", None)
            .await
            .expect("transition");
        db.complete_action_internal(&agent_id, "Session restore now follows ancestry.")
            .await
            .expect("complete");

        let wm = db
            .get_working_memory_internal(&agent_id)
            .await
            .expect("wm")
            .expect("wm row");
        assert!(wm.current_action.is_none());
        assert_eq!(wm.recent_actions.len(), 1);
        assert_eq!(
            wm.recent_actions[0].outcome,
            "Session restore now follows ancestry."
        );

        let events = db
            .list_quest_events_internal(&quest_id)
            .await
            .expect("events");
        let action = events
            .iter()
            .find(|ev| ev.event_type == "coherent_action")
            .expect("action");
        let outcome = events
            .iter()
            .find(|ev| ev.event_type == "action_outcome")
            .expect("outcome");
        assert_eq!(outcome.parent_event_id.as_deref(), Some(action.id.as_str()));
    }

    #[tokio::test]
    async fn tool_call_start_and_end_update_one_child_event() {
        let db = Database::new_in_memory().await.expect("db");
        let (agent_id, quest_id) = seed_agent_and_quest(&db).await;
        db.record_action_transition_internal(&agent_id, &quest_id, "Run focused test", None)
            .await
            .expect("transition");

        db.record_tool_call_start_internal(
            &agent_id,
            &quest_id,
            "tc-1",
            "bash",
            Some(serde_json::json!({ "cmd": "cargo test auth" })),
        )
        .await
        .expect("tool start");
        db.record_tool_call_end_internal(
            "tc-1",
            Some(serde_json::json!({ "output": "ok" })),
            false,
            Some(123),
        )
        .await
        .expect("tool end");

        let events = db
            .list_quest_events_internal(&quest_id)
            .await
            .expect("events");
        let tools: Vec<_> = events
            .iter()
            .filter(|ev| ev.event_type == "tool_call")
            .collect();
        assert_eq!(tools.len(), 1);
        assert!(tools[0].parent_event_id.is_some());
        let payload: serde_json::Value =
            serde_json::from_str(tools[0].payload_json.as_deref().unwrap()).unwrap();
        assert_eq!(payload["tool_call_id"], "tc-1");
        assert_eq!(payload["status"], "done");
        assert_eq!(payload["duration_ms"], 123);
    }

    // ---- P4b (MON-111) plan lifecycle tests ----

    fn plan_input(title: &str) -> PlanItemInput {
        PlanItemInput {
            id: None,
            title: title.to_string(),
            rationale: None,
            status: None,
            parent_id: None,
        }
    }

    async fn seed_plan(db: &Database, quest_id: &str, titles: &[&str]) -> Vec<String> {
        let payload = SetPlanPayload {
            quest_id: quest_id.to_string(),
            items: titles.iter().map(|t| plan_input(t)).collect(),
            created_by: Some("captain".to_string()),
            rationale: None,
        };
        db.set_plan_internal(&payload).await.expect("set plan");
        let items = db
            .list_plan_items_internal(quest_id)
            .await
            .expect("list items");
        items.into_iter().map(|i| i.id).collect()
    }

    #[tokio::test]
    async fn set_plan_inserts_ordered_items_and_emits_plan_created() {
        let db = Database::new_in_memory().await.expect("db");
        let (_, quest_id) = seed_agent_and_quest(&db).await;

        db.set_plan_internal(&SetPlanPayload {
            quest_id: quest_id.clone(),
            items: vec![plan_input("inspect auth flow"), plan_input("patch handler")],
            created_by: Some("captain".to_string()),
            rationale: Some("expiry redirect bug".to_string()),
        })
        .await
        .expect("set plan");

        let items = db.list_plan_items_internal(&quest_id).await.expect("list");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "inspect auth flow");
        assert_eq!(items[0].order_index, 0);
        assert_eq!(items[0].status, "pending");
        assert_eq!(items[1].title, "patch handler");
        assert_eq!(items[1].order_index, 1);

        let events = db
            .list_quest_events_internal(&quest_id)
            .await
            .expect("events");
        let plan_events: Vec<_> = events
            .iter()
            .filter(|ev| ev.event_type == "plan_created")
            .collect();
        assert_eq!(plan_events.len(), 1, "exactly one plan_created event");
    }

    #[tokio::test]
    async fn set_plan_replaces_existing_items_and_emits_plan_changed() {
        let db = Database::new_in_memory().await.expect("db");
        let (_, quest_id) = seed_agent_and_quest(&db).await;
        let ids = seed_plan(&db, &quest_id, &["A", "B", "C"]).await;

        // Keep B (by id), drop A and C, add a new D at the end.
        db.set_plan_internal(&SetPlanPayload {
            quest_id: quest_id.clone(),
            items: vec![
                PlanItemInput {
                    id: Some(ids[1].clone()),
                    title: "B".to_string(),
                    rationale: None,
                    status: None,
                    parent_id: None,
                },
                plan_input("D"),
            ],
            created_by: Some("captain".to_string()),
            rationale: None,
        })
        .await
        .expect("replace");

        let items = db.list_plan_items_internal(&quest_id).await.expect("list");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, ids[1]);
        assert_eq!(items[1].title, "D");

        let events = db
            .list_quest_events_internal(&quest_id)
            .await
            .expect("events");
        assert!(events.iter().any(|ev| ev.event_type == "plan_changed"));
    }

    #[tokio::test]
    async fn add_plan_item_appends_at_end_when_no_after_id() {
        let db = Database::new_in_memory().await.expect("db");
        let (_, quest_id) = seed_agent_and_quest(&db).await;
        seed_plan(&db, &quest_id, &["A", "B"]).await;

        db.add_plan_item_internal(&AddPlanItemPayload {
            quest_id: quest_id.clone(),
            title: "C".to_string(),
            rationale: None,
            after_item_id: None,
            created_by: Some("captain".to_string()),
        })
        .await
        .expect("add");

        let items = db.list_plan_items_internal(&quest_id).await.expect("list");
        assert_eq!(items.len(), 3);
        assert_eq!(items[2].title, "C");
        assert_eq!(items[2].order_index, 2);
    }

    #[tokio::test]
    async fn add_plan_item_inserts_after_named_item_and_shifts_following() {
        let db = Database::new_in_memory().await.expect("db");
        let (_, quest_id) = seed_agent_and_quest(&db).await;
        let ids = seed_plan(&db, &quest_id, &["A", "B", "C"]).await;

        db.add_plan_item_internal(&AddPlanItemPayload {
            quest_id: quest_id.clone(),
            title: "Between".to_string(),
            rationale: None,
            after_item_id: Some(ids[0].clone()),
            created_by: Some("captain".to_string()),
        })
        .await
        .expect("insert");

        let items = db.list_plan_items_internal(&quest_id).await.expect("list");
        assert_eq!(
            items.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(),
            vec!["A", "Between", "B", "C"]
        );
    }

    #[tokio::test]
    async fn start_plan_item_sets_active_and_resets_prior_active() {
        let db = Database::new_in_memory().await.expect("db");
        let (agent_id, quest_id) = seed_agent_and_quest(&db).await;
        // Anchor L2's currentQuestId so sync_plan_l2_tx picks up the row.
        db.record_action_transition_internal(&agent_id, &quest_id, "warm up", None)
            .await
            .expect("anchor");
        let ids = seed_plan(&db, &quest_id, &["A", "B"]).await;

        db.start_plan_item_internal(&ids[0]).await.expect("start A");
        let wm_after_a = db
            .get_working_memory_internal(&agent_id)
            .await
            .expect("wm")
            .expect("row");
        assert_eq!(wm_after_a.active_plan_item_id.as_ref(), Some(&ids[0]));
        assert_eq!(wm_after_a.next_plan_item_ids, vec![ids[1].clone()]);

        db.start_plan_item_internal(&ids[1]).await.expect("start B");
        let after_b = db.list_plan_items_internal(&quest_id).await.expect("list");
        let by_id: std::collections::HashMap<&str, &str> = after_b
            .iter()
            .map(|i| (i.id.as_str(), i.status.as_str()))
            .collect();
        assert_eq!(by_id[ids[0].as_str()], "pending", "A reverts to pending");
        assert_eq!(by_id[ids[1].as_str()], "active", "B is now active");

        let wm_after_b = db
            .get_working_memory_internal(&agent_id)
            .await
            .expect("wm")
            .expect("row");
        assert_eq!(wm_after_b.active_plan_item_id.as_ref(), Some(&ids[1]));
    }

    #[tokio::test]
    async fn complete_plan_item_clears_active_no_auto_advance() {
        let db = Database::new_in_memory().await.expect("db");
        let (agent_id, quest_id) = seed_agent_and_quest(&db).await;
        db.record_action_transition_internal(&agent_id, &quest_id, "warm up", None)
            .await
            .expect("anchor");
        let ids = seed_plan(&db, &quest_id, &["A", "B"]).await;
        db.start_plan_item_internal(&ids[0]).await.expect("start");

        db.complete_plan_item_internal(&ids[0], Some("done"))
            .await
            .expect("complete");

        let item = db
            .get_plan_item_internal(&ids[0])
            .await
            .expect("get")
            .expect("row");
        assert_eq!(item.status, "completed");
        assert!(item.completed_at.is_some());

        let wm = db
            .get_working_memory_internal(&agent_id)
            .await
            .expect("wm")
            .expect("row");
        // No auto-advance — captain decides what's next.
        assert!(wm.active_plan_item_id.is_none());
    }

    #[tokio::test]
    async fn skip_and_block_record_status_and_emit_events() {
        let db = Database::new_in_memory().await.expect("db");
        let (_, quest_id) = seed_agent_and_quest(&db).await;
        let ids = seed_plan(&db, &quest_id, &["A", "B"]).await;

        db.skip_plan_item_internal(&ids[0], Some("not needed"))
            .await
            .expect("skip");
        db.block_plan_item_internal(&ids[1], "waiting on review")
            .await
            .expect("block");

        let items = db.list_plan_items_internal(&quest_id).await.expect("list");
        assert_eq!(items[0].status, "skipped");
        assert_eq!(items[1].status, "blocked");

        let events = db
            .list_quest_events_internal(&quest_id)
            .await
            .expect("events");
        assert!(events.iter().any(|ev| ev.event_type == "plan_item_skipped"));
        assert!(events.iter().any(|ev| ev.event_type == "plan_item_blocked"));
    }

    #[tokio::test]
    async fn coherent_action_stamps_plan_item_id_when_item_active() {
        let db = Database::new_in_memory().await.expect("db");
        let (agent_id, quest_id) = seed_agent_and_quest(&db).await;
        db.record_action_transition_internal(&agent_id, &quest_id, "warm up", None)
            .await
            .expect("anchor");
        let ids = seed_plan(&db, &quest_id, &["A"]).await;
        db.start_plan_item_internal(&ids[0]).await.expect("start");

        db.record_action_transition_internal(
            &agent_id,
            &quest_id,
            "patch handler",
            Some("warmed up"),
        )
        .await
        .expect("transition");

        let events = db
            .list_quest_events_internal(&quest_id)
            .await
            .expect("events");
        let action = events
            .iter()
            .filter(|ev| ev.event_type == "coherent_action")
            .find(|ev| {
                ev.payload_json
                    .as_deref()
                    .map_or(false, |p| p.contains("patch handler"))
            })
            .expect("action row");
        // We wrote the column directly; verify it lands by reading back.
        let plan_item_id_in_row: Option<String> = db
            .conn
            .call({
                let id = action.id.clone();
                move |c| -> tokio_rusqlite::Result<Option<String>> {
                    let v = c.query_row(
                        "SELECT plan_item_id FROM quest_events WHERE id = ?1",
                        params![id],
                        |row| row.get::<_, Option<String>>(0),
                    )?;
                    Ok(v)
                }
            })
            .await
            .expect("query");
        assert_eq!(plan_item_id_in_row.as_ref(), Some(&ids[0]));
    }

    #[tokio::test]
    async fn coherent_action_skips_plan_item_id_when_no_active_item() {
        let db = Database::new_in_memory().await.expect("db");
        let (agent_id, quest_id) = seed_agent_and_quest(&db).await;
        // Plan exists but nothing is active.
        seed_plan(&db, &quest_id, &["A", "B"]).await;

        db.record_action_transition_internal(&agent_id, &quest_id, "freeform exploration", None)
            .await
            .expect("transition");

        let events = db
            .list_quest_events_internal(&quest_id)
            .await
            .expect("events");
        let action = events
            .iter()
            .find(|ev| ev.event_type == "coherent_action")
            .expect("action");
        let plan_item_id_in_row: Option<String> = db
            .conn
            .call({
                let id = action.id.clone();
                move |c| -> tokio_rusqlite::Result<Option<String>> {
                    let v = c.query_row(
                        "SELECT plan_item_id FROM quest_events WHERE id = ?1",
                        params![id],
                        |row| row.get::<_, Option<String>>(0),
                    )?;
                    Ok(v)
                }
            })
            .await
            .expect("query");
        assert!(plan_item_id_in_row.is_none());
    }

    #[tokio::test]
    async fn get_active_plan_item_for_agent_resolves_via_l2() {
        let db = Database::new_in_memory().await.expect("db");
        let (agent_id, quest_id) = seed_agent_and_quest(&db).await;
        db.record_action_transition_internal(&agent_id, &quest_id, "warm up", None)
            .await
            .expect("anchor");
        let ids = seed_plan(&db, &quest_id, &["A"]).await;
        db.start_plan_item_internal(&ids[0]).await.expect("start");

        let resolved = db
            .get_active_plan_item_for_agent_internal(&agent_id)
            .await
            .expect("resolve");
        assert_eq!(resolved.as_ref(), Some(&ids[0]));
    }

    #[tokio::test]
    async fn working_memory_v1_payload_deserializes_with_default_plan_slice() {
        let db = Database::new_in_memory().await.expect("db");
        let (agent_id, _quest_id) = seed_agent_and_quest(&db).await;
        // Manually write a v1-shaped payload (no plan slice fields).
        let now = crate::util::chrono_now();
        let v1_payload = serde_json::json!({
            "schemaVersion": 1,
            "currentQuestId": null,
            "currentQuestPath": [],
            "currentAction": null,
            "recentActions": [],
            "updatedAt": now,
        });
        db.conn
            .call({
                let agent_id = agent_id.clone();
                let v1 = v1_payload.to_string();
                let now = now.clone();
                move |c| -> tokio_rusqlite::Result<()> {
                    c.execute(
                        "INSERT INTO agent_working_memory (agent_id, payload_json, updated_at)
                         VALUES (?1, ?2, ?3)",
                        params![agent_id, v1, now],
                    )?;
                    Ok(())
                }
            })
            .await
            .expect("insert v1 row");

        let wm = db
            .get_working_memory_internal(&agent_id)
            .await
            .expect("wm")
            .expect("row");
        assert_eq!(wm.schema_version, 1);
        assert!(wm.active_plan_item_id.is_none());
        assert!(wm.next_plan_item_ids.is_empty());
    }
}
