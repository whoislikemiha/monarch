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
                    "INSERT INTO agents (id, name, project_id, shadow_name, shadow_title, shadow_grade, provider, model, thinking_level, cwd, custom_prompt, context_window, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                     ON CONFLICT(id) DO NOTHING",
                    params![
                        agent.id, agent.name, agent.project_id, agent.shadow_name, agent.shadow_title,
                        agent.shadow_grade, agent.provider, agent.model, agent.thinking_level,
                        agent.cwd, agent.custom_prompt, agent.context_window, agent.created_at, agent.updated_at,
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
                    "INSERT INTO agents (id, name, project_id, shadow_name, shadow_title, shadow_grade, provider, model, thinking_level, cwd, custom_prompt, context_window, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                     ON CONFLICT(id) DO UPDATE SET
                       name=excluded.name, project_id=excluded.project_id,
                       shadow_name=excluded.shadow_name, shadow_title=excluded.shadow_title,
                       shadow_grade=excluded.shadow_grade, provider=excluded.provider, model=excluded.model,
                       thinking_level=excluded.thinking_level, cwd=excluded.cwd, custom_prompt=excluded.custom_prompt,
                       context_window=excluded.context_window,
                       updated_at=strftime('%Y-%m-%dT%H:%M:%SZ','now')",
                    params![
                        agent.id, agent.name, agent.project_id, agent.shadow_name, agent.shadow_title,
                        agent.shadow_grade, agent.provider, agent.model, agent.thinking_level,
                        agent.cwd, agent.custom_prompt, agent.context_window, agent.created_at, agent.updated_at,
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
                    "INSERT INTO messages (session_id, role, content, model, tokens, cost, timestamp)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        message.session_id, message.role, message.content,
                        message.model, message.tokens, message.cost, message.timestamp,
                    ],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await?)
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
                    params![agent_id, session_id, event_type, data, crate::agent::chrono_now()],
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

                let mut all_messages = Vec::new();
                for sid in &chain {
                    let mut stmt = conn.prepare(
                        "SELECT id, session_id, role, content, model, tokens, cost, timestamp FROM messages WHERE session_id = ?1 ORDER BY id ASC",
                    )?;
                    let rows = stmt.query_map(params![sid], map_message)?;
                    for row in rows {
                        all_messages.push(row?);
                    }
                }
                Ok(all_messages)
            })
            .await?)
    }

    pub async fn get_agents_internal(&self) -> Result<Vec<AgentRow>, MonarchError> {
        Ok(self
            .conn
            .call(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, name, project_id, shadow_name, shadow_title, shadow_grade, provider, model, thinking_level, cwd, custom_prompt, context_window, created_at, updated_at FROM agents ORDER BY updated_at DESC",
                )?;
                let rows = stmt.query_map([], map_agent)?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
            })
            .await?)
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
                    "SELECT id, session_id, role, content, model, tokens, cost, timestamp FROM messages WHERE session_id = ?1 ORDER BY id ASC",
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
pub async fn db_get_agents(
    db: tauri::State<'_, Arc<Database>>,
) -> Result<Vec<AgentRow>, MonarchError> {
    db.get_agents_internal().await
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
