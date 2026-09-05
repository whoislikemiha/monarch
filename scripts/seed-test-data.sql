-- Monarch showcase demo data.
-- Idempotent: deletes everything it owns (ids prefixed 'demo-') before inserting.
-- Apply:   sqlite3 ~/.config/monarch/monarch.db < scripts/seed-demo.sql
-- Remove:  run the DELETE block below by itself.

PRAGMA foreign_keys = ON;
BEGIN;

-- ---------------------------------------------------------------- cleanup --
DELETE FROM classifications WHERE agent_id LIKE 'demo-%';
DELETE FROM messages WHERE session_id LIKE 'demo-%';
DELETE FROM sessions WHERE id LIKE 'demo-%';
DELETE FROM objective_events WHERE id LIKE 'demo-%';
DELETE FROM objective_plan_items WHERE id LIKE 'demo-%';
DELETE FROM objective_reports WHERE id LIKE 'demo-%';
DELETE FROM objective_refs WHERE id LIKE 'demo-%';
DELETE FROM agent_working_memory WHERE agent_id LIKE 'demo-%';
DELETE FROM memory_keeper_runs WHERE agent_id LIKE 'demo-%';
DELETE FROM memories WHERE agent_id LIKE 'demo-%';
DELETE FROM agent_tool_usage WHERE agent_id LIKE 'demo-%';
DELETE FROM agent_stats WHERE agent_id LIKE 'demo-%';
DELETE FROM shadow_identity_versions WHERE agent_id LIKE 'demo-%';
UPDATE agents SET current_objective_id = NULL WHERE id LIKE 'demo-%';
UPDATE projects SET root_objective_id = NULL WHERE id LIKE 'demo-%';
DELETE FROM objective_nodes WHERE id LIKE 'demo-%';
DELETE FROM agents WHERE id LIKE 'demo-%';
DELETE FROM agent_templates WHERE id LIKE 'demo-%';
DELETE FROM projects WHERE id LIKE 'demo-%';

-- ---------------------------------------------------------------- project --
INSERT INTO projects (id, name, root_path, instructions, created_at, updated_at) VALUES
  ('demo-project-helios', 'Helios', '/home/miha/pro/helios',
   'Helios is a realtime collaboration server (TypeScript + Rust). Prefer small PRs. All protocol changes need a soak test at 5% packet loss before merge.',
   '2026-05-11T09:00:00Z', '2026-07-01T08:30:00Z');

-- campaign root (kind=campaign, self-rooted, never closed)
INSERT INTO objective_nodes (id, root_id, parent_id, title, status, created_by, kind, created_at)
VALUES ('demo-obj-campaign', 'demo-obj-campaign', NULL, 'Helios', 'in_progress', 'monarch', 'campaign', '2026-05-11T09:00:01Z');
UPDATE projects SET root_objective_id = 'demo-obj-campaign' WHERE id = 'demo-project-helios';

-- ----------------------------------------------------------------- agents --
INSERT INTO agents (id, name, shadow_name, shadow_title, shadow_grade, provider, model, thinking_level,
                    cwd, project_id, context_window, created_at, updated_at) VALUES
  ('demo-agent-aria',     'Aria',     'Aria',     'Frontend Specialist',      'Senior', 'anthropic', 'claude-opus-4-7',   'high',   '/home/miha/pro/helios', 'demo-project-helios', 200000, '2026-05-12T10:00:00Z', '2026-07-03T15:07:00Z'),
  ('demo-agent-forge',    'Forge',    'Forge',    'Infrastructure & Systems', 'Principal', 'anthropic', 'claude-opus-4-7',   'xhigh',  '/home/miha/pro/helios', 'demo-project-helios', 200000, '2026-05-11T09:20:00Z', '2026-07-03T11:40:00Z'),
  ('demo-agent-scout',    'Scout',    'Scout',    'Research & Discovery',     'Mid', 'anthropic', 'claude-haiku-4-5',  'low',    '/home/miha/pro/helios', 'demo-project-helios', 200000, '2026-05-20T14:00:00Z', '2026-07-02T16:12:00Z'),
  ('demo-agent-sentinel', 'Sentinel', 'Sentinel', 'QA & Security',            'Junior', 'anthropic', 'claude-sonnet-4-5', 'medium', '/home/miha/pro/helios', 'demo-project-helios', 200000, '2026-06-02T11:00:00Z', '2026-07-01T18:03:00Z'),
  ('demo-agent-quill',    'Quill',    'Quill',    'Docs & Release Notes',     'Trainee', 'anthropic', 'claude-haiku-4-5',  'off',    '/home/miha/pro/helios', 'demo-project-helios', 200000, '2026-06-10T09:30:00Z', '2026-06-30T10:15:00Z');

-- ------------------------------------------------------------------ stats --
INSERT INTO agent_stats (agent_id, total_sessions, total_messages, total_turns,
                         total_input_tokens, total_output_tokens, total_cost, updated_at) VALUES
  ('demo-agent-forge',    214, 5120, 2380, 48200000, 3100000, 412.60, '2026-07-03T11:40:00Z'),
  ('demo-agent-aria',     132, 3411, 1690, 29400000, 2200000, 268.90, '2026-07-03T15:07:00Z'),
  ('demo-agent-scout',     88, 1904,  940, 15100000,  700000,  41.20, '2026-07-02T16:12:00Z'),
  ('demo-agent-sentinel',  61, 1233,  610,  9800000,  500000,  78.40, '2026-07-01T18:03:00Z'),
  ('demo-agent-quill',     37,  702,  350,  4200000,  300000,  12.80, '2026-06-30T10:15:00Z');

