# MON-55 — Implementation notes

## What was implemented

Replaced the unconditional force-scroll-to-bottom with a stick-to-bottom pattern in `AgentView.svelte`.

- New `isAtBottom` state (defaults `true`) tracked by an `onscroll` handler on the scroll container, with a 20px near-bottom threshold to absorb layout jitter.
- `scrollToBottom()` now takes an optional `force` flag. Default (non-forced) paths — streaming state snapshots, agent exit — respect `isAtBottom` and skip the scroll if the user has scrolled up.
- Force paths — `session_ready` — always jump, so the first render lands at the bottom.

## Key decisions

- **20px threshold.** Small enough that users notice if they meant to scroll up; large enough to absorb layout shifts from markdown rendering or streaming token insertion.
- **Agent-exit handler kept non-forced.** Felt more respectful of user intent: if they're reading history when the agent exits, don't yank them to the "Agent stopped" banner. Cheap to flip if that turns out wrong.
- **No "jump to bottom" indicator.** The ticket listed it as optional; skipped to keep the diff minimal. Can be added later if users ask for it.

## Files touched

- `src/lib/AgentView.svelte` — new `isAtBottom` state + `updateIsAtBottom` handler, revised `scrollToBottom(force)` signature, `onscroll` wired on the scroll container, `session_ready` call site updated to `scrollToBottom(true)`.

## What was left out

- Optional "new messages, click to jump to bottom" chip — deferred; not an AC.

Shipped as part of the bundled bug-sweep PR (#57) alongside MON-48 and MON-49.
