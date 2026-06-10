use crate::error::MonarchError;

use super::Database;

impl Database {
    pub(super) async fn init_schema(&self) -> Result<(), MonarchError> {
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
    pub(super) async fn migrate_timestamps_to_rfc3339(&self) -> Result<(), MonarchError> {
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
}