INSERT INTO agent_tool_usage (agent_id, tool_name, call_count, error_count) VALUES
  ('demo-agent-aria', 'Edit', 812, 14), ('demo-agent-aria', 'Read', 730, 3),
  ('demo-agent-aria', 'Write', 217, 2), ('demo-agent-aria', 'Bash', 402, 31),
  ('demo-agent-aria', 'Grep', 240, 0),  ('demo-agent-aria', 'TodoWrite', 95, 0),
  ('demo-agent-aria', 'WebFetch', 44, 5),
  ('demo-agent-forge', 'Bash', 1520, 88), ('demo-agent-forge', 'Edit', 610, 9),
  ('demo-agent-forge', 'Read', 890, 1),   ('demo-agent-forge', 'Write', 180, 0),
  ('demo-agent-forge', 'Grep', 300, 0),   ('demo-agent-forge', 'Agent', 40, 2),
  ('demo-agent-scout', 'Read', 1500, 2),  ('demo-agent-scout', 'Grep', 720, 0),
  ('demo-agent-scout', 'Glob', 410, 0),   ('demo-agent-scout', 'WebSearch', 260, 12),
  ('demo-agent-scout', 'WebFetch', 310, 24), ('demo-agent-scout', 'Edit', 60, 1),
  ('demo-agent-sentinel', 'Bash', 640, 52), ('demo-agent-sentinel', 'Read', 720, 0),
  ('demo-agent-sentinel', 'Grep', 510, 0),  ('demo-agent-sentinel', 'Edit', 210, 4),
  ('demo-agent-quill', 'Write', 340, 1), ('demo-agent-quill', 'Read', 610, 0),
  ('demo-agent-quill', 'WebFetch', 120, 8), ('demo-agent-quill', 'SendMessage', 88, 0),
  ('demo-agent-quill', 'Edit', 150, 2);

-- --------------------------------------------------------------- sessions --
INSERT INTO sessions (id, agent_id, model, provider, started_at, ended_at, message_count, total_tokens, total_cost, title) VALUES
  ('demo-sess-aria-1', 'demo-agent-aria', 'claude-opus-4-7', 'anthropic', '2026-07-03T14:05:00Z', NULL,                    8, 145200, 1.84, 'Websocket reconnect stabilization'),
  ('demo-sess-aria-2', 'demo-agent-aria', 'claude-opus-4-7', 'anthropic', '2026-06-28T09:12:00Z', '2026-06-29T17:44:00Z', 42, 812000, 9.13, 'OAuth token refresh flow'),
  ('demo-sess-aria-3', 'demo-agent-aria', 'claude-opus-4-7', 'anthropic', '2026-06-20T10:00:00Z', '2026-06-21T12:30:00Z', 18, 240100, 2.95, 'Onboarding empty-states polish'),
  ('demo-sess-forge-1','demo-agent-forge','claude-opus-4-7', 'anthropic', '2026-07-03T09:30:00Z', '2026-07-03T11:40:00Z', 24, 511000, 6.20, 'Cold-start profiling'),
  ('demo-sess-scout-1','demo-agent-scout','claude-haiku-4-5','anthropic', '2026-07-02T13:00:00Z', '2026-07-02T16:12:00Z', 31, 388000, 0.92, 'Competitor auth flow research');

-- messages: live showcase conversation (Pi content-block format)
INSERT INTO messages (session_id, role, content, model, tokens, cost, timestamp, duration_ms) VALUES
  ('demo-sess-aria-1', 'user',
   '[{"text":"The websocket bridge drops the connection every ~90s when the network gets flaky, and on every reconnect it replays the whole backlog. Can you make reconnect incremental?","type":"text"}]',
   NULL, 61, 0, '2026-07-03T14:05:12Z', NULL),
  ('demo-sess-aria-1', 'assistant',
   '[{"thinking":"The 90s cadence smells like the server heartbeat timeout, not the network itself. If replay saturates the socket after reconnect, the client pong never goes out and the server kills a healthy connection. Plan: reproduce under controlled packet loss, then split the fix into backoff (client side) and incremental replay (protocol).","type":"thinking"},{"text":"That 90s rhythm points at the server heartbeat timeout rather than the flaky network itself — if replay floods the socket after reconnect, our pong never gets out and the server closes a healthy connection.\n\nI''ll work it in three steps:\n1. Reproduce under 5% packet loss with the toxiproxy harness\n2. Add jittered exponential backoff to the reconnect loop\n3. Replay only frames newer than the last acked id, so reconnect cost stays flat\n\nStarting with the repro now.","type":"text"}]',
   'claude-opus-4-7', 412, 0.031, '2026-07-03T14:06:03Z', 51000),
  ('demo-sess-aria-1', 'user',
   '[{"text":"Prefer exponential backoff with jitter, and cap the replay window at the last acked frame.","type":"text"}]',
   NULL, 24, 0, '2026-07-03T14:31:40Z', NULL),
  ('demo-sess-aria-1', 'assistant',
   '[{"text":"Done on the backoff side: 250ms base doubling to an 8s ceiling, ±20% jitter so a fleet of clients doesn''t stampede the server after an outage. `ws-client` unit tests are green.\n\nNow wiring the replay cap — the server keeps a ring buffer of the last 512 frames, so replay-from-last-acked is a single seek, no full scan.","type":"text"}]',
   'claude-opus-4-7', 288, 0.022, '2026-07-03T14:48:12Z', 38000),
  ('demo-sess-aria-1', 'user',
   '[{"text":"Run the soak test again with 5% packet loss once the replay change is in.","type":"text"}]',
   NULL, 21, 0, '2026-07-03T15:01:30Z', NULL),
  ('demo-sess-aria-1', 'assistant',
   '[{"text":"Will do — replay-from-acked is half wired. Once the frame index lands I''ll kick the 30-minute soak at 5% loss and report reconnect counts against the baseline (14 drops / 30min).","type":"text"}]',
   'claude-opus-4-7', 96, 0.008, '2026-07-03T15:02:20Z', 12000);

INSERT INTO messages (session_id, role, content, model, tokens, cost, timestamp, duration_ms) VALUES
  ('demo-sess-aria-2', 'user',
   '[{"text":"Sessions die every hour when the access token expires. We need silent refresh.","type":"text"}]',
   NULL, 22, 0, '2026-06-28T09:12:30Z', NULL),
  ('demo-sess-aria-2', 'assistant',
   '[{"text":"Confirmed — the client holds the access token until a request 401s, then drops to the login screen. I''ll add a refresh scheduler that rotates the token 90 seconds before expiry, with a cross-tab lock so only one tab hits the token endpoint.","type":"text"}]',
   'claude-opus-4-7', 204, 0.016, '2026-06-28T09:13:15Z', 34000),
  ('demo-sess-aria-2', 'user',
   '[{"text":"Watch out for the token endpoint rate limit, we got 429s in staging before.","type":"text"}]',
   NULL, 19, 0, '2026-06-28T10:02:00Z', NULL),
  ('demo-sess-aria-2', 'assistant',
   '[{"text":"Good catch — it''s 10/min per client id. The shared-worker lock covers tabs in one browser; I''ll also add a ±15s spread on the refresh timer so multiple devices don''t sync up. Shipping the scheduler + broadcast channel now.","type":"text"}]',
   'claude-opus-4-7', 178, 0.014, '2026-06-28T10:03:10Z', 29000);

