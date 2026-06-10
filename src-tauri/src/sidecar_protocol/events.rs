use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::agent_state::{
    ApplyOutcome, ContentBlocks, DisplayItem, LiveAgentState, StreamingMessage, ToolExecution,
    ToolStatus,
};

use super::types::{Message, QuestReport};

// ========================================================================
// Inbound: SidecarEvent + InnerEvent
// ========================================================================

/// The inner event carried by `SidecarEvent::Event { event }`. One variant
/// per `case "xxx":` arm in the pre-MON-32 `apply_event` switch, plus an
/// `Unknown { raw }` fallback for forward-compat with sidecar versions that
/// ship new event kinds — Unknown flips `desynced` via `apply_event` the
/// same way the old catch-all did, but keeps the raw payload around for
/// debugging.
#[derive(Debug, Clone)]
pub enum InnerEvent {
    AgentStart,
    AgentEnd,
    TurnStart,
    TurnEnd,
    MessageStart {
        message: Message,
    },
    MessageUpdate {
        message: Message,
    },
    MessageEnd {
        message: Message,
        /// MON-82: present on user-role messages when a classification was
        /// in flight for this turn. Sidecar pairs it with the Pi-emitted
        /// user `message_end` so the persist pipeline can backfill
        /// `classifications.message_id` inline after the user row saves.
        classification_id: Option<String>,
    },
    ToolExecutionStart {
        tool_call_id: String,
        tool_name: String,
        args: Option<Value>,
    },
    ToolExecutionEnd {
        tool_call_id: String,
        tool_name: Option<String>,
        result: Option<Value>,
        is_error: bool,
    },
    MemorySuggestion {
        title: String,
        summary: String,
        content: String,
    },
    ActionTransition {
        intent: String,
        previous_outcome: Option<String>,
    },
    ActionComplete {
        outcome: String,
    },
    ExecutorDecision {
        decision: String,
        rationale: Option<String>,
    },
    /// P4b: full plan replace authored by the executor. `items` is the
    /// new ordered list; entries with no `id` get one minted server-side.
    PlanSet {
        items: Vec<crate::db::PlanItemInput>,
        rationale: Option<String>,
    },
    /// P4b: mark a plan item active. The previously active item on the
    /// same quest (if any) is silently reset to pending.
    PlanItemStart {
        item_id: String,
    },
    /// P4b: complete the currently active plan item with optional outcome.
    /// Item id resolution happens on the persistence side from the live
    /// plan slice — the sidecar doesn't carry plan state.
    PlanItemComplete {
        outcome: Option<String>,
    },
    /// P4b: skip a plan item. `item_id` is optional; when omitted the
    /// currently active item is skipped.
    PlanItemSkip {
        item_id: Option<String>,
        reason: Option<String>,
    },
    /// P4b: mark a plan item blocked. `item_id` is optional; when omitted
    /// the currently active item is blocked. `reason` is required.
    PlanItemBlock {
        item_id: Option<String>,
        reason: String,
    },
    /// P6 Slice B (MON-120): executor-authored first-person quest report.
    /// Persisted to `quest_reports`; a `done` / `abandoned` outcome also
    /// closes the quest.
    QuestReport {
        report: QuestReport,
    },
    CompactionStart {
        reason: Option<String>,
    },
    CompactionEnd {
        aborted: Option<bool>,
    },
    AutoRetryStart {
        attempt: i64,
    },
    AutoRetryEnd,
    QueueUpdate,
    ToolExecutionUpdate,
    Unknown {
        raw: Value,
    },
}

