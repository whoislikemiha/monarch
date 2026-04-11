<script lang="ts">
  import { onMount } from "svelte";
  import { Rive, EventType, StateMachineInputType } from "@rive-app/webgl2";
  import type { StateMachineInput } from "@rive-app/webgl2";
  import { liveAgentStore, detachedLiveState } from "../toolbox/liveAgentStore.svelte";
  import {
    deriveAnimationState,
    detectTriggers,
    type AnimationState,
  } from "./stateMapper";

  const DEFAULT_RIV = "/avatars/shadow_animations.riv";
  const DEFAULT_STATE_MACHINE = "State Machine 1";

  let {
    agentId,
    size = 64,
    stateMachine = DEFAULT_STATE_MACHINE,
    riveFile = DEFAULT_RIV,
    useOffscreenRenderer = true,
  }: {
    agentId: string;
    size?: number;
    stateMachine?: string;
    riveFile?: string;
    useOffscreenRenderer?: boolean;
  } = $props();

  let canvasEl: HTMLCanvasElement;
  let riveInstance: Rive | null = null;

  // Cached input references keyed by name for fast access in $effect
  let inputMap = new Map<string, StateMachineInput>();

  // Track previous animation state for trigger detection
  let prevAnimState: AnimationState | null = null;

  const live = $derived(liveAgentStore.byAgent.get(agentId) ?? detachedLiveState());
  const animState = $derived(deriveAnimationState(live));

  function cacheInputs(rive: Rive, smName: string): void {
    inputMap.clear();
    try {
      const inputs = rive.stateMachineInputs(smName);
      for (const input of inputs) {
        inputMap.set(input.name, input);
      }
    } catch {
      // State machine might not exist in placeholder .riv — that's fine
    }
  }

  function setBool(name: string, value: boolean): void {
    const input = inputMap.get(name);
    if (input && input.type === StateMachineInputType.Boolean) {
      input.value = value;
    }
  }

  function fireTrigger(name: string): void {
    const input = inputMap.get(name);
    if (input && input.type === StateMachineInputType.Trigger) {
      input.fire();
    }
  }

  function setNumber(name: string, value: number): void {
    const input = inputMap.get(name);
    if (input && input.type === StateMachineInputType.Number) {
      input.value = value;
    }
  }

  onMount(() => {
    const dpr = window.devicePixelRatio || 1;
    canvasEl.width = size * dpr;
    canvasEl.height = size * dpr;

    riveInstance = new Rive({
      src: riveFile,
      canvas: canvasEl,
      stateMachines: stateMachine,
      autoplay: true,
      useOffscreenRenderer,
    });

    riveInstance.on(EventType.Load, () => {
      riveInstance!.resizeDrawingSurfaceToCanvas();
      cacheInputs(riveInstance!, stateMachine);
    });

    return () => {
      riveInstance?.cleanup();
      riveInstance = null;
      inputMap.clear();
    };
  });

  // Drive Rive inputs from agent state changes
  $effect(() => {
    if (inputMap.size === 0) return;

    // Apply boolean states
    setBool("isIdle", animState.isIdle);
    setBool("isThinking", animState.isThinking);
    setBool("isCoding", animState.isCoding);
    setBool("isReading", animState.isReading);
    setBool("isUsingTool", animState.isUsingTool);
    setBool("isWaiting", animState.isWaiting);
    setBool("isError", animState.isError);

    // Detect and fire triggers
    const triggers = detectTriggers(prevAnimState, animState);
    if (triggers.taskComplete) fireTrigger("taskComplete");
    if (triggers.summon) fireTrigger("summon");

    prevAnimState = { ...animState };
  });

  /**
   * Set the grade input (1-5, E through S rank).
   * Exposed for parent components to call when agent data is available.
   */
  export function setGrade(grade: number): void {
    setNumber("grade", Math.max(1, Math.min(5, grade)));
  }

  /**
   * Set the experience input (0-100, normalized from total tokens).
   * Exposed for parent components to call when stats are available.
   */
  export function setExperience(experience: number): void {
    setNumber("experience", Math.max(0, Math.min(100, experience)));
  }
</script>

<canvas
  bind:this={canvasEl}
  style="width: {size}px; height: {size}px;"
  class="shadow-avatar"
></canvas>

<style>
  .shadow-avatar {
    display: block;
    image-rendering: auto;
  }
</style>