-- ---------------------------------------------------------- classifications --
INSERT INTO classifications (id, message_id, agent_id, session_id, complexity, confidence, rationale, model, tokens_in, tokens_out, latency_ms, created_at)
SELECT 'demo-clf-1', id, 'demo-agent-aria', 'demo-sess-aria-1', 'decomposable', 0.86,
       'Multi-step: diagnosis, protocol change, and replay logic — decomposes into a plan.',
       'anthropic/claude-haiku-4-5', 448, 38, 342, '2026-07-03T14:05:13Z'
FROM messages WHERE session_id='demo-sess-aria-1' AND role='user' AND content LIKE '%drops the connection%';
INSERT INTO classifications (id, message_id, agent_id, session_id, complexity, confidence, rationale, model, tokens_in, tokens_out, latency_ms, created_at)
SELECT 'demo-clf-2', id, 'demo-agent-aria', 'demo-sess-aria-1', 'simple', 0.91,
       'Direct implementation directive with explicit constraints.',
       'anthropic/claude-haiku-4-5', 210, 30, 208, '2026-07-03T14:31:41Z'
FROM messages WHERE session_id='demo-sess-aria-1' AND role='user' AND content LIKE '%exponential backoff with jitter%';
INSERT INTO classifications (id, message_id, agent_id, session_id, complexity, confidence, rationale, model, tokens_in, tokens_out, latency_ms, created_at)
SELECT 'demo-clf-3', id, 'demo-agent-aria', 'demo-sess-aria-1', 'simple', 0.88,
       'Single command execution request; no decomposition needed.',
       'anthropic/claude-haiku-4-5', 190, 27, 191, '2026-07-03T15:01:31Z'
FROM messages WHERE session_id='demo-sess-aria-1' AND role='user' AND content LIKE '%soak test again%';
INSERT INTO classifications (id, message_id, agent_id, session_id, complexity, confidence, rationale, model, tokens_in, tokens_out, latency_ms, created_at)
SELECT 'demo-clf-4', id, 'demo-agent-aria', 'demo-sess-aria-2', 'decomposable', 0.83,
       'Feature request spanning scheduler, cross-tab coordination, and error paths.',
       'anthropic/claude-haiku-4-5', 391, 41, 366, '2026-06-28T09:12:31Z'
FROM messages WHERE session_id='demo-sess-aria-2' AND role='user' AND content LIKE '%silent refresh%';

-- ------------------------------------------------------------- objectives --
INSERT INTO objective_nodes (id, root_id, parent_id, title, description, status, grade, exec_hint,
                             assignee_shadow_id, created_by, kind, scope, current_direction,
                             created_at, started_at, completed_at, estimated_tokens, actual_tokens, summary) VALUES
  ('demo-obj-ws', 'demo-obj-campaign', 'demo-obj-campaign',
   'Stabilize websocket reconnect under packet loss',
   'Connections drop every ~90s on flaky networks and reconnect replays the full backlog. Make reconnect incremental and survivable.',
   'in_progress', NULL, 'in_context', 'demo-agent-aria', 'monarch', 'objective',
   'sidecar bridge + frontend api layer',
   'Incremental replay from last acked frame; jittered backoff on the client loop',
   '2026-07-03T14:06:00Z', '2026-07-03T14:06:02Z', NULL, 600000, NULL, NULL),
  ('demo-obj-auth', 'demo-obj-campaign', 'demo-obj-campaign',
   'Ship OAuth token refresh flow',
   'Access tokens expire after an hour and kill the session. Refresh silently before expiry.',
   'done', 'A', 'in_context', 'demo-agent-aria', 'architect', 'objective',
   'auth layer', NULL,
   '2026-06-28T09:10:00Z', '2026-06-28T09:12:00Z', '2026-06-29T17:44:00Z', 700000, 812000,
   'Refresh tokens rotate silently; sessions survive expiry across tabs.'),
  ('demo-obj-cold', 'demo-obj-campaign', 'demo-obj-campaign',
   'Cut cold-start time below 800ms',
   'Server cold start is 2.3s; budget is 800ms. Profile, then split init.',
   'pending', NULL, 'delegate', 'demo-agent-forge', 'architect', 'objective',
   'server boot path', NULL,
   '2026-07-01T08:00:00Z', NULL, NULL, 400000, NULL, NULL),
  ('demo-obj-attach', 'demo-obj-campaign', 'demo-obj-campaign',
   'Move attachment store to content-addressed layout',
   'Dedupe identical uploads and make GC safe under concurrent writers.',
   'in_progress', NULL, 'in_context', 'demo-agent-forge', 'monarch', 'objective',
   'storage service', 'Hash-on-write with a two-phase GC mark',
   '2026-07-03T09:28:00Z', '2026-07-03T09:30:00Z', NULL, 500000, NULL, NULL),
  ('demo-obj-research', 'demo-obj-campaign', 'demo-obj-campaign',
   'Map competitor auth flows',
   'Survey how Linear, Figma and Notion handle silent refresh + multi-device sessions.',
   'done', 'B', 'in_context', 'demo-agent-scout', 'monarch', 'objective',
   'research', NULL,
   '2026-07-02T12:58:00Z', '2026-07-02T13:00:00Z', '2026-07-02T16:12:00Z', 200000, 388000,
   'Three flows mapped; all refresh ahead of expiry, none refresh on 401.');

UPDATE agents SET current_objective_id = 'demo-obj-ws'     WHERE id = 'demo-agent-aria';
UPDATE agents SET current_objective_id = 'demo-obj-attach' WHERE id = 'demo-agent-forge';

