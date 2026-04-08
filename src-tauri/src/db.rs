use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

pub struct Database {
    conn: Mutex<Connection>,
}

fn db_path() -> PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("monarch");
    std::fs::create_dir_all(&dir).ok();
    dir.join("monarch.db")
}

impl Database {
    pub fn new() -> Result<Self, String> {
        let path = db_path();
        let conn = Connection::open(&path).map_err(|e| format!("Failed to open DB: {}", e))?;

        // Enable WAL mode for better concurrent access
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .map_err(|e| e.to_string())?;

        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS agents (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                shadow_name TEXT,
                shadow_title TEXT,
                shadow_grade TEXT,
                provider TEXT,
                model TEXT,
                thinking_level TEXT,
                cwd TEXT,
                custom_prompt TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
                pi_session_file TEXT,
                model TEXT,
                provider TEXT,
                started_at TEXT NOT NULL DEFAULT (datetime('now')),
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
                timestamp TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS memories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                agent_id TEXT REFERENCES agents(id) ON DELETE CASCADE,
                layer TEXT NOT NULL DEFAULT 'hot',
                category TEXT NOT NULL DEFAULT 'general',
                content TEXT NOT NULL,
                relevance REAL DEFAULT 1.0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                last_accessed TEXT,
                access_count INTEGER DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                agent_id TEXT,
                session_id TEXT,
                event_type TEXT NOT NULL,
                data TEXT,
                timestamp TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_sessions_agent ON sessions(agent_id);
            CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);
            CREATE INDEX IF NOT EXISTS idx_memories_agent ON memories(agent_id);
            CREATE INDEX IF NOT EXISTS idx_memories_layer ON memories(layer);
            CREATE INDEX IF NOT EXISTS idx_events_agent ON events(agent_id);
            CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
            ",
        )
        .map_err(|e| format!("Failed to init schema: {}", e))?;

        // Migration: add parent_session_id column if it doesn't exist
        let _ = conn.execute_batch(
            "ALTER TABLE sessions ADD COLUMN parent_session_id TEXT REFERENCES sessions(id);",
        ); // Ignore error if column already exists

        Ok(())
    }
}

// ---- Data types ----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRow {
    pub id: String,
    pub name: String,
    pub shadow_name: Option<String>,
    pub shadow_title: Option<String>,
    pub shadow_grade: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking_level: Option<String>,
    pub cwd: Option<String>,
    pub custom_prompt: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

// ---- Internal methods (called from Rust event handler thread) ----

impl Database {
    pub fn save_message_internal(&self, message: &MessageRow) -> Result<i64, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO messages (session_id, role, content, model, tokens, cost, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                message.session_id, message.role, message.content,
                message.model, message.tokens, message.cost, message.timestamp,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(conn.last_insert_rowid())
    }

