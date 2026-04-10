<script lang="ts">
  import { getTool } from "./registry";
  import {
    TOOLBOX_MIN_WIDTH,
    TOOLBOX_MAX_WIDTH,
    clampWidth,
  } from "./persistence";
  import type { AgentContext } from "./types";

  let {
    openToolIds,
    agentContext,
    width,
    onclose,
    onresize,
  }: {
    openToolIds: string[];
    agentContext: AgentContext;
    width: number;
    onclose: (id: string) => void;
    onresize: (width: number) => void;
  } = $props();

  let openTools = $derived(
    openToolIds
      .map((id) => getTool(id))
      .filter((t): t is NonNullable<ReturnType<typeof getTool>> => !!t),
  );

  let dragging = $state(false);
  let dragStartX = 0;
  let dragStartWidth = 0;

  function onHandlePointerDown(event: PointerEvent) {
    dragging = true;
    dragStartX = event.clientX;
    dragStartWidth = width;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function onHandlePointerMove(event: PointerEvent) {
    if (!dragging) return;
    const dx = dragStartX - event.clientX;
    onresize(clampWidth(dragStartWidth + dx));
  }

  function onHandlePointerUp(event: PointerEvent) {
    if (!dragging) return;
    dragging = false;
    (event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId);
  }
</script>

<div
  class="tool-panel-stack"
  class:empty={openTools.length === 0}
  style="width: {openTools.length === 0 ? 0 : width}px"
>
  {#if openTools.length > 0}
    <div
      class="resize-handle"
      role="separator"
      aria-orientation="vertical"
      aria-valuenow={width}
      aria-valuemin={TOOLBOX_MIN_WIDTH}
      aria-valuemax={TOOLBOX_MAX_WIDTH}
      onpointerdown={onHandlePointerDown}
      onpointermove={onHandlePointerMove}
      onpointerup={onHandlePointerUp}
      onpointercancel={onHandlePointerUp}
    ></div>
    <div class="panels">
      {#each openTools as tool (tool.id)}
        {@const ToolComponent = tool.component}
        <section class="panel" aria-label={tool.title}>
          <header class="panel-header">
            <span class="panel-title">{tool.title}</span>
            <button
              type="button"
              class="panel-close"
              aria-label="Close {tool.title}"
              onclick={() => onclose(tool.id)}
            >×</button>
          </header>
          <div class="panel-body">
            <ToolComponent {agentContext} />
          </div>
        </section>
      {/each}
    </div>
  {/if}
</div>

<style>
  .tool-panel-stack {
    position: relative;
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    background: var(--bg-panel, #171126);
    border-left: 1px solid var(--border-subtle, #35274f);
    min-height: 0;
    overflow: hidden;
  }

  .tool-panel-stack.empty {
    border-left: none;
  }

  .resize-handle {
    position: absolute;
    top: 0;
    left: -3px;
    width: 6px;
    height: 100%;
    cursor: ew-resize;
    z-index: 1;
    background: transparent;
    transition: background 0.15s;
  }

  .resize-handle:hover,
  .resize-handle:active {
    background: rgba(190, 149, 255, 0.35);
  }

  .panels {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }

  .panel {
    flex: 1 1 0;
    min-height: 160px;
    display: flex;
    flex-direction: column;
    min-width: 0;
    border-bottom: 1px solid var(--border-subtle, #35274f);
  }

  .panel:last-child {
    border-bottom: none;
  }

  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 10px;
    background: var(--bg-sidebar, #0c0816);
    border-bottom: 1px solid var(--border-subtle, #35274f);
    flex-shrink: 0;
  }

  .panel-title {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-secondary, #dde1e6);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .panel-close {
    width: 20px;
    height: 20px;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--text-muted, #9aa0a6);
    font-size: 16px;
    line-height: 1;
    cursor: pointer;
    border-radius: 4px;
  }

  .panel-close:hover {
    background: var(--bg-panel-2, #201734);
    color: var(--text-primary, #f2f4f8);
  }

  .panel-body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 12px;
    font-size: 12px;
    color: var(--text-primary, #f2f4f8);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }
</style>