-- plan items: websocket objective (2 done, 1 active, 2 pending)
INSERT INTO objective_plan_items (id, objective_id, title, status, order_index, created_by, created_at, updated_at, completed_at) VALUES
  ('demo-plan-ws-1', 'demo-obj-ws', 'Reproduce drop with 5% packet loss harness',      'completed', 0, 'executor', '2026-07-03T14:07:00Z', '2026-07-03T14:15:20Z', '2026-07-03T14:15:20Z'),
  ('demo-plan-ws-2', 'demo-obj-ws', 'Add jittered exponential backoff to reconnect',   'completed', 1, 'executor', '2026-07-03T14:07:00Z', '2026-07-03T14:48:12Z', '2026-07-03T14:48:12Z'),
  ('demo-plan-ws-3', 'demo-obj-ws', 'Replay only frames after the last acked id',      'active',    2, 'executor', '2026-07-03T14:07:00Z', '2026-07-03T15:02:41Z', NULL),
  ('demo-plan-ws-4', 'demo-obj-ws', 'Soak test: 30 min at 5% packet loss',             'pending',   3, 'executor', '2026-07-03T14:07:00Z', '2026-07-03T14:07:00Z', NULL),
  ('demo-plan-ws-5', 'demo-obj-ws', 'Document the reconnect contract',                 'pending',   4, 'executor', '2026-07-03T14:07:00Z', '2026-07-03T14:07:00Z', NULL);

INSERT INTO objective_plan_items (id, objective_id, title, status, order_index, created_by, created_at, updated_at, completed_at) VALUES
  ('demo-plan-auth-1', 'demo-obj-auth', 'Refresh scheduler: rotate 90s before expiry', 'completed', 0, 'executor', '2026-06-28T09:14:00Z', '2026-06-28T14:02:00Z', '2026-06-28T14:02:00Z'),
  ('demo-plan-auth-2', 'demo-obj-auth', 'Cross-tab lock via shared worker',            'completed', 1, 'executor', '2026-06-28T09:14:00Z', '2026-06-29T11:20:00Z', '2026-06-29T11:20:00Z'),
  ('demo-plan-auth-3', 'demo-obj-auth', 'Backoff + spread to respect 10/min limit',    'completed', 2, 'executor', '2026-06-28T10:05:00Z', '2026-06-29T15:00:00Z', '2026-06-29T15:00:00Z'),
  ('demo-plan-auth-4', 'demo-obj-auth', 'E2E: session survives expiry in 3 tabs',      'completed', 3, 'executor', '2026-06-28T09:14:00Z', '2026-06-29T17:30:00Z', '2026-06-29T17:30:00Z');

-- ------------------------------------------------- timeline: websocket obj --
INSERT INTO objective_events (id, objective_id, event_type, actor, payload_json, created_at) VALUES
  ('demo-ev-ws-status1', 'demo-obj-ws', 'status_change', 'monarch',
   '{"from":"pending","to":"in_progress"}', '2026-07-03T14:06:02Z'),
  ('demo-ev-ws-plan', 'demo-obj-ws', 'plan_created', 'executor',
   '{"title":"Reproduce, backoff, incremental replay, soak, document","item_count":5}', '2026-07-03T14:07:01Z');

-- action 1: repro (closed)
INSERT INTO objective_events (id, objective_id, event_type, actor, payload_json, created_at, parent_event_id, plan_item_id) VALUES
  ('demo-ev-ws-a1', 'demo-obj-ws', 'coherent_action', 'executor',
   '{"intent":"Reproduce the drop under 5% packet loss","started_at":"2026-07-03T14:08:05Z"}',
   '2026-07-03T14:08:05Z', NULL, 'demo-plan-ws-1'),
  ('demo-ev-ws-a1-t1', 'demo-obj-ws', 'tool_call', 'executor',
   '{"tool_call_id":"demo-tc-ws-1","tool_name":"bash","target":"toxiproxy-cli create -l :26379 -u :6379 helios_ws","args_preview":"toxiproxy-cli create -l :26379 -u :6379 helios_ws","result_preview":"proxy helios_ws created","status":"done","is_error":false,"started_at":"2026-07-03T14:08:20Z","completed_at":"2026-07-03T14:08:22Z","duration_ms":2100}',
   '2026-07-03T14:08:20Z', 'demo-ev-ws-a1', NULL),
  ('demo-ev-ws-a1-t2', 'demo-obj-ws', 'tool_call', 'executor',
   '{"tool_call_id":"demo-tc-ws-2","tool_name":"bash","target":"npm run soak -- --loss 0.05 --duration 300","args_preview":"npm run soak -- --loss 0.05 --duration 300","result_preview":"5 drops in 300s; all at ~90s intervals; pong latency spikes to 41s during replay","status":"done","is_error":false,"started_at":"2026-07-03T14:09:00Z","completed_at":"2026-07-03T14:14:04Z","duration_ms":304000}',
   '2026-07-03T14:09:00Z', 'demo-ev-ws-a1', NULL),
  ('demo-ev-ws-a1-d1', 'demo-obj-ws', 'executor_decision', 'executor',
   '{"decision":"Root cause: replay backlog starves the pong frame, so the server heartbeat timeout kills a healthy connection","rationale":"Pong latency spikes to 41s during replay — well past the 30s heartbeat window"}',
   '2026-07-03T14:14:40Z', 'demo-ev-ws-a1', NULL),
  ('demo-ev-ws-a1-out', 'demo-obj-ws', 'action_outcome', 'executor',
   '{"outcome":"Repro harness in place; root cause identified (replay starves heartbeat pong)","auto_closed":false}',
   '2026-07-03T14:15:10Z', 'demo-ev-ws-a1', NULL);

INSERT INTO objective_events (id, objective_id, event_type, actor, payload_json, created_at, plan_item_id) VALUES
  ('demo-ev-ws-m1', 'demo-obj-ws', 'plan_item_completed', 'executor',
   '{"title":"Reproduce drop with 5% packet loss harness"}', '2026-07-03T14:15:20Z', 'demo-plan-ws-1');