    pub fn update_session_internal(
        &self,
        session_id: &str,
        message_count: Option<i32>,
        total_tokens: Option<i32>,
        total_cost: Option<f64>,
        ended_at: Option<&str>,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        if let Some(mc) = message_count {
            conn.execute("UPDATE sessions SET message_count = ?1 WHERE id = ?2", params![mc, session_id])
                .map_err(|e| e.to_string())?;
        }
        if let Some(tt) = total_tokens {
            conn.execute("UPDATE sessions SET total_tokens = ?1 WHERE id = ?2", params![tt, session_id])
                .map_err(|e| e.to_string())?;
        }
        if let Some(tc) = total_cost {
            conn.execute("UPDATE sessions SET total_cost = ?1 WHERE id = ?2", params![tc, session_id])
                .map_err(|e| e.to_string())?;
        }
        if let Some(ea) = ended_at {
            conn.execute("UPDATE sessions SET ended_at = ?1 WHERE id = ?2", params![ea, session_id])
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn log_event_internal(
        &self,
        agent_id: Option<&str>,
        session_id: Option<&str>,
        event_type: &str,
        data: Option<&str>,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO events (agent_id, session_id, event_type, data) VALUES (?1, ?2, ?3, ?4)",
            params![agent_id, session_id, event_type, data],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Atomically increment message_count, total_tokens, and total_cost for a session
    pub fn increment_session_message_count(
        &self,
        session_id: &str,
        tokens: i32,
        cost: f64,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE sessions SET message_count = message_count + 1, total_tokens = total_tokens + ?1, total_cost = total_cost + ?2 WHERE id = ?3",
            params![tokens, cost, session_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn create_session_internal(&self, session: &SessionRow) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
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
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Get the full message chain for a session, following parent_session_id links.
    /// Returns messages from oldest ancestor to current, in chronological order.
    pub fn get_messages_with_ancestry(&self, session_id: &str) -> Result<Vec<MessageRow>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;

        // Walk the parent chain to collect all session IDs (oldest first)
        let mut chain: Vec<String> = vec![session_id.to_string()];
        let mut current = session_id.to_string();
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

        // Load messages from each session in order
        let mut all_messages = Vec::new();
        for sid in &chain {
            let mut stmt = conn
                .prepare("SELECT id, session_id, role, content, model, tokens, cost, timestamp FROM messages WHERE session_id = ?1 ORDER BY id ASC")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![sid], |row| {
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
                })
                .map_err(|e| e.to_string())?;
            for row in rows {
                all_messages.push(row.map_err(|e| e.to_string())?);
            }
        }

        Ok(all_messages)
    }
}

// ---- Tauri Commands: Agents ----

#[tauri::command]
pub fn db_upsert_agent(db: tauri::State<'_, Database>, agent: AgentRow) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO agents (id, name, shadow_name, shadow_title, shadow_grade, provider, model, thinking_level, cwd, custom_prompt, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
         ON CONFLICT(id) DO UPDATE SET
           name=excluded.name, shadow_name=excluded.shadow_name, shadow_title=excluded.shadow_title,
           shadow_grade=excluded.shadow_grade, provider=excluded.provider, model=excluded.model,
           thinking_level=excluded.thinking_level, cwd=excluded.cwd, custom_prompt=excluded.custom_prompt,
           updated_at=datetime('now')",
        params![
            agent.id, agent.name, agent.shadow_name, agent.shadow_title, agent.shadow_grade,
            agent.provider, agent.model, agent.thinking_level, agent.cwd, agent.custom_prompt,
            agent.created_at, agent.updated_at,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn db_get_agents(db: tauri::State<'_, Database>) -> Result<Vec<AgentRow>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, name, shadow_name, shadow_title, shadow_grade, provider, model, thinking_level, cwd, custom_prompt, created_at, updated_at FROM agents ORDER BY updated_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(AgentRow {
                id: row.get(0)?,
                name: row.get(1)?,
                shadow_name: row.get(2)?,
                shadow_title: row.get(3)?,
                shadow_grade: row.get(4)?,
                provider: row.get(5)?,
                model: row.get(6)?,
                thinking_level: row.get(7)?,
                cwd: row.get(8)?,
                custom_prompt: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn db_delete_agent(db: tauri::State<'_, Database>, agent_id: String) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM agents WHERE id = ?1", params![agent_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ---- Tauri Commands: Sessions ----

#[tauri::command]
pub fn db_create_session(db: tauri::State<'_, Database>, session: SessionRow) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
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
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn db_get_sessions(db: tauri::State<'_, Database>, agent_id: String) -> Result<Vec<SessionRow>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, agent_id, pi_session_file, model, provider, started_at, ended_at, message_count, total_tokens, total_cost, parent_session_id FROM sessions WHERE agent_id = ?1 ORDER BY started_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![agent_id], |row| {
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
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

// ---- Tauri Commands: Messages ----

#[tauri::command]
pub fn db_save_message(db: tauri::State<'_, Database>, message: MessageRow) -> Result<i64, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO messages (session_id, role, content, model, tokens, cost, timestamp)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            message.session_id, message.role, message.content,
            message.model, message.tokens, message.cost, message.timestamp,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

#[tauri::command]
pub fn db_get_messages(db: tauri::State<'_, Database>, session_id: String) -> Result<Vec<MessageRow>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, session_id, role, content, model, tokens, cost, timestamp FROM messages WHERE session_id = ?1 ORDER BY id ASC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![session_id], |row| {
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
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

/// Get messages for a session, including all ancestor sessions (for continued sessions)
#[tauri::command]
pub fn db_get_messages_with_ancestry(
    db: tauri::State<'_, Database>,
    session_id: String,
) -> Result<Vec<MessageRow>, String> {
    db.get_messages_with_ancestry(&session_id)
}

// ---- Tauri Commands: Memories ----

#[tauri::command]
pub fn db_save_memory(db: tauri::State<'_, Database>, memory: MemoryRow) -> Result<i64, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO memories (agent_id, layer, category, content, relevance, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            memory.agent_id, memory.layer, memory.category,
            memory.content, memory.relevance, memory.created_at,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

#[tauri::command]
pub fn db_get_memories(
    db: tauri::State<'_, Database>,
    agent_id: Option<String>,
    layer: Option<String>,
) -> Result<Vec<MemoryRow>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let sql = match (&agent_id, &layer) {
        (Some(_), Some(_)) => "SELECT id, agent_id, layer, category, content, relevance, created_at, last_accessed, access_count FROM memories WHERE (agent_id = ?1 OR agent_id IS NULL) AND layer = ?2 ORDER BY relevance DESC, created_at DESC",
        (Some(_), None) => "SELECT id, agent_id, layer, category, content, relevance, created_at, last_accessed, access_count FROM memories WHERE (agent_id = ?1 OR agent_id IS NULL) ORDER BY relevance DESC, created_at DESC",
        (None, Some(_)) => "SELECT id, agent_id, layer, category, content, relevance, created_at, last_accessed, access_count FROM memories WHERE layer = ?1 ORDER BY relevance DESC, created_at DESC",
        (None, None) => "SELECT id, agent_id, layer, category, content, relevance, created_at, last_accessed, access_count FROM memories ORDER BY relevance DESC, created_at DESC",
    };

    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let rows = match (&agent_id, &layer) {
        (Some(a), Some(l)) => stmt.query_map(params![a, l], map_memory).map_err(|e| e.to_string())?,
        (Some(a), None) => stmt.query_map(params![a], map_memory).map_err(|e| e.to_string())?,
        (None, Some(l)) => stmt.query_map(params![l], map_memory).map_err(|e| e.to_string())?,
        (None, None) => stmt.query_map([], map_memory).map_err(|e| e.to_string())?,
    };
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

fn map_memory(row: &rusqlite::Row) -> rusqlite::Result<MemoryRow> {
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

// ---- Tauri Commands: Events ----

#[tauri::command]
pub fn db_log_event(
    db: tauri::State<'_, Database>,
    agent_id: Option<String>,
    session_id: Option<String>,
    event_type: String,
    data: Option<String>,
) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO events (agent_id, session_id, event_type, data) VALUES (?1, ?2, ?3, ?4)",
        params![agent_id, session_id, event_type, data],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
