# MON-8 — LM Studio context size: configurable window + fix inflated context meter

## Summary

Monarch currently lies to users about LM Studio context usage in two independent ways, and the symptoms compound. First, the sidecar registers every LM Studio model with a hardcoded `contextWindow` of 32000 — it has no knowledge of the context length the user actually loaded the model with in LM Studio, so an 8k `ministral-3-14b-reasoning` is still labelled "32k" in the meter. Second, the context meter's *numerator* is `sessionStats.totalTokens`, which is a lifetime billing accumulator (summed per-turn `usage.totalTokens`), not a measure of what is currently sitting in the model's context window. Across several turns that number grows far faster than real context occupancy, and quickly pegs the meter. The investigation confirms the meter is a display bug, not LM Studio flushing anything unexpectedly: LM Studio was almost certainly truncating at 8k internally while the meter happily reported ~32k used. This plan addresses both halves together: plumb a real, user-configurable context window for LM Studio models end-to-end, and rewrite the meter's numerator to reflect live context occupancy instead of session billing.

## Relevant files and areas

- `sidecar/src/runtime-manager.ts`
  - Line 31: `LMSTUDIO_DEFAULT_CONTEXT_WINDOW = 32000` — the hardcoded fallback.
  - Lines 83–125: `buildDynamicModel` constructs the `Model<Api>` for both `openrouter` and `lmstudio` and hands it to `session.setModel`. This is where a user-supplied context window needs to land.
  - Lines 148–154: `resolveModel` — falls back to `buildDynamicModel` for unknown models. Any context override must flow through here so both `createSession` and `setModel` paths honour it.
  - Lines 164–286: `createSession` — receives `CreateSessionCommand` and emits `session_ready` with `contextWindow: model?.contextWindow`. This is the event the frontend currently reads to size the meter's denominator.
  - Lines 325–347: `setModel` — mid-session model switch; must accept and apply a new context window too.
- `sidecar/src/protocol.ts` — shape of `CreateSessionCommand` and the `session_ready`/`setModel` messages. Needs an optional `contextWindow` field on the inbound commands.
- `src-tauri/src/agent.rs`
  - Lines 360–407: `message_end` handler — pulls `totalTokens` off each assistant message and calls `db.increment_session_message_count`. This is where billing accumulates. The bug is not here per se (the accumulator is correct *as billing*), but this is the code path whose output the meter was mistakenly using.
  - The Tauri commands that build and send `CreateSessionCommand`/`setModel` to the sidecar — they need to forward a context window value from the frontend.
- `src-tauri/src/db.rs`
  - `increment_session_message_count` at line 406: adds `tokens` and `cost` to `sessions.total_tokens` / `total_cost`. Stays as-is (billing is correct).
  - `sessions` schema (line 71+) — decide whether the per-agent or per-session chosen LM Studio context window wants to be persisted here, or on a different table keyed by (provider, model).
- `src/lib/AgentControls.svelte`
  - Lines 1–62: the meter. `displayTokens = sessionStats?.totalTokens ?? lastUsage?.totalTokens ?? 0` is the faulty numerator. `contextWindow` prop is the denominator. Both need to change — numerator should come from a live measure, denominator should honour a user-provided LM Studio value and drop the "estimated" flag when it is present.
- `src/lib/AgentView.svelte`
  - Lines ~227–230: composes `sessionStats` from the active session and forwards it to `AgentControls`.
  - Lines ~311–314: consumes `session_ready.contextWindow` and stores it on the agent.
  - Lines ~406–428: collects `lastUsage` from streaming assistant messages — this is already available and is the correct source for live context occupancy going forward.
- `src/lib/SpawnDialog.svelte` — the agent spawn UI (already includes LM Studio provider support per prior MON-7 work). This is the natural place for the new "context window" input when an LM Studio model is picked. Lines 36–42 show the provider list and the refresh set.
- `src-tauri/src/models.rs` — LM Studio discovery (`/v1/models`) lives here. Relevant mainly because it's where a future auto-detect (via LM Studio's newer REST API) would plug in, and because the `ModelInfo` shape is what the SpawnDialog consumes.
- `src/lib/types.ts` — `Usage` and `SessionStats` shapes; any new live-context field travels through here.
- `ONBOARDING.md` — has a short "LM Studio" section that will need a one-line update to mention the new context setting (keep edit minimal).

## What needs to change