-- action 2: backoff (closed)
INSERT INTO objective_events (id, objective_id, event_type, actor, payload_json, created_at, parent_event_id, plan_item_id) VALUES
  ('demo-ev-ws-a2', 'demo-obj-ws', 'coherent_action', 'executor',
   '{"intent":"Add jittered exponential backoff to the reconnect loop","started_at":"2026-07-03T14:32:10Z"}',
   '2026-07-03T14:32:10Z', NULL, 'demo-plan-ws-2'),
  ('demo-ev-ws-a2-t1', 'demo-obj-ws', 'tool_call', 'executor',
   '{"tool_call_id":"demo-tc-ws-3","tool_name":"read","target":"sidecar/src/ws-client.ts","args_preview":"sidecar/src/ws-client.ts","result_preview":"312 lines","status":"done","is_error":false,"started_at":"2026-07-03T14:32:20Z","completed_at":"2026-07-03T14:32:21Z","duration_ms":800}',
   '2026-07-03T14:32:20Z', 'demo-ev-ws-a2', NULL),
  ('demo-ev-ws-a2-t2', 'demo-obj-ws', 'tool_call', 'executor',
   '{"tool_call_id":"demo-tc-ws-4","tool_name":"edit","target":"sidecar/src/ws-client.ts","args_preview":"sidecar/src/ws-client.ts","result_preview":"reconnect(): 250ms base, x2 to 8s cap, ±20% jitter","status":"done","is_error":false,"started_at":"2026-07-03T14:35:02Z","completed_at":"2026-07-03T14:35:04Z","duration_ms":1900}',
   '2026-07-03T14:35:02Z', 'demo-ev-ws-a2', NULL),
  ('demo-ev-ws-a2-t3', 'demo-obj-ws', 'tool_call', 'executor',
   '{"tool_call_id":"demo-tc-ws-5","tool_name":"bash","target":"npm test -- ws-client","args_preview":"npm test -- ws-client","result_preview":"18 passed, 0 failed","status":"done","is_error":false,"started_at":"2026-07-03T14:44:00Z","completed_at":"2026-07-03T14:46:30Z","duration_ms":150000}',
   '2026-07-03T14:44:00Z', 'demo-ev-ws-a2', NULL),
  ('demo-ev-ws-a2-out', 'demo-obj-ws', 'action_outcome', 'executor',
   '{"outcome":"Backoff 250ms→8s with ±20% jitter; ws-client tests green","auto_closed":false}',
   '2026-07-03T14:47:50Z', 'demo-ev-ws-a2', NULL);

INSERT INTO objective_events (id, objective_id, event_type, actor, payload_json, created_at, plan_item_id) VALUES
  ('demo-ev-ws-m2', 'demo-obj-ws', 'plan_item_completed', 'executor',
   '{"title":"Add jittered exponential backoff to reconnect"}', '2026-07-03T14:48:12Z', 'demo-plan-ws-2');

-- action 3: incremental replay (OPEN — this is the live NOW action)
INSERT INTO objective_events (id, objective_id, event_type, actor, payload_json, created_at, parent_event_id, plan_item_id) VALUES
  ('demo-ev-ws-a3', 'demo-obj-ws', 'coherent_action', 'executor',
   '{"intent":"Replay only frames newer than the last acked id","started_at":"2026-07-03T15:02:41Z"}',
   '2026-07-03T15:02:41Z', NULL, 'demo-plan-ws-3'),
  ('demo-ev-ws-a3-t1', 'demo-obj-ws', 'tool_call', 'executor',
   '{"tool_call_id":"demo-tc-ws-6","tool_name":"read","target":"server/src/frame-buffer.rs","args_preview":"server/src/frame-buffer.rs","result_preview":"ring buffer, 512 frames, no index","status":"done","is_error":false,"started_at":"2026-07-03T15:03:00Z","completed_at":"2026-07-03T15:03:01Z","duration_ms":700}',
   '2026-07-03T15:03:00Z', 'demo-ev-ws-a3', NULL),
  ('demo-ev-ws-a3-t2', 'demo-obj-ws', 'tool_call', 'executor',
   '{"tool_call_id":"demo-tc-ws-7","tool_name":"edit","target":"server/src/frame-buffer.rs","args_preview":"server/src/frame-buffer.rs","status":"running","is_error":false,"started_at":"2026-07-03T15:05:12Z"}',
   '2026-07-03T15:05:12Z', 'demo-ev-ws-a3', NULL);

-- ------------------------------------------------- timeline: auth obj (done) --
INSERT INTO objective_events (id, objective_id, event_type, actor, payload_json, created_at) VALUES
  ('demo-ev-auth-s1', 'demo-obj-auth', 'status_change', 'monarch',
   '{"from":"pending","to":"in_progress"}', '2026-06-28T09:12:00Z');
INSERT INTO objective_events (id, objective_id, event_type, actor, payload_json, created_at, parent_event_id, plan_item_id) VALUES
  ('demo-ev-auth-a1', 'demo-obj-auth', 'coherent_action', 'executor',
   '{"intent":"Build the refresh scheduler with pre-expiry rotation","started_at":"2026-06-28T09:20:00Z"}',
   '2026-06-28T09:20:00Z', NULL, 'demo-plan-auth-1'),
  ('demo-ev-auth-a1-t1', 'demo-obj-auth', 'tool_call', 'executor',
   '{"tool_call_id":"demo-tc-auth-1","tool_name":"write","target":"src/auth/refresh.ts","args_preview":"src/auth/refresh.ts","result_preview":"scheduler: refresh at exp-90s, ±15s spread","status":"done","is_error":false,"started_at":"2026-06-28T10:15:00Z","completed_at":"2026-06-28T10:15:02Z","duration_ms":2200}',
   '2026-06-28T10:15:00Z', 'demo-ev-auth-a1', NULL),
  ('demo-ev-auth-a1-d1', 'demo-obj-auth', 'executor_decision', 'executor',
   '{"decision":"Refresh 90s before expiry instead of reacting to 401s","rationale":"Avoids one guaranteed failed request in the hot path per expiry"}',
   '2026-06-28T11:00:00Z', 'demo-ev-auth-a1', NULL),
  ('demo-ev-auth-a1-out', 'demo-obj-auth', 'action_outcome', 'executor',
   '{"outcome":"Scheduler in place; tokens rotate ahead of expiry","auto_closed":false}',
   '2026-06-28T13:58:00Z', 'demo-ev-auth-a1', NULL);
