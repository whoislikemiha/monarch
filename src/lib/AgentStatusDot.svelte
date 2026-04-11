<script lang="ts">
  import type { Agent } from "./types";
  import { liveAgentStore } from "./toolbox/liveAgentStore.svelte";

  let { agent, baseClass }: { agent: Agent; baseClass: "tab-dot" | "status-dot" } = $props();

  const live = $derived(liveAgentStore.byAgent.get(agent.id));
  const streaming = $derived(live?.isStreaming ?? false);
</script>

<span class="{baseClass} {streaming ? 'streaming' : agent.status}"></span>

<style>
  .tab-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .tab-dot.running {
    background: var(--success);
    box-shadow: 0 0 3px var(--success-glow);
  }
  .tab-dot.stopped {
    background: var(--text-muted);
  }
  .tab-dot.starting {
    background: var(--warning);
  }
  .tab-dot.streaming {
    background: var(--accent-blue);
    box-shadow: 0 0 3px var(--success-glow);
    animation: pulse 1s ease-in-out infinite;
  }
  .tab-dot.error {
    background: var(--error);
  }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .status-dot.running {
    background: var(--success);
    box-shadow: 0 0 4px var(--success-glow);
  }
  .status-dot.stopped {
    background: var(--text-muted);
  }
  .status-dot.starting {
    background: var(--warning);
  }
  .status-dot.streaming {
    background: var(--accent-blue);
    box-shadow: 0 0 4px var(--success-glow);
    animation: pulse 1s ease-in-out infinite;
  }
  .status-dot.error {
    background: var(--error);
  }

  @keyframes pulse {
    0%, 100% { opacity: 0.6; }
    50% { opacity: 1; }
  }
</style>
