<script lang="ts">
  /**
   * Slim header above the solo workspace. Identity + live status at a glance.
   * The relocated portrait controls (model / thinking / prompt / history /
   * new session) land here in later slices via a controls menu.
   */
  import type { Agent } from "$lib/types";
  import { liveAgentStore } from "$lib/toolbox/liveAgentStore.svelte";
  import { formatCost } from "$lib/format";
  import Avatar from "$lib/ui/Avatar.svelte";
  import { gradeLetter } from "$lib/ui/grade";

  interface Props {
    agent: Agent;
  }
  let { agent }: Props = $props();

  let live = $derived(liveAgentStore.byAgent.get(agent.id));
  let streaming = $derived(!!live?.isStreaming);
  let grade = $derived(gradeLetter(agent.shadow?.shadowGrade));
  let spend = $derived(formatCost(agent.lifetimeCost));
  let presence = $derived(
    streaming ? "var(--status-info)" : agent.status === "error" ? "var(--status-error)"
      : agent.status === "stopped" ? "var(--text-muted)" : "var(--status-success)",
  );
  let status = $derived(
    streaming ? (live?.activityStatus || "Working…")
      : agent.status === "stopped" ? "Paused"
        : (agent.shadow?.shadowTitle || agent.model || "Idle"),
  );
</script>

<header class="head">
  <Avatar name={agent.name} size={28} {grade} {presence} avatarType={agent.avatarType} avatarPath={agent.avatarPath} />
  <span class="nm">{agent.name}</span>
  <span class="gchip mono" style="--gc:var(--grade-{grade.toLowerCase()})">{grade}</span>
  <span class="dot" aria-hidden="true">·</span>
  <span class="status" class:live={streaming}>{status}</span>
  <div class="grow"></div>
  {#if spend}<span class="spend mono">{spend}</span>{/if}
</header>

<style>
  .head {
    display: flex;
    align-items: center;
    gap: var(--s2);
    height: 44px;
    flex: none;
    padding: 0 var(--s4);
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-base);
  }
  .nm { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .gchip {
    display: inline-flex; align-items: center; justify-content: center;
    width: 16px; height: 16px; border-radius: var(--r-sm);
    font-size: 9.5px; font-weight: 700; color: var(--gc);
    border: 1px solid color-mix(in srgb, var(--gc) 40%, transparent);
    background: color-mix(in srgb, var(--gc) 10%, transparent);
  }
  .dot { color: var(--text-muted); }
  .status { font-size: 12px; color: var(--text-muted); }
  .status.live { color: var(--status-info); }
  .grow { flex: 1; }
  .spend { font-size: 11px; color: var(--text-muted); }
</style>