INSERT INTO objective_events (id, objective_id, event_type, actor, payload_json, created_at, parent_event_id, plan_item_id) VALUES
  ('demo-ev-auth-a2', 'demo-obj-auth', 'coherent_action', 'executor',
   '{"intent":"Coordinate refresh across tabs with a shared-worker lock","started_at":"2026-06-29T09:05:00Z"}',
   '2026-06-29T09:05:00Z', NULL, 'demo-plan-auth-2'),
  ('demo-ev-auth-a2-t1', 'demo-obj-auth', 'tool_call', 'executor',
   '{"tool_call_id":"demo-tc-auth-2","tool_name":"write","target":"src/auth/broadcast.ts","args_preview":"src/auth/broadcast.ts","result_preview":"BroadcastChannel + Web Lock; single refresher elected","status":"done","is_error":false,"started_at":"2026-06-29T09:40:00Z","completed_at":"2026-06-29T09:40:02Z","duration_ms":1800}',
   '2026-06-29T09:40:00Z', 'demo-ev-auth-a2', NULL),
  ('demo-ev-auth-a2-t2', 'demo-obj-auth', 'tool_call', 'executor',
   '{"tool_call_id":"demo-tc-auth-3","tool_name":"bash","target":"npm run e2e -- auth-refresh --tabs 3","args_preview":"npm run e2e -- auth-refresh --tabs 3","result_preview":"3 tabs, 2h simulated: 0 logouts, 24 rotations, 1 refresher per cycle","status":"done","is_error":false,"started_at":"2026-06-29T16:10:00Z","completed_at":"2026-06-29T17:28:00Z","duration_ms":4680000}',
   '2026-06-29T16:10:00Z', 'demo-ev-auth-a2', NULL),
  ('demo-ev-auth-a2-out', 'demo-obj-auth', 'action_outcome', 'executor',
   '{"outcome":"Cross-tab election works; e2e survives expiry in 3 tabs","auto_closed":false}',
   '2026-06-29T17:30:00Z', 'demo-ev-auth-a2', NULL);
INSERT INTO objective_events (id, objective_id, event_type, actor, payload_json, created_at) VALUES
  ('demo-ev-auth-s2', 'demo-obj-auth', 'status_change', 'executor',
   '{"from":"in_progress","to":"done"}', '2026-06-29T17:44:00Z');

-- first-person report on the finished auth objective
INSERT INTO objective_reports (id, objective_id, agent_id, payload, created_at, updated_at) VALUES
  ('demo-report-auth', 'demo-obj-auth', 'demo-agent-aria',
   '{"summary":"Access tokens now rotate silently 90 seconds before expiry, coordinated across tabs through a shared-worker lock. Sessions survived a full simulated workday in three tabs with zero logouts.","outcome":"shipped","grade":"A","decisions":[{"decision":"Rotate refresh tokens on every use","rationale":"Limits the replay window if a token ever leaks"},{"decision":"Refresh 90s before expiry, not on 401","rationale":"Avoids a guaranteed failed request in the hot path"}],"learned":["The token endpoint rate-limits at 10/min per client id — spread timers ±15s so devices do not sync up","BroadcastChannel alone is not enough; the Web Locks API is what makes single-refresher election race-free"],"artifacts":[{"file":"src/auth/refresh.ts","role":"refresh scheduler"},{"file":"src/auth/broadcast.ts","role":"cross-tab election"}],"open_threads":["Mobile webview cannot use the shared-worker path — needs a native-side lock"],"reflection":"I should have read the rate-limit docs before the first staging run — the 429 storm cost an hour."}',
   '2026-06-29T17:45:00Z', '2026-06-29T17:45:00Z');

-- forge: one open action on the attachment objective
INSERT INTO objective_events (id, objective_id, event_type, actor, payload_json, created_at, parent_event_id) VALUES
  ('demo-ev-att-a1', 'demo-obj-attach', 'coherent_action', 'executor',
   '{"intent":"Hash uploads on write and dual-write to the CAS layout","started_at":"2026-07-03T09:45:00Z"}',
   '2026-07-03T09:45:00Z', NULL),
  ('demo-ev-att-a1-t1', 'demo-obj-attach', 'tool_call', 'executor',
   '{"tool_call_id":"demo-tc-att-1","tool_name":"edit","target":"server/src/storage/attachments.rs","args_preview":"server/src/storage/attachments.rs","result_preview":"blake3 on ingest; write to cas/{prefix}/{hash}","status":"done","is_error":false,"started_at":"2026-07-03T10:02:00Z","completed_at":"2026-07-03T10:02:03Z","duration_ms":2600}',
   '2026-07-03T10:02:00Z', 'demo-ev-att-a1'),
  ('demo-ev-att-a1-t2', 'demo-obj-attach', 'tool_call', 'executor',
   '{"tool_call_id":"demo-tc-att-2","tool_name":"bash","target":"cargo test -p storage","args_preview":"cargo test -p storage","result_preview":"42 passed; dedupe ratio on fixture set: 3.1x","status":"done","is_error":false,"started_at":"2026-07-03T11:20:00Z","completed_at":"2026-07-03T11:24:10Z","duration_ms":250000}',
   '2026-07-03T11:20:00Z', 'demo-ev-att-a1');

-- scout: research objective report
INSERT INTO objective_events (id, objective_id, event_type, actor, payload_json, created_at) VALUES
  ('demo-ev-res-s1', 'demo-obj-research', 'status_change', 'monarch',
   '{"from":"pending","to":"in_progress"}', '2026-07-02T13:00:00Z'),
  ('demo-ev-res-s2', 'demo-obj-research', 'status_change', 'executor',
   '{"from":"in_progress","to":"done"}', '2026-07-02T16:12:00Z');
INSERT INTO objective_reports (id, objective_id, agent_id, payload, created_at, updated_at) VALUES
  ('demo-report-research', 'demo-obj-research', 'demo-agent-scout',
   '{"summary":"All three competitors refresh ahead of expiry rather than reacting to 401s. Linear rotates refresh tokens per use; Figma pins one refresher per device, not per tab; Notion falls back to a full re-auth iframe when refresh fails twice.","outcome":"delivered","grade":"B","decisions":[],"learned":["Pre-expiry refresh is industry consensus — reactive 401 handling appears in none of the three","Per-device (not per-tab) refresher election is the common pattern"],"artifacts":[{"file":"thoughts/research/auth-flows.md","role":"survey write-up"}],"open_threads":["Could not verify Notion mobile behavior without a jailbroken device"],"reflection":"Web archive snapshots were enough — no need for the traffic-capture setup I planned."}',
   '2026-07-02T16:13:00Z', '2026-07-02T16:13:00Z');