/// Private Deserialize helper. `InnerEvent::deserialize` first peeks at the
/// `type` tag; known tags are routed through this helper (which has a
/// plain derived `Deserialize`), unknown tags fall through to
/// `InnerEvent::Unknown { raw }`. Keeping the helper distinct from the
/// public enum lets `Unknown` carry a `serde_json::Value` payload —
/// `#[serde(other)]` only supports unit variants.
#[derive(Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum KnownInnerEvent {
    AgentStart,
    AgentEnd,
    TurnStart,
    TurnEnd,
    MessageStart {
        message: Message,
    },
    MessageUpdate {
        message: Message,
    },
    MessageEnd {
        message: Message,
        #[serde(default)]
        classification_id: Option<String>,
    },
    ToolExecutionStart {
        tool_call_id: String,
        tool_name: String,
        #[serde(default)]
        args: Option<Value>,
    },
    ToolExecutionEnd {
        tool_call_id: String,
        #[serde(default)]
        tool_name: Option<String>,
        #[serde(default)]
        result: Option<Value>,
        is_error: bool,
    },
    MemorySuggestion {
        title: String,
        summary: String,
        content: String,
    },
    ActionTransition {
        intent: String,
        #[serde(default)]
        previous_outcome: Option<String>,
    },
    ActionComplete {
        outcome: String,
    },
    ExecutorDecision {
        decision: String,
        #[serde(default)]
        rationale: Option<String>,
    },
    PlanSet {
        items: Vec<crate::db::PlanItemInput>,
        #[serde(default)]
        rationale: Option<String>,
    },
    PlanItemStart {
        item_id: String,
    },
    PlanItemComplete {
        #[serde(default)]
        outcome: Option<String>,
    },
    PlanItemSkip {
        #[serde(default)]
        item_id: Option<String>,
        #[serde(default)]
        reason: Option<String>,
    },
    PlanItemBlock {
        #[serde(default)]
        item_id: Option<String>,
        reason: String,
    },
    QuestReport {
        report: QuestReport,
    },
    CompactionStart {
        #[serde(default)]
        reason: Option<String>,
    },
    CompactionEnd {
        #[serde(default)]
        aborted: Option<bool>,
    },
    AutoRetryStart {
        attempt: i64,
    },
    AutoRetryEnd,
    QueueUpdate,
    ToolExecutionUpdate,
}

impl From<KnownInnerEvent> for InnerEvent {
    fn from(k: KnownInnerEvent) -> Self {
        match k {
            KnownInnerEvent::AgentStart => Self::AgentStart,
            KnownInnerEvent::AgentEnd => Self::AgentEnd,
            KnownInnerEvent::TurnStart => Self::TurnStart,
            KnownInnerEvent::TurnEnd => Self::TurnEnd,
            KnownInnerEvent::MessageStart { message } => Self::MessageStart { message },
            KnownInnerEvent::MessageUpdate { message } => Self::MessageUpdate { message },
            KnownInnerEvent::MessageEnd {
                message,
                classification_id,
            } => Self::MessageEnd {
                message,
                classification_id,
            },
            KnownInnerEvent::ToolExecutionStart {
                tool_call_id,
                tool_name,
                args,
            } => Self::ToolExecutionStart {
                tool_call_id,
                tool_name,
                args,
            },
            KnownInnerEvent::ToolExecutionEnd {
                tool_call_id,
                tool_name,
                result,
                is_error,
            } => Self::ToolExecutionEnd {
                tool_call_id,
                tool_name,
                result,
                is_error,
            },
            KnownInnerEvent::MemorySuggestion {
                title,
                summary,
                content,
            } => Self::MemorySuggestion {
                title,
                summary,
                content,
            },
            KnownInnerEvent::ActionTransition {
                intent,
                previous_outcome,
            } => Self::ActionTransition {
                intent,
                previous_outcome,
            },
            KnownInnerEvent::ActionComplete { outcome } => Self::ActionComplete { outcome },
            KnownInnerEvent::ExecutorDecision {
                decision,
                rationale,
            } => Self::ExecutorDecision {
                decision,
                rationale,
            },
            KnownInnerEvent::PlanSet { items, rationale } => Self::PlanSet { items, rationale },
            KnownInnerEvent::PlanItemStart { item_id } => Self::PlanItemStart { item_id },
            KnownInnerEvent::PlanItemComplete { outcome } => Self::PlanItemComplete { outcome },
            KnownInnerEvent::PlanItemSkip { item_id, reason } => {
                Self::PlanItemSkip { item_id, reason }
            }
            KnownInnerEvent::PlanItemBlock { item_id, reason } => {
                Self::PlanItemBlock { item_id, reason }
            }
            KnownInnerEvent::QuestReport { report } => Self::QuestReport { report },
            KnownInnerEvent::CompactionStart { reason } => Self::CompactionStart { reason },
            KnownInnerEvent::CompactionEnd { aborted } => Self::CompactionEnd { aborted },
            KnownInnerEvent::AutoRetryStart { attempt } => Self::AutoRetryStart { attempt },
            KnownInnerEvent::AutoRetryEnd => Self::AutoRetryEnd,
            KnownInnerEvent::QueueUpdate => Self::QueueUpdate,
            KnownInnerEvent::ToolExecutionUpdate => Self::ToolExecutionUpdate,
        }
    }
}

