<script lang="ts">
  /**
   * Minimal confirmation dialog. Matches the existing overlay + role="dialog"
   * pattern used by SettingsDialog / SpawnDialog. Consumers pass copy and a
   * confirm callback; Escape or overlay click cancels.
   *
   * Set `danger` to style the confirm button red for irreversible actions
   * like permanent delete.
   */
  let {
    open,
    title,
    message,
    confirmLabel = "Confirm",
    cancelLabel = "Cancel",
    danger = false,
    onconfirm,
    oncancel,
  }: {
    open: boolean;
    title: string;
    message: string;
    confirmLabel?: string;
    cancelLabel?: string;
    danger?: boolean;
    onconfirm: () => void;
    oncancel: () => void;
  } = $props();

  function onKeydown(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === "Escape") {
      e.preventDefault();
      oncancel();
    } else if (e.key === "Enter") {
      e.preventDefault();
      onconfirm();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="overlay" onclick={oncancel} role="presentation">
    <div
      class="dialog"
      onclick={(e: MouseEvent) => e.stopPropagation()}
      role="dialog"
      aria-labelledby="confirm-title"
      tabindex="-1"
    >
      <h2 id="confirm-title">{title}</h2>
      <p>{message}</p>
      <div class="actions">
        <button class="btn-cancel" onclick={oncancel}>{cancelLabel}</button>
        <button
          class="btn-confirm"
          class:danger
          onclick={onconfirm}
        >{confirmLabel}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .dialog {
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    padding: 20px 22px;
    width: min(420px, 90vw);
    box-shadow: 0 24px 64px var(--shadow-dark);
    font-family: 'JetBrainsMono Nerd Font', 'JetBrains Mono', monospace;
  }

  h2 {
    margin: 0 0 8px;
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    letter-spacing: 0.3px;
  }

  p {
    margin: 0 0 16px;
    font-size: 12px;
    color: var(--text-secondary);
    line-height: 1.5;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .btn-cancel, .btn-confirm {
    font-family: inherit;
    font-size: 12px;
    padding: 6px 14px;
    border-radius: 6px;
    border: 1px solid var(--border-strong);
    cursor: pointer;
    transition: background 0.15s, color 0.15s, border-color 0.15s;
  }

  .btn-cancel {
    background: transparent;
    color: var(--text-secondary);
  }
  .btn-cancel:hover {
    background: var(--bg-panel-2);
    color: var(--text-primary);
  }

  .btn-confirm {
    background: var(--bg-panel-2);
    color: var(--accent);
  }
  .btn-confirm:hover {
    background: var(--bg-panel-3);
    color: var(--accent-light);
  }

  .btn-confirm.danger {
    color: var(--error, #eb5757);
    border-color: var(--error, #eb5757);
  }
  .btn-confirm.danger:hover {
    background: var(--error, #eb5757);
    color: var(--bg-panel);
  }
</style>
