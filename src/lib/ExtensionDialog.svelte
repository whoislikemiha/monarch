<script lang="ts">
  import type { ExtensionUIRequest } from "./types";

  let {
    request,
    onrespond,
    oncancel,
  }: {
    request: ExtensionUIRequest;
    onrespond: (value: any) => void;
    oncancel: () => void;
  } = $props();

  let inputValue = $state(request.prefill || "");
  let selectedOption = $state(request.options?.[0] || "");

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") oncancel();
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      submit();
    }
  }

  function submit() {
    switch (request.method) {
      case "select":
        onrespond({ value: selectedOption });
        break;
      case "confirm":
        onrespond({ confirmed: true });
        break;
      case "input":
        onrespond({ value: inputValue });
        break;
      case "editor":
        onrespond({ value: inputValue });
        break;
    }
  }

  function deny() {
    if (request.method === "confirm") {
      onrespond({ confirmed: false });
    } else {
      oncancel();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="overlay" onclick={oncancel} role="presentation">
  <div
    class="dialog"
    onclick={(e: MouseEvent) => e.stopPropagation()}
    role="dialog"
    tabindex="-1"
  >
    <h3>{request.title || "Extension Request"}</h3>

    {#if request.method === "select"}
      <div class="options">
        {#each request.options || [] as option}
          <button
            class="option-btn"
            class:active={selectedOption === option}
            onclick={() => { selectedOption = option; }}
          >
            {option}
          </button>
        {/each}
      </div>
      <div class="actions">
        <button class="btn-cancel" onclick={oncancel}>Cancel</button>
        <button class="btn-confirm" onclick={submit}>Select</button>
      </div>

    {:else if request.method === "confirm"}
      {#if request.message}
        <p class="message">{request.message}</p>
      {/if}
      <div class="actions">
        <button class="btn-cancel" onclick={deny}>No</button>
        <button class="btn-confirm" onclick={submit}>Yes</button>
      </div>

    {:else if request.method === "input"}
      <input
        type="text"
        bind:value={inputValue}
        placeholder={request.placeholder || ""}
        autofocus
      />
      <div class="actions">
        <button class="btn-cancel" onclick={oncancel}>Cancel</button>
        <button class="btn-confirm" onclick={submit}>OK</button>
      </div>

    {:else if request.method === "editor"}
      <textarea
        bind:value={inputValue}
        placeholder={request.placeholder || ""}
        rows="10"
        autofocus
      ></textarea>
      <div class="actions">
        <button class="btn-cancel" onclick={oncancel}>Cancel</button>
        <button class="btn-confirm" onclick={submit}>Save</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
  }

  .dialog {
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: 12px;
    padding: 20px;
    width: 420px;
    max-width: 90vw;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  h3 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .message {
    margin: 0;
    font-size: 13px;
    color: var(--text-secondary);
    line-height: 1.5;
  }

  .options {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 300px;
    overflow-y: auto;
  }

  .option-btn {
    width: 100%;
    padding: 8px 12px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: var(--bg-panel-2);
    color: var(--text-secondary);
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    text-align: left;
    transition: background 0.15s, border-color 0.15s;
  }

  .option-btn:hover {
    background: var(--bg-panel-3);
  }

  .option-btn.active {
    border-color: var(--accent-purple);
    color: var(--accent-purple);
  }

  input, textarea {
    background: var(--bg-panel-2);
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    color: var(--text-primary);
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    padding: 8px 10px;
    outline: none;
  }

  input:focus, textarea:focus {
    border-color: var(--accent-purple);
  }

  input::placeholder, textarea::placeholder {
    color: var(--text-muted);
    opacity: 0.6;
  }

  textarea {
    resize: vertical;
    min-height: 120px;
    line-height: 1.5;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .btn-cancel {
    padding: 6px 14px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn-cancel:hover {
    background: var(--bg-panel-2);
  }

  .btn-confirm {
    padding: 6px 14px;
    border: none;
    border-radius: 6px;
    background: var(--accent-purple);
    color: #140d22;
    font-size: 12px;
    font-weight: 600;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn-confirm:hover {
    background: #d5bbff;
  }
</style>
