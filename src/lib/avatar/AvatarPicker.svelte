<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { invoke } from "$lib/api";

  interface AvatarPreset {
    label: string;
    type: "rive" | "image";
    path: string;
  }

  const BUILT_IN_PRESETS: AvatarPreset[] = [
    { label: "Shadow (animated)", type: "rive", path: "/avatars/shadow_animations.riv" },
    { label: "Shadow (static)", type: "image", path: "/avatars/shadow_silhouette.svg" },
  ];

  let {
    agentId,
    avatarType = $bindable(undefined),
    avatarPath = $bindable(undefined),
  }: {
    agentId: string;
    avatarType?: "rive" | "image";
    avatarPath?: string;
  } = $props();

  let uploading = $state(false);

  function isSelected(preset: AvatarPreset): boolean {
    // Default state: no avatarType means the default rive preset
    if (!avatarType && preset.type === "rive" && preset.path === "/avatars/shadow_animations.riv") {
      return true;
    }
    return avatarType === preset.type && avatarPath === preset.path;
  }

  function selectPreset(preset: AvatarPreset): void {
    avatarType = preset.type;
    avatarPath = preset.path;
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
    {#each BUILT_IN_PRESETS as preset (preset.path)}
      <button
        class="preset-card"
        class:selected={isSelected(preset)}
        onclick={() => selectPreset(preset)}
        type="button"
        title={preset.label}
      >
        {#if preset.type === "image"}
          <img src={preset.path} alt={preset.label} class="preset-thumb" />
        {:else}
          <div class="preset-rive-thumb">
            <svg viewBox="0 0 32 32" width="32" height="32" aria-hidden="true">
              <circle cx="16" cy="16" r="14" fill="#1a1a2e" stroke="#7c3aed" stroke-width="1.5"/>
              <circle cx="16" cy="16" r="8" fill="#7c3aed" opacity="0.4"/>
              <ellipse cx="13" cy="16" rx="2" ry="2.5" fill="#c084fc"/>
              <ellipse cx="19" cy="16" rx="2" ry="2.5" fill="#c084fc"/>
            </svg>
          </div>
        {/if}
        <span class="preset-label">{preset.label}</span>
      </button>
    {/each}

    {#if avatarType === "image" && avatarPath && !avatarPath.startsWith("/avatars/")}
      <!-- Custom uploaded image currently selected -->
      {@const customSrc = avatarPath.startsWith("/") ? convertFileSrc(avatarPath) : avatarPath}
      <button
        class="preset-card selected"
        type="button"
        title="Custom upload"
        onclick={() => {}}
      >
        <img src={customSrc} alt="Custom" class="preset-thumb" />
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

  .preset-rive-thumb {
    width: 44px;
    height: 44px;
    display: flex;
    align-items: center;
    justify-content: center;
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