**1. User-configurable context window for LM Studio.**
Add an optional context window field to the `CreateSessionCommand` and `setModel` protocol messages between Rust and the sidecar. In `runtime-manager.ts::buildDynamicModel`, when provider is `lmstudio`, prefer the supplied value over `LMSTUDIO_DEFAULT_CONTEXT_WINDOW`. The default stays as a last-resort fallback so users who don't care still get working behaviour.

Decide where the value is collected: the simplest and most aligned with the current design is a per-agent value captured at spawn time in `SpawnDialog.svelte`, only shown when the selected provider is `lmstudio`. A secondary, lower-cost option is a per-(provider, model) persisted value so the user configures once per model they run. Both options are discussed under open questions; the plan assumes a per-spawn input as the default to keep the first pass scoped.

On the Rust side, `agent.rs` needs to accept the context window from the frontend Tauri command (extend the spawn command parameters), carry it into `CreateSessionCommand`, and forward it on `setModel` if the user changes models mid-session. If we go with the persisted per-model variant, a small new table (or a JSON blob in settings) will store these, and Rust will look it up at spawn time.

**2. Fix the meter numerator.**
The meter's "current tokens" should reflect what is actually in the model's context for the *next* request, not the sum of billing across every turn. The simplest correct answer: use the most recent assistant message's `usage.input` (or `usage.totalTokens` if `input` is not exposed) as the live figure. `lastUsage` is already being tracked in `AgentView.svelte` from streaming events — the fix is mostly removing the `sessionStats?.totalTokens ??` preference in `AgentControls.svelte:34–35` and picking the right field from `lastUsage`.

Keep `sessionStats.totalTokens` and `sessionStats.totalCost` for what they actually are — session-lifetime billing — and surface them separately (either as a distinct readout in the controls bar or as a tooltip). Do not feed them into the meter.

Update the "estimated" flag in `AgentControls.svelte:44`: once a user has supplied an LM Studio context window, the denominator is authoritative and should not be marked estimated.

**3. Session restore and continuation.**
When an agent is restored (sidecar recovery or session continuation), Rust rebuilds the `CreateSessionCommand`. The restored command must carry the same LM Studio context window the agent was originally spawned with, or the meter will silently revert to 32k after a crash/restart. This means the context window value has to be persisted somewhere Rust can read at restore time — either on the `agents` row, on the `sessions` row, or in a per-(provider, model) settings store.

**4. Docs.**
`ONBOARDING.md`'s LM Studio paragraph gets a single sentence noting that the user should set the context size to match what they loaded in LM Studio. No other doc updates.

## Decisions (confirmed with user)

1. **Per-agent at spawn**, input shown in `SpawnDialog.svelte` when the selected provider is `lmstudio`. Marko's local setup runs ~4 LM Studio agents in parallel, so the per-agent input needs to be quick to fill (default to last-used value for the same model is a nice-to-have, not a requirement).
2. **Auto-detect via LM Studio's `/api/v0/models`** is out of scope for MON-8 and tracked as a separate issue/plan.
3. **OpenRouter's hardcoded 128k default is fixed in this same issue** — same plumbing path, same meter fix. Treat as a second instance of the same bug, not a separate concern.
4. **Keep a session-lifetime billing readout** in the controls bar alongside the live context meter. Not critical for LM Studio (local, free) but important for paid providers, and it's cheap to keep the number visible.

## Remaining open questions

1. **Which `usage` field is the right live-context measure?** The ideal is "tokens in the prompt at time of the most recent send", which is `usage.input` on the most recent assistant message. If `Usage.input` in Monarch's type system is reliably populated by the Pi SDK for LM Studio's OpenAI-compatible responses, use it directly. If not (LM Studio's token accounting is known to be patchy), fall back to `usage.totalTokens - usage.output`, and if that's also missing, fall back to `usage.totalTokens` as an upper bound. Quick empirical check during implementation.
2. **Persistence location for the per-agent context window.** Candidates: a new column on the `agents` row (simple, survives restart), a column on `sessions` (survives restart per-session, but doesn't carry across new sessions for the same agent), or a small per-(provider, model) settings blob (better ergonomics long-term). Recommend: column on `agents`, since the context window is an agent-level property tied to the model selection made at spawn time. To be finalised during implementation.

## Out of scope reminders

- Auto-detection from LM Studio's REST API (tracked separately).
- Any change to how billing (`total_tokens`, `total_cost`) is persisted — only how it's *displayed*.
- Context compaction, pruning, summarisation, or other real context management. This issue is strictly about honestly measuring and displaying context usage.
- OpenRouter's own hardcoded 128k default (flagged as related but explicitly not fixed here).
- UI redesign of the controls bar beyond what's required to show live-context and surface billing separately.
