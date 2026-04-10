# MON-26 — Remove Council mode entirely (implementation notes)

## What was implemented

Council mode is gone. The single-agent view is the only live mode. Every Council surface, type, command, keybind, and doc reference was deleted in a single PR ([#17](https://github.com/whoislikemiha/monarch/pull/17)) across four commits:

1. **Rust command handlers** — `broadcast_prompt` (Tauri command), `ws_broadcast_prompt` (WebSocket twin), their registration in `lib.rs`, and the dispatcher arm in `ws.rs`.
2. **Frontend component + wiring** — `CouncilView.svelte` deleted, `CouncilSession`/`CouncilResponse` types removed from `src/lib/types.ts`, and every reference in `App.svelte` and `Sidebar.svelte`.
3. **Docs** — `ONBOARDING.md` (component tree, state cheatsheet, gotcha, repo map) and `FEATURES.md`.
4. **Plan file** — `thoughts/plan/MON-26.md` committed with the rest.

Driving motivation: MON-14 (lift event assembly into Rust) otherwise had to either migrate CouncilView to the new `agent-state-{id}` channel or leave it stuck on the soon-to-be-deprecated raw channel. Deleting it removes the second consumer of the raw `agent-event-{id}` stream and narrows MON-14's frontend blast radius to `AgentView.svelte` only.

## Scope deltas found during research

The original Linear description was drafted against stale line numbers and missed two things. Both were captured in `thoughts/plan/MON-26.md` and handled in the same PR:

- **`ws_broadcast_prompt`** — a parallel WebSocket path for broadcast at `agent.rs:1120` with a dispatcher arm in `ws.rs:195-200`. Deleted alongside `broadcast_prompt`.
- **Dual Sidebar mount sites** — MON-15's sidebar refactor added a collapsed icon-rail form that duplicated the council toggle. There were two council buttons to delete: `Sidebar.svelte:114-115` (collapsed rail) and `202-212` (expanded section), plus a `.council-rail-btn.active` CSS rule.

## Key decisions

- **`runningCount` in Sidebar** — deleted, not preserved. Both its uses were the council gates; grep confirmed no other callers. If any future feature wants a "count of running agents" derived, it can be re-added cheaply.
- **`Ctrl+L` slot** — left dark, not repurposed. The keybind branch is gone; pressing it now falls through to browser default. Repurposing was out of scope and would have muddied the PR.
- **Historical plan files** — `thoughts/plan/MON-10.md`, `MON-12.md`, `MON-15.md`, and `thoughts/impl/MON-12.md` all reference Council in their narrative. Explicitly **not** touched — they're historical planning artifacts, not executable code.
- **Linear description patching** — I proceeded against the plan file, not the (stale) Linear checklist. Patched Linear post-implementation rather than mid-work to avoid churn.

## Files touched

- **Deleted:** `src/lib/CouncilView.svelte` (570 lines).
- **Rust:** `src-tauri/src/agent.rs` (−48), `src-tauri/src/lib.rs` (−1), `src-tauri/src/ws.rs` (−8).
- **Frontend:** `src/App.svelte` (−26), `src/lib/Sidebar.svelte` (−65), `src/lib/types.ts` (−22).
- **Docs:** `ONBOARDING.md` (−5), `FEATURES.md` (−1).
- **Plan:** `thoughts/plan/MON-26.md` (+91).

Totals: 740 lines removed, 94 added (most of the additions are the plan file).

## Verification

- `cargo check` — clean, no dead-code warnings.
- `svelte-check --threshold error` — 261 files, 0 errors, 0 warnings.
- `grep -rni council src src-tauri ONBOARDING.md FEATURES.md` — zero matches. The sweep is complete.

Manual smoke test (spawn ≥2 agents, confirm no council UI, `Ctrl+L` no-op, single-agent view unchanged) is the user's responsibility since it requires running the desktop app.

## What was left out

- **Linear description patching of MON-26** — done post-implementation rather than mid-work.
- **`Ctrl+L` repurposing** — deliberately out of scope. The slot is available for a future feature.
- **Cross-branch plan/impl files** — `thoughts/plan/MON-14.md` exists in the working tree but belongs on the MON-14 branch, so it stays untracked on this branch.
