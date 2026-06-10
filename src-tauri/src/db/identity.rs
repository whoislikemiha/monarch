use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::error::MonarchError;

use super::Database;

// ---- Row types ----

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

// ---- impl Database ----

impl Database {
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
