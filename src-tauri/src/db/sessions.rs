use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::MonarchError;

use super::Database;

// ---- Row types ----

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

// ---- Row mappers ----

pub(super) fn map_session(row: &Row<'_>) -> rusqlite::Result<SessionRow> {
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

pub(super) fn map_message(row: &Row<'_>) -> rusqlite::Result<MessageRow> {
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

// ---- impl Database ----

impl Database {
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
