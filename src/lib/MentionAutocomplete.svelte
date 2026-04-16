<script lang="ts">
  /*
   * @-mention file/folder autocomplete (MON-76).
   *
   * Sibling component to a <textarea>. Watches the textarea for an active
   * `@…` token at the caret, debounces a `list_paths` call, and renders a
   * ModelSelector-styled dropdown with keyboard navigation.
   *
   * - Arrow Up / Down cycle suggestions
   * - Tab accepts the highlighted item (inserts `@<relative-path>`)
   * - Escape closes the dropdown without inserting
   * - Mouse click also accepts
   *
   * When `cwd` is missing the whole feature short-circuits — nothing is ever
   * shown, no backend calls are made.
   */
  import { invoke } from "$lib/api";
  import type { PathSuggestion } from "./bindings";

  let {
    textareaEl,
    cwd,
    text = $bindable(""),
  }: {
    textareaEl: HTMLTextAreaElement | undefined;
    cwd: string | undefined;
    text?: string;
  } = $props();

  let open = $state(false);
  let tokenStart = $state(-1); // index of `@` in `text`, -1 when inactive
  let query = $state("");
  let suggestions = $state<PathSuggestion[]>([]);
  let highlightedIndex = $state(0);
  let loading = $state(false);
  let dropdownStyle = $state("");

  // Request-token pattern (mirrors ModelSelector) so a stale response from a
  // now-outdated keystroke cannot clobber the current one.
  let fetchToken = 0;
  let debounceTimer: number | undefined;

  // ---- Token detection ----

  /**
   * Find the `@` that opens the current mention token, or -1 if the caret is
   * not inside one. A valid token:
   *   - starts at a `@` with no whitespace (or newline) between it and the caret
   *   - has nothing before the `@`, or a whitespace/newline/( char
   */
  function detectToken(str: string, caret: number): number {
    for (let i = caret - 1; i >= 0; i--) {
      const ch = str[i];
      if (ch === "@") {
        // Opener must sit at the start of the string or after whitespace /
        // newline / opening bracket so we don't light up on user@host-style
        // text already in the textarea.
        if (i === 0) return i;
        const prev = str[i - 1];
        if (prev === " " || prev === "\n" || prev === "\t" || prev === "(") return i;
        return -1;
      }
      // Whitespace or newline closes any candidate token — you cannot have
      // a multi-word mention. Path separators, dots, `..`, letters all OK.
      if (ch === " " || ch === "\n" || ch === "\t") return -1;
    }
    return -1;
  }

  function recomputeToken() {
    if (!textareaEl || !cwd) {
      close();
      return;
    }
    const caret = textareaEl.selectionStart ?? 0;
    const start = detectToken(text, caret);
    if (start < 0) {
      close();
      return;
    }
    tokenStart = start;
    query = text.slice(start + 1, caret);
    open = true;
    scheduleFetch();
  }

  function close() {
    open = false;
    tokenStart = -1;
    query = "";
    suggestions = [];
    highlightedIndex = 0;
    fetchToken++;
    if (debounceTimer !== undefined) {
      clearTimeout(debounceTimer);
      debounceTimer = undefined;
    }
  }

  // ---- Suggestion fetch ----

  function scheduleFetch() {
    if (debounceTimer !== undefined) {
      clearTimeout(debounceTimer);
    }
    // 80ms is roughly one keystroke in a hurry — short enough to feel live,
    // long enough to coalesce a rapid burst of characters into one IPC call.
    debounceTimer = window.setTimeout(fetchNow, 80);
  }

  async function fetchNow() {
    if (!cwd || !open) return;
    const token = ++fetchToken;
    loading = true;
    try {
      const res = await invoke<PathSuggestion[]>("list_paths", { cwd, query });
      if (token !== fetchToken) return; // stale — a newer keystroke is in flight
      suggestions = res;
      highlightedIndex = 0;
    } catch {
      if (token !== fetchToken) return;
      suggestions = [];
    } finally {
      if (token === fetchToken) loading = false;
    }
  }

  // ---- Selection ----

  function accept(i: number) {
    if (!textareaEl || tokenStart < 0) return;
    const item = suggestions[i];
    if (!item) return;
    const before = text.slice(0, tokenStart);
    const caret = textareaEl.selectionStart ?? text.length;
    const after = text.slice(caret);
    // Insert `@<path>` followed by a space — mentions usually precede more
    // text, and the trailing space prevents the next keystroke from
    // re-entering the mention token.
    const inserted = "@" + item.path + " ";
    const newText = before + inserted + after;
    const newCaret = before.length + inserted.length;
    text = newText;
    close();
    // Restore focus + caret on the next tick so the text update has landed.
    queueMicrotask(() => {
      if (!textareaEl) return;
      textareaEl.focus();
      textareaEl.setSelectionRange(newCaret, newCaret);
      // Fire an input event so the host textarea's auto-resize handler
      // re-runs against the new content.
      textareaEl.dispatchEvent(new Event("input", { bubbles: true }));
    });
  }

  // ---- Dropdown position ----

  function updatePosition() {
    if (!textareaEl) return;
    const rect = textareaEl.getBoundingClientRect();
    // Anchor the dropdown below the textarea, aligned to its left edge, and
    // clamp its width to the textarea's width. Using fixed positioning keeps
    // the host markup (ChatInput) untouched.
    dropdownStyle =
      `top: ${rect.bottom + 4}px;` +
      `left: ${rect.left}px;` +
      `width: ${rect.width}px;`;
  }

  // ---- Listener wiring ----
  //
  // We attach listeners in an effect so the component re-subscribes when
  // the parent swaps textareaEl (e.g. mount order). `input` handles typing
  // and pastes; `keydown` owns dropdown navigation; `keyup`/`click` catch
  // caret-only movement (arrow keys in text, mouse click) that should
  // re-evaluate whether we're still inside a token.

  function handleInput() {
    recomputeToken();
  }

  function handleCaretMove() {
    if (open) recomputeToken();
  }

  function handleBlur() {
    // Delay so a mousedown on a suggestion lands before the dropdown closes.
    setTimeout(() => close(), 150);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (!open) return;
    switch (e.key) {
      case "ArrowDown":
        if (suggestions.length === 0) return;
        e.preventDefault();
        e.stopPropagation();
        highlightedIndex = (highlightedIndex + 1) % suggestions.length;
        break;
      case "ArrowUp":
        if (suggestions.length === 0) return;
        e.preventDefault();
        e.stopPropagation();
        highlightedIndex =
          highlightedIndex <= 0 ? suggestions.length - 1 : highlightedIndex - 1;
        break;
      case "Tab":
        if (suggestions.length === 0) return;
        e.preventDefault();
        e.stopPropagation();
        accept(highlightedIndex);
        break;
      case "Escape":
        e.preventDefault();
        e.stopPropagation();
        close();
        break;
      case "Enter":
        // Let the host textarea handle Enter (typically: send message).
        // Closing the dropdown here is optional — the send will reset it —
        // but closing keeps state tidy if the send is disabled.
        close();
        break;
    }
  }

  $effect(() => {
    const el = textareaEl;
    if (!el) return;
    el.addEventListener("input", handleInput);
    el.addEventListener("keydown", handleKeydown, true);
    el.addEventListener("keyup", handleCaretMove);
    el.addEventListener("click", handleCaretMove);
    el.addEventListener("blur", handleBlur);
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);
    return () => {
      el.removeEventListener("input", handleInput);
      el.removeEventListener("keydown", handleKeydown, true);
      el.removeEventListener("keyup", handleCaretMove);
      el.removeEventListener("click", handleCaretMove);
      el.removeEventListener("blur", handleBlur);
      window.removeEventListener("resize", updatePosition);
      window.removeEventListener("scroll", updatePosition, true);
    };
  });

  // Re-measure the anchor whenever the dropdown opens or the suggestions
  // shift size. Cheap enough to run unconditionally while open.
  $effect(() => {
    if (open) {
      // Touch suggestions.length so the effect re-runs when the list grows.
      void suggestions.length;
      updatePosition();
    }
  });
