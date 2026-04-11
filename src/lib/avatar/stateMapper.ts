import type { LiveAgentState } from "../toolbox/types";

/**
 * Animation state derived from LiveAgentState.
 *
 * Boolean fields are mutually exclusive activity states — exactly one should
 * be true at any time. Triggers fire on transitions (e.g. task completion).
 * Numbers are continuous values driven by stats.
 *
 * Field names match the Rive state machine input names defined in the .riv
 * file (MON-57 contract).
 */
export interface AnimationState {
  // Activity booleans (mutually exclusive)
  isIdle: boolean;
  isThinking: boolean;
  isCoding: boolean;
  isReading: boolean;
  isUsingTool: boolean;
  isWaiting: boolean;
  isError: boolean;
}

/**
 * Trigger events that fire once on state transitions.
 * Callers compare previous and current AnimationState to detect these.
 */
export interface AnimationTriggers {
  taskComplete: boolean;
  summon: boolean;
}

/** Names of tools whose execution looks like "reading" rather than "building". */
const READ_TOOLS = new Set([
  "Read",
  "Glob",
  "Grep",
  "LS",
  "ListDir",
  "Search",
  "WebSearch",
  "WebFetch",
]);

/**
 * Derive the current animation state from a LiveAgentState snapshot.
 *
 * Priority order (first match wins):
 *   1. Error (desynced or any tool execution in error state)
 *   2. Tool running — subdivided into reading vs general tool use
 *   3. Coding (streaming with content being produced)
 *   4. Thinking (streaming but no content yet — model is reasoning)
 *   5. Idle (nothing happening)
 *
 * "Waiting" is intentionally not derived here — it would need a time-based
 * heuristic (tool running for >N seconds) which doesn't belong in a pure
 * mapper. The component can layer that on with a timer if needed.
 */
export function deriveAnimationState(live: LiveAgentState): AnimationState {
  const idle: AnimationState = {
    isIdle: true,
    isThinking: false,
    isCoding: false,
    isReading: false,
    isUsingTool: false,
    isWaiting: false,
    isError: false,
  };

  // 1. Error state
  if (live.desynced) {
    return { ...idle, isIdle: false, isError: true };
  }

  // Check tool executions for running/error states
  let hasRunningTool = false;
  let runningToolIsRead = false;
  let hasErrorTool = false;

  for (const [, exec] of live.toolExecutions) {
    if (exec.status === "error") hasErrorTool = true;
    if (exec.status === "running") {
      hasRunningTool = true;
      if (READ_TOOLS.has(exec.toolName)) runningToolIsRead = true;
    }
  }

  if (hasErrorTool) {
    return { ...idle, isIdle: false, isError: true };
  }

  // 2. Tool running
  if (hasRunningTool) {
    if (runningToolIsRead) {
      return { ...idle, isIdle: false, isReading: true };
    }
    return { ...idle, isIdle: false, isUsingTool: true };
  }

  // 3-4. Streaming states
  if (live.isStreaming) {
    if (live.streamingMessage && live.streamingMessage.content.length > 0) {
      return { ...idle, isIdle: false, isCoding: true };
    }
    return { ...idle, isIdle: false, isThinking: true };
  }

  // 5. Idle
  return idle;
}

/**
 * Detect trigger events by comparing previous and current animation states.
 * Returns which triggers should fire this update.
 */
export function detectTriggers(
  prev: AnimationState | null,
  current: AnimationState,
): AnimationTriggers {
  return {
    // Task complete: was actively doing something, now idle
    taskComplete:
      prev !== null &&
      !prev.isIdle &&
      !prev.isError &&
      current.isIdle,
    // Summon: no previous state (first render)
    summon: prev === null,
  };
}
