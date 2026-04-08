<script lang="ts">
  import ToolCallCard from "./ToolCallCard.svelte";
  import type { ToolExecution } from "./types";

  let { executions, turnComplete }: { executions: ToolExecution[]; turnComplete: boolean } = $props();

  let expanded = $state(false);

  let runningTool = $derived(executions.find((e) => e.status === "running"));
  let doneCount = $derived(executions.filter((e) => e.status !== "running").length);
  let errorCount = $derived(executions.filter((e) => e.isError).length);
  let totalCount = $derived(executions.length);

  function toolSummary(exec: ToolExecution): string {
    const args = exec.args;
    switch (exec.toolName) {
      case "bash": return args?.command?.slice(0, 50) || "";
      case "read": return args?.file_path || args?.path || "";
      case "write": return args?.file_path || args?.path || "";
      case "edit": return args?.file_path || args?.path || "";
      case "grep": return `"${args?.pattern || ""}"`;
      case "find": return args?.pattern || args?.glob || "";
      case "ls": return args?.path || ".";
      default: return "";
    }
  }
</script>

<div class="tool-group" class:has-errors={errorCount > 0}>
  <button class="tool-group-header" onclick={() => (expanded = !expanded)}>
    <span class="toggle-arrow">{expanded ? "▾" : "▸"}</span>

    {#if runningTool}
      <span class="running-dot"></span>
      <span class="tool-current">
        {runningTool.toolName}
        <span class="tool-arg">{toolSummary(runningTool)}</span>
      </span>
    {:else}
      <span class="done-dot"></span>
      <span class="tool-summary-text">
        {totalCount} tool{totalCount !== 1 ? "s" : ""} executed
      </span>
    {/if}

    <span class="tool-counts">
      {#if errorCount > 0}
        <span class="count-error">{errorCount} failed</span>
      {/if}
      {#if runningTool}
        <span class="count-done">{doneCount}/{totalCount}</span>
      {/if}
    </span>
  </button>

  {#if !expanded && !runningTool}
    <div class="tool-quick-list">
      {#each executions as exec}
        <span class="quick-item" class:error={exec.isError}>
          {exec.toolName}
        </span>
      {/each}
    </div>
  {/if}

  {#if expanded}
    <div class="tool-group-body">
      {#each executions as exec (exec.toolCallId)}
        <ToolCallCard execution={exec} />
      {/each}
    </div>
  {/if}
</div>

<style>
  .tool-group {
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: var(--bg-panel);
    overflow: hidden;
  }

  .tool-group.has-errors {
    border-color: rgba(238, 83, 150, 0.3);
  }

  .tool-group-header {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: none;
    border: none;
    color: var(--text-secondary);
    cursor: pointer;
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    text-align: left;
  }

  .tool-group-header:hover {
    background: var(--bg-panel-2);
  }

  .toggle-arrow {
    font-size: 10px;
    width: 10px;
    color: var(--text-muted);
  }

  .running-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--warning, #ffe97b);
    animation: pulse 1s ease-in-out infinite;
    flex-shrink: 0;
  }

  .done-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--success, #42be65);
    flex-shrink: 0;
  }

  .tool-current {
    color: var(--accent-cyan);
    display: flex;
    gap: 6px;
    align-items: center;
    min-width: 0;
    flex: 1;
  }

  .tool-arg {
    color: var(--text-muted);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tool-summary-text {
    color: var(--text-muted);
    flex: 1;
  }

  .tool-counts {
    display: flex;
    gap: 8px;
    flex-shrink: 0;
    font-size: 10px;
  }

  .count-done {
    color: var(--text-muted);
  }

  .count-error {
    color: var(--error);
  }

  .tool-quick-list {
    display: flex;
    gap: 4px;
    padding: 0 12px 8px;
    flex-wrap: wrap;
  }

  .quick-item {
    font-size: 10px;
    padding: 1px 6px;
    border-radius: 3px;
    background: var(--bg-panel-2);
    color: var(--text-muted);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .quick-item.error {
    color: var(--error);
    background: rgba(238, 83, 150, 0.08);
  }

  .tool-group-body {
    border-top: 1px solid var(--border-subtle);
    display: flex;
    flex-direction: column;
  }

  .tool-group-body :global(.tool-card) {
    border: none;
    border-radius: 0;
    border-bottom: 1px solid var(--border-subtle);
  }

  .tool-group-body :global(.tool-card:last-child) {
    border-bottom: none;
  }

  @keyframes pulse {
    0%, 100% { opacity: 0.4; }
    50% { opacity: 1; }
  }
</style>
