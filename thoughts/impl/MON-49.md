# MON-49 — Implementation notes

## What was implemented

Added two new indexes on the `events` table and a 30-day retention prune that runs on DB init.

- `idx_events_agent_session` on `(agent_id, session_id)` — covers the "get events for session X" query called out in the ticket.
- `idx_events_timestamp` on `(timestamp)` — supports the retention sweep itself and any time-range forensic queries.
- Startup prune: `DELETE FROM events WHERE timestamp < datetime('now', '-30 days')`. Errors swallowed so a failed prune can't block app boot.

## Key decisions

- **30-day startup prune over max-rows cap.** Matches the ticket's suggested SQL verbatim. Simpler than a row-count cap, and the events table is forensic (not operational), so bounded-by-time is the right model.
- **Prune errors are swallowed.** Consistent with the other `let _ = conn.execute_batch(…)` migration calls in the same block. Retention is a nicety, not load-bearing — boot must not depend on it.

## Files touched

- `src-tauri/src/db.rs` — two new `CREATE INDEX IF NOT EXISTS` lines in the init block; a new `DELETE FROM events` call after the existing migrations.

## What was left out

- No configurable retention window. 30 days is hardcoded; if that becomes a pain, expose it via `ui_state` or a settings row later.

Shipped as part of the bundled bug-sweep PR (#57) alongside MON-48 and MON-55.
