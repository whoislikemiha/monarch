<script lang="ts">
  /**
   * Architect panel — placeholder. The Architect (per-project planning agent:
   * triage + reasoning trail + proposed objectives) has no backend yet, so this
   * is a deliberate "coming online" empty state, registered now so the rail and
   * layout reserve its place.
   */
  import type { ToolProps } from "$lib/toolbox/types";
  let { agentContext }: ToolProps = $props();
</script>

<div class="architect">
  <div class="empty">
    <div class="glyph" aria-hidden="true"></div>
    <h4>The Architect is offline</h4>
    <p>Planning triage, reasoning trail, and proposed objectives will appear here once the Architect ships.</p>
    {#if agentContext}<span class="mono note">bound · {agentContext.agent.name}</span>{/if}
  </div>
</div>

<style>
  .architect { padding: var(--s4); }
  .empty { display: flex; flex-direction: column; align-items: center; text-align: center; gap: var(--s3); padding: var(--s6) var(--s4); }
  .empty .glyph { width: 36px; height: 36px; border: 1.5px solid var(--border-strong); border-radius: var(--r-sm); transform: rotate(45deg); }
  .empty h4 { font-size: 13px; color: var(--text-primary); }
  .empty p { font-size: 11.5px; color: var(--text-muted); max-width: 32ch; line-height: 1.6; }
  .note { font-size: 10px; color: var(--text-muted); }
</style>
