<script lang="ts">
  /**
   * A group of tool executions for one assistant turn, rendered compactly:
   * one row per call (name · target · status), expandable to show args/result.
   * Flat, instrument-grade — not chat bubbles.
   */
  import type { ToolExecution } from "$lib/types";

  interface Props {
    executions: ToolExecution[];
  }
  let { executions }: Props = $props();

  let open = $state(new Set<string>());
  function toggle(id: string) {
    if (open.has(id)) open.delete(id);
    else open.add(id);
    open = new Set(open);
  }

  /** Best-effort one-line target from common arg shapes (path, command, query). */
  function target(args: any): string {
    if (!args || typeof args !== "object") return "";
    return (
      args.path ?? args.file_path ?? args.filePath ?? args.command ?? args.query ?? args.pattern ?? ""
    )
      .toString()
      .slice(0, 80);
  }

  function preview(value: any): string {
    if (value == null) return "";
    if (typeof value === "string") return value;
    try {
      return JSON.stringify(value, null, 2);
    } catch {
      return String(value);
    }
  }
</script>

<div class="tools">
  {#each executions as ex (ex.toolCallId)}
    <div class="tool">
      <button class="trow" onclick={() => toggle(ex.toolCallId)}>
        <span class="dot {ex.status}" aria-hidden="true"></span>
        <span class="name mono">{ex.toolName}</span>
        {#if target(ex.args)}<span class="tgt mono">{target(ex.args)}</span>{/if}
        <span class="chev" class:open={open.has(ex.toolCallId)} aria-hidden="true">›</span>
      </button>
      {#if open.has(ex.toolCallId)}
        <div class="detail">
          {#if ex.args && Object.keys(ex.args).length}
            <div class="codeblock"><pre>{preview(ex.args)}</pre></div>
          {/if}
          {#if ex.result !== undefined}
            <div class="codeblock" class:err={ex.isError}><pre>{preview(ex.result)}</pre></div>
          {/if}
        </div>
      {/if}
    </div>
  {/each}
</div>

<style>
  .tools { display: flex; flex-direction: column; gap: 1px; border: 1px solid var(--border-subtle); border-radius: var(--r-md); overflow: hidden; }
  .tool { background: var(--bg-panel); }
  .trow {
    display: flex; align-items: center; gap: var(--s2); width: 100%;
    padding: 5px var(--s3); background: none; border: none; cursor: pointer;
    font: inherit; text-align: left; color: var(--text-primary);
    border-bottom: 1px solid transparent;
  }
  .trow:hover { background: var(--bg-raised); }
  .dot { width: 7px; height: 7px; border-radius: var(--r-full); flex: none; background: var(--text-muted); }
  .dot.running { background: var(--status-info); }
  .dot.done { background: var(--status-success); }
  .dot.error { background: var(--status-error); }
  .name { font-size: 11px; color: var(--text-secondary); flex: none; }
  .tgt { font-size: 10.5px; color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .chev { margin-left: auto; color: var(--text-muted); transition: transform 0.15s; flex: none; }
  .chev.open { transform: rotate(90deg); }
  .detail { padding: var(--s2) var(--s3) var(--s3); display: flex; flex-direction: column; gap: var(--s2); }
  .codeblock { background: var(--bg-sink); border: 1px solid var(--border-subtle); border-radius: var(--r-sm); }
  .codeblock.err { border-color: var(--error-border-subtle); }
  .codeblock pre { margin: 0; padding: var(--s2) var(--s3); font-family: "JetBrains Mono", monospace; font-size: 11px; line-height: 1.6; color: var(--text-secondary); overflow-x: auto; white-space: pre-wrap; word-break: break-word; max-height: 320px; }
</style>
