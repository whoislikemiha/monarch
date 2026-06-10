use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::MonarchError;

use super::Database;

// ---- Row types ----

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

// ---- Row mappers ----

pub(super) fn map_project(row: &Row<'_>) -> rusqlite::Result<ProjectRow> {
    Ok(ProjectRow {
        id: row.get(0)?,
        name: row.get(1)?,
        root_path: row.get(2)?,
        instructions: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

// ---- impl Database ----

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