-- --------------------------------------------------------- working memory --
INSERT INTO agent_working_memory (agent_id, payload_json, updated_at) VALUES
  ('demo-agent-aria',
   '{"schemaVersion":1,"currentObjectiveId":"demo-obj-ws","currentObjectivePath":["Helios","Stabilize websocket reconnect under packet loss"],"currentAction":{"eventId":"demo-ev-ws-a3","objectiveId":"demo-obj-ws","intent":"Replay only frames newer than the last acked id","startedAt":"2026-07-03T15:02:41Z"},"recentActions":[{"eventId":"demo-ev-ws-a2","objectiveId":"demo-obj-ws","intent":"Add jittered exponential backoff to the reconnect loop","outcome":"Backoff 250ms→8s with ±20% jitter; ws-client tests green","completedAt":"2026-07-03T14:47:50Z"},{"eventId":"demo-ev-ws-a1","objectiveId":"demo-obj-ws","intent":"Reproduce the drop under 5% packet loss","outcome":"Repro harness in place; root cause identified","completedAt":"2026-07-03T14:15:10Z"}],"updatedAt":"2026-07-03T15:07:00Z","activePlanItemId":"demo-plan-ws-3","nextPlanItemIds":["demo-plan-ws-4","demo-plan-ws-5"]}',
   '2026-07-03T15:07:00Z'),
  ('demo-agent-forge',
   '{"schemaVersion":1,"currentObjectiveId":"demo-obj-attach","currentObjectivePath":["Helios","Move attachment store to content-addressed layout"],"currentAction":{"eventId":"demo-ev-att-a1","objectiveId":"demo-obj-attach","intent":"Hash uploads on write and dual-write to the CAS layout","startedAt":"2026-07-03T09:45:00Z"},"recentActions":[],"updatedAt":"2026-07-03T11:24:10Z","activePlanItemId":null,"nextPlanItemIds":[]}',
   '2026-07-03T11:24:10Z');

-- ---------------------------------------------------------------- memories --
-- Aria: self scope
INSERT INTO memories (agent_id, layer, category, content, scope, kind, title, summary, created_at, access_count, last_accessed_at, source_session_id) VALUES
  ('demo-agent-aria', 'hot', 'general',
   'The frontend is Svelte 5 runes only: $state, $derived, $effect. Legacy $: reactive statements and writable stores are banned — they break under the new compiler output and reviewers reject them on sight.',
   'self', 'convention', 'Svelte 5 runes only', 'No legacy $: statements or writable stores anywhere in the frontend.',
   '2026-06-12T10:00:00Z', 34, '2026-07-03T14:32:00Z', 'demo-sess-aria-3'),
  ('demo-agent-aria', 'hot', 'general',
   'Run the affected test file before and after any refactor of shared modules. A green suite before the change is the only trustworthy baseline.',
   'self', 'preference', 'Baseline tests before refactors', 'Always establish a green baseline before touching shared code.',
   '2026-06-18T09:00:00Z', 21, '2026-07-03T14:44:00Z', 'demo-sess-aria-3');

-- Aria: project scope — parent + children
INSERT INTO memories (id, agent_id, layer, category, content, scope, project_id, kind, title, summary, created_at, access_count, last_accessed_at, source_objective_id, source_session_id) VALUES
  (9001, 'demo-agent-aria', 'hot', 'general',
   'Helios auth: access tokens live 1h; refresh happens 90s before expiry via src/auth/refresh.ts; one refresher is elected per browser through a shared-worker Web Lock (src/auth/broadcast.ts).',
   'project', 'demo-project-helios', 'fact', 'Helios auth architecture',
   'Silent pre-expiry refresh with cross-tab single-refresher election.',
   '2026-06-29T18:00:00Z', 18, '2026-07-03T09:10:00Z', 'demo-obj-auth', 'demo-sess-aria-2');
INSERT INTO memories (agent_id, layer, category, content, scope, project_id, parent_id, kind, title, summary, created_at, access_count, last_accessed_at, source_objective_id) VALUES
  ('demo-agent-aria', 'hot', 'general',
   'Refresh tokens rotate on every use. A refresh response always carries a new refresh token; reusing an old one revokes the whole session family.',
   'project', 'demo-project-helios', 9001, 'decision', 'Refresh tokens rotate on use',
   'One-time-use refresh tokens; reuse revokes the session family.',
   '2026-06-29T18:01:00Z', 12, '2026-07-02T13:40:00Z', 'demo-obj-auth'),
  ('demo-agent-aria', 'hot', 'general',
   'The token endpoint rate-limits at 10 requests/min per client id. Spread refresh timers ±15s and never retry a 429 sooner than 30s.',
   'project', 'demo-project-helios', 9001, 'constraint', 'Token endpoint: 10/min per client id',
   'Hard rate limit on the token endpoint; spread timers, back off 429s.',
   '2026-06-29T18:02:00Z', 15, '2026-07-03T08:55:00Z', 'demo-obj-auth'),
  ('demo-agent-aria', 'hot', 'general',
   'sidecar/src/ws-client.ts is the single entry point for reconnect behavior — every reconnect policy change goes there, nowhere else. The server side ring buffer lives in server/src/frame-buffer.rs (512 frames).',
   'project', 'demo-project-helios', NULL, 'landmark', 'ws-client.ts owns reconnect',
   'Single entry point for reconnect policy; server ring buffer is frame-buffer.rs.',
   '2026-07-03T14:20:00Z', 6, '2026-07-03T15:03:00Z', 'demo-obj-ws');