/// Tags the sidecar ships today. Keep in sync with `KnownInnerEvent`
/// variants. Drift risk is bounded — a missing tag just routes the event
/// through `Unknown` (benign: desync indicator flips), and a stale tag
/// here can't cause typed deserialization to misfire because the second
/// pass does a strict decode anyway.
const KNOWN_INNER_TAGS: &[&str] = &[
    "agent_start",
    "agent_end",
    "turn_start",
    "turn_end",
    "message_start",
    "message_update",
    "message_end",
    "tool_execution_start",
    "tool_execution_end",
    "memory_suggestion",
    "action_transition",
    "action_complete",
    "executor_decision",
    "plan_set",
    "plan_item_start",
    "plan_item_complete",
    "plan_item_skip",
    "plan_item_block",
    "quest_report",
    "compaction_start",
    "compaction_end",
    "auto_retry_start",
    "auto_retry_end",
    "queue_update",
    "tool_execution_update",
];

impl<'de> Deserialize<'de> for InnerEvent {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let raw = Value::deserialize(d)?;
        let tag = raw
            .get("type")
            .and_then(|t| t.as_str())
            .ok_or_else(|| D::Error::missing_field("type"))?;
        if !KNOWN_INNER_TAGS.contains(&tag) {
            return Ok(Self::Unknown { raw });
        }
        let known: KnownInnerEvent = serde_json::from_value(raw).map_err(D::Error::custom)?;
        Ok(known.into())
    }
}

/// Top-level sidecar → Rust envelope. Mirrors the `SidecarEvent` union in
/// `sidecar/src/protocol.ts`.
#[derive(Debug, Clone)]
pub enum SidecarEvent {
    SessionReady {
        agent_id: String,
        context_window: Option<i64>,
    },
    SessionDestroyed {
        agent_id: String,
    },
    Event {
        agent_id: String,
        event: InnerEvent,
    },
    ExtensionUiRequest {
        agent_id: String,
    },
    Error {
        agent_id: String,
        error: String,
    },
    /// MON-82: classifier output for a user turn. Emitted independently of
    /// the Pi turn (see runtime-manager). `complexity`/metrics populated on
    /// success; `error` populated on failure.
    Classification {
        agent_id: String,
        id: String,
        complexity: Option<String>,
        confidence: Option<f64>,
        rationale: Option<String>,
        model: Option<String>,
        tokens_in: Option<i32>,
        tokens_out: Option<i32>,
        latency_ms: Option<i32>,
        error: Option<String>,
    },
    /// MON-100: Keeper run result. `claims` + `compaction_summary` populated
    /// on success; `error` populated on failure. The sidecar handles the Pi
    /// `state.messages` rewrite inline; Rust only persists rows + resets the
    /// live token counter on success.
    KeeperResult {
        agent_id: String,
        run_id: i64,
        claims: Option<Vec<AtomicClaim>>,
        compaction_summary: Option<String>,
        model: Option<String>,
        tokens_in: Option<i64>,
        tokens_out: Option<i64>,
        latency_ms: Option<i64>,
        error: Option<String>,
    },
    /// MON-101: sidecar asks Rust to retrieve memories before forwarding a
    /// user turn to Pi.
    MemorySearchRequest {
        agent_id: String,
        request_id: String,
        query: String,
        top_k: Option<u32>,
    },
    Unknown {
        raw: Value,
    },
}

