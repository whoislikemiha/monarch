<script lang="ts">
  /**
   * MON-130: expanded body of a timeline tool row — the FULL tool input (and
   * result), fetched on demand by tool_call_id via `db_get_tool_call_detail`.
   * The persisted event payload only carries a 500-char preview; the full
   * call lives in the messages table. Falls back to the preview when the
   * backing message is gone (pruned session, in-flight call).
   */
  import { invoke } from "$lib/api";
  import type { ToolCallDetail } from "$lib/bindings";
  import type { ToolCallView } from "./timelineModel";

  interface Props {
    tool: ToolCallView;
  }
  let { tool }: Props = $props();

  let detail = $state<ToolCallDetail | null>(null);
  let loading = $state(true);
  let failed = $state(false);

  $effect(() => {
    const id = tool.toolCallId;
    loading = true;
    failed = false;
    detail = null;
    invoke<ToolCallDetail>("db_get_tool_call_detail", { toolCallId: id })
      .then((d) => (detail = d))
      .catch(() => (failed = true))
      .finally(() => (loading = false));
  });

  let args = $derived(detail?.argsJson ?? (failed || !loading ? tool.argsPreview || null : null));
  let result = $derived(detail?.resultText ?? tool.resultPreview);
  let partial = $derived(!loading && !detail?.argsJson);
</script>

<div class="detail">
  {#if loading}
    <div class="hint mono">loading full input…</div>
  {:else}
    {#if args}
      <div class="sect">
        <span class="label mono">input{partial ? " (preview — full call not in the message store)" : ""}</span>
        <pre class="block mono">{args}</pre>
      </div>
    {/if}
    {#if result}
      <div class="sect">
        <span class="label mono" class:err={tool.isError}>{tool.isError ? "error output" : "output"}</span>
        <pre class="block mono">{result}</pre>
      </div>
    {/if}
    {#if !args && !result}
      <div class="hint mono">no recorded input for this call</div>
    {/if}
  {/if}
</div>

<style>
  .detail {
    display: flex;
    flex-direction: column;
    gap: var(--s2);
    padding: var(--s2) 0 var(--s2) calc(14px + var(--s3));
    min-width: 0;
  }
  .sect { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .label {
    font-size: 9px; font-weight: 700; letter-spacing: 0.1em;
    text-transform: uppercase; color: var(--text-muted);
  }
  .label.err { color: var(--status-error); }
  .block {
    margin: 0;
    padding: var(--s2) var(--s3);
    background: var(--bg-sink);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-sm);
    font-size: 10.5px;
    line-height: 1.5;
    color: var(--text-secondary);
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 320px;
    overflow-y: auto;
    min-width: 0;
  }
  .hint { font-size: 10px; color: var(--text-muted); }
</style>
