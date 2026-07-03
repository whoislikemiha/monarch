<script lang="ts">
  /**
   * MON-82: read-only complexity tag under a user turn. The always-on
   * classifier grades every prompt (chitchat / simple / decomposable /
   * delegate); this makes its verdict visible in the dialogue. Advisory
   * only — failures render as a muted "failed" tag, never block the turn.
   */
  import type { ClassificationInfo } from "$lib/classifierStore.svelte";

  interface Props {
    info: ClassificationInfo;
  }
  let { info }: Props = $props();

  const TONE: Record<string, string> = {
    chitchat: "muted",
    simple: "info",
    decomposable: "accent",
    delegate: "warning",
  };

  let label = $derived(info.error ? "failed" : (info.complexity ?? "…"));
  let tone = $derived(info.error ? "muted" : (TONE[info.complexity ?? ""] ?? "muted"));
  let title = $derived.by(() => {
    if (info.error) return `Classifier failed: ${info.error}`;
    const parts: string[] = [];
    if (info.confidence != null) parts.push(`confidence ${(info.confidence * 100).toFixed(0)}%`);
    if (info.rationale) parts.push(info.rationale);
    if (info.model) parts.push(info.model);
    if (info.latencyMs != null) parts.push(`${info.latencyMs}ms`);
    return parts.join(" · ");
  });
</script>

<span class="pill {tone}" {title}>
  <span class="dot" aria-hidden="true"></span>{label}
</span>

<style>
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    padding: 1px 6px;
    border-radius: var(--r-sm);
    border: 1px solid var(--border-subtle);
    color: var(--text-muted);
    background: transparent;
    cursor: default;
  }
  .dot { width: 4px; height: 4px; border-radius: var(--r-full); background: currentColor; }

  .pill.info { color: var(--status-info); border-color: color-mix(in srgb, var(--status-info) 30%, transparent); }
  .pill.accent { color: var(--accent-2); border-color: color-mix(in srgb, var(--accent-2) 30%, transparent); }
  .pill.warning { color: var(--status-warning); border-color: color-mix(in srgb, var(--status-warning) 35%, transparent); }
</style>
