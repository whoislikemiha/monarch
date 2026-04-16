# MON-78 — Provider-aware thinking levels + per-model defaults

## What was implemented

Thinking-level selection is now provider- and model-aware end-to-end:

- The silent "off = high on adaptive Claude" bug is gone — the sidecar
  imports `ThinkingLevel` from `pi-agent-core` (where `"off"` is a
  first-class value) instead of `pi-ai`, and pi-agent-core translates
  `"off"` to `undefined` reasoning on each turn.
- UI surfaces (runtime AgentControls, Spawn dialog, Edit dialog) all
  render the same shared `ThinkingPicker` component. It shows only the
  levels Pi will honor for the current `(provider, model)` and labels
  them with the provider's native terminology (`max` on Opus 4.6,
  uppercase for Gemini, etc.). Non-reasoning models (LM Studio,
  non-reasoning OpenRouter/Anthropic ids) hide the picker entirely.
- Per-model defaults come from `~/.config/monarch/thinking.toml`
  keyed by `(provider, pattern)`. Absence of a matching entry falls
  through to a conservative built-in table. The Rust spawn path asks
  the config; the SpawnForm seeds the picker from it on every
  `(provider, model)` change.
- Visual flair: a five-cell power meter next to the picker button and
  inside every option. Cold cyan → blue → accent purple → glowing
  accent → warning-yellow with a slow pulse at `xhigh/max`. The
  trigger button border + text color track the meter so the current
  level reads at a glance.

## Key decisions

- **Pi-canonical stored, native displayed.** After the spike revealed
  that `pi-agent-core` already owns the `off/minimal/low/medium/high/xhigh`
  enum and clamps per-provider internally, I dropped the plan's
  bidirectional native-storage mapping in favor of a frontend-only
  display transform. Adds no Rust / sidecar churn and keeps the DB
  value interpretable without context.
- **No new IPC.** The plan suggested emitting per-model capability
  events from sidecar to Rust to frontend. Instead, the frontend
  capability table in `thinking.ts` mirrors Pi's logic directly, and
  Pi is the safety net — if the table ever drifts, Pi clamps silently
  rather than breaking. Symptoms of drift would be cosmetic (UI shows
  a level Pi quietly lowers), never unsafe.
- **Config seeds on model change, not on every read.** Existing
  agents keep their stored level. Only when the user switches
  `(provider, model)` in the spawn form does the default get
  re-applied.
- **Shared ThinkingPicker over native `<select>`** — needed custom
  children (the meter in every option), and it keeps the three
  surfaces visually identical. The `direction` prop flips it upward
  in AgentControls (sits at the bottom of the screen) and downward
  in dialogs.

## Files touched

**New**
- `src/lib/thinking.ts` — capability table, display-label transform, intensity helper
- `src/lib/ThinkingMeter.svelte` — five-cell power meter
- `src/lib/ThinkingPicker.svelte` — shared trigger + dropdown
- `src-tauri/src/thinking_config.rs` — TOML loader + Tauri commands

**Modified**
- `sidecar/src/runtime-manager.ts` — root bug fix + validation
- `src-tauri/src/agent/manager.rs` — config-driven default
- `src-tauri/src/lib.rs` — module + command registration
- `src-tauri/Cargo.toml` — `toml = "0.8"` dep
- `src/lib/bindings.ts` — regenerated
- `src/lib/providers.ts` — removed the generic `THINKING_LEVELS`
- `src/lib/AgentControls.svelte` — uses ThinkingPicker
- `src/lib/ModelSelector.svelte` — uses ThinkingPicker
- `src/lib/EditAgentDialog.svelte` — uses ThinkingPicker
- `src/lib/SpawnForm.svelte` — config-default seeding, no more off-stripping
- `src/lib/AgentView.svelte` — threads `agent.provider` into AgentControls
- `ONBOARDING.md`, `CLAUDE.md` — doc updates

## What was left out

- No migration of historical `thinking_level` values in existing
  SQLite rows — thinking is applied per chat anyway and reading a
  legacy string through `clampLevel` handles the edge cases.
- No per-user custom thinking ladders beyond what the TOML defaults
  section covers.
- Non-adaptive Anthropic models map `xhigh` off the picker (Pi uses
  token budgets below 4.6 and doesn't honor `xhigh` there); kept
  intentionally to avoid surfacing a level the provider ignores.
- No spike on forcing a mid-session "reset reasoning to undefined" —
  Pi's `AgentSession.setThinkingLevel("off")` already reaches
  pi-agent-core's `_state.thinkingLevel === "off" ? undefined`, so no
  session recreation was needed.