/// MON-100: atomic claim shape. Mirrors `AtomicClaim` in
/// `sidecar/src/protocol.ts`. `kind` is open-string on the wire — the Keeper
/// system prompt restricts it to fact/decision/constraint/convention/
/// preference/correction/landmark, but Rust persists whatever the model
/// emits to keep the substrate forward-compatible with prompt evolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AtomicClaim {
    pub title: String,
    pub summary: String,
    pub content: String,
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum KnownSidecarEvent {
    SessionReady {
        agent_id: String,
        #[serde(default)]
        context_window: Option<i64>,
    },
    SessionDestroyed {
        agent_id: String,
    },
    Event {
        agent_id: String,
        event: InnerEvent,
    },
    ExtensionUiRequest {
        agent_id: String,
    },
    Error {
        agent_id: String,
        error: String,
    },
    Classification {
        agent_id: String,
        id: String,
        #[serde(default)]
        complexity: Option<String>,
        #[serde(default)]
        confidence: Option<f64>,
        #[serde(default)]
        rationale: Option<String>,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        tokens_in: Option<i32>,
        #[serde(default)]
        tokens_out: Option<i32>,
        #[serde(default)]
        latency_ms: Option<i32>,
        #[serde(default)]
        error: Option<String>,
    },
    KeeperResult {
        agent_id: String,
        run_id: i64,
        #[serde(default)]
        claims: Option<Vec<AtomicClaim>>,
        #[serde(default)]
        compaction_summary: Option<String>,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        tokens_in: Option<i64>,
        #[serde(default)]
        tokens_out: Option<i64>,
        #[serde(default)]
        latency_ms: Option<i64>,
        #[serde(default)]
        error: Option<String>,
    },
    MemorySearchRequest {
        agent_id: String,
        request_id: String,
        query: String,
        #[serde(default)]
        top_k: Option<u32>,
    },
}

impl From<KnownSidecarEvent> for SidecarEvent {
    fn from(k: KnownSidecarEvent) -> Self {
        match k {
            KnownSidecarEvent::SessionReady {
                agent_id,
                context_window,
            } => Self::SessionReady {
                agent_id,
                context_window,
            },
            KnownSidecarEvent::SessionDestroyed { agent_id } => Self::SessionDestroyed { agent_id },
            KnownSidecarEvent::Event { agent_id, event } => Self::Event { agent_id, event },
            KnownSidecarEvent::ExtensionUiRequest { agent_id } => {
                Self::ExtensionUiRequest { agent_id }
            }
            KnownSidecarEvent::Error { agent_id, error } => Self::Error { agent_id, error },
            KnownSidecarEvent::Classification {
                agent_id,
                id,
                complexity,
                confidence,
                rationale,
                model,
                tokens_in,
                tokens_out,
                latency_ms,
                error,
            } => Self::Classification {
                agent_id,
                id,
                complexity,
                confidence,
                rationale,
                model,
                tokens_in,
                tokens_out,
                latency_ms,
                error,
            },
            KnownSidecarEvent::KeeperResult {
                agent_id,
                run_id,
                claims,
                compaction_summary,
                model,
                tokens_in,
                tokens_out,
                latency_ms,
                error,
            } => Self::KeeperResult {
                agent_id,
                run_id,
                claims,
                compaction_summary,
                model,
                tokens_in,
                tokens_out,
                latency_ms,
                error,
            },
            KnownSidecarEvent::MemorySearchRequest {
                agent_id,
                request_id,
                query,
                top_k,
            } => Self::MemorySearchRequest {
                agent_id,
                request_id,
                query,
                top_k,
            },
        }
    }
}

