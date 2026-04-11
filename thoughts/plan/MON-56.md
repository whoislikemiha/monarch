# MON-56: Rive Runtime Integration + ShadowAvatar Svelte Component

## Summary

Set up the Rive web animation runtime in Monarch's Svelte 5 frontend and build a reusable `ShadowAvatar.svelte` component that loads a `.riv` file, runs a Rive state machine, and has its inputs driven reactively from `LiveAgentState` via `$effect()`. This is the foundation layer — every other avatar ticket (placement, interaction, War Room, progression) depends on this component existing, rendering correctly in Tauri's WebKitGTK webview, and cleaning up without leaks.

## Relevant files and areas

| File | Why it matters |
|------|---------------|
| `src/lib/toolbox/liveAgentStore.svelte.ts` | The canonical store for per-agent state. `liveAgentStore.byAgent` is a `SvelteMap<string, LiveAgentState>` with `$state()` proxies. ShadowAvatar will derive animation inputs from this. |
| `src-tauri/src/agent_state.rs` (lines 130-164) | Defines `LiveAgentState` struct on the Rust side. Key fields for animation mapping: `is_streaming`, `tool_executions`, `streaming_message`, `activity_status`, `desynced`. |
| `src/lib/toolbox/types.ts` (lines 20-36) | TypeScript mirror of `LiveAgentState`. Defines `ToolExecution` with `status` field (running/done/error). |
| `src/lib/AgentStatusDot.svelte` | Existing component that derives `streaming` state from `liveAgentStore.byAgent.get(agent.id)`. Shows the current reactive pattern — ShadowAvatar will follow the same approach. |
| `src/lib/AgentView.svelte` (lines 64-70) | Shows how components subscribe to live state: `$derived((boundAgentId && liveAgentStore.byAgent.get(boundAgentId)) || DETACHED_LIVE)`. |
| `vite.config.ts` | Currently no WASM handling. Rive's WebGL2 runtime bundles its own WASM — need to verify Vite serves `.wasm` files correctly or add config. |
| `package.json` | No animation/canvas libraries currently. Adding `@rive-app/webgl2` (or `@rive-app/canvas`). |

## What needs to change

### 1. Install Rive runtime

Add `@rive-app/webgl2` as a dependency. This is the recommended runtime — it bundles a WASM-based renderer with a JS API. If WebGL2 causes issues on WebKitGTK, fall back to `@rive-app/canvas` which uses the Canvas 2D API instead. Both have the same JS API surface, so swapping is a one-line change.

### 2. Vite WASM configuration

Rive's runtime loads a `.wasm` file at init. Vite needs to serve this correctly. May need to configure `optimizeDeps.exclude` for the Rive package so Vite doesn't try to pre-bundle the WASM, or add `vite-plugin-wasm` / `vite-plugin-top-level-await` if the runtime uses top-level await. Test first — it might just work with `build.target: "esnext"` (which is already set).

### 3. Create static assets directory for .riv files

Create `static/avatars/` to hold `.riv` files. Vite serves `static/` at the root, so `static/avatars/shadow-base.riv` becomes `/avatars/shadow-base.riv` at runtime. Include a placeholder `.riv` for development/testing — either a minimal file created in the Rive editor or a community sample.

### 4. Build the ShadowAvatar.svelte component

New file at `src/lib/components/ShadowAvatar.svelte`. Responsibilities:

- **Props**: `agentId` (string), `size` (number, px), `stateMachine` (string, defaults to `"ShadowBehavior"`), optional `riveFile` (string, path to .riv, defaults to placeholder)
- **Canvas mount**: Bind an `HTMLCanvasElement`, size it to `size` prop with proper device pixel ratio handling
- **Rive initialization**: In `onMount`, create a `new Rive({ src, canvas, stateMachines, autoplay: true, useOffscreenRenderer })` instance. Return cleanup function that calls `rive.cleanup()`.
- **Input access**: After Rive loads (via `onLoad` callback), cache references to state machine inputs by name for fast access in effects.
- **Reactive state mapping**: Use `$effect()` to watch agent state and set Rive inputs. Derive agent state from `liveAgentStore.byAgent.get(agentId)`.

### 5. Build the agent-state-to-animation mapping layer

A pure function or module at `src/lib/avatar/stateMapper.ts` that takes a `LiveAgentState` and returns a flat object of Rive input values. This decouples the animation input contract from the component itself.

Mapping logic:
- **isIdle**: `!live.is_streaming && no running tool executions`
- **isCoding**: `live.is_streaming && live.streaming_message exists` (agent is writing)
- **isThinking**: `live.is_streaming && no streaming_message yet` (agent is reasoning)
- **isUsingTool**: `any tool_execution with status === "running"`
- **isWaiting**: Could map to specific tool patterns (e.g., bash running for extended time)
- **isError**: `live.desynced === true` or a tool_execution with error status
- **taskComplete**: Trigger when `is_streaming` transitions from true to false (session turn ends)

The mapper returns something like `{ isIdle: true, isCoding: false, isThinking: false, ... }` — the component applies these to Rive inputs.

### 6. Handle multi-instance rendering

For the sidebar (MON-58) and War Room (MON-61), many ShadowAvatar instances will render simultaneously. The component should accept a `useOffscreenRenderer` prop (default true) so all instances share a single WebGL context. This is a Rive constructor option — no extra code needed, just pass it through. Document the canvas renderer fallback path for extreme cases (20+ instances).

## Open questions

1. **WebGL2 on WebKitGTK** — Does Tauri's WebKitGTK on Linux support WebGL2? If not, we need to default to `@rive-app/canvas` on Linux. Need to test this early — it's a potential blocker. Alternatively, we could detect WebGL2 support at runtime and dynamically import the right package.

2. **Placeholder .riv** — Should I create a minimal placeholder in the Rive editor (a pulsing circle with a few states), or grab an open-source sample .riv from the Rive community? A community sample would look better for testing but won't match the final art direction. Since you're building the real one in parallel, a minimal placeholder is probably fine.

3. **WASM file serving in Tauri production builds** — In dev mode, Vite serves the WASM. In production, Tauri bundles the frontend as static files. Need to verify the Rive WASM file gets included in the production bundle and loads correctly from the Tauri webview's local file protocol.

4. **State machine input contract** — The mapping layer needs to agree on input names with the .riv file (MON-57). Proposed names: `isIdle`, `isCoding`, `isThinking`, `isReading`, `isUsingTool`, `isWaiting`, `isError`, `taskComplete` (trigger), `summon` (trigger), `grade` (number), `experience` (number). Does this list look right to you, or should we adjust before you start building the .riv?

## Out of scope

- Designing the actual shadow character animation (MON-57)
- Integrating the avatar into sidebar or detail view layout (MON-58)
- Interactive hover/click/drag behavior (MON-59)
- Stats-driven visual progression (MON-60)
- War Room view (MON-61)
- Shadow stats backend (MON-63)
