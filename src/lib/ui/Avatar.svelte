<script lang="ts">
  /**
   * Static shadow avatar — monogram or uploaded image, with an optional grade
   * ring and presence pip. Deliberately NOT animated (no Rive): the avatar's
   * job is visual tracking at a glance, not motion. Wraps the `.avatar` atom.
   */
  import { invoke } from "$lib/api";
  import { gradeColor, type GradeLetter } from "./grade";

  interface Props {
    name: string;
    size?: number;
    grade?: GradeLetter;
    /** Presence pip color (CSS var or color). Omit to hide the pip. */
    presence?: string | null;
    avatarType?: "rive" | "image";
    avatarPath?: string;
  }
  let { name, size = 32, grade, presence = null, avatarType, avatarPath }: Props = $props();

  let monogram = $derived((name?.trim()?.[0] ?? "?").toUpperCase());
  let isImage = $derived(avatarType === "image" && !!avatarPath);

  // Resolve the image src. Bundled (/avatars/…) and data: URLs render directly;
  // absolute upload paths go through Rust (the asset protocol isn't scoped).
  let imgSrc = $state("");
  $effect(() => {
    if (!isImage || !avatarPath) {
      imgSrc = "";
      return;
    }
    if (avatarPath.startsWith("/avatars/") || avatarPath.startsWith("data:")) {
      imgSrc = avatarPath;
      return;
    }
    let cancelled = false;
    invoke<string>("read_avatar_data_url", { path: avatarPath })
      .then((url) => { if (!cancelled) imgSrc = url; })
      .catch(() => { if (!cancelled) imgSrc = ""; });
    return () => { cancelled = true; };
  });

  let ringStyle = $derived(grade ? `--gc:${gradeColor(grade)}` : "");
</script>

<span
  class="avatar"
  class:ring={!!grade}
  style="width:{size}px;height:{size}px;font-size:{Math.round(size * 0.4)}px;{ringStyle}"
  title={name}
>
  {#if isImage && imgSrc}
    <img src={imgSrc} alt={name} />
  {:else}
    {monogram}
  {/if}
  {#if presence}
    <span class="pip" style="background:{presence}"></span>
  {/if}
</span>

<style>
  .avatar { overflow: hidden; }
  .avatar img { width: 100%; height: 100%; object-fit: cover; border-radius: inherit; }
</style>
