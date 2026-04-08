<script lang="ts">
  let {
    onsend,
    disabled = false,
    placeholder: customPlaceholder,
  }: {
    onsend: (message: string) => void;
    disabled?: boolean;
    placeholder?: string;
  } = $props();

  let text = $state("");
  let textareaEl: HTMLTextAreaElement;

  export function focus() {
    textareaEl?.focus();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      send();
    }
  }

  function send() {
    const trimmed = text.trim();
    if (!trimmed || disabled) return;
    onsend(trimmed);
    text = "";
    if (textareaEl) {
      textareaEl.style.height = "auto";
    }
  }

  function autoResize(e: Event) {
    const el = e.target as HTMLTextAreaElement;
    el.style.height = "auto";
    el.style.height = Math.min(el.scrollHeight, 200) + "px";
  }
</script>

<div class="chat-input" class:disabled>
  <textarea
    bind:this={textareaEl}
    bind:value={text}
    onkeydown={handleKeydown}
    oninput={autoResize}
    placeholder={customPlaceholder || (disabled ? "Agent is working..." : "Message...")}
    {disabled}
    rows="1"
  ></textarea>
  <button class="send-btn" onclick={send} disabled={disabled || !text.trim()}>
    Send
  </button>
</div>

<style>
  .chat-input {
    display: flex;
    gap: 8px;
    align-items: flex-end;
  }

  .chat-input.disabled {
    opacity: 0.6;
  }

  textarea {
    flex: 1;
    background: var(--bg-panel-2);
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    color: var(--text-primary);
    font-size: 13px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    padding: 10px 14px;
    resize: none;
    outline: none;
    line-height: 1.5;
    min-height: 40px;
    max-height: 200px;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.03);
  }

  textarea::placeholder {
    color: var(--text-muted);
  }

  textarea:focus {
    border-color: var(--accent-purple);
  }

  .send-btn {
    background: var(--accent-purple);
    color: #140d22;
    border: none;
    border-radius: 8px;
    padding: 10px 16px;
    font-size: 12px;
    font-weight: 600;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s;
    white-space: nowrap;
  }

  .send-btn:hover:not(:disabled) {
    background: #d5bbff;
  }

  .send-btn:disabled {
    background: var(--bg-panel-3);
    color: var(--text-muted);
    cursor: not-allowed;
  }
</style>