const KNOWN_SIDECAR_TAGS: &[&str] = &[
    "session_ready",
    "session_destroyed",
    "event",
    "extension_ui_request",
    "error",
    "classification",
    "keeper_result",
    "memory_search_request",
];

impl<'de> Deserialize<'de> for SidecarEvent {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let raw = Value::deserialize(d)?;
        let tag = raw
            .get("type")
            .and_then(|t| t.as_str())
            .ok_or_else(|| D::Error::missing_field("type"))?;
        if !KNOWN_SIDECAR_TAGS.contains(&tag) {
            return Ok(Self::Unknown { raw });
        }
        let known: KnownSidecarEvent = serde_json::from_value(raw).map_err(D::Error::custom)?;
        Ok(known.into())
    }
}

// ========================================================================
// Event application: typed apply_event
// ========================================================================

/// Apply one typed inner event from the sidecar to the per-agent
/// `LiveAgentState`. Moved out of `LiveAgentState::apply_event` in MON-32
/// so the protocol knowledge (tag dispatch + field destructuring) lives
/// next to the enum definitions, leaving `agent_state.rs` focused on the
/// snapshot shape.
///
/// `state_version` is bumped whenever state actually changed (i.e. not on
/// `NoOp` outcomes).
pub fn apply_event(state: &mut LiveAgentState, event: &InnerEvent) -> ApplyOutcome {
    state.event_count = state.event_count.saturating_add(1);

    let outcome = match event {
        InnerEvent::AgentStart => {
            state.activity_status = "Agent processing...".to_string();
            // MON-71: stamp agent-span start so AgentEnd can decorate the
            // "finished" status line with elapsed time.
            state.agent_started_at_ms = Some(now_ms());
            state.items.push(DisplayItem::Status {
                text: "Agent started".to_string(),
            });
            ApplyOutcome::EmitNow
        }
        InnerEvent::AgentEnd => {
            state.activity_status = String::new();
            state.is_streaming = false;
            state.commit_streaming_message();
            // MON-71: "Agent finished in 2 min 14 sec". Omit the suffix
            // (and show plain "Agent finished") when we never saw an
            // AgentStart (replayed session) or when the span was
            // sub-1-second — matches the TS formatter's null behaviour.
            let text = match state
                .agent_started_at_ms
                .take()
                .map(|start| now_ms().saturating_sub(start))
                .and_then(crate::agent_state::format_duration_ms)
            {
                Some(d) => format!("Agent finished in {}", d),
                None => "Agent finished".to_string(),
            };
            state.items.push(DisplayItem::Status { text });
            ApplyOutcome::EmitNow
        }
        InnerEvent::TurnStart => {
            state.activity_status = "LLM call in progress...".to_string();
            state.current_tool_group_idx = None;
            // MON-71: stamp the turn's start time. Used at MessageStart to seed
            // `StreamingMessage.turn_started_at_ms` (so the frontend ticker has
            // an anchor) and at MessageEnd to compute the final duration.
            state.turn_started_at_ms = Some(now_ms());
            state.thinking_block_starts.clear();
            ApplyOutcome::EmitNow
        }
        InnerEvent::TurnEnd => {
            state.activity_status = "Processing response...".to_string();
            if let Some(idx) = state.current_tool_group_idx {
                if let Some(DisplayItem::ToolGroup { turn_complete, .. }) = state.items.get_mut(idx)
                {
                    *turn_complete = true;
                }
            }
            state.current_tool_group_idx = None;
            ApplyOutcome::EmitNow
        }
        InnerEvent::MessageStart { message } => {
            state.desynced = false;
            match message.role.as_str() {
                "user" => {
                    let content = message
                        .content
                        .as_ref()
                        .map(crate::agent_state::extract_user_text)
                        .unwrap_or_default();
                    state.items.push(DisplayItem::User {
                        content,
                        timestamp: message.timestamp,
                        // Live-stream path — attachments only surface on
                        // the follow-up DB-driven snapshot. The frontend
                        // bridges the gap with its ephemeral `sentImages`
                        // map until the rebuild catches up.
                        attachments: Vec::new(),
                    });
                    ApplyOutcome::EmitNow
                }
                "assistant" => {
                    let mut sm = streaming_from(message);
                    // MON-71: if no TurnStart preceded this (compaction, retry),
                    // stamp now so the ticker still has something to anchor to.
                    let anchor = state.turn_started_at_ms.unwrap_or_else(now_ms);
                    state.turn_started_at_ms = Some(anchor);
                    sm.turn_started_at_ms = Some(anchor);
                    record_new_thinking_blocks(&mut state.thinking_block_starts, &sm.content);
                    state.streaming_message = Some(sm);
                    state.activity_status = "Receiving response...".to_string();
                    state.is_streaming = true;
                    ApplyOutcome::EmitNow
                }
                _ => ApplyOutcome::NoOp,
            }
        }
        InnerEvent::MessageUpdate { message } => {
            if message.role == "assistant" {
                let mut sm = streaming_from(message);
                if let Some(usage) = sm.usage.clone() {
                    state.last_usage = Some(usage);
                }
                // MON-71: preserve the turn anchor set at MessageStart so the
                // frontend's live ticker doesn't jitter between updates.
                sm.turn_started_at_ms = state.turn_started_at_ms;
                record_new_thinking_blocks(&mut state.thinking_block_starts, &sm.content);
                state.streaming_message = Some(sm);
                // MON-70: fall through so `state_version` gets bumped at the
                // end of the match. Previously this early-returned, meaning
                // every debounced snapshot during a streaming turn carried
                // the same stateVersion and the frontend's `<=` stale-drop
                // check discarded all but the first one — visually manifesting
                // as "nothing streams, everything dumps at the end."
                ApplyOutcome::Debounce
            } else {
                ApplyOutcome::NoOp
            }
        }
        InnerEvent::MessageEnd { message, .. } => {
            if message.role == "assistant" {
                let sm = streaming_from(message);
                if let Some(usage) = sm.usage.clone() {
                    // MON-123: accumulate genuinely-NEW tokens for the memory
                    // trigger — uncached input + freshly-cached input + output.
                    // Deliberately EXCLUDES `cache_read`: that is the prior
                    // context re-read every turn, so counting it made the sum
                    // grow ~quadratically and fire the Keeper roughly every
                    // turn (≈0 realized savings, see MON-123). New material is
                    // the right unit for "enough happened to distill." Live-
                    // context size is Pi's native compaction's concern now.
                    let delta = usage
                        .input
                        .saturating_add(usage.output)
                        .saturating_add(usage.cache_write);
                    state.tokens_since_last_compaction =
                        state.tokens_since_last_compaction.saturating_add(delta);
                    state.last_usage = Some(usage);
                }
                // MON-71: seal remaining thinking blocks and compute turn
                // duration before pushing the finalized assistant item.
                let now = now_ms();
                let mut content = sm.content;
                record_new_thinking_blocks(&mut state.thinking_block_starts, &content);
                finalize_thinking_durations(&state.thinking_block_starts, &mut content, now);
                state.thinking_block_starts.clear();
                let duration_ms = state
                    .turn_started_at_ms
                    .map(|start| now.saturating_sub(start));
                state.turn_started_at_ms = None;
                state.items.push(DisplayItem::Assistant {
                    content,
                    model: sm.model,
                    usage: sm.usage,
                    timestamp: sm.timestamp,
                    duration_ms,
                });
                state.streaming_message = None;
            }
            ApplyOutcome::EmitNow
        }
        InnerEvent::ToolExecutionStart {
            tool_call_id,
            tool_name,
            args,
        } => {
            state.activity_status = format!("Running tool: {}", tool_name);
            state.is_streaming = true;
            // MON-71: stamp start time. Drives live ticker while the tool is
            // running; subtracted from the end stamp to compute duration.
            let started_at_ms = now_ms();
            let exec = ToolExecution {
                tool_call_id: tool_call_id.clone(),
                tool_name: tool_name.clone(),
                args: args.clone(),
                result: None,
                is_error: None,
                status: ToolStatus::Running,
                started_at_ms: Some(started_at_ms),
                duration_ms: None,
            };
            state
                .tool_executions
                .insert(tool_call_id.clone(), exec.clone());

            match state.current_tool_group_idx {
                Some(idx) => {
                    if let Some(DisplayItem::ToolGroup { executions, .. }) =
                        state.items.get_mut(idx)
                    {
                        executions.push(exec);
                    }
                }
                None => {
                    state.items.push(DisplayItem::ToolGroup {
                        executions: vec![exec],
                        turn_complete: false,
                    });
                    state.current_tool_group_idx = Some(state.items.len() - 1);
                }
            }
            ApplyOutcome::EmitNow
        }
        InnerEvent::ToolExecutionEnd {
            tool_call_id,
            tool_name: _,
            result,
            is_error,
        } => {
            let status = if *is_error {
                ToolStatus::Error
            } else {
                ToolStatus::Done
            };
            state.activity_status = String::new();

            // MON-71: compute duration once from the stamped start time on
            // the canonical `tool_executions` entry, then mirror it onto the
            // tool group's execution below so both views stay in sync.
            let duration_ms = state
                .tool_executions
                .get(tool_call_id)
                .and_then(|e| e.started_at_ms)
                .map(|start| now_ms().saturating_sub(start));

            if let Some(existing) = state.tool_executions.get_mut(tool_call_id) {
                existing.result = result.clone();
                existing.is_error = Some(*is_error);
                existing.status = status;
                existing.duration_ms = duration_ms;
            }

            if let Some(idx) = state.current_tool_group_idx {
                if let Some(DisplayItem::ToolGroup { executions, .. }) = state.items.get_mut(idx) {
                    if let Some(exec) = executions
                        .iter_mut()
                        .find(|e| &e.tool_call_id == tool_call_id)
                    {
                        exec.result = result.clone();
                        exec.is_error = Some(*is_error);
                        exec.status = status;
                        exec.duration_ms = duration_ms;
                    }
                }
            }
            ApplyOutcome::EmitNow
        }
        InnerEvent::CompactionStart { reason } => {
            let reason = reason.as_deref().unwrap_or("unknown");
            state.activity_status = "Compacting context...".to_string();
            state.items.push(DisplayItem::Status {
                text: format!("Context compaction started ({})", reason),
            });
            ApplyOutcome::EmitNow
        }
        InnerEvent::CompactionEnd { aborted } => {
            let aborted = aborted.unwrap_or(false);
            state.activity_status = String::new();
            state.items.push(DisplayItem::Status {
                text: if aborted {
                    "Compaction aborted".to_string()
                } else {
                    "Context compacted".to_string()
                },
            });
            ApplyOutcome::EmitNow
        }
        InnerEvent::AutoRetryStart { attempt } => {
            state.activity_status = format!("Auto-retry attempt {}...", attempt);
            state.items.push(DisplayItem::Status {
                text: format!("Auto-retry attempt {}", attempt),
            });
            ApplyOutcome::EmitNow
        }
        InnerEvent::AutoRetryEnd => ApplyOutcome::NoOp,
        InnerEvent::QueueUpdate => ApplyOutcome::NoOp,
        InnerEvent::ToolExecutionUpdate => ApplyOutcome::NoOp,
        InnerEvent::MemorySuggestion { .. } => ApplyOutcome::NoOp,
        InnerEvent::ActionTransition { .. } => ApplyOutcome::NoOp,
        InnerEvent::ActionComplete { .. } => ApplyOutcome::NoOp,
        InnerEvent::ExecutorDecision { .. } => ApplyOutcome::NoOp,
        // P4b: plan-lifecycle events affect persistence + L2 slice but
        // don't mutate the chat-side LiveAgentState. The Quest store
        // wakes via `quest-event-{id}` instead.
        InnerEvent::PlanSet { .. } => ApplyOutcome::NoOp,
        InnerEvent::PlanItemStart { .. } => ApplyOutcome::NoOp,
        InnerEvent::PlanItemComplete { .. } => ApplyOutcome::NoOp,
        InnerEvent::PlanItemSkip { .. } => ApplyOutcome::NoOp,
        InnerEvent::PlanItemBlock { .. } => ApplyOutcome::NoOp,
        // P6 Slice B: the quest report drives persistence + a quest-status
        // transition but does not mutate the chat-side LiveAgentState.
        InnerEvent::QuestReport { .. } => ApplyOutcome::NoOp,
        // MON-39 item 9: unknown events return NoOp so `state_version`
        // does not bump per event. The reader-side path in
        // `handle_sidecar_event` is the canonical entry that flips
        // `desynced` via `mark_desynced`, bumping the version once per
        // desync transition. This arm is defense-in-depth for unknown
        // events that bypass the early-return (e.g. empty agent_id).
        InnerEvent::Unknown { .. } => ApplyOutcome::NoOp,
    };

    if !matches!(outcome, ApplyOutcome::NoOp) {
        state.state_version = state.state_version.saturating_add(1);
    }
    outcome
}

