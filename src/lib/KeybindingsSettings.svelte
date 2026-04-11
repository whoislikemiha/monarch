<script lang="ts">
  import {
    BINDING_GROUPS,
    getAllBindings,
    setBinding,
    resetAllBindings,
    eventToBindingString,
    formatBindingParts,
    type BindingGroup,
  } from "$lib/keybindings.svelte";

  let capturingId: string | null = $state(null);
  let captureOverlayRef: HTMLDivElement | undefined = $state(undefined);

  $effect(() => {
    if (capturingId && captureOverlayRef) {
      captureOverlayRef.focus();
    }
  });

  let bindings = $derived(getAllBindings());

  function groupedBindings(): Array<{ group: BindingGroup; items: typeof bindings }> {
    return BINDING_GROUPS.map((group) => ({
      group,
      items: bindings.filter((b) => b.group === group),
    })).filter((g) => g.items.length > 0);
  }

  function startCapture(id: string) {
    capturingId = id;
  }

  function handleCaptureKeydown(e: KeyboardEvent) {
    if (!capturingId) return;
    e.preventDefault();
    e.stopPropagation();

    if (e.key === "Escape") {
      capturingId = null;
      return;
    }

    const keys = eventToBindingString(e);
    if (!keys) return;

    setBinding(capturingId, keys);
    capturingId = null;
  }

  function resetBinding(id: string) {
    setBinding(id, getAllBindings().find((b) => b.id === id)?.defaultKeys ?? "");
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div class="keybindings-panel" onkeydown={capturingId ? handleCaptureKeydown : undefined}>
  {#if capturingId}
    <div class="capture-overlay" tabindex="0" onkeydown={handleCaptureKeydown}
      bind:this={captureOverlayRef}>
      <span>Press a new shortcut or Escape to cancel</span>
    </div>
  {/if}
  {#each groupedBindings() as { group, items } (group)}
    <div class="group">
      <span class="group-label">{group}</span>
      <div class="group-items">
        {#each items as binding (binding.id)}
          <div class="binding-row" class:non-editable={!binding.editable}>
            <div class="binding-info">
              <span class="binding-label">{binding.label}</span>
              {#if binding.hint}
                <span class="binding-hint">{binding.hint}</span>
              {/if}
            </div>
            <div class="binding-keys">
              {#if capturingId === binding.id}
                <span class="capture-prompt">Press shortcut...</span>
              {:else}
                <div class="kbd-group">
                  {#each formatBindingParts(binding.currentKeys) as part}
                    <kbd>{part}</kbd>
                  {/each}
                </div>
                {#if binding.editable}
                  <button class="btn-edit" onclick={() => startCapture(binding.id)}>Edit</button>
                  {#if binding.isOverridden}
                    <button class="btn-reset-single" onclick={() => resetBinding(binding.id)} title="Reset to default">
                      <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
                        <path d="M2 8a6 6 0 1 1 1.76 4.24" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                        <path d="M2 12V8h4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                      </svg>
                    </button>
                  {/if}
                {:else}
                  <span class="badge-system">System</span>
                {/if}
              {/if}
            </div>
          </div>
        {/each}
      </div>
    </div>
  {/each}

  <div class="reset-section">
    <button class="btn-reset-all" onclick={resetAllBindings}>Reset all to defaults</button>
  </div>
</div>

<style>
  .keybindings-panel {
    display: flex;
    flex-direction: column;
    gap: 20px;
    position: relative;
  }

  .capture-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
    outline: none;
  }

  .capture-overlay span {
    font-size: 14px;
    color: #fff;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    animation: pulse 1.2s ease-in-out infinite;
  }

  .group {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .group-label {
    font-size: 10px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    margin-bottom: 4px;
  }

  .group-items {
    display: flex;
    flex-direction: column;
  }

  .binding-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 0;
    border-bottom: 1px solid var(--border-subtle);
    gap: 12px;
  }

  .binding-row.non-editable {
    opacity: 0.7;
  }

  .binding-info {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .binding-label {
    font-size: 12px;
    color: var(--text-primary);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .binding-hint {
    font-size: 10px;
    color: var(--text-muted);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .binding-keys {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .kbd-group {
    display: flex;
    align-items: center;
    gap: 3px;
  }

  kbd {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 22px;
    height: 22px;
    padding: 0 6px;
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    background: var(--bg-panel-2);
    color: var(--text-secondary);
    font-size: 10px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    line-height: 1;
    white-space: nowrap;
  }

  .capture-prompt {
    font-size: 11px;
    color: var(--accent);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    animation: pulse 1.2s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }

  .btn-edit {
    padding: 2px 8px;
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    background: transparent;
    color: var(--text-muted);
    font-size: 10px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }

  .btn-edit:hover {
    background: var(--bg-panel-2);
    color: var(--text-primary);
  }

  .btn-reset-single {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }

  .btn-reset-single:hover {
    background: var(--bg-panel-2);
    color: var(--text-primary);
  }

  .badge-system {
    font-size: 9px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    padding: 2px 6px;
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
  }

  .reset-section {
    padding-top: 8px;
  }

  .btn-reset-all {
    padding: 6px 12px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: transparent;
    color: var(--text-muted);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }

  .btn-reset-all:hover {
    background: var(--bg-panel-2);
    color: var(--text-primary);
  }
</style>
