# MON-16 — Collapse thinking into expandable bubble (hidden by default)

## Summary

Assistant `thinking` content is today rendered inline inside the assistant message bubble, expanded on first paint, with a per-block toggle to collapse it. The task flips that default — thinking blocks render collapsed, showing only a lightweight affordance until the user opts in — and upgrades the streaming-time indicator from a static "thinking..." label to a subtle animated one. The work is isolated to the Svelte chat-rendering layer; no Rust, sidecar, persistence, or message-assembly logic needs to change.

## Relevant files and areas

- `src/lib/AssistantMessage.svelte` — owns per-block rendering for a completed assistant message, including the thinking toggle.
  - `collapsedThinking` state (line 7) and `toggleThinking` (lines 9–16) — the current per-index expansion set. Starts empty, which is why blocks paint expanded.
  - Template branch `{:else if block.type === "thinking"}` (lines 124–138) — the toggle button + conditional `.thinking-content` render.
  - `.thinking-block` / `.thinking-toggle` / `.thinking-content` CSS (lines 283–322) — visual treatment of the bubble; relevant for bubble/accordion restyling and for the expand/collapse transition.
- `src/lib/MessageList.svelte` — renders finalized items via `AssistantMessage` and handles the in-flight streaming message separately.
  - Streaming block (lines 53–69) — detects `isThinking` on the streaming message and currently shows a static `"thinking..."` string (line 65). This is the place for the active-thinking animated indicator.
  - `.streaming-thinking` CSS (lines 182–185) — current styling; the animated affordance belongs here or in a small shared component.
- `src/lib/types.ts` — `ThinkingContent` (lines 138–142) and `ContentBlock` (line 157). Informational only; no type changes required.
- `src/lib/stores/agentStore.svelte.ts` and `AgentView.svelte` — confirmed to produce `ContentBlock` arrays that feed `MessageList`. Not edited, but worth re-reading to confirm restored-from-history messages travel the same `AssistantMessage` path (they do) so the default-collapsed behavior applies uniformly.

## What needs to change

At the conceptual level:

1. **Invert the default state of per-block thinking expansion in `AssistantMessage.svelte`.** Instead of tracking which blocks are *collapsed*, track which are *expanded* (or flip the default any other way). The effect is that freshly rendered messages — streaming-completed or history-restored — start collapsed.

2. **Promote the thinking element from an inline `<div>` + button into a proper accordion "bubble".** Visually it should read as a discrete, pill/bubble-shaped control; functionally it remains a toggle. Likely uses `<details>`/`<summary>` semantics or stays as a button+region with `aria-expanded`. Either is fine; choose whichever interacts most cleanly with the chosen transition.

3. **Add a smooth expand/collapse height transition.** Options: Svelte's built-in `slide` transition on the content region, a CSS `grid-template-rows: 0fr → 1fr` trick, or a `max-height` animation. The region must not clip copy-paste or accessibility semantics. Keep the interaction snappy (≤200ms).

4. **Replace the static "thinking..." streaming label in `MessageList.svelte` with an animated indicator.** A pulsing dot or inline spinner, driven by CSS keyframes, matching the existing streaming-indicator language already in the message label. Should remain text-free-friendly for screen readers (aria-live / visually-hidden label).

5. **Optional consolidation (consider during impl).** The streaming-state indicator in `MessageList` and the completed-state thinking bubble in `AssistantMessage` are the same concept at two lifecycle stages. It may be worth extracting a tiny `ThinkingBubble.svelte` so both paths share the chrome and the animation. Leave to implementer's judgement — if extraction adds more scaffolding than it saves, keep them separate.

No schema changes. No protocol changes. No changes to `getAllText` / copy behavior (explicitly out of scope per AC).

## Resolved decisions

- **Toggle mechanism:** custom `button` + `aria-expanded` + Svelte `slide` transition. Not `<details>`/`<summary>`.
- **Redacted thinking:** remains a toggleable bubble, collapsed by default like any other thinking block. No special non-toggle pill.
- **Streaming indicator scope:** drop-in replacement for the existing pre-text "thinking..." label. Do not introduce a new mid-response streaming surface for thinking that arrives after text has started.
- **Stream → finalized handoff:** acceptable for per-block expand state to reset when the streaming message finalizes into an `AssistantMessage` item. Do not lift state into the store.

## Out of scope reminders

- No persistence of expand/collapse state across reloads, sessions, or agent switches.
- No changes to copy / `getAllText` payload — thinking continues to be copy-included as today.
- No changes in Rust, the sidecar, SQLite schema, or `ContentBlock` types.
- No broader redesign of assistant message chrome, model/token/cost tags, or tool-group rendering.