-- Aria: supervisor scope
INSERT INTO memories (agent_id, layer, category, content, scope, kind, title, summary, created_at, access_count, last_accessed_at) VALUES
  ('demo-agent-aria', 'hot', 'general',
   'The supervisor wants status updates as one short paragraph with the cost number included — no bullet lists, no play-by-play.',
   'captain', 'preference', 'Terse status updates with cost', 'One paragraph, include spend, skip the play-by-play.',
   '2026-06-15T12:00:00Z', 28, '2026-07-03T14:06:00Z'),
  ('demo-agent-aria', 'hot', 'general',
   'Corrected once: never claim a soak test passed from a partial run. Report the configured duration and the actual duration together.',
   'captain', 'correction', 'Report soak tests honestly', 'State configured vs actual duration; partial runs are not passes.',
   '2026-06-22T16:30:00Z', 9, '2026-07-03T14:14:00Z');

-- Forge memories
INSERT INTO memories (agent_id, layer, category, content, scope, project_id, kind, title, summary, created_at, access_count, last_accessed_at, source_objective_id) VALUES
  ('demo-agent-forge', 'hot', 'general',
   'Attachment bytes are content-addressed under cas/{first-2-hex}/{blake3}. The legacy uuid layout is read-only during migration; writes are dual-path until the GC epoch flips.',
   'project', 'demo-project-helios', 'fact', 'CAS attachment layout',
   'blake3-addressed storage; dual-write during migration.',
   '2026-07-03T10:30:00Z', 4, '2026-07-03T11:20:00Z', 'demo-obj-attach'),
  ('demo-agent-forge', 'hot', 'general',
   'Cold-start budget is 800ms, measured at p95 on the CI runner class, not on dev machines. Current p95 is 2.3s; the schema migration check alone costs 900ms.',
   'project', 'demo-project-helios', 'constraint', 'Cold-start budget: 800ms p95',
   'Budget measured on CI runner class; migrations are the big cost.',
   '2026-07-01T08:10:00Z', 7, '2026-07-03T09:31:00Z', 'demo-obj-cold');
INSERT INTO memories (agent_id, layer, category, content, scope, kind, title, summary, created_at, access_count, last_accessed_at) VALUES
  ('demo-agent-forge', 'hot', 'general',
   'Prefer boring infrastructure: no new datastore unless two existing ones demonstrably cannot express the workload.',
   'self', 'preference', 'Boring infrastructure first', 'New datastores need proof two existing ones fail.',
   '2026-05-20T09:00:00Z', 41, '2026-07-03T09:29:00Z');

-- Scout memories
INSERT INTO memories (agent_id, layer, category, content, scope, project_id, kind, title, summary, created_at, access_count, last_accessed_at, source_objective_id) VALUES
  ('demo-agent-scout', 'hot', 'general',
   'Industry consensus on auth refresh: rotate ahead of expiry, elect one refresher per device, never rely on 401 reactions. Sources: Linear, Figma, Notion (July 2026 survey).',
   'project', 'demo-project-helios', 'fact', 'Auth refresh: industry consensus',
   'Pre-expiry rotation + per-device election is the standard pattern.',
   '2026-07-02T16:20:00Z', 11, '2026-07-03T08:50:00Z', 'demo-obj-research');

-- ------------------------------------------------------------ keeper runs --
INSERT INTO memory_keeper_runs (agent_id, trigger, objective_id, started_at, completed_at,
                                tokens_input, tokens_output, model_id, output_summary, outcome) VALUES
  ('demo-agent-aria', 'objective_complete', 'demo-obj-auth', '2026-06-29T17:50:00Z', '2026-06-29T17:51:40Z',
   48200, 1850, 'anthropic/claude-haiku-4-5',
   'Distilled 4 claims from the OAuth refresh objective: architecture fact, rotation decision, rate-limit constraint, webview open thread.', 'completed'),
  ('demo-agent-aria', 'session_end', NULL, '2026-06-21T12:35:00Z', '2026-06-21T12:36:10Z',
   21400, 920, 'anthropic/claude-haiku-4-5',
   'Distilled 2 claims from the onboarding polish session: runes convention, baseline-test preference.', 'completed'),
  ('demo-agent-scout', 'objective_complete', 'demo-obj-research', '2026-07-02T16:15:00Z', '2026-07-02T16:16:05Z',
   30100, 1100, 'anthropic/claude-haiku-4-5',
   'Distilled 1 claim: industry consensus on pre-expiry refresh rotation.', 'completed');

-- --------------------------------------------------------------- identity --
INSERT INTO shadow_identity_versions (agent_id, payload, created_at, edit_note) VALUES
  ('demo-agent-aria',
   'You are Aria, the frontend specialist of the Helios fleet. You own everything the user sees and the client half of every protocol. You are precise about reactive state, allergic to flaky tests, and you never ship a reconnect path you have not watched fail first. When a change spans the wire protocol, you coordinate with Forge before touching the server side.',
   '2026-06-15T10:00:00Z', 'tightened protocol-coordination rule'),
  ('demo-agent-forge',
   'You are Forge, infrastructure and systems. You keep the servers boring, the storage durable, and the budgets honest. You measure before you optimize and you write down every operational constraint you discover.',
   '2026-05-11T09:25:00Z', NULL);
UPDATE agents SET identity_version_id = (SELECT id FROM shadow_identity_versions WHERE agent_id='demo-agent-aria' ORDER BY id DESC LIMIT 1) WHERE id='demo-agent-aria';
UPDATE agents SET identity_version_id = (SELECT id FROM shadow_identity_versions WHERE agent_id='demo-agent-forge' ORDER BY id DESC LIMIT 1) WHERE id='demo-agent-forge';

-- -------------------------------------------------------------- templates --
INSERT INTO agent_templates (id, name, provider, model, thinking_level, cwd, shadow_name, shadow_title, shadow_grade, created_at, updated_at) VALUES
  ('demo-tmpl-frontend', 'Frontend Specialist', 'anthropic', 'claude-opus-4-7', 'high', '/home/miha/pro/helios', NULL, 'Frontend Specialist', 'Mid', '2026-06-01T09:00:00Z', '2026-06-01T09:00:00Z'),
  ('demo-tmpl-researcher', 'Researcher', 'anthropic', 'claude-haiku-4-5', 'low', '/home/miha/pro/helios', NULL, 'Research & Discovery', 'Junior', '2026-06-01T09:05:00Z', '2026-06-01T09:05:00Z');

COMMIT;
