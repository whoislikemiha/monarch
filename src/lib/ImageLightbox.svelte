<script lang="ts">
  let {
    src,
    onclose,
  }: {
    src: string;
    onclose: () => void;
  } = $props();

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }
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
    onclick={onclose}
    aria-label="Close preview"
  ></button>
  <button class="close-btn" onclick={onclose} aria-label="Close">×</button>
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

  .close-btn:hover {
    background: rgba(255, 255, 255, 0.2);
  }
</style>
