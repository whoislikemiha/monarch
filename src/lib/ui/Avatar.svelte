<script lang="ts">
  /**
   * Static agent avatar — monogram or uploaded image, with an optional grade
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
    /** When true, the avatar shows an animated "working" ring; idle = static. */
    working?: boolean;
    avatarType?: "image";
    avatarPath?: string;
  }
  let { name, size = 32, grade, presence = null, working = false, avatarType, avatarPath }: Props = $props();

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
  class:working
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
  .avatar { overflow: visible; }
  .avatar img { width: 100%; height: 100%; object-fit: cover; border-radius: inherit; }

  /* Working indicator — a soft accent ring that pulses outward. Idle = nothing. */
  .avatar.working::after {
    content: "";
    position: absolute;
    inset: -3px;
    border-radius: var(--r-full);
    border: 1.5px solid var(--accent);
    opacity: 0;
    animation: avatar-pulse 1.6s ease-out infinite;
    pointer-events: none;
  }
  @keyframes avatar-pulse {
    0% { opacity: 0.7; transform: scale(0.82); }
    100% { opacity: 0; transform: scale(1.18); }
  }
  @media (prefers-reduced-motion: reduce) {
    .avatar.working::after { animation: none; opacity: 0.6; transform: scale(1.05); }
  }
</style>
