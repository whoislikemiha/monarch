<script lang="ts">
  /**
   * Chat composer — textarea + send/stop. Enter sends, Shift+Enter newlines.
   * Auto-grows up to a cap. Attachments / @-mentions land in a later slice.
   */
  interface Props {
    streaming: boolean;
    placeholder?: string;
    onsend: (text: string) => void;
    onstop?: () => void;
  }
  let { streaming, placeholder = "Message this agent…", onsend, onstop }: Props = $props();

  let value = $state("");
  let textarea: HTMLTextAreaElement | undefined = $state();

  export function focus() {
    textarea?.focus();
  }

  function grow() {
    if (!textarea) return;
    textarea.style.height = "auto";
    textarea.style.height = Math.min(textarea.scrollHeight, 180) + "px";
  }

  function submit() {
    const text = value.trim();
    if (!text) return;
    onsend(text);
    value = "";
    queueMicrotask(grow);
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      submit();
    }
  }
</script>

<div class="composer">
  <textarea
    bind:this={textarea}
    bind:value
    {placeholder}
    rows="1"
    oninput={grow}
    onkeydown={onKeydown}
  ></textarea>
  {#if streaming}
    <button class="send stop" title="Stop" aria-label="Stop" onclick={() => onstop?.()}>
      <svg viewBox="0 0 24 24" width="13" height="13" fill="currentColor"><rect x="6" y="6" width="12" height="12" rx="2" /></svg>
    </button>
  {:else}
    <button class="send" title="Send" aria-label="Send" disabled={!value.trim()} onclick={submit}>
      <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2"><path d="M4 12h15M13 6l6 6-6 6" /></svg>
    </button>
  {/if}
</div>

<style>
  .composer {
    display: flex;
    align-items: flex-end;
    gap: var(--s2);
    padding: var(--s3) var(--s4);
    border-top: 1px solid var(--border-subtle);
    background: var(--bg-base);
  }
  textarea {
    flex: 1;
    resize: none;
    font: inherit;
    font-size: 13px;
    line-height: 1.5;
    color: var(--text-primary);
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: var(--s2) var(--s3);
    max-height: 180px;
    min-height: 36px;
  }
  textarea::placeholder { color: var(--text-muted); }
  textarea:focus { outline: 2px solid var(--focus); outline-offset: 1px; border-color: var(--accent); }

  .send {
    flex: none;
    width: 36px;
    height: 36px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--r-md);
    border: 1px solid var(--accent);
    background: var(--accent);
    color: var(--accent-ink);
    cursor: pointer;
    transition: background 0.14s, opacity 0.14s;
  }
  .send:hover:not(:disabled) { background: var(--accent-hover); }
  .send:disabled { opacity: 0.4; cursor: default; }
  .send.stop {
    background: transparent;
    color: var(--status-error);
    border-color: color-mix(in srgb, var(--status-error) 45%, transparent);
  }
  .send.stop:hover { background: color-mix(in srgb, var(--status-error) 14%, transparent); }
</style>
