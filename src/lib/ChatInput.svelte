<script lang="ts">
  import { readImage } from "@tauri-apps/plugin-clipboard-manager";

  export interface PendingImage {
    id: string;
    dataUrl: string;
    data: string;
    mimeType: string;
  }

  const MAX_IMAGES = 5;
  const MAX_BYTES = 5 * 1024 * 1024;

  let {
    onsend,
    onthumbclick,
    disabled = false,
    placeholder: customPlaceholder,
  }: {
    onsend: (message: string, images: PendingImage[]) => void;
    onthumbclick?: (src: string) => void;
    disabled?: boolean;
    placeholder?: string;
  } = $props();

  let text = $state("");
  let images = $state<PendingImage[]>([]);
  let textareaEl: HTMLTextAreaElement;
  let fileInputEl: HTMLInputElement;

  export function focus() {
    textareaEl?.focus();
  }

  export function addImageFile(file: File) {
    void addFile(file);
  }

  async function addFile(file: File) {
    if (images.length >= MAX_IMAGES) return;
    if (!file.type.startsWith("image/")) return;
    if (file.size > MAX_BYTES) return;

    const dataUrl = await readAsDataUrl(file);
    const commaIdx = dataUrl.indexOf(",");
    const data = dataUrl.slice(commaIdx + 1);
    images = [
      ...images,
      { id: crypto.randomUUID(), dataUrl, data, mimeType: file.type },
    ];
  }

  function readAsDataUrl(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(reader.result as string);
      reader.onerror = reject;
      reader.readAsDataURL(file);
    });
  }

  function removeImage(id: string) {
    images = images.filter((img) => img.id !== id);
  }

  /**
   * WebKitGTK on Linux does not expose images through
   * ClipboardEvent.clipboardData.items. Try that path first for portability,
   * then fall back to the Tauri clipboard plugin which reads the image
   * through the native clipboard service. The fallback returns RGBA bytes,
   * so we draw them onto a canvas to produce a PNG data URL.
   */
  async function handlePaste(e: ClipboardEvent) {
    const items = e.clipboardData?.items;
    if (items) {
      for (const item of items) {
        if (item.type.startsWith("image/")) {
          e.preventDefault();
          const file = item.getAsFile();
          if (file) await addFile(file);
          return;
        }
      }
    }

    // Linux fallback — ask Tauri for the clipboard image directly.
    try {
      const img = await readImage();
      const { width, height } = await img.size();
      const rgba = await img.rgba();
      const dataUrl = rgbaToDataUrl(
        rgba instanceof Uint8Array ? rgba : new Uint8Array(rgba),
        width,
        height,
      );
      if (!dataUrl) return;
      e.preventDefault();
      if (images.length >= MAX_IMAGES) return;
      const commaIdx = dataUrl.indexOf(",");
      const data = dataUrl.slice(commaIdx + 1);
      // Rough size guard — PNGs compress well but a giant RGBA buffer could
      // still produce a too-large image. Check the encoded string length.
      if (data.length * 0.75 > MAX_BYTES) return;
      images = [
        ...images,
        { id: crypto.randomUUID(), dataUrl, data, mimeType: "image/png" },
      ];
    } catch {
      // No image on the clipboard (just text) — let the default paste happen.
    }
  }

  function rgbaToDataUrl(rgba: Uint8Array, width: number, height: number): string | null {
    if (!width || !height) return null;
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext("2d");
    if (!ctx) return null;
    const imageData = ctx.createImageData(width, height);
    imageData.data.set(rgba);
    ctx.putImageData(imageData, 0, 0);
    return canvas.toDataURL("image/png");
  }

  function handleFileInput(e: Event) {
    const input = e.target as HTMLInputElement;
    if (!input.files) return;
    for (const file of input.files) {
      void addFile(file);
    }
    input.value = "";
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      send();
    }
  }

  function send() {
    const trimmed = text.trim();
    if ((!trimmed && images.length === 0) || disabled) return;
    onsend(trimmed, images);
    text = "";
    images = [];
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
  {#if images.length > 0}
    <div class="image-strip">
      {#each images as img (img.id)}
        <div class="thumb">
          <button
            type="button"
            class="thumb-btn"
            onclick={() => onthumbclick?.(img.dataUrl)}
            aria-label="View image"
          >
            <img src={img.dataUrl} alt="attachment" />
          </button>
          <button
            class="remove-btn"
            onclick={() => removeImage(img.id)}
            aria-label="Remove image"
          >×</button>
        </div>
      {/each}
    </div>
  {/if}

  <div class="input-row">
    <button
      class="attach-btn"
      onclick={() => fileInputEl.click()}
      disabled={disabled || images.length >= MAX_IMAGES}
      title="Attach image"
      aria-label="Attach image"
    >
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48"/>
      </svg>
    </button>

    <textarea
      bind:this={textareaEl}
      bind:value={text}
      onkeydown={handleKeydown}
      oninput={autoResize}
      onpaste={handlePaste}
      placeholder={customPlaceholder || (disabled ? "Agent is working..." : "Message...")}
      {disabled}
      rows="1"
    ></textarea>

    <button
      class="send-btn"
      onclick={send}
      disabled={disabled || (!text.trim() && images.length === 0)}
    >
      Send
    </button>
  </div>

  <input
    bind:this={fileInputEl}
    type="file"
    accept="image/*"
    multiple
    class="hidden-input"
    onchange={handleFileInput}
  />
</div>

<style>
  .chat-input {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .chat-input.disabled {
    opacity: 0.6;
  }

  .image-strip {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 4px 0;
  }

  .thumb {
    position: relative;
    width: 52px;
    height: 52px;
    border-radius: 6px;
    overflow: hidden;
    border: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }

  .thumb-btn {
    display: block;
    width: 100%;
    height: 100%;
    padding: 0;
    border: none;
    background: transparent;
    cursor: zoom-in;
  }

  .thumb-btn img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .remove-btn {
    position: absolute;
    top: 2px;
    right: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--bg-panel-3);
    color: var(--text-primary);
    border: none;
    font-size: 11px;
    line-height: 1;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    opacity: 0;
    transition: opacity 0.1s;
  }

  .thumb:hover .remove-btn {
    opacity: 1;
  }

  .input-row {
    display: flex;
    gap: 8px;
    align-items: flex-end;
  }

  .attach-btn {
    background: transparent;
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    color: var(--text-muted);
    cursor: pointer;
    padding: 9px 10px;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    transition: color 0.15s, border-color 0.15s;
    min-height: 40px;
  }

  .attach-btn:hover:not(:disabled) {
    color: var(--text-primary);
    border-color: var(--accent);
  }

  .attach-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
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
    box-shadow: inset 0 1px 0 var(--input-inset-shadow);
  }

  textarea::placeholder {
    color: var(--text-muted);
  }

  textarea:focus {
    border-color: var(--accent);
  }

  .send-btn {
    background: var(--accent);
    color: var(--text-on-accent);
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
    background: var(--accent-hover);
  }

  .send-btn:disabled {
    background: var(--bg-panel-3);
    color: var(--text-muted);
    cursor: not-allowed;
  }

  .hidden-input {
    display: none;
  }
</style>
