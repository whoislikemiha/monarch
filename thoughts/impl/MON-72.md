# MON-72 — Show agent name instead of "Assistant" in chat headers

## What was implemented
Added an `agentName` prop to `MessageList.svelte` and wired it through from `AgentView.svelte`. Both message label sites (finalized turn and streaming bubble) now render the agent's name.

## Key decisions
- Prop defaults to `"Assistant"` so any future callsite that omits it degrades gracefully rather than showing blank.
- No new state or derived value needed — `agent.name` is already available at the `AgentView` level.

## Files touched
- `src/lib/MessageList.svelte` — added `agentName` prop, replaced two hardcoded strings
- `src/lib/AgentView.svelte` — passes `agentName={agent.name}`

## What was left out
Nothing descoped — change is fully contained.
