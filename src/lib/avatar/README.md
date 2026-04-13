# Shadow Avatars — Rive Integration Guide

Quick reference for building `.riv` files that work with the Monarch avatar system, and for anyone working on the avatar code.

## TL;DR

1. Put your character on **an artboard** (not as a loose component)
2. Create **one state machine** on that artboard
3. Add the inputs listed below (any subset — missing ones are skipped)
4. Export as `.riv`, drop in `static/avatars/shadow_animations.riv`
5. Done — component auto-detects the state machine name

## Architecture

```
┌────────────────────────────────────────────────────────┐
│  Rust: LiveAgentState (agent_state.rs)                 │
│  is_streaming, tool_executions, desynced, ...          │
└────────────────────┬───────────────────────────────────┘
                     │ agent-state-{id} events
                     ▼
┌────────────────────────────────────────────────────────┐
│  Frontend: liveAgentStore.svelte.ts                    │
│  SvelteMap<agentId, LiveAgentState>                    │
└────────────────────┬───────────────────────────────────┘
                     │ $derived
                     ▼
┌────────────────────────────────────────────────────────┐
│  stateMapper.ts — deriveAnimationState()               │
│  Returns { isIdle, isCoding, isThinking, ... }         │
└────────────────────┬───────────────────────────────────┘
                     │ $effect
                     ▼
┌────────────────────────────────────────────────────────┐
│  ShadowAvatar.svelte                                   │
│  setBool/fireTrigger/setNumber on Rive inputs          │
└────────────────────┬───────────────────────────────────┘
                     │ Rive runtime
                     ▼
┌────────────────────────────────────────────────────────┐
│  shadow_animations.riv — your state machine            │
│  Transitions between animation states                  │
└────────────────────────────────────────────────────────┘
```

## State Machine Inputs

All inputs are **optional** — the code calls `setBool`/`fireTrigger`/`setNumber` which no-op on missing inputs. Add only the ones whose animations you've designed.

### Booleans (mutually exclusive — only one true at a time)

| Input        | When true                                     | Suggested animation                   |
|--------------|-----------------------------------------------|---------------------------------------|
| `isIdle`     | Agent doing nothing                           | Breathing, subtle pulse               |
| `isThinking` | Agent reasoning, no output text yet           | Head tilt, thought particles          |
| `isCoding`   | Agent streaming text/code                     | Faster pulse, energetic glow          |
| `isReading`  | Running Read/Grep/Glob/LS/WebSearch/WebFetch  | Eye scan, scroll effect               |
| `isUsingTool`| Running other tools (Edit/Write/Bash/etc.)    | Hammer strike, wielding               |
| `isError`    | Desynced or tool errored                      | Red flash, shake                      |

**Implementation note:** exactly one of these is true at any moment. The mapper enforces priority: `isError` > `isUsingTool`/`isReading` > `isCoding` > `isThinking` > `isIdle`.

### Triggers (fire once on transition)

| Input          | When it fires                                   | Suggested animation              |
|----------------|-------------------------------------------------|----------------------------------|
| `taskComplete` | Agent finishes a turn (was active, now idle)    | Victory burst, energy ring       |
| `summon`       | First render (avatar mounts)                    | Dramatic entrance from portal    |

### Numbers (continuous, optional)

| Input        | Range | What drives it                              | Suggested use              |
|--------------|-------|---------------------------------------------|----------------------------|
| `grade`      | 1-5   | Shadow rank (E=1, D=2, C=3, B=4, A=5, S=5+) | Glow intensity, complexity |
| `experience` | 0-100 | Lifetime tokens, log10(tokens) × 15         | Particle density, aura size|

Called via `setGrade(n)` / `setExperience(n)` — exposed as public methods on the component, not wired by default. MON-60 will wire these to stats data.

## Rive File Requirements

### Must have

1. **An artboard** with visible content inside its bounds
2. **At least one state machine** on that artboard
3. **Artboard background transparent** (or whatever — it'll show through)

### Gotchas

- **Components are not artboards.** Your glowing ball needs to be placed *on* an artboard, not used as a nested component reference.
- The runtime loads the **default artboard** (first one). If you have multiple, make sure the one with content is first or pass `artboard: "Name"` to the component.
- Inputs must be added explicitly in the state machine's **Inputs panel** (bottom of the state machine editor). Creating states/transitions doesn't automatically create inputs.
- Export via **File → Download → .riv** — don't confuse with `.rev` (editor file).

## Minimum Viable .riv

If you only want to ship something quick:

- **1 artboard** with your character
- **1 state machine** with states: `Idle`, `Coding`, `Error`, `Victory`
- **3 booleans + 1 trigger**: `isIdle`, `isCoding`, `isError`, `taskComplete`
- Transitions: `isIdle → Idle`, `isCoding → Coding`, `isError → Error`, `taskComplete → Victory → Idle`

That covers ~80% of the value. Everything else is polish.

## Code Reference

### Files

| File                                     | What it does                                        |
|------------------------------------------|-----------------------------------------------------|
| `src/lib/avatar/ShadowAvatar.svelte`     | The component — Rive lifecycle, input binding       |
| `src/lib/avatar/stateMapper.ts`          | Pure function: `LiveAgentState` → input booleans    |
| `src/lib/avatar/index.ts`                | Barrel export                                       |
| `static/avatars/shadow_animations.riv`   | The art file (drop your export here)                |

### Usage

```svelte
<script>
  import { ShadowAvatar } from '$lib/avatar';
</script>

<ShadowAvatar agentId={agent.id} size={32} />
```

Props:
- `agentId: string` — required, binds to liveAgentStore
- `size?: number` — pixels, default 64
- `stateMachine?: string` — defaults to first SM found in file
- `riveFile?: string` — defaults to `/avatars/shadow_animations.riv`

### Current placements

| Location                     | Size  |
|------------------------------|-------|
| `Sidebar.svelte` (agent list)| 32px  |
| `AgentHeader.svelte` (header)| 40px  |

### Runtime

Using `@rive-app/canvas` (Canvas 2D) not `@rive-app/webgl2`. WebGL limits contexts per page (~16) — with 10+ agents in the sidebar that blows the limit. Canvas has no such restriction.

## Debugging

If avatars aren't rendering:

1. **Check the file loads** — open browser devtools, Network tab, look for `shadow_animations.riv`. If 404, the path is wrong or publicDir isn't set.
2. **Check the console** — the component logs load errors via `EventType.LoadError`.
3. **Inspect the file contents** — add logging to the `onLoad` handler:
   ```ts
   const r = riveInstance as any;
   console.log("sms:", r.stateMachineNames);
   console.log("animations:", r.animationNames);
   console.log("inputs:", r.stateMachineInputs(smName)?.map(i => i.name));
   ```
4. **Verify the artboard has content** — open the .riv in Rive editor and hit Play. If nothing animates there, the runtime won't show anything either.

## Input Type Reference (Rive internal)

From `@rive-app/canvas`:
```ts
enum StateMachineInputType {
  Number = 56,
  Trigger = 58,
  Boolean = 59,
}
```

Setting values:
```ts
// Boolean
input.value = true;

// Number
input.value = 50;

// Trigger (no value — just fires)
input.fire();
```

## Related Tickets

- MON-56 — Rive runtime + component (done)
- MON-57 — The .riv file design (in progress)
- MON-58 — Avatar placement (done — needs .riv to render)
- MON-60 — Stats-driven progression (wires `grade`/`experience`)
- MON-61 — War Room (multi-avatar scene)
