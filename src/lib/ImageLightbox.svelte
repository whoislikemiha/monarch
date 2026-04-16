<script lang="ts">
  import { onDestroy, onMount } from "svelte";

  let {
    src,
    onclose,
  }: {
    src: string;
    onclose: () => void;
  } = $props();

  let closeBtn: HTMLButtonElement;
  let backdropBtn: HTMLButtonElement;
  let previouslyFocused: HTMLElement | null = null;

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      onclose();
      return;
    }
    if (e.key !== "Tab") return;
    // Keep focus inside the dialog — we only have two tab stops so the
    // cycle is trivial: shift+Tab from backdrop goes to close, Tab from
    // close goes to backdrop, and vice versa.
    const active = document.activeElement;
    if (e.shiftKey) {
      if (active === backdropBtn) {
        e.preventDefault();
        closeBtn.focus();
      }
    } else {
      if (active === closeBtn) {
        e.preventDefault();
        backdropBtn.focus();
      }
    }
  }

  onMount(() => {
    previouslyFocused = document.activeElement as HTMLElement | null;
    closeBtn?.focus();
  });

  onDestroy(() => {
    previouslyFocused?.focus?.();
  });
</script>

<svelte:window onkeydown={handleKeydown} />

<div
  class="lightbox"
  role="dialog"
  aria-label="Image preview"
  aria-modal="true"
>
  <button
    type="button"
    class="backdrop"
    bind:this={backdropBtn}
    onclick={onclose}
    aria-label="Close preview"
  ></button>
  <button
    type="button"
    class="close-btn"
    bind:this={closeBtn}
    onclick={onclose}
    aria-label="Close"
  >×</button>
  <img {src} alt="preview" />
</div>

<style>
  .lightbox {
    position: fixed;
    inset: 0;
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 40px;
  }

  .backdrop {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.85);
    border: none;
    padding: 0;
    margin: 0;
    cursor: zoom-out;
  }

  .lightbox img {
    position: relative;
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    border-radius: 4px;
    pointer-events: none;
  }

  .close-btn {
    position: absolute;
    top: 16px;
    right: 20px;
    width: 36px;
    height: 36px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.1);
    color: #fff;
    border: none;
    font-size: 22px;
    line-height: 1;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    transition: background 0.15s;
  }

  .close-btn:hover,
  .close-btn:focus-visible {
    background: rgba(255, 255, 255, 0.2);
    outline: none;
  }

  .backdrop:focus-visible {
    outline: 2px solid rgba(255, 255, 255, 0.4);
    outline-offset: -4px;
  }
</style>
