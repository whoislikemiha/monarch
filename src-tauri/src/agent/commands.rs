//! Tauri command layer and request DTOs.
//!
//! Every `#[tauri::command]` in the `agent` module lives here; the structs
//! are the typed request payloads that keep specta under its 10-arg
//! `SpectaFn` cap and give the WebSocket dispatch path the same shape as
//! the IPC path.

use std::sync::Arc;

use serde::Deserialize;
use tauri::AppHandle;

use crate::agent::state::LiveAgentState;
use crate::db::{CaptainIdentityRow, Database, ShadowIdentityRow};
use crate::error::MonarchError;

use super::AgentManager;

/// Shadow identity block carried inside `SpawnAgentRequest`. Mirrors the
/// frontend's nested `config.shadow` object (name/title/grade), which the
/// backend then maps into the sidecar-facing `ShadowConfig` by injecting the
/// synthesized agent id at command-build time.
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ShadowSpec {
    pub shadow_name: Option<String>,
    pub shadow_title: Option<String>,
    pub shadow_grade: Option<String>,
}

/// Request payload for the `spawn_agent` Tauri command. Collapsing the ten
/// per-field params into a struct keeps the command under specta's 10-arg
/// `SpectaFn` cap so it can participate in typed binding generation — see
/// `lib.rs::specta_builder` for the registration site.
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SpawnAgentRequest {
    pub id: String,
    pub session_id: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking_level: Option<String>,
    pub cwd: Option<String>,
    pub shadow: Option<ShadowSpec>,
    pub context_window: Option<i32>,
}

/// Request payload for the `respond_extension_ui` Tauri command. MON-33 folds
/// the three scattered `agent_id` / `request_id` / `value` args into a single
/// typed struct so both the IPC and WS transports decode the same shape.
/// `value` stays `serde_json::Value` because the extension UI contract is
/// intentionally open-ended — different widget kinds return different payloads
/// and the sidecar is the ultimate authority on shape validation.
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionUiResponseRequest {
    pub agent_id: String,
    pub request_id: String,
    pub value: serde_json::Value,
}

#[tauri::command]
#[specta::specta]
pub async fn spawn_agent(
    app: AppHandle,
    state: tauri::State<'_, Arc<AgentManager>>,
    db: tauri::State<'_, Arc<Database>>,
    req: SpawnAgentRequest,
) -> Result<(), MonarchError> {
    state.spawn(&app, &db, req).await
}

#[tauri::command]
#[specta::specta]
pub async fn send_command(
    app: AppHandle,
    state: tauri::State<'_, Arc<AgentManager>>,
    db: tauri::State<'_, Arc<Database>>,
    id: String,
    command_json: String,
) -> Result<(), MonarchError> {
    state.send_command(&app, &db, id, command_json).await
}

#[tauri::command]
#[specta::specta]
pub async fn kill_agent(
    state: tauri::State<'_, Arc<AgentManager>>,
    id: String,
    _graceful: Option<bool>,
) -> Result<(), MonarchError> {
    state.kill(&id).await
}

/// Return the current assembled live state for an agent. This is the "pull"
/// half of the pull-then-subscribe pattern: Phase 2's frontend calls this on
/// mount to seed `liveAgentStore`, then listens on `agent-state-{id}` for
/// subsequent updates and reconciles by `stateVersion`.
///
/// Returns `None` if no entry exists for this agent (no events have arrived
/// yet, or the agent was killed). Callers should treat `None` as "empty
/// state" rather than an error.
#[tauri::command]
#[specta::specta]
pub async fn get_agent_state(
    state: tauri::State<'_, Arc<AgentManager>>,
    agent_id: String,
) -> Result<Option<LiveAgentState>, MonarchError> {
    let entry = match state.live_states.get(&agent_id) {
        Some(e) => e.clone(),
        None => return Ok(None),
    };
    let guard = entry.inner.read().await;
    Ok(Some(guard.state.clone()))
}

