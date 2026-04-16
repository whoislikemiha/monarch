<script lang="ts">
  import { invoke } from "./api";
  import type { MessageAttachment } from "./types";

  let {
    attachment,
    onclick,
  }: {
    attachment: MessageAttachment;
    onclick?: (src: string) => void;
  } = $props();

  // MON-75: cache data URLs by absolute path for the browsing-context
  // lifetime. Attachment files are immutable once written (UUID per blob),
  // so we never need to bust the cache — and avoiding redundant file reads
  // keeps MessageList snappy across scrolls and session switches.
  const cache = dataUrlCache();
  let loadedUrl = $state<string | null>(null);
  const cached = $derived(cache.get(attachment.path) ?? null);
  const dataUrl = $derived(loadedUrl ?? cached);

  $effect(() => {
    if (cache.has(attachment.path)) {
      loadedUrl = cache.get(attachment.path)!;
      return;
    }
    let cancelled = false;
    const path = attachment.path;
    invoke<string>("read_attachment_data_url", { path })
      .then((url) => {
        if (cancelled) return;
        cache.set(path, url);
        loadedUrl = url;
      })
      .catch((e) => console.error("Failed to read attachment:", e));
    return () => {
      cancelled = true;
    };
  });

  function dataUrlCache() {
    const g = globalThis as unknown as {
      __monarchAttachmentCache__?: Map<string, string>;
    };
    if (!g.__monarchAttachmentCache__) {
      g.__monarchAttachmentCache__ = new Map();
    }
    return g.__monarchAttachmentCache__;
  }
</script>

{#if dataUrl}
  <button
    type="button"
    class="sent-thumb"
    onclick={() => dataUrl && onclick?.(dataUrl)}
    aria-label="View image"
  >
    <img src={dataUrl} alt="attachment" />
  </button>
{:else}
  <div class="sent-thumb placeholder" aria-hidden="true"></div>
{/if}

<style>
  .sent-thumb {
    width: 72px;
    height: 72px;
    padding: 0;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    overflow: hidden;
    background: var(--bg-panel-2);
    cursor: zoom-in;
    flex-shrink: 0;
    transition: border-color 0.15s;
  }

  .sent-thumb:hover {
    border-color: var(--accent-blue);
  }

  .sent-thumb.placeholder {
    cursor: default;
    opacity: 0.5;
  }

  .sent-thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
</style>
