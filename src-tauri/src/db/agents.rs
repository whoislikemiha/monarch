use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::MonarchError;

use super::Database;

// ---- Row types ----

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

// ---- Row mappers ----

pub(super) fn map_agent(row: &Row<'_>) -> rusqlite::Result<AgentRow> {
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

pub(super) fn map_agent_template(row: &Row<'_>) -> rusqlite::Result<AgentTemplateRow> {
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

/// Map tool names to specialization categories and compute normalized scores.
pub(super) fn compute_specialization(tool_usage: &[ToolUsageEntry]) -> SpecializationScores {
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

// ---- impl Database ----

impl Database {
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

    /// MON-100: lookup `agents.current_objective_id`. Returns None when the
    /// agent has no current objective, when the row is missing, or on read
    /// error (caller treats those identically — record no objective event).
    pub async fn get_agent_current_objective_id_internal(
        &self,
        agent_id: &str,
    ) -> Result<Option<String>, MonarchError> {
        let agent_id = agent_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                let v: Option<String> = conn
                    .query_row(
                        "SELECT current_objective_id FROM agents WHERE id = ?1",
                        params![agent_id],
                        |row| row.get(0),
                    )
                    .ok()
                    .flatten();
                Ok(v)
            })
            .await?)
    }

    /// MON-103: when a objective closes, clear the agent pointer only if it
    /// still points at that objective. This lets the next meaningful prompt
    /// auto-create a fresh current objective without disturbing newer work.
    pub async fn clear_agent_current_objective_if_matches_internal(
        &self,
        agent_id: &str,
        objective_id: &str,
    ) -> Result<(), MonarchError> {
        let agent_id = agent_id.to_string();
        let objective_id = objective_id.to_string();
        Ok(self
            .conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE agents
                     SET current_objective_id = NULL,
                         updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
                     WHERE id = ?1 AND current_objective_id = ?2",
                    params![agent_id, objective_id],
                )?;
                Ok(())
            })
            .await?)
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

// ---- Tauri Commands: Agent Stats ----

#[tauri::command]
#[specta::specta]
pub async fn db_get_agent_stats(
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
) -> Result<AgentStats, MonarchError> {
    db.get_agent_stats_internal(&agent_id).await
}

