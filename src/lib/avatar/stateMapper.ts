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
 *   1. Error — only when desynced (live signal). Past tool errors are NOT
 *      persistent; they live forever in toolExecutions, so checking them
 *      pinned the avatar red. Tool error UX should be a transient flash via
 *      a trigger input, not a sticky boolean (TODO when triggers wired up).
 *   2. Reading — currently running tool is a read-style tool
 *   3. UsingTool — currently running tool is anything else
 *   4. Coding — streaming with content
 *   5. Thinking — streaming, no content yet
 *   6. Idle
 *
 * Note: while the mapper distinguishes thinking/coding/reading/usingTool, the
 * .riv currently only animates Idle/Coding/Error. Missing inputs no-op in the
 * component, so for now any "doing something" state implicitly visualizes via
 * isCoding only when its dedicated boolean has no animation. To explicitly
 * fold all activity into Coding, set BROAD_CODING = true below.
 */
const BROAD_CODING = true;

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

  // 1. Error state — only desynced is a live signal
  if (live.desynced) {
    return { ...idle, isIdle: false, isError: true };
  }

  // Inspect tool executions for currently running ones
  let hasRunningTool = false;
  let runningToolIsRead = false;

  for (const [, exec] of live.toolExecutions) {
    if (exec.status === "running") {
      hasRunningTool = true;
      if (READ_TOOLS.has(exec.toolName)) runningToolIsRead = true;
    }
  }

  // Decide active state
  const active = hasRunningTool || live.isStreaming;
  if (!active) return idle;

  if (BROAD_CODING) {
    // Collapse all activity → isCoding (until other animations are authored)
    return { ...idle, isIdle: false, isCoding: true };
  }

  if (hasRunningTool) {
    if (runningToolIsRead) {
      return { ...idle, isIdle: false, isReading: true };
    }
    return { ...idle, isIdle: false, isUsingTool: true };
  }

  if (live.streamingMessage && live.streamingMessage.content.length > 0) {
    return { ...idle, isIdle: false, isCoding: true };
  }
  return { ...idle, isIdle: false, isThinking: true };
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
