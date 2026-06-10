//! MON-32: typed Rust ↔ sidecar JSONL protocol.
//!
//! Mirrors the canonical contract in `sidecar/src/protocol.ts`. Outbound
//! commands are constructed as `SidecarCommand` values and serialized once at
//! the send site; inbound events are parsed once at the reader boundary into
//! `SidecarEvent`, with the `event` envelope carrying a typed `InnerEvent`
//! that replaces the `get("type").and_then(as_str).unwrap_or("")` dispatch
//! the per-agent `LiveAgentState` used to do against `serde_json::Value`.
//!
//! Unknown event types (envelope or inner) are represented as explicit
//! `Unknown { raw }` variants carrying the original payload, so the reader
//! can flip the dev-only desync indicator without losing the debugging
//! context. Parse failures on *known* tags propagate as
//! `serde_json::Error` → `MonarchError::Serde` via the existing `From` impl.

pub mod commands;
pub mod config;
pub mod events;
pub mod types;

pub use commands::SidecarCommand;
pub use config::{
    ClassifierInvocation, ClassifierInvocationConfig, ClassifierProvider, KeeperConfig,
    LoadSessionMessage, ShadowConfig,
};
pub use events::{apply_event, AtomicClaim, InnerEvent, SidecarEvent};
pub use types::QuestReport;

#[cfg(test)]
mod tests {
    use super::types::Message;
    use super::*;
    use crate::agent::state::{ApplyOutcome, DisplayItem, LiveAgentState, ToolStatus, Usage};
    use serde_json::json;
    use serde_json::Value;

    // ---- helpers -----------------------------------------------------------

    fn fresh_state() -> LiveAgentState {
        LiveAgentState::default()
    }

    fn msg(role: &str, content: Option<Value>) -> Message {
        Message {
            role: role.to_string(),
            content,
            model: Some("test-model".to_string()),
            usage: None,
            timestamp: Some(1000),
        }
    }

    fn msg_with_usage(role: &str, content: Option<Value>, usage: Usage) -> Message {
        Message {
            role: role.to_string(),
            content,
            model: Some("test-model".to_string()),
            usage: Some(usage),
            timestamp: Some(1000),
        }
    }

    fn assistant_content() -> Option<Value> {
        Some(json!([{"type": "text", "text": "hello"}]))
    }

    fn tool_start(id: &str, name: &str) -> InnerEvent {
        InnerEvent::ToolExecutionStart {
            tool_call_id: id.to_string(),
            tool_name: name.to_string(),
            args: Some(json!({"path": "/tmp"})),
        }
    }

    fn tool_end(id: &str, is_error: bool) -> InnerEvent {
        InnerEvent::ToolExecutionEnd {
            tool_call_id: id.to_string(),
            tool_name: Some("read_file".to_string()),
            result: Some(json!("ok")),
            is_error,
        }
    }

    /// Count items of a specific kind.
    fn count_items(state: &LiveAgentState, kind: &str) -> usize {
        state
            .items
            .iter()
            .filter(|item| match (item, kind) {
                (DisplayItem::User { .. }, "user") => true,
                (DisplayItem::Assistant { .. }, "assistant") => true,
                (DisplayItem::ToolGroup { .. }, "tool-group") => true,
                (DisplayItem::Status { .. }, "status") => true,
                _ => false,
            })
            .count()
    }

    // ---- invariant: event_count always increments --------------------------

    #[test]
    fn event_count_always_increments() {
        let mut s = fresh_state();
        let events: Vec<InnerEvent> = vec![
            InnerEvent::AgentStart,
            InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
            InnerEvent::MessageUpdate {
                message: msg("assistant", assistant_content()),
            },
            InnerEvent::MessageEnd {
                message: msg("assistant", assistant_content()),
                classification_id: None,
            },
            InnerEvent::Unknown {
                raw: json!({"type": "future_event"}),
            },
            InnerEvent::QueueUpdate,
            InnerEvent::AgentEnd,
        ];
        for (i, ev) in events.iter().enumerate() {
            apply_event(&mut s, ev);
            assert_eq!(s.event_count, (i + 1) as u64, "after event {}", i);
        }
    }

    // ---- invariant: state_version -----------------------------------------

    #[test]
    fn state_version_bumps_on_non_noop() {
        let mut s = fresh_state();
        let v_before = s.state_version;
        apply_event(&mut s, &InnerEvent::AgentStart);
        assert_eq!(s.state_version, v_before + 1);
    }

