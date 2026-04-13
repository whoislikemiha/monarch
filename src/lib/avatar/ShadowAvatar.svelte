<script lang="ts">
  import { onMount } from "svelte";
  import { Rive, EventType, StateMachineInputType, Layout, Fit, Alignment } from "@rive-app/canvas";
  import type { StateMachineInput } from "@rive-app/canvas";
  import { liveAgentStore, detachedLiveState } from "../toolbox/liveAgentStore.svelte";
  import {
    deriveAnimationState,
    detectTriggers,
    type AnimationState,
  } from "./stateMapper";

  const DEFAULT_RIV = "/avatars/shadow_animations.riv";
  const DEFAULT_STATE_MACHINE = "ShadowSM";

  let {
    agentId,
    size = 64,
    stateMachine = DEFAULT_STATE_MACHINE,
    riveFile = DEFAULT_RIV,
  }: {
    agentId: string;
    size?: number;
    stateMachine?: string;
    riveFile?: string;
  } = $props();

  let canvasEl: HTMLCanvasElement;
  let riveInstance: Rive | null = null;

  // Cached input references keyed by name for fast access in $effect
  let inputMap = new Map<string, StateMachineInput>();
  // Reactive flag so the input-driving $effect re-runs once Rive has loaded
  // and `cacheInputs` has populated `inputMap`. Without this, an agent that
  // is idle from mount onwards would never trigger the effect again, leaving
  // the state machine stuck in whatever its default state is (e.g. Coding).
  let riveReady = $state(false);

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

  /**
   * Write the animation booleans + dispatch any pending triggers.
   *
   * Shared by the Load handler (runs synchronously the moment inputs are
   * cached, before Rive paints its first frame) and the reactive `$effect`
   * (runs on every subsequent state change). Keeping them in one function
   * guarantees the `prevAnimState` bookkeeping for trigger detection is
   * consistent across both entry points.
   *
   * Fixes an initial flash where Rive would autoplay its default state
   * (Coding) for a frame or two between `Load` and the first `$effect`
   * tick, visually snapping to Idle a moment after mount.
   */
  function applyAnimState(state: AnimationState): void {
    setBool("isIdle", state.isIdle);
    setBool("isThinking", state.isThinking);
    setBool("isCoding", state.isCoding);
    setBool("isReading", state.isReading);
    setBool("isUsingTool", state.isUsingTool);
    setBool("isWaiting", state.isWaiting);
    setBool("isError", state.isError);

    const triggers = detectTriggers(prevAnimState, state);
    if (triggers.taskComplete) fireTrigger("taskComplete");
    if (triggers.summon) fireTrigger("summon");

    prevAnimState = { ...state };
  }

  onMount(() => {
    const dpr = window.devicePixelRatio || 1;
    canvasEl.width = size * dpr;
    canvasEl.height = size * dpr;

    riveInstance = new Rive({
      src: riveFile,
      canvas: canvasEl,
      stateMachines: stateMachine,
      layout: new Layout({ fit: Fit.Contain, alignment: Alignment.Center }),
      autoplay: true,
    });

    riveInstance.on(EventType.Load, () => {
      riveInstance!.resizeDrawingSurfaceToCanvas();
      // Auto-detect: use first available state machine
      const r = riveInstance! as any;
      const smNames: string[] = r.stateMachineNames ?? [];
      const smName = smNames[0] ?? stateMachine;
      cacheInputs(riveInstance!, smName);
      // Write inputs synchronously in the same tick the SM becomes playable.
      // Must happen BEFORE `riveReady = true` so we beat the default-state
      // render that Rive would otherwise paint for a frame or two while the
      // reactive `$effect` waits its turn on the scheduler.
      applyAnimState(animState);
      riveReady = true;
    });

    riveInstance.on(EventType.LoadError, (e: any) => {
      console.error("[ShadowAvatar] Rive load error", agentId, e);
    });

    return () => {
      riveInstance?.cleanup();
      riveInstance = null;
      inputMap.clear();
      riveReady = false;
    };
  });

  // Drive Rive inputs from agent state changes. First application is done
  // inline in the Load handler; this effect handles every subsequent update.
  $effect(() => {
    if (!riveReady) return;
    applyAnimState(animState);
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
