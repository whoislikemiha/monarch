//! Agent module facade.
//!
//! - `manager`        — `AgentManager`, live-state types, high-level lifecycle methods.
//! - `sidecar`        — `SidecarProcess` + the `impl AgentManager` block that owns
//!                      sidecar spawn / stdin-stdout / crash recovery.
//! - `event_handler`  — inbound sidecar event dispatch and snapshot emission.
//! - `persist`        — single-consumer persistence pipeline (MON-37).
//! - `commands`       — Tauri command wrappers + request DTOs.
//! - `keeper`         — `render_keeper_slice`, `maybe_trigger_keeper`, `handle_keeper_result`.
//! - `quest_prompt`   — quest-prompt heuristics and content helpers.
//!
//! Cross-cutting types (`WsBroadcast`, `TaskHandle`) and the
//! `DEBOUNCE_MILLIS` constant live here so every submodule can reach them
//! without a circular import chain.

use serde::Serialize;

pub mod commands;
mod event_handler;
mod keeper;
mod manager;
mod persist;
mod quest_prompt;
mod sidecar;

// DTOs re-exported at the module root so `crate::agent::SpawnAgentRequest`
// etc. keep working for ws.rs. Tauri command fns themselves stay addressed
// as `agent::commands::X` because `#[tauri::command]` emits a paired
// `__cmd__<name>` helper that must share the fn's module.
pub use commands::{ExtensionUiResponseRequest, SpawnAgentRequest};
pub use manager::AgentManager;
pub(crate) use manager::KeeperRunTrigger;

/// MON-83: cross-module access to the dual (Tauri + WS) emit helper so
/// non-agent command surfaces can broadcast their own event channels
/// (e.g. `quest-created-{id}`) through the same broadcast pipeline.
pub(crate) use event_handler::emit_event;

/// Debounce window for streaming `message_update` events. Token-rate chunks
/// would otherwise clone + serialize the full snapshot per token; 16ms caps
/// the emit rate at ~60fps which is visually equivalent and ~10x cheaper on
/// token-heavy turns. Terminal events (message_end, tool_execution_end, etc.)
/// bypass this and flush immediately so perceived "done" transitions stay
/// latency-free.
pub(crate) const DEBOUNCE_MILLIS: u64 = 16;

/// Shared alias for the debounce-task `JoinHandle`. Lives at the module
/// root because `AgentStateInner` holds one and `apply_and_maybe_emit`
/// spawns one.
pub(crate) type TaskHandle = tauri::async_runtime::JoinHandle<()>;

/// A broadcast event sent to WebSocket clients.
#[derive(Debug, Clone, Serialize)]
pub struct WsBroadcast {
    pub event: String,
    pub payload: String,
}