/// Rebuild the assembled `LiveAgentState` for an agent from a SQLite session
/// and publish a snapshot on `agent-state-{id}`. Returns the new state so the
/// frontend can seed its store without waiting for the event loopback.
///
/// `session_id = None` clears the state (used for "new session" flows).
#[tauri::command]
#[specta::specta]
pub async fn rebuild_agent_state_from_session(
    app: AppHandle,
    state: tauri::State<'_, Arc<AgentManager>>,
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
    session_id: Option<String>,
    status_text: String,
) -> Result<LiveAgentState, MonarchError> {
    state
        .rebuild_state_from_session(&app, &db, &agent_id, session_id.as_deref(), &status_text)
        .await
}

/// Load messages from a previous SQLite session into the sidecar's agent context.
/// This gives the LLM conversational continuity when restoring.
#[tauri::command]
#[specta::specta]
pub async fn load_session_context(
    app: AppHandle,
    state: tauri::State<'_, Arc<AgentManager>>,
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
    source_session_id: String,
) -> Result<(), MonarchError> {
    state
        .load_session_context(&app, &db, agent_id, source_session_id)
        .await
}

/// Create a new session for an existing agent.
/// Creates a DB row, updates the agent→session mapping, and tells the sidecar to reset.
#[tauri::command]
#[specta::specta]
pub async fn new_agent_session(
    app: AppHandle,
    state: tauri::State<'_, Arc<AgentManager>>,
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
    new_session_id: String,
    parent_session_id: Option<String>,
) -> Result<(), MonarchError> {
    state
        .new_session(&app, &db, agent_id, new_session_id, parent_session_id)
        .await
}

/// Switch an agent to an existing persisted session instead of creating a new one.
/// Resets the sidecar's in-memory conversation and updates DB/session routing so
/// subsequent messages are appended to the selected session.
#[tauri::command]
#[specta::specta]
pub async fn switch_agent_session(
    app: AppHandle,
    state: tauri::State<'_, Arc<AgentManager>>,
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
    session_id: String,
) -> Result<(), MonarchError> {
    state.switch_session(&app, &db, agent_id, session_id).await
}

// ---- MON-98: Captain / shadow identity commands ----

#[derive(Debug, Clone, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpsertCaptainIdentityRequest {
    pub name: String,
    pub payload: String,
    pub edit_note: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpsertShadowIdentityRequest {
    pub agent_id: String,
    pub payload: String,
    pub edit_note: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn get_captain_identity(
    db: tauri::State<'_, Arc<Database>>,
) -> Result<CaptainIdentityRow, MonarchError> {
    db.get_captain_identity_internal().await
}

#[tauri::command]
#[specta::specta]
pub async fn upsert_captain_identity(
    state: tauri::State<'_, Arc<AgentManager>>,
    db: tauri::State<'_, Arc<Database>>,
    req: UpsertCaptainIdentityRequest,
) -> Result<(), MonarchError> {
    db.upsert_captain_identity_internal(&req.name, &req.payload, req.edit_note.as_deref())
        .await?;
    let payload = if req.payload.is_empty() {
        None
    } else {
        Some(req.payload)
    };
    state.refresh_captain_identity(payload).await
}

#[tauri::command]
#[specta::specta]
pub async fn get_shadow_identity(
    db: tauri::State<'_, Arc<Database>>,
    agent_id: String,
) -> Result<Option<ShadowIdentityRow>, MonarchError> {
    db.get_shadow_identity_internal(&agent_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn upsert_shadow_identity(
    state: tauri::State<'_, Arc<AgentManager>>,
    db: tauri::State<'_, Arc<Database>>,
    req: UpsertShadowIdentityRequest,
) -> Result<(), MonarchError> {
    db.upsert_shadow_identity_internal(&req.agent_id, &req.payload, req.edit_note.as_deref())
        .await?;
    let payload = if req.payload.is_empty() {
        None
    } else {
        Some(req.payload)
    };
    state.refresh_shadow_identity(&req.agent_id, payload).await
}

/// Forward extension UI response from frontend to sidecar
#[tauri::command]
#[specta::specta]
pub async fn respond_extension_ui(
    app: AppHandle,
    state: tauri::State<'_, Arc<AgentManager>>,
    db: tauri::State<'_, Arc<Database>>,
    req: ExtensionUiResponseRequest,
) -> Result<(), MonarchError> {
    state.respond_extension_ui(&app, &db, req).await
}
