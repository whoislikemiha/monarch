<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { invoke } from "$lib/api";
  import Avatar from "$lib/ui/Avatar.svelte";
  import { PROVIDER_LOGO_PRESETS } from "./providerLogos";

  interface AvatarPreset {
    label: string;
    path: string;
  }

  let {
    agentId,
    name = "",
    provider = undefined,
    avatarType = $bindable(undefined),
    avatarPath = $bindable(undefined),
  }: {
    agentId: string;
    /** Agent display name — drives the monogram preview. */
    name?: string;
    /** Agent's provider id — drives the "Auto" (provider logo) preview. */
    provider?: string;
    avatarType?: "image";
    avatarPath?: string;
  } = $props();

  let uploading = $state(false);

  /**
   * Uploaded image stored independently of the current selection so clicking
   * another preset doesn't lose track of the uploaded file. Initialized from
   * the agent's existing custom avatar (if any) when the dialog opens.
   */
  let uploadedPath = $state<string | undefined>(
    avatarType === "image" && avatarPath && !avatarPath.startsWith("/avatars/")
      ? avatarPath
      : undefined
  );
  let uploadedDataUrl = $state<string | undefined>(undefined);

  // Load the data URL for the uploaded image on mount (or when uploadedPath changes).
  $effect(() => {
    const path = uploadedPath;
    if (!path) { uploadedDataUrl = undefined; return; }
    invoke<string>("read_avatar_data_url", { path })
      .then((url) => { uploadedDataUrl = url; })
      .catch(() => { uploadedDataUrl = undefined; });
  });

  const isAutoSelected = $derived(avatarType !== "image" || !avatarPath);

  function isSelected(preset: AvatarPreset): boolean {
    return avatarType === "image" && avatarPath === preset.path;
  }

  const isCustomSelected = $derived(
    avatarType === "image" && !!avatarPath && !avatarPath.startsWith("/avatars/")
  );

  function selectAuto(): void {
    avatarType = undefined;
    avatarPath = undefined;
  }

  function selectPreset(preset: AvatarPreset): void {
    avatarType = "image";
    avatarPath = preset.path;
  }

  function selectCustom(): void {
    if (!uploadedPath) return;
    avatarType = "image";
    avatarPath = uploadedPath;
  }

  async function uploadImage(): Promise<void> {
    const selected = await open({
      multiple: false,
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "gif", "webp", "svg"] }],
      title: "Choose avatar image",
    });
    if (!selected || typeof selected !== "string") return;
    uploading = true;
    try {
      const storedPath = await invoke<string>("save_avatar_image", {
        agentId,
        srcPath: selected,
      });
      const dataUrl = await invoke<string>("read_avatar_data_url", { path: storedPath });
      uploadedPath = storedPath;
      uploadedDataUrl = dataUrl;
      avatarType = "image";
      avatarPath = storedPath;
    } catch (e) {
      console.error("[AvatarPicker] Failed to save avatar image:", e);
    } finally {
      uploading = false;
    }
  }
</script>

<div class="avatar-picker">
  <div class="presets">
    <!-- Auto: follows the agent's provider logo (monogram if provider unknown) -->
    <button
      class="preset-card"
      class:selected={isAutoSelected}
      onclick={selectAuto}
      type="button"
      title="Auto — provider logo"
    >
      <Avatar {name} {provider} size={44} />
      <span class="preset-label">Auto</span>
    </button>

    {#each PROVIDER_LOGO_PRESETS as preset (preset.path)}
      <button
        class="preset-card"
        class:selected={isSelected(preset)}
        onclick={() => selectPreset(preset)}
        type="button"
        title={preset.label}
      >
        <img src={preset.path} alt={preset.label} class="preset-thumb" />
        <span class="preset-label">{preset.label}</span>
      </button>
    {/each}

    {#if uploadedPath && uploadedDataUrl}
      <!-- Custom uploaded image — always visible; clicking re-selects it -->
      <button
        class="preset-card"
        class:selected={isCustomSelected}
        type="button"
        title="Custom upload"
        onclick={selectCustom}
      >
        <img src={uploadedDataUrl} alt="Custom" class="preset-thumb" />
        <span class="preset-label">Custom</span>
      </button>
    {/if}
  </div>

  <button
    class="upload-btn"
    onclick={uploadImage}
    type="button"
    disabled={uploading}
  >
    {uploading ? "Uploading…" : "Upload image…"}
  </button>
</div>

<style>
  .avatar-picker {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .presets {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .preset-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 6px;
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    background: var(--bg-input);
    cursor: pointer;
    transition: border-color 0.15s, background 0.15s;
    width: 72px;
  }

  .preset-card:hover {
    border-color: var(--accent-blue);
    background: var(--bg-hover);
  }

  .preset-card.selected {
    border-color: var(--accent-purple, #7c3aed);
    background: color-mix(in srgb, var(--accent-purple, #7c3aed) 12%, transparent);
  }

  .preset-thumb {
    width: 44px;
    height: 44px;
    border-radius: 50%;
    object-fit: cover;
  }

  .preset-label {
    font-size: 10px;
    color: var(--text-muted);
    text-align: center;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 64px;
  }

  .upload-btn {
    align-self: flex-start;
    padding: 5px 12px;
    font-size: 12px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: var(--bg-input);
    color: var(--text-secondary);
    cursor: pointer;
    transition: border-color 0.15s, color 0.15s;
  }

  .upload-btn:hover:not(:disabled) {
    border-color: var(--accent-blue);
    color: var(--text-primary);
  }

  .upload-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
