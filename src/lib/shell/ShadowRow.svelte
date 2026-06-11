<script lang="ts">
  /**
   * One shadow in the fleet rail: avatar (grade ring + presence pip) · name ·
   * rank chip · one-line live status · spend · stop control when working.
   */
  import type { Agent } from "$lib/types";
  import { agentStore } from "$lib/stores/agentStore.svelte";
  import { liveAgentStore, abortAgent } from "$lib/toolbox/liveAgentStore.svelte";
  import { formatCost } from "$lib/format";
  import Avatar from "$lib/ui/Avatar.svelte";
  import { gradeLetter } from "$lib/ui/grade";

  interface Props {
    agent: Agent;
    oncontextmenu?: (e: MouseEvent, agent: Agent) => void;
  }
  let { agent, oncontextmenu }: Props = $props();

  let live = $derived(liveAgentStore.byAgent.get(agent.id));
  let streaming = $derived(!!live?.isStreaming);
  let archived = $derived(!!agent.archivedAt);
  let active = $derived(agent.id === agentStore.activeTabId);

  let grade = $derived(gradeLetter(agent.shadow?.shadowGrade));
  let spend = $derived(formatCost(agent.lifetimeCost));

  let presence = $derived(
    streaming
      ? "var(--status-info)"
      : archived
        ? "var(--text-muted)"
        : agent.status === "error"
          ? "var(--status-error)"
          : agent.status === "stopped"
            ? "var(--text-muted)"
            : "var(--status-success)",
  );

  let statusLine = $derived.by(() => {
    const liveStatus = live?.activityStatus || "";
    const subtitle = agent.shadow?.shadowTitle || agent.model || "";
    if (streaming) return liveStatus || "Working…";
    if (archived) return "Dismissed";
    if (agent.status === "stopped") return "Paused";
    if (agent.status === "error") return "Error";
    return liveStatus || subtitle || "Idle";
  });
</script>

<div
  class="row"
  class:active
  class:archived
  role="button"
  tabindex="0"
  title={agent.name}
  onclick={() => agentStore.selectAgent(agent.id)}
  onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); agentStore.selectAgent(agent.id); } }}
  oncontextmenu={(e) => oncontextmenu?.(e, agent)}
>
  <Avatar
    name={agent.name}
    size={32}
    {grade}
    {presence}
    avatarType={agent.avatarType}
    avatarPath={agent.avatarPath}
  />
  <div class="id">
    <div class="top">
      <span class="nm">{agent.name}</span>
      <span class="gchip mono" style="--gc:var(--grade-{grade.toLowerCase()})">{grade}</span>
    </div>
    <div class="status" class:live={streaming}>{statusLine}</div>
  </div>
  <div class="trail">
    {#if spend}<span class="spend mono">{spend}</span>{/if}
    {#if streaming}
      <button
        class="stop"
        title="Stop {agent.name}"
        aria-label="Stop {agent.name}"
        onclick={(e) => { e.stopPropagation(); void abortAgent(agent.id); }}
      >
        <svg viewBox="0 0 24 24" width="9" height="9" fill="currentColor"><rect x="6" y="6" width="12" height="12" rx="1.5" /></svg>
      </button>
    {/if}
  </div>
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: var(--s3);
    padding: var(--s2) var(--s2);
    border: 1px solid transparent;
    border-radius: var(--r-md);
    cursor: pointer;
    transition: background 0.12s, border-color 0.12s;
    min-width: 0;
  }
  .row:hover { background: var(--bg-base); }
  .row:focus-visible { outline: 2px solid var(--focus); outline-offset: -2px; }
  .row.active { background: var(--bg-overlay); border-color: var(--accent-border-subtle); }
  .row.archived { opacity: 0.5; }
  .row.archived:hover { opacity: 0.8; }

  .id { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .top { display: flex; align-items: center; gap: var(--s2); min-width: 0; }
  .nm {
    font-size: 12.5px; font-weight: 600; color: var(--text-primary);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0;
  }
  .gchip {
    flex: none;
    display: inline-flex; align-items: center; justify-content: center;
    width: 15px; height: 15px; border-radius: var(--r-sm);
    font-size: 9px; font-weight: 700; color: var(--gc);
    border: 1px solid color-mix(in srgb, var(--gc) 40%, transparent);
    background: color-mix(in srgb, var(--gc) 10%, transparent);
  }
  .status {
    font-size: 10.5px; color: var(--text-muted);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .status.live { color: var(--status-info); }

  .trail { display: flex; align-items: center; gap: var(--s2); flex: none; }
  .spend { font-size: 10px; color: var(--text-muted); }
  .stop {
    display: inline-flex; align-items: center; justify-content: center;
    width: 16px; height: 16px; padding: 0;
    border: 1px solid color-mix(in srgb, var(--status-error) 40%, transparent);
    border-radius: var(--r-sm);
    background: color-mix(in srgb, var(--status-error) 12%, transparent);
    color: var(--status-error); cursor: pointer;
  }
  .stop:hover { background: color-mix(in srgb, var(--status-error) 22%, transparent); }
</style>
