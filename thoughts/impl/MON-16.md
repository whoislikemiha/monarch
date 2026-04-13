# MON-16 — implementation notes

## What was implemented

Flipped the default visibility of `thinking` blocks in the chat UI and introduced a streaming-time render that follows the agent's reasoning live without letting a verbose thinking block push the real reply off-screen.

### Two lifecycle stages

1. **Streaming (`MessageList.svelte`)**
   - While thinking deltas arrive and no text has started: render the thinking text live in a muted/italic block under a small animated "● ● ● Thinking" label.
   - The moment the first text delta lands: auto-collapse the live thinking block into a static "▸ Thinking" pill and let text stream below it. Thinking content is not discarded — the finalized message carries it through.
   - This matters because some local models stuff their entire response into thinking; without the auto-collapse the real reply would sit below a wall of text.

2. **Finalized (`AssistantMessage.svelte`)**
   - Tracks *expanded* blocks instead of *collapsed* ones, so everything renders collapsed on first paint (including history-restored messages).
   - The toggle is a rounded pill with `aria-expanded`; content expands with Svelte's `slide` transition (180ms).
   - Redacted thinking blocks get the same pill; clicking shows the redacted marker body.

## Key decisions

- **Custom button + `slide`** over `<details>`/`<summary>`: predictable animation, clean keyboard semantics with `aria-expanded`.
- **Auto-collapse on text start** (not on thinking_end): simpler and more reliable — we don't need to track distinct lifecycle events, just "has text arrived?"
- **No cross-lifecycle state persistence**: expand/collapse state resets when a streaming message finalizes into an `AssistantMessage` item. AC didn't require it and lifting state into the store would be meaningful scaffolding for negligible UX gain.
- **Same pill style in both stages** so the visual handoff from streaming to finalized is seamless.

## Files touched

- `src/lib/AssistantMessage.svelte` — inverted state, pill-shaped toggle with `aria-expanded`, `slide` transition, refreshed CSS.
- `src/lib/MessageList.svelte` — streaming block now extracts thinking + text separately, renders thinking live, auto-collapses on text start. Animated dots affordance + sr-only label for a11y.

## Dependency on MON-70

This branch was blocked on MON-70. The frontend paths assume `streamingMessage.content` mutates token-by-token — but before MON-70, the Rust side was not bumping `state_version` on debounced `message_update` emits, so the frontend's stale-drop check discarded everything between `message_start` and `message_end`. MON-70 shipped first; MON-16 rebased on top and works as designed.

## What was left out

- Persisting expand/collapse state across reloads / session switches.
- Filtering thinking out of the copy-message (`getAllText`) payload — explicitly out of scope.
- Any changes in Rust, sidecar, or the `ContentBlock` wire shape.
