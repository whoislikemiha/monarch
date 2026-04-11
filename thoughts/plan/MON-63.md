# MON-63: Shadow Stats Tracking — Task Categorization, Tokens, Tool Usage

## Summary

Add per-agent lifetime stats tracking to the Rust backend. The data already flows through the system — sessions track `total_tokens` and `total_cost`, messages store per-message `tokens` and `cost`, and tool executions are logged in the events table. The job here is to aggregate this data at the agent level, add tool usage counters, and derive specialization scores. Stats are persisted in SQLite and exposed via a Tauri command for the frontend (MON-62) and avatar progression system (MON-60).

## Relevant files and areas

| File | Why it matters |
|------|---------------|
| `src-tauri/src/db.rs` (lines 123-218) | SQLite schema. Sessions already have `total_tokens` and `total_cost`. Messages have per-row `tokens` and `cost`. This is where new tables/columns and query methods go. |
| `src-tauri/src/db.rs` (lines 575-592) | `increment_session_message_count` — the existing hook where per-message token/cost is accumulated into the session row. The pattern to follow for agent-level aggregation. |
| `src-tauri/src/agent.rs` (lines 1347-1598) | Persistence pipeline. `PersistCommand::SaveAssistantMessage` is where token data enters the DB. This is the hook point for also updating agent-level stats. |
| `src-tauri/src/agent.rs` (lines 1440-1531) | `build_persist_commands` — extracts Usage from `MessageEnd` events and tool data from `ToolExecutionEnd`. Where tool usage counting would hook in. |
| `src-tauri/src/agent_state.rs` (lines 40-49, 61-70) | `Usage` and `ToolExecution` structs. The data shapes we'll be aggregating. |
| `src-tauri/src/sidecar_protocol.rs` | Event types — `MessageEnd` carries `Usage`, `ToolExecutionEnd` carries tool name and error status. |
| `src-tauri/src/lib.rs` (lines 38-96, 178-220) | Tauri command registration and specta builder. Where `get_agent_stats` gets registered. |

## What needs to change

### 1. New SQLite tables

Two new tables:

**`agent_stats`** — Aggregated lifetime stats per agent. One row per agent. Fields: agent_id (FK), total_sessions, total_messages, total_turns, total_tokens (input + output), total_cost, updated_at. This is a materialized aggregate — updated incrementally, not recomputed from scratch.

**`agent_tool_usage`** — Per-agent tool usage counters. One row per (agent_id, tool_name) pair. Fields: agent_id (FK), tool_name, call_count, error_count. Upserted on each tool execution.

Both tables should be created in `init_schema()` alongside the existing tables, with the same `CREATE TABLE IF NOT EXISTS` pattern.

### 2. Incremental stats updates in the persistence pipeline

Extend `PersistCommand` with two new variants:

- `IncrementAgentStats` — fired alongside `SaveAssistantMessage`, carries the token/cost delta for this message. The DB method does an upsert: insert with the delta if no row exists, or increment existing values.
- `RecordToolUsage` — fired alongside `SaveToolResult` (from `ToolExecutionEnd`), carries agent_id, tool_name, and is_error. The DB method upserts into `agent_tool_usage`, incrementing call_count and conditionally error_count.

This keeps stats aggregation in the same FIFO pipeline as message persistence — no ordering or consistency issues.

### 3. Session count tracking

When a new session is created (`spawn`, `new_session`, `switch_session`), increment the agent's `total_sessions` counter. This can be done directly in the existing session creation methods in `db.rs` rather than through the persist pipeline, since session creation is already a DB operation.

### 4. Specialization scoring

A pure Rust function (not persisted — computed on demand) that takes the agent's tool usage data and derives specialization scores. The heuristic:

- Map tool names to categories: `Read/Grep/Glob` → research, `Edit/Write` → coding, `Bash(test)` → testing, `Bash(git)` → devops, etc.
- Normalize counts into a 0-1 score per category
- Return as a struct: `{ frontend: f64, backend: f64, testing: f64, research: f64, devops: f64 }`

This is a best-effort heuristic. Tool names alone can't perfectly distinguish frontend vs backend — that would need file path analysis or LLM classification (out of scope). But tool distribution still gives a useful signal.

### 5. Tauri command: `get_agent_stats`

A single command that returns the full stats picture for an agent:

- Reads from `agent_stats` table (lifetime aggregates)
- Reads from `agent_tool_usage` table (tool breakdown)
- Computes specialization scores from tool usage
- Returns a single `AgentStats` struct (specta-typed for TS bindings)

Register in both `specta_builder()` and `generate_handler!()` in `lib.rs`.

### 6. Type definitions

New structs in `db.rs` (alongside existing row types):

- `AgentStats` — the full stats response object (serde + specta typed)
- `ToolUsageEntry` — per-tool stats (tool_name, call_count, error_count)
- `SpecializationScores` — derived category scores

All `#[derive(Serialize, specta::Type)]` so they auto-export to `bindings.ts`.

## Open questions

1. **Specialization categories** — I'm thinking: `research`, `coding`, `testing`, `devops`, `docs`. The tool-to-category mapping is lossy (e.g., `Bash` could be anything). Is this granularity fine, or do you want more/fewer categories? Frontend vs backend distinction specifically needs file path analysis which I'd defer.

2. **Experience normalization** — For the avatar's `experience` input (0-100), we said total tokens. What's the scale? Linear mapping with a cap (e.g., 10M tokens = 100)? Log scale? This affects how fast shadows "level up" visually.

3. **Backfill existing data** — Should we compute initial stats from existing sessions/messages for agents that already have history? This is a one-time migration query. Without it, existing agents start at zero stats.

## Out of scope

- Stats UI / profile card (MON-62)
- Avatar visual progression wiring (MON-60)
- LLM-based task categorization
- Real-time stats streaming
- Frontend/backend distinction in specialization (needs file path analysis)
- Per-session stats breakdown (we store session-level data already — this is agent-level aggregation)