/// Build a `StreamingMessage` from the typed inner `Message`. Replaces the
/// pre-MON-32 `streaming_from_json` helper in `agent_state.rs` which had to
/// walk `serde_json::Value` by hand. `content` extraction keeps the same
/// semantics: take the array if present, empty vec otherwise.
fn streaming_from(message: &Message) -> StreamingMessage {
    let content: ContentBlocks = message
        .content
        .as_ref()
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();
    StreamingMessage {
        content,
        model: message.model.clone(),
        usage: message.usage.clone(),
        timestamp: message.timestamp,
        turn_started_at_ms: None,
    }
}

/// MON-71: current wall-clock in milliseconds since epoch. Thin wrapper so
/// test code can stub it if needed; production uses `chrono::Utc::now()`.
fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// MON-71: record start times for any thinking blocks that have appeared
/// in the streaming content since the last call. Uses content-block index
/// as the key — Pi SDK appends blocks in order, so index is stable for the
/// lifetime of a turn. Existing entries are never overwritten so we keep
/// the earliest-observed start (the block's real start moment, not the
/// latest update that carried more thinking text).
fn record_new_thinking_blocks(
    starts: &mut std::collections::HashMap<usize, i64>,
    content: &[serde_json::Value],
) {
    let now = now_ms();
    for (i, block) in content.iter().enumerate() {
        if block.get("type").and_then(|t| t.as_str()) == Some("thinking") {
            starts.entry(i).or_insert(now);
        }
    }
}

