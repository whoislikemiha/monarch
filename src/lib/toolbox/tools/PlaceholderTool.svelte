<script lang="ts">
  import { invoke } from "$lib/api";
  import type { ToolProps } from "../types";

  let { agentContext }: ToolProps = $props();

  let pingResult = $state<string | null>(null);
  let pingError = $state<string | null>(null);
  let pinging = $state(false);

  let messageCount = $derived(
    agentContext
      ? agentContext.live.items.filter(
          (i) => i.kind === "user" || i.kind === "assistant",
        ).length
      : 0,
  );

  async function ping() {
    pinging = true;
    pingError = null;
    try {
      pingResult = await invoke<string>("toolbox_placeholder_ping");
    } catch (e) {
      pingError = String(e);
      pingResult = null;
    } finally {
      pinging = false;
    }
  }
</script>

<div class="placeholder-tool">
  {#if agentContext}
    <div class="row">
      <span class="label">Agent</span>
      <span class="value">{agentContext.agent.shadow?.shadowName ?? agentContext.agent.name}</span>
    </div>
    <div class="row">
      <span class="label">ID</span>
      <span class="value mono">{agentContext.agentId}</span>
    </div>
    <div class="row">
      <span class="label">Messages</span>
      <span class="value">{messageCount}</span>
    </div>
  {:else}
    <p class="empty">No agent selected.</p>
  {/if}

  <button class="ping-btn" type="button" onclick={ping} disabled={pinging}>
    {pinging ? "Pinging…" : "Ping backend"}
  </button>

  {#if pingResult}
    <pre class="result">{pingResult}</pre>
  {/if}
  {#if pingError}
    <pre class="error">{pingError}</pre>
  {/if}
</div>

<style>
  .placeholder-tool {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .row {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    font-size: 11px;
  }

  .label {
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .value {
    color: var(--text-primary);
  }

  .value.mono {
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    font-size: 10px;
    word-break: break-all;
    text-align: right;
  }

  .empty {
    margin: 0;
    color: var(--text-muted);
    font-size: 11px;
    font-style: italic;
  }

  .ping-btn {
    margin-top: 4px;
    padding: 6px 10px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: var(--bg-panel-2);
    color: var(--text-primary);
    font-family: inherit;
    font-size: 11px;
    cursor: pointer;
    transition: background 0.15s;
  }

  .ping-btn:hover:not(:disabled) {
    background: var(--accent-bg-hover);
  }

  .ping-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .result,
  .error {
    margin: 0;
    padding: 6px 8px;
    border-radius: 4px;
    font-size: 10px;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .result {
    background: var(--accent-bg-subtle);
    color: var(--text-primary);
  }

  .error {
    background: var(--error-bg-faint);
    color: var(--error-light);
  }
</style>