</script>

{#if open && (loading || suggestions.length > 0)}
  <div class="mention-dropdown" style={dropdownStyle}>
    {#if suggestions.length === 0 && loading}
      <div class="mention-empty">searching…</div>
    {:else}
      {#each suggestions as s, i (s.path + (s.isDir ? "/" : ""))}
        <button
          type="button"
          class="mention-option"
          class:highlighted={i === highlightedIndex}
          onmousedown={(e: MouseEvent) => {
            // Prevent textarea blur; accept before the browser focuses the button.
            e.preventDefault();
            accept(i);
          }}
          onmouseenter={() => (highlightedIndex = i)}
        >
          <span class="mention-icon" class:dir={s.isDir}>
            {s.isDir ? "▸" : "·"}
          </span>
          <span class="mention-path">{s.path}</span>
          {#if s.isDir}
            <span class="mention-tag">dir</span>
          {/if}
        </button>
      {/each}
    {/if}
  </div>
{/if}

<style>
  .mention-dropdown {
    position: fixed;
    z-index: 300;
    max-height: 240px;
    overflow-y: auto;
    background: var(--bg-panel-2);
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.25);
    display: flex;
    flex-direction: column;
    padding: 4px;
  }

  .mention-empty {
    padding: 8px 10px;
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    color: var(--text-muted);
  }

  .mention-option {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    text-align: left;
    transition: background 0.1s;
  }

  .mention-option:hover,
  .mention-option.highlighted {
    background: var(--bg-panel-3);
    color: var(--text-primary);
  }

  .mention-icon {
    width: 12px;
    color: var(--text-muted);
    flex-shrink: 0;
    text-align: center;
  }

  .mention-icon.dir {
    color: var(--accent);
  }

  .mention-path {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mention-tag {
    font-size: 10px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    flex-shrink: 0;
  }
</style>
