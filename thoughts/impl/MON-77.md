# MON-77 — Warcraft-style agent portrait (impl notes)

## What shipped

The live agent UI was reorganised around a single **AgentPortrait** HUD plus a horizontal **AgentRoster** top bar. Three older surfaces — `Sidebar`, `AgentHeader`, `TabBar`, `AgentControls` — were deleted and their responsibilities merged into those two components.

### Portrait (`src/lib/AgentPortrait.svelte`)

- Floats over the chat messages area, anchored to one of four corners (`top-left`, `top-right`, `bottom-left`, `bottom-right`), user-draggable via a thin grip at the top. Corner is persisted to `ui_state` so it sticks across sessions.
- Contains, top to bottom: grip + minify button, a flat ctx bar inside the avatar frame, the avatar (180px / 128px), the thinking picker, a rate tag (live tok/s while streaming, last-turn rate otherwise), a context sparkline, a billing tag (cost in accent, session tokens muted).
- Clicking the avatar opens a command menu (Abort / New chat / Compact / Session history / System prompt / Project instructions) that auto-flips its open direction based on the portrait's corner.
- Minify toggle collapses everything to the avatar + a small ctx bar overlayed on the frame. Minify state is also persisted to `ui_state`.
- Streaming signals: border accent, inner avatar glow, outer portrait breath, shimmer sweep on the ctx fill, pulsing dot in matching roster pill.

### Roster (`src/lib/AgentRoster.svelte`)

- Horizontal top bar replacing the left sidebar and the tab bar. Grouped per project; each group is a 2-row auto-flow grid that scrolls horizontally when it overflows.
- Each pill shows a 28px avatar, agent name, lifetime cost, and a live status line fed from `liveAgentStore` (`activityStatus` while streaming, otherwise `Idle / Paused / Dismissed`). Streaming pills accent their border and animate a pulsing dot.
- Active/All toggle + `+ extract` button live in the roster header; `Ctrl+B` still toggles `sidebarCollapsed`, which now collapses the roster height.

### Chat shell

- `.app` flipped to a column: roster on top, then the existing row of `main-panel` + `ToolPanelStack` + `ToolRail`.
- `AgentView` drops the old top header; messages scroll area keeps a left gutter equal to the portrait so messages never slide behind it.
- Context-token math (`estimatedContextTokens`, `liveContextTokens`, `itemsCostTotal`, `usedRatio`, `contextState`) was lifted verbatim out of the old `AgentControls` into the portrait; no behaviour change there.

## Key decisions

- **Identity lives in the portrait, not in a top bar.** Removing `AgentHeader.svelte` means one less horizontal strip above the chat; the avatar itself became the command affordance via click.
- **One source of truth for the ctx math.** `AgentControls.svelte` was deleted instead of kept around — sparkline, healthbar, and mini ctx overlay all consume the same derivations in the portrait.
- **Drag smoothness via direct DOM writes.** `pointermove` writes the transform straight to `portraitEl.style.transform` inside `requestAnimationFrame` rather than updating `$state` per event. Backdrop-filter blur + streaming breath are paused while dragging.
- **Theming without new tokens.** Every colour in the portrait and roster already had a theme variable (`--accent`, `--accent-blue`, `--warning`, `--error`, `--context-track-bg`, `--bg-panel`, etc.); no per-theme additions were needed.
- **Persistence via existing `ui_state`.** Both `portraitCorner` and `portraitMinified` ride on the same `db_set_ui_state` / `db_get_ui_state` pattern that zoom/theme already use; no schema changes, no new Tauri commands.

## Files touched

Created:
- `src/lib/AgentPortrait.svelte`
- `src/lib/AgentRoster.svelte`

Deleted:
- `src/lib/Sidebar.svelte`
- `src/lib/AgentHeader.svelte`
- `src/lib/TabBar.svelte`
- `src/lib/AgentControls.svelte`

Modified:
- `src/App.svelte` — column shell, roster on top, dropped Sidebar/TabBar.
- `src/lib/AgentView.svelte` — drop AgentHeader + AgentControls, wire portrait, persist corner + minify, pass `nowMs` and `streamingMessage` to portrait.

Plus one plan doc: `thoughts/plan/MON-77.md`.

## Left out / deferred

- **No backend changes.** Zero Rust, zero sidecar, zero `bindings.ts` regeneration.
- **No per-agent portrait preferences.** Corner + minify are global across agents, not per-shadow; keeping preference scope simple.
- **No keyboard shortcut for minify / move.** Only click/drag today. `Ctrl+B` still collapses the roster.
- **Sparkline data is session-local.** It lives in the portrait component, so switching agents resets the rolling buffer — acceptable since it's a coarse trend indicator.
- **Tok/s between turns goes muted as `— tok/s`** rather than back-computing from `items[].usage.output` divided by an inferred duration; we don't persist per-turn durations and didn't want to infer from neighbouring timestamps.
- **Narrow-width collapse is a media-query hide below 720px.** A compact inline mode for very narrow chats could be a follow-up.
