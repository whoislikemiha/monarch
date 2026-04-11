# MON-63: Shadow Stats Tracking

## What was implemented

Per-agent lifetime stats tracking in the Rust backend. Token usage, tool call counts, session/turn/message metrics all accumulate incrementally via the existing FIFO persistence pipeline. A Tauri command returns the full stats picture including 12-category specialization scoring and a log-scale experience level.

## Key decisions

- **Incremental upserts, not recomputation** — Stats are updated atomically via SQL `ON CONFLICT DO UPDATE SET col = col + delta`. No expensive aggregation queries on read.
- **Stats wired into existing persist pipeline** — New `PersistCommand` variants (`IncrementAgentStats`, `RecordToolUsage`, `IncrementAgentTurns`) flow through the same FIFO channel as message persistence. Ordering guaranteed.
- **12 specialization categories** — coding, research, testing, debugging, devops, documentation, database, configuration, design, communication, refactoring, security. Most categories will only score above zero once MON-64 (file path analysis) lands — tool names alone primarily distinguish coding vs research vs devops.
- **Experience = log10(total_tokens) * 15, capped at 100** — Log scale so early growth feels fast, later growth slows. ~100K tokens ≈ 75, ~1M ≈ 90, 10M = 100.
- **No backfill** — Existing agents start at zero. Simpler and avoids migration complexity.
- **Input/output tokens tracked separately** — Enables future analysis of agent verbosity vs consumption.

## Files touched

- `src-tauri/src/db.rs` — New tables, types (AgentStats, ToolUsageEntry, SpecializationScores), DB methods, compute_specialization(), Tauri command
- `src-tauri/src/agent.rs` — Extended PersistCommand enum + apply(), build_persist_commands() emits stats commands, session creation calls increment_agent_sessions
- `src-tauri/src/lib.rs` — Registered db_get_agent_stats in specta + generate_handler
- `src/lib/bindings.ts` — Regenerated with new types

## What was left out

- Stats UI (MON-62)
- Avatar progression wiring (MON-60)
- File-path-based specialization (MON-64)
- LLM-based task classification (MON-65)
- Backfill of existing agent history
