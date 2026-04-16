<script lang="ts">
  import type { AgentConfig } from "./types";
  import SpawnForm from "./SpawnForm.svelte";

  let {
    onspawn,
    oncancel,
  }: {
    onspawn: (config: AgentConfig) => void;
    oncancel: () => void;
  } = $props();
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="overlay" role="presentation">
  <div
    class="dialog"
    onclick={(e: MouseEvent) => e.stopPropagation()}
    role="dialog"
    tabindex="-1"
  >
    <h2>Extract Shadow</h2>
    <SpawnForm {onspawn} {oncancel} />
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: var(--overlay-backdrop);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
    overflow-y: auto;
    z-index: 100;
  }

  .dialog {
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: 12px;
    padding: 24px;
    width: min(560px, 100%);
    max-width: min(560px, calc(100vw - 48px));
    max-height: calc(100vh - 48px);
    display: flex;
    flex-direction: column;
    gap: 16px;
    overflow-y: auto;
    box-shadow: 0 28px 80px var(--shadow-dark);
  }

  h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    color: var(--text-primary);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  @media (max-width: 640px) {
    .overlay {
      padding: 16px;
    }

    .dialog {
      padding: 20px;
      max-width: calc(100vw - 32px);
      max-height: calc(100vh - 32px);
    }
  }
</style>
