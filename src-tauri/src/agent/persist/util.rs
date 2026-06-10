use crate::sidecar_protocol::InnerEvent;

/// Stable snake_case tag for an `InnerEvent`, used by `LogEvent.event_type`
/// so the persisted shape matches the pre-MON-32 string dispatch.
pub(super) fn inner_event_tag(event: &InnerEvent) -> &'static str {
    match event {
        InnerEvent::AgentStart => "agent_start",
        InnerEvent::AgentEnd => "agent_end",
        InnerEvent::TurnStart => "turn_start",
        InnerEvent::TurnEnd => "turn_end",
        InnerEvent::MessageStart { .. } => "message_start",
        InnerEvent::MessageUpdate { .. } => "message_update",
        InnerEvent::MessageEnd { .. } => "message_end",
        InnerEvent::ToolExecutionStart { .. } => "tool_execution_start",
        InnerEvent::ToolExecutionEnd { .. } => "tool_execution_end",
        InnerEvent::MemorySuggestion { .. } => "memory_suggestion",
        InnerEvent::ActionTransition { .. } => "action_transition",
        InnerEvent::ActionComplete { .. } => "action_complete",
        InnerEvent::ExecutorDecision { .. } => "executor_decision",
        InnerEvent::PlanSet { .. } => "plan_set",
        InnerEvent::PlanItemStart { .. } => "plan_item_start",
        InnerEvent::PlanItemComplete { .. } => "plan_item_complete",
        InnerEvent::PlanItemSkip { .. } => "plan_item_skip",
        InnerEvent::PlanItemBlock { .. } => "plan_item_block",
        InnerEvent::QuestReport { .. } => "quest_report",
        InnerEvent::CompactionStart { .. } => "compaction_start",
        InnerEvent::CompactionEnd { .. } => "compaction_end",
        InnerEvent::AutoRetryStart { .. } => "auto_retry_start",
        InnerEvent::AutoRetryEnd => "auto_retry_end",
        InnerEvent::QueueUpdate => "queue_update",
        InnerEvent::ToolExecutionUpdate => "tool_execution_update",
        InnerEvent::Unknown { .. } => "unknown",
    }
}

pub(super) fn is_narration_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "set_current_action"
            | "complete_action"
            | "record_decision"
            // P4b plan-narration tools (Slice B). Hidden from ordinary
            // tool_call rendering — their semantic events land as
            // plan_* rows on the timeline instead.
            | "set_plan"
            | "update_plan"
            | "start_plan_item"
            | "complete_plan_item"
            | "skip_plan_item"
            | "block_plan_item"
    )
}
