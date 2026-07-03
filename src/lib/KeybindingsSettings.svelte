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
  let captureRef: HTMLDivElement | undefined = $state(undefined);

  $effect(() => {
    if (capturingId && captureRef) {
      captureRef.focus();
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

<div class="keybindings-panel">
  {#each groupedBindings() as { group, items } (group)}
    <div class="group">
      <span class="group-label">{group}</span>
      <div class="group-items">
        {#each items as binding (binding.id)}
          <div class="binding-row" class:non-editable={!binding.editable} class:capturing={capturingId === binding.id}>
            <div class="binding-info">
              <span class="binding-label">{binding.label}</span>
              {#if binding.hint}
                <span class="binding-hint">{binding.hint}</span>
              {/if}
            </div>
            <div class="binding-keys">
              {#if capturingId === binding.id}
                <div
                  class="capture-box"
                  role="textbox"
                  aria-label="Type shortcut"
                  tabindex="0"
                  onkeydown={handleCaptureKeydown}
                  onblur={() => (capturingId = null)}
                  bind:this={captureRef}
                >
                  <span class="capture-text">Type shortcut...</span>
                  <span class="capture-hint">Esc to cancel</span>
                </div>
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
    gap: var(--s4);
  }

  .group {
    display: flex;
    flex-direction: column;
    gap: var(--s1);
  }

  .group-label {
    font-size: 10px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.12em;
    margin-bottom: var(--s1);
  }

  .group-items {
    display: flex;
    flex-direction: column;
  }

  .binding-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--s2) 0;
    border-bottom: 1px solid var(--border-subtle);
    gap: var(--s3);
  }

  .binding-row.non-editable {
    opacity: 0.7;
  }

  .binding-row.capturing {
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    margin: 0 calc(-1 * var(--s2));
    padding: var(--s2);
    border-radius: var(--r-sm);
    border-bottom-color: transparent;
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
  }

  .binding-hint {
    font-size: 10px;
    color: var(--text-muted);
  }

  .binding-keys {
    display: flex;
    align-items: center;
    gap: var(--s2);
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
    padding: 0 var(--s2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--bg-raised);
    color: var(--text-secondary);
    font-size: 10px;
    font-family: "JetBrains Mono", monospace;
    line-height: 1;
    white-space: nowrap;
  }

  .capture-box {
    display: flex;
    align-items: center;
    gap: var(--s2);
    padding: 4px var(--s3);
    border: 1px solid var(--accent);
    border-radius: var(--r-sm);
    background: var(--bg-raised);
    outline: none;
    min-width: 160px;
  }

  .capture-text {
    font-size: 11px;
    color: var(--accent);
    animation: pulse 1.2s ease-in-out infinite;
  }

  .capture-hint {
    font-size: 9px;
    color: var(--text-muted);
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }

  .btn-edit {
    font: inherit;
    padding: 2px var(--s2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text-muted);
    font-size: 10px;
    cursor: pointer;
    transition: background 0.14s, color 0.14s;
  }

  .btn-edit:hover {
    background: var(--bg-raised);
    color: var(--text-primary);
  }

  .btn-reset-single {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    transition: background 0.14s, color 0.14s;
  }

  .btn-reset-single:hover {
    background: var(--bg-raised);
    color: var(--text-primary);
  }

  .badge-system {
    font-size: 9px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.12em;
    padding: 2px var(--s2);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-sm);
  }

  .reset-section {
    padding-top: var(--s2);
  }

  .btn-reset-all {
    font: inherit;
    padding: var(--s2) var(--s3);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background: transparent;
    color: var(--text-muted);
    font-size: 11px;
    cursor: pointer;
    transition: background 0.14s, color 0.14s;
  }

  .btn-reset-all:hover {
    background: var(--bg-raised);
    color: var(--text-primary);
  }
</style>