    #[test]
    fn state_version_unchanged_on_noop() {
        let mut s = fresh_state();
        let v_before = s.state_version;
        let outcome = apply_event(&mut s, &InnerEvent::QueueUpdate);
        assert_eq!(outcome, ApplyOutcome::NoOp);
        assert_eq!(s.state_version, v_before);
    }

    #[test]
    fn state_version_bumps_on_debounce() {
        // MON-70: MessageUpdate(assistant) returns Debounce *and* bumps
        // state_version so each debounced snapshot carries a monotonically
        // increasing version. The frontend's stale-drop check (`incoming
        // <= existing`) otherwise discards every snapshot after the first
        // during a streaming turn.
        let mut s = fresh_state();
        // Set up streaming state so the update is meaningful.
        apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
        );
        let v_before = s.state_version;
        let outcome = apply_event(
            &mut s,
            &InnerEvent::MessageUpdate {
                message: msg("assistant", assistant_content()),
            },
        );
        assert_eq!(outcome, ApplyOutcome::Debounce);
        assert_eq!(s.state_version, v_before + 1);
    }

    // ---- AgentStart / AgentEnd --------------------------------------------

    #[test]
    fn agent_start_sets_activity_and_pushes_status() {
        let mut s = fresh_state();
        let outcome = apply_event(&mut s, &InnerEvent::AgentStart);
        assert_eq!(outcome, ApplyOutcome::EmitNow);
        assert_eq!(s.activity_status, "Agent processing...");
        assert_eq!(count_items(&s, "status"), 1);
    }

    #[test]
    fn agent_end_clears_streaming_and_pushes_status() {
        let mut s = fresh_state();
        s.is_streaming = true;
        s.activity_status = "something".to_string();
        let outcome = apply_event(&mut s, &InnerEvent::AgentEnd);
        assert_eq!(outcome, ApplyOutcome::EmitNow);
        assert!(!s.is_streaming);
        assert!(s.activity_status.is_empty());
        assert_eq!(count_items(&s, "status"), 1);
    }

    #[test]
    fn agent_end_commits_streaming_message() {
        let mut s = fresh_state();
        // Simulate assistant streaming in progress without a MessageEnd.
        apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
        );
        assert!(s.streaming_message.is_some());

        apply_event(&mut s, &InnerEvent::AgentEnd);
        assert!(s.streaming_message.is_none());
        assert_eq!(count_items(&s, "assistant"), 1);
        assert!(!s.is_streaming);
    }

    // ---- TurnStart / TurnEnd ----------------------------------------------

    #[test]
    fn turn_start_resets_tool_group_idx() {
        let mut s = fresh_state();
        s.current_tool_group_idx = Some(3);
        let outcome = apply_event(&mut s, &InnerEvent::TurnStart);
        assert_eq!(outcome, ApplyOutcome::EmitNow);
        assert!(s.current_tool_group_idx.is_none());
        assert_eq!(s.activity_status, "LLM call in progress...");
    }

    #[test]
    fn turn_end_marks_tool_group_complete() {
        let mut s = fresh_state();
        // Create a tool group first.
        apply_event(&mut s, &tool_start("t1", "read_file"));
        assert!(s.current_tool_group_idx.is_some());

        let outcome = apply_event(&mut s, &InnerEvent::TurnEnd);
        assert_eq!(outcome, ApplyOutcome::EmitNow);
        assert!(s.current_tool_group_idx.is_none());

        // The tool group should be marked turn_complete.
        match &s.items.last() {
            Some(DisplayItem::ToolGroup { turn_complete, .. }) => {
                assert!(turn_complete);
            }
            other => panic!("expected ToolGroup, got {:?}", other),
        }
    }

    // ---- MessageStart -----------------------------------------------------

    #[test]
    fn message_start_user_pushes_display_item() {
        let mut s = fresh_state();
        let outcome = apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("user", Some(json!("hello world"))),
            },
        );
        assert_eq!(outcome, ApplyOutcome::EmitNow);
        assert_eq!(count_items(&s, "user"), 1);
        match &s.items[0] {
            DisplayItem::User {
                content, timestamp, ..
            } => {
                assert_eq!(content, "hello world");
                assert_eq!(*timestamp, Some(1000));
            }
            other => panic!("expected User, got {:?}", other),
        }
    }

    #[test]
    fn message_start_user_extracts_text_from_content_blocks() {
        let mut s = fresh_state();
        apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg(
                    "user",
                    Some(json!([
                        {"type": "text", "text": "hello "},
                        {"type": "image", "source": "..."},
                        {"type": "text", "text": "world"}
                    ])),
                ),
            },
        );
        match &s.items[0] {
            DisplayItem::User { content, .. } => {
                assert_eq!(content, "hello world");
            }
            other => panic!("expected User, got {:?}", other),
        }
    }

    #[test]
    fn message_start_assistant_begins_streaming() {
        let mut s = fresh_state();
        let outcome = apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
        );
        assert_eq!(outcome, ApplyOutcome::EmitNow);
        assert!(s.is_streaming);
        assert!(s.streaming_message.is_some());
        assert_eq!(s.activity_status, "Receiving response...");
    }

    #[test]
    fn message_start_resets_desynced() {
        let mut s = fresh_state();
        s.desynced = true;
        apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
        );
        assert!(!s.desynced);
    }

    #[test]
    fn message_start_unknown_role_is_noop() {
        let mut s = fresh_state();
        let v_before = s.state_version;
        let outcome = apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("system", None),
            },
        );
        // NoOp from the match arm, but desynced is still reset — and then
        // the version bump guard sees NoOp and skips the bump.
        assert_eq!(outcome, ApplyOutcome::NoOp);
        assert_eq!(s.state_version, v_before);
    }

    // ---- MessageUpdate ----------------------------------------------------

    #[test]
    fn message_update_assistant_returns_debounce() {
        let mut s = fresh_state();
        apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
        );
        let outcome = apply_event(
            &mut s,
            &InnerEvent::MessageUpdate {
                message: msg(
                    "assistant",
                    Some(json!([{"type": "text", "text": "hello world"}])),
                ),
            },
        );
        assert_eq!(outcome, ApplyOutcome::Debounce);
        assert!(s.streaming_message.is_some());
        let sm = s.streaming_message.as_ref().unwrap();
        assert_eq!(sm.content.len(), 1);
    }

    #[test]
    fn message_update_non_assistant_is_noop() {
        let mut s = fresh_state();
        let outcome = apply_event(
            &mut s,
            &InnerEvent::MessageUpdate {
                message: msg("user", None),
            },
        );
        assert_eq!(outcome, ApplyOutcome::NoOp);
    }

    #[test]
    fn message_update_captures_usage() {
        let mut s = fresh_state();
        apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
        );
        let usage = Usage {
            input: 100,
            output: 50,
            total_tokens: 150,
            ..Default::default()
        };
        apply_event(
            &mut s,
            &InnerEvent::MessageUpdate {
                message: msg_with_usage("assistant", assistant_content(), usage),
            },
        );
        assert!(s.last_usage.is_some());
        assert_eq!(s.last_usage.as_ref().unwrap().input, 100);
    }

    // ---- MessageEnd -------------------------------------------------------

    #[test]
    fn message_end_assistant_commits_to_items() {
        let mut s = fresh_state();
        apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
        );
        apply_event(
            &mut s,
            &InnerEvent::MessageUpdate {
                message: msg("assistant", assistant_content()),
            },
        );
        let outcome = apply_event(
            &mut s,
            &InnerEvent::MessageEnd {
                message: msg("assistant", assistant_content()),
                classification_id: None,
            },
        );
        assert_eq!(outcome, ApplyOutcome::EmitNow);
        assert!(s.streaming_message.is_none());
        assert_eq!(count_items(&s, "assistant"), 1);
    }

    #[test]
    fn message_end_non_assistant_still_emits() {
        // MessageEnd always returns EmitNow regardless of role, but only
        // pushes a DisplayItem for assistant.
        let mut s = fresh_state();
        let outcome = apply_event(
            &mut s,
            &InnerEvent::MessageEnd {
                message: msg("user", None),
                classification_id: None,
            },
        );
        assert_eq!(outcome, ApplyOutcome::EmitNow);
        assert_eq!(count_items(&s, "assistant"), 0);
    }

    // ---- Full assistant message lifecycle ----------------------------------

    #[test]
    fn full_assistant_message_lifecycle() {
        let mut s = fresh_state();

        // Start
        apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
        );
        assert!(s.is_streaming);
        assert!(s.streaming_message.is_some());

        // Multiple updates
        for _ in 0..5 {
            let outcome = apply_event(
                &mut s,
                &InnerEvent::MessageUpdate {
                    message: msg("assistant", assistant_content()),
                },
            );
            assert_eq!(outcome, ApplyOutcome::Debounce);
        }
        assert!(s.streaming_message.is_some());

        // End
        apply_event(
            &mut s,
            &InnerEvent::MessageEnd {
                message: msg("assistant", assistant_content()),
                classification_id: None,
            },
        );
        assert!(s.streaming_message.is_none());
        assert_eq!(count_items(&s, "assistant"), 1);
        // is_streaming is NOT cleared by MessageEnd — only by AgentEnd.
        assert!(s.is_streaming);
    }

    // ---- ToolExecution lifecycle -------------------------------------------

    #[test]
    fn tool_execution_start_creates_group_and_map_entry() {
        let mut s = fresh_state();
        let outcome = apply_event(&mut s, &tool_start("tc1", "read_file"));
        assert_eq!(outcome, ApplyOutcome::EmitNow);
        assert!(s.is_streaming);
        assert_eq!(s.activity_status, "Running tool: read_file");
        assert!(s.tool_executions.contains_key("tc1"));
        assert_eq!(s.tool_executions["tc1"].status, ToolStatus::Running);
        assert_eq!(count_items(&s, "tool-group"), 1);
        assert!(s.current_tool_group_idx.is_some());
    }

    #[test]
    fn tool_execution_end_updates_status_to_done() {
        let mut s = fresh_state();
        apply_event(&mut s, &tool_start("tc1", "read_file"));
        let outcome = apply_event(&mut s, &tool_end("tc1", false));
        assert_eq!(outcome, ApplyOutcome::EmitNow);
        assert!(s.activity_status.is_empty());
        assert_eq!(s.tool_executions["tc1"].status, ToolStatus::Done);
        assert_eq!(s.tool_executions["tc1"].is_error, Some(false));
        assert!(s.tool_executions["tc1"].result.is_some());
    }

    #[test]
    fn tool_execution_end_error_sets_error_status() {
        let mut s = fresh_state();
        apply_event(&mut s, &tool_start("tc1", "write_file"));
        apply_event(&mut s, &tool_end("tc1", true));
        assert_eq!(s.tool_executions["tc1"].status, ToolStatus::Error);
        assert_eq!(s.tool_executions["tc1"].is_error, Some(true));
    }

    #[test]
    fn tool_execution_end_updates_display_item_too() {
        let mut s = fresh_state();
        apply_event(&mut s, &tool_start("tc1", "read_file"));
        apply_event(&mut s, &tool_end("tc1", false));

        // The ToolGroup's execution should also be updated.
        match &s.items.last() {
            Some(DisplayItem::ToolGroup { executions, .. }) => {
                assert_eq!(executions[0].status, ToolStatus::Done);
                assert!(executions[0].result.is_some());
            }
            other => panic!("expected ToolGroup, got {:?}", other),
        }
    }

    // ---- Tool grouping ----------------------------------------------------

    #[test]
    fn consecutive_tools_share_one_group() {
        let mut s = fresh_state();
        apply_event(&mut s, &tool_start("tc1", "read_file"));
        apply_event(&mut s, &tool_start("tc2", "write_file"));

        assert_eq!(count_items(&s, "tool-group"), 1);
        match &s.items.last() {
            Some(DisplayItem::ToolGroup { executions, .. }) => {
                assert_eq!(executions.len(), 2);
                assert_eq!(executions[0].tool_call_id, "tc1");
                assert_eq!(executions[1].tool_call_id, "tc2");
            }
            other => panic!("expected ToolGroup with 2 execs, got {:?}", other),
        }
    }

    #[test]
    fn new_turn_starts_new_tool_group() {
        let mut s = fresh_state();
        // First turn: one tool.
        apply_event(&mut s, &tool_start("tc1", "read_file"));
        apply_event(&mut s, &tool_end("tc1", false));
        apply_event(&mut s, &InnerEvent::TurnEnd);

        // Second turn.
        apply_event(&mut s, &InnerEvent::TurnStart);
        apply_event(&mut s, &tool_start("tc2", "write_file"));

        assert_eq!(count_items(&s, "tool-group"), 2);
    }

    // ---- Interleaved tool + message events ---------------------------------

    #[test]
    fn interleaved_message_and_tool_events() {
        let mut s = fresh_state();

        // Assistant message.
        apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
        );
        apply_event(
            &mut s,
            &InnerEvent::MessageEnd {
                message: msg("assistant", assistant_content()),
                classification_id: None,
            },
        );

        // Tool execution.
        apply_event(&mut s, &tool_start("tc1", "read_file"));
        apply_event(&mut s, &tool_end("tc1", false));

        // Another assistant message.
        apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
        );
        apply_event(
            &mut s,
            &InnerEvent::MessageEnd {
                message: msg("assistant", assistant_content()),
                classification_id: None,
            },
        );

        assert_eq!(count_items(&s, "assistant"), 2);
        assert_eq!(count_items(&s, "tool-group"), 1);

        // Order: Assistant, ToolGroup, Assistant.
        assert!(matches!(&s.items[0], DisplayItem::Assistant { .. }));
        assert!(matches!(&s.items[1], DisplayItem::ToolGroup { .. }));
        assert!(matches!(&s.items[2], DisplayItem::Assistant { .. }));
    }

    // ---- Compaction -------------------------------------------------------

    #[test]
    fn compaction_start_with_reason() {
        let mut s = fresh_state();
        let outcome = apply_event(
            &mut s,
            &InnerEvent::CompactionStart {
                reason: Some("token limit".to_string()),
            },
        );
        assert_eq!(outcome, ApplyOutcome::EmitNow);
        assert_eq!(s.activity_status, "Compacting context...");
        match &s.items[0] {
            DisplayItem::Status { text } => {
                assert!(text.contains("token limit"));
            }
            other => panic!("expected Status, got {:?}", other),
        }
    }

    #[test]
    fn compaction_start_without_reason() {
        let mut s = fresh_state();
        apply_event(&mut s, &InnerEvent::CompactionStart { reason: None });
        match &s.items[0] {
            DisplayItem::Status { text } => {
                assert!(text.contains("unknown"));
            }
            other => panic!("expected Status, got {:?}", other),
        }
    }

    #[test]
    fn compaction_end_normal() {
        let mut s = fresh_state();
        apply_event(
            &mut s,
            &InnerEvent::CompactionEnd {
                aborted: Some(false),
            },
        );
        assert!(s.activity_status.is_empty());
        match &s.items[0] {
            DisplayItem::Status { text } => assert_eq!(text, "Context compacted"),
            other => panic!("expected Status, got {:?}", other),
        }
    }

    #[test]
    fn compaction_end_aborted() {
        let mut s = fresh_state();
        apply_event(
            &mut s,
            &InnerEvent::CompactionEnd {
                aborted: Some(true),
            },
        );
        match &s.items[0] {
            DisplayItem::Status { text } => assert_eq!(text, "Compaction aborted"),
            other => panic!("expected Status, got {:?}", other),
        }
    }

    // ---- AutoRetry --------------------------------------------------------

    #[test]
    fn auto_retry_start() {
        let mut s = fresh_state();
        let outcome = apply_event(&mut s, &InnerEvent::AutoRetryStart { attempt: 3 });
        assert_eq!(outcome, ApplyOutcome::EmitNow);
        assert!(s.activity_status.contains("3"));
    }

    #[test]
    fn auto_retry_end_is_noop() {
        let mut s = fresh_state();
        let outcome = apply_event(&mut s, &InnerEvent::AutoRetryEnd);
        assert_eq!(outcome, ApplyOutcome::NoOp);
    }

    // ---- NoOp events ------------------------------------------------------

    #[test]
    fn noop_events_do_not_bump_version() {
        let noop_events = vec![
            InnerEvent::AutoRetryEnd,
            InnerEvent::QueueUpdate,
            InnerEvent::ToolExecutionUpdate,
            InnerEvent::Unknown {
                raw: json!({"type": "from_the_future"}),
            },
        ];
        for ev in &noop_events {
            let mut s = fresh_state();
            let v = s.state_version;
            let outcome = apply_event(&mut s, ev);
            assert_eq!(outcome, ApplyOutcome::NoOp);
            assert_eq!(s.state_version, v, "version bumped on {:?}", ev);
            assert_eq!(s.event_count, 1, "event_count wrong on {:?}", ev);
        }
    }

    // ---- Full agent turn scenario -----------------------------------------

    #[test]
    fn full_agent_turn_scenario() {
        let mut s = fresh_state();

        // Agent starts.
        apply_event(&mut s, &InnerEvent::AgentStart);
        apply_event(&mut s, &InnerEvent::TurnStart);

        // Assistant thinks, then calls a tool.
        apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
        );
        apply_event(
            &mut s,
            &InnerEvent::MessageEnd {
                message: msg("assistant", assistant_content()),
                classification_id: None,
            },
        );
        apply_event(&mut s, &tool_start("tc1", "read_file"));
        apply_event(&mut s, &tool_end("tc1", false));

        // Second LLM call with response.
        apply_event(
            &mut s,
            &InnerEvent::MessageStart {
                message: msg("assistant", assistant_content()),
            },
        );
        apply_event(
            &mut s,
            &InnerEvent::MessageUpdate {
                message: msg("assistant", assistant_content()),
            },
        );
        apply_event(
            &mut s,
            &InnerEvent::MessageEnd {
                message: msg("assistant", assistant_content()),
                classification_id: None,
            },
        );

        apply_event(&mut s, &InnerEvent::TurnEnd);
        apply_event(&mut s, &InnerEvent::AgentEnd);

        // Final state assertions.
        assert!(!s.is_streaming);
        assert!(s.activity_status.is_empty());
        assert!(s.streaming_message.is_none());
        assert_eq!(count_items(&s, "assistant"), 2);
        assert_eq!(count_items(&s, "tool-group"), 1);
        assert_eq!(count_items(&s, "status"), 2); // AgentStart, AgentEnd
        assert_eq!(s.event_count, 11);
        assert!(s.state_version > 0);
    }

    // ---- P6 Slice B (MON-120): quest_report event -------------------------

    #[test]
    fn quest_report_event_deserializes_with_nested_fields() {
        let raw = json!({
            "type": "quest_report",
            "report": {
                "summary": "shipped the auth fix",
                "outcome": "done",
                "decisions": [
                    {"decision": "used a parallel call", "rationale": "latency"}
                ],
                "learned": ["sidecar event ordering is not guaranteed"],
                "artifacts": [{"file": "src/auth.rs", "role": "modified"}],
                "open_threads": ["no consumer yet"],
                "reflection": "tight slice",
                "grade": "A"
            }
        });
        let event: InnerEvent = serde_json::from_value(raw).expect("deserialize");
        match event {
            InnerEvent::QuestReport { report } => {
                assert_eq!(report.summary, "shipped the auth fix");
                assert_eq!(report.outcome, "done");
                assert_eq!(report.decisions.len(), 1);
                assert_eq!(report.decisions[0].decision, "used a parallel call");
                assert_eq!(report.decisions[0].rationale.as_deref(), Some("latency"));
                assert_eq!(report.learned.len(), 1);
                assert_eq!(report.learned[0], "sidecar event ordering is not guaranteed");
                assert_eq!(report.artifacts.len(), 1);
                assert_eq!(report.artifacts[0].file, "src/auth.rs");
                assert_eq!(report.artifacts[0].role, "modified");
                assert_eq!(report.open_threads.len(), 1);
                assert_eq!(report.grade, "A");
            }
            other => panic!("expected QuestReport, got {:?}", other),
        }
    }

    #[test]
    fn quest_report_event_tolerates_missing_fields() {
        // A sparse report must still deserialize — every field defaults — so
        // a malformed report can never fail the line and desync the agent.
        let raw = json!({ "type": "quest_report", "report": { "outcome": "blocked" } });
        let event: InnerEvent = serde_json::from_value(raw).expect("deserialize");
        match event {
            InnerEvent::QuestReport { report } => {
                assert_eq!(report.outcome, "blocked");
                assert!(report.summary.is_empty());
                assert!(report.decisions.is_empty());
                assert!(report.learned.is_empty());
            }
            other => panic!("expected QuestReport, got {:?}", other),
        }
    }

    #[test]
    fn quest_report_apply_event_is_noop() {
        let mut s = fresh_state();
        let before_version = s.state_version;
        let event = InnerEvent::QuestReport {
            report: QuestReport {
                summary: "done".to_string(),
                outcome: "done".to_string(),
                decisions: vec![],
                learned: vec![],
                artifacts: vec![],
                open_threads: vec![],
                reflection: String::new(),
                grade: "A".to_string(),
            },
        };
        assert_eq!(apply_event(&mut s, &event), ApplyOutcome::NoOp);
        // NoOp must not bump the version — the chat surface is untouched.
        assert_eq!(s.state_version, before_version);
    }
}
