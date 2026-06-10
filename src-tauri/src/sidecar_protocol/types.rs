use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent::state::Usage;

/// Typed `message` field carried by `message_start` / `message_update` /
/// `message_end` inner events. `content` is kept as an opaque
/// `serde_json::Value` because the per-block shape is owned by the Pi SDK
/// (same reasoning as `ContentBlocks` in `agent_state.rs` — the maintenance
/// cost of mirroring the SDK's union outweighs the type safety benefit).
///
/// `role` defaults to empty string to preserve the pre-MON-32 fall-through:
/// an absent or unknown role lets `apply_event` emit a `NoOp` instead of
/// flipping desync. This matches the current `unwrap_or("")` behavior
/// exactly — the ticket's "no silent defaulting" rule is about numeric /
/// id-like fields that must not silently become `0` or `""`, not about
/// best-effort fall-throughs on well-known enum-like strings.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub content: Option<Value>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub usage: Option<Usage>,
    #[serde(default)]
    pub timestamp: Option<i64>,
}

/// P6 Slice B (MON-120): one explicit decision inside a first-person quest
/// report. Mirrors the `decisions[]` entry shape the `complete_quest` tool
/// emits. All fields default so a malformed report can't desync the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestReportDecision {
    #[serde(default)]
    pub decision: String,
    #[serde(default)]
    pub rationale: Option<String>,
}

/// P6 Slice B (MON-120): one artifact reference inside a quest report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestReportArtifact {
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub role: String,
}

/// P6 Slice B (MON-120): the structured first-person quest report the
/// executor emits via `complete_quest`. Serialized verbatim into the
/// `quest_reports.payload` JSON column. Snake-case field names match the
/// sidecar tool payload and `distillation.md` § "First-person quest report".
/// Every field defaults — a malformed report still deserializes (worst case
/// an empty report with no terminal `outcome`) rather than desyncing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestReport {
    #[serde(default)]
    pub summary: String,
    /// `done` | `blocked` | `abandoned` | `partial`. Open-string on the wire;
    /// only `done` / `abandoned` drive a quest-status transition.
    #[serde(default)]
    pub outcome: String,
    #[serde(default)]
    pub decisions: Vec<QuestReportDecision>,
    #[serde(default)]
    pub learned: Vec<String>,
    #[serde(default)]
    pub artifacts: Vec<QuestReportArtifact>,
    #[serde(default)]
    pub open_threads: Vec<String>,
    #[serde(default)]
    pub reflection: String,
    #[serde(default)]
    pub grade: String,
}
