<script lang="ts">
  import type { ToolExecution } from "./types";

  let { execution }: { execution: ToolExecution } = $props();

  let expanded = $state(false);

  // Smart arg display per tool type
  function formatArgs(toolName: string, args: any): string {
    if (!args) return "";
    switch (toolName) {
      case "bash":
        return args.command || JSON.stringify(args, null, 2);
      case "read":
        return args.file_path || args.path || JSON.stringify(args, null, 2);
      case "write":
        return args.file_path || args.path || JSON.stringify(args, null, 2);
      case "edit":
        return args.file_path || args.path || JSON.stringify(args, null, 2);
      case "grep":
        return `${args.pattern || ""} ${args.path || ""}`.trim() || JSON.stringify(args, null, 2);
      case "find":
        return `${args.pattern || args.glob || ""} ${args.path || ""}`.trim() || JSON.stringify(args, null, 2);
      default:
        return JSON.stringify(args, null, 2);
    }
  }

  // Summary line for the header
  function argSummary(toolName: string, args: any): string {
    if (!args) return "";
    switch (toolName) {
      case "bash":
        return truncate(args.command || "", 60);
      case "read":
        return args.file_path || args.path || "";
      case "write":
        return args.file_path || args.path || "";
      case "edit":
        return args.file_path || args.path || "";
      case "grep":
        return truncate(`"${args.pattern || ""}" in ${args.path || "."}`, 60);
      case "find":
        return truncate(`${args.pattern || args.glob || ""} in ${args.path || "."}`, 60);
      case "ls":
        return args.path || ".";
      default:
        return "";
    }
  }

  function truncate(s: string, max: number): string {
    return s.length > max ? s.slice(0, max) + "..." : s;
  }

  function getResultText(result: any): string {
    if (!result) return "";
    if (typeof result === "string") return result;
    if (result.content) {
      return result.content
        .map((c: any) => (c.type === "text" ? c.text : `[${c.type}]`))
        .join("\n");
    }
    return JSON.stringify(result, null, 2);
  }

  // Check if result contains diff-like content
  function isDiff(text: string): boolean {
    const lines = text.split("\n").slice(0, 10);
    return lines.some(
      (l) =>
        l.startsWith("---") ||
        l.startsWith("+++") ||
        l.startsWith("@@") ||
        (l.startsWith("-") && !l.startsWith("---")) ||
        (l.startsWith("+") && !l.startsWith("+++")),
    );
  }

  function renderDiffLine(line: string): { cls: string; text: string } {
    if (line.startsWith("@@")) return { cls: "diff-hunk", text: line };
    if (line.startsWith("---") || line.startsWith("+++"))
      return { cls: "diff-header", text: line };
    if (line.startsWith("-")) return { cls: "diff-del", text: line };
    if (line.startsWith("+")) return { cls: "diff-add", text: line };
    return { cls: "", text: line };
  }

  const statusColors: Record<string, string> = {
    running: "#ffe97b",
    done: "#42be65",
    error: "#ee5396",
  };

  let resultText = $derived(getResultText(execution.result));
  let showDiff = $derived(resultText && isDiff(resultText));
  let summary = $derived(argSummary(execution.toolName, execution.args));
</script>

<div class="tool-card" class:error={execution.isError}>
  <button class="tool-header" onclick={() => (expanded = !expanded)}>
    <span class="toggle-arrow">{expanded ? "▾" : "▸"}</span>
    <span
      class="status-dot"
      style="background: {statusColors[execution.status]}"
    ></span>
    <span class="tool-name">{execution.toolName}</span>
    {#if summary}
      <span class="tool-summary">{summary}</span>
    {/if}
    {#if execution.status === "running"}
      <span class="running-tag">running...</span>
    {/if}
    {#if execution.isError}
      <span class="error-tag">error</span>
    {/if}
  </button>

  {#if expanded}
    <div class="tool-body">
      {#if execution.args}
        <div class="tool-section">
          <div class="section-label">Arguments</div>
          <pre class="tool-pre">{formatArgs(execution.toolName, execution.args)}</pre>
        </div>
      {/if}
      {#if execution.result !== undefined}
        <div class="tool-section">
          <div class="section-label">Result</div>
          {#if showDiff}
            <div class="diff-view">
              {#each resultText.split("\n") as line}
                {@const d = renderDiffLine(line)}
                <div class="diff-line {d.cls}">{d.text}</div>
              {/each}
            </div>
          {:else}
            <pre class="tool-pre" class:error-text={execution.isError}>{resultText}</pre>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .tool-card {
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: var(--bg-panel);
    overflow: hidden;
  }

  .tool-card.error {
    border-color: rgba(238, 83, 150, 0.35);
  }

  .tool-header {
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

  .tool-header:hover {
    background: var(--bg-panel-2);
  }

  .toggle-arrow {
    font-size: 10px;
    width: 10px;
    color: var(--text-muted);
  }

  .status-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .tool-name {
    color: var(--accent-cyan);
    flex-shrink: 0;
  }

  .tool-summary {
    color: var(--text-muted);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
  }

  .running-tag {
    color: var(--warning);
    font-size: 10px;
    flex-shrink: 0;
  }

  .error-tag {
    color: var(--error);
    font-size: 10px;
    flex-shrink: 0;
  }

  .tool-body {
    border-top: 1px solid var(--border-subtle);
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .section-label {
    font-size: 10px;
    color: var(--text-muted);
    margin-bottom: 4px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .tool-pre {
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    color: var(--text-secondary);
    background: #110b1d;
    padding: 8px 10px;
    border-radius: 4px;
    overflow-x: auto;
    white-space: pre-wrap;
    word-break: break-word;
    margin: 0;
    max-height: 400px;
    overflow-y: auto;
  }

  .error-text {
    color: var(--error);
  }

  /* Diff rendering */
  .diff-view {
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    background: #110b1d;
    border-radius: 4px;
    overflow-x: auto;
    max-height: 400px;
    overflow-y: auto;
  }

  .diff-line {
    padding: 1px 10px;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .diff-add {
    background: rgba(66, 190, 101, 0.12);
    color: #42be65;
  }

  .diff-del {
    background: rgba(238, 83, 150, 0.12);
    color: #ee5396;
  }

  .diff-hunk {
    color: var(--accent-blue);
    padding-top: 6px;
    padding-bottom: 2px;
  }

  .diff-header {
    color: var(--text-muted);
    font-weight: 600;
  }
</style>
