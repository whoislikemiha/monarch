# MON-8 — LM Studio configurable context window + meter fix

PR: https://github.com/whoislikemiha/monarch/pull/5

## What was implemented

Two independent bugs that compounded into "the meter lies about LM Studio context usage":

1. **Configurable LM Studio context window.** The sidecar previously hardcoded `contextWindow: 32000` for every LM Studio model, regardless of what the user actually loaded the model with. There's now an optional `contextWindow` field on `CreateSessionCommand`/`SetModelCommand`, plumbed from a new per-agent slider in `SpawnDialog` (shown only when provider = `lmstudio`) through `AgentConfig` → `spawn_agent` Tauri command → sidecar `buildDynamicModel`. The 32k default survives as a last-resort fallback.

2. **Meter numerator fix.** The meter was using `sessionStats.totalTokens` — a session-lifetime billing accumulator — as its "current context" number. Across a few turns that pegs the meter regardless of real occupancy. Numerator now comes from the most recent assistant message's `usage` (input + cacheRead/Write, with `totalTokens - output` and then `totalTokens` as fallbacks). Display format is `used/total (pct%)` and warning/critical thresholds flipped to "% used" (≥70% warn, ≥90% critical). Session-lifetime billing is kept visible as a separate `Σ tokens · $cost` chip.

## Key decisions

- **Persistence location**: `agents.context_window` column, not `sessions`, not a provider/model settings blob. An agent is spawned with a specific LM Studio model at a specific loaded context length; if you continue or restart, the same agent keeps the same window. `spawn_agent` has a fallback lookup that reads the persisted value when the JS caller omits it (restore / crash-recovery path), so the window survives restart without the UI having to repopulate it.
- **OpenRouter**: the numerator fix applies to OpenRouter for free (provider-agnostic), but the user-facing context input is LM Studio-only. OpenRouter knows its own context lengths; the hardcoded 128k in `buildDynamicModel` is left alone. This deviates from one reading of the plan (which said "OpenRouter is fixed in the same issue") but matches the intent confirmed during implementation.
- **Slider, not number input**. First pass was a plain `<input type="number">` with `$state<number>(...) + bind:value`. In practice typed values weren't propagating (user typed 8000, meter still showed 32k). Replaced with a range slider (1024–262144, 1024 step) + preset chips (4k/8k/16k/32k/64k/128k/256k), explicit `oninput` handler with `Number()` coercion, and the idiomatic `let x: number = $state(...)` declaration. Also gives users what they actually asked for.
- **Stale `sidecar/dist/`** bit us twice during verification — nothing in the repo auto-rebuilds the sidecar. Worth adding a `prebuild` hook or CI check in a follow-up; noted as a footnote on the PR.
- **`icon.ico`** was missing locally and blocked `cargo check`. Generated a throwaway ICO to unblock, deleted before committing. Unrelated environment issue — a real icon probably belongs in `src-tauri/icons/` but that's out of scope for MON-8.
- **Numerator fallback chain**: `input + cache` → `totalTokens - output` → `totalTokens`. Defensive because I couldn't verify at rest whether the Pi SDK always populates `input` for LM Studio's OpenAI-compatible responses. Worst case we show `totalTokens`, still more honest than the old cumulative billing.

## Files touched

- `sidecar/src/protocol.ts` — `contextWindow` field on `CreateSessionCommand`, `SetModelCommand`.
- `sidecar/src/runtime-manager.ts` — `buildDynamicModel` / `resolveModel` / `createSession` / `setModel` accept and apply the override. LM Studio always rebuilds dynamic model so the override wins over any registry entry.
- `sidecar/src/index.ts` — forwards `contextWindow` on `set_model` dispatch.
- `src-tauri/src/db.rs` — `agents.context_window` migration, `AgentRow.context_window`, `get_agent_context_window_internal`, upsert/select SQL.
- `src-tauri/src/agent.rs` — `spawn_agent` extra `context_window` param, fallback lookup for restore, injects into `create_session` JSON.
- `src/lib/types.ts` — `AgentConfig.contextWindow`.
- `src/lib/SpawnDialog.svelte` — LM Studio slider + preset chips + live value readout.
- `src/App.svelte` — forwards `contextWindow` on the spawn invoke.
- `src/lib/AgentControls.svelte` — meter numerator rewrite, `used/total (pct%)` display, separate `Σ` billing chip, threshold flip.
- `ONBOARDING.md` — one-line LM Studio bullet update.
- `thoughts/plan/MON-8.md` — committed per project convention.

## What was left out

- **LM Studio `/api/v0/models` auto-detect** — explicitly out of scope, tracked separately. Manual input is the MVP.
- **Per-(provider, model) remembered context window** — nice-to-have from the plan; templates don't carry context window either. User sets it per spawn for now.
- **Mid-session `setModel` UI for changing context window** — protocol plumbing accepts it, but there's no UI surface to trigger it.
- **OpenRouter user-facing context input** — confirmed out of scope.
- **Shadow-oath / CLAUDE.md token footprint optimisation** — came up during verification ("hi" = ~1k tokens on turn 1). Acknowledged as a real lever for small-window LM Studio models, but a separate concern from the meter bug. A follow-up issue will be opened.
- **Sidecar auto-rebuild on source change** — pre-existing ergonomics issue, surfaced here but not fixed. Candidate for a small follow-up (prebuild hook or git hook).