/// MON-71: on `MessageEnd`, inject `_monarch.durationMs` into each thinking
/// block's JSON so the duration survives the round trip through SQLite
/// (content is persisted as opaque JSON). The injection key is namespaced
/// to avoid colliding with any Pi SDK field that may land on these blocks.
///
/// End time uses `message_end_ms` for blocks that are immediately followed
/// by another block (text or tool call) and for the final block — the
/// tightest sensible boundary given we only observe deltas, not explicit
/// thinking_end events at this layer. Rounding error is ≤ one debounce
/// window (16 ms) which is invisible at the second-granularity display.
fn finalize_thinking_durations(
    starts: &std::collections::HashMap<usize, i64>,
    content: &mut [serde_json::Value],
    message_end_ms: i64,
) {
    for (i, block) in content.iter_mut().enumerate() {
        if block.get("type").and_then(|t| t.as_str()) != Some("thinking") {
            continue;
        }
        let Some(start) = starts.get(&i).copied() else {
            continue;
        };
        let duration = message_end_ms.saturating_sub(start);
        if let Some(obj) = block.as_object_mut() {
            let meta = obj
                .entry("_monarch".to_string())
                .or_insert_with(|| serde_json::json!({}));
            if let Some(meta_obj) = meta.as_object_mut() {
                meta_obj.insert(
                    "durationMs".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(duration)),
                );
            }
        }
    }
}
