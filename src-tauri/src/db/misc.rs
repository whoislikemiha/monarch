use rusqlite::params;
use std::sync::Arc;

use crate::error::MonarchError;

use super::Database;

// ---- impl Database ----

impl Database {
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
