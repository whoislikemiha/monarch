<script lang="ts">
  /**
   * One shadow in the fleet rail: avatar (grade ring + presence pip) · name +
   * rank · status + spend · full model name · stop/dismiss control.
   */
  import type { Agent } from "$lib/types";
  import { agentStore } from "$lib/stores/agentStore.svelte";
  import { liveAgentStore, abortAgent } from "$lib/toolbox/liveAgentStore.svelte";
  import Avatar from "$lib/ui/Avatar.svelte";
  import { gradeLetter } from "$lib/ui/grade";

  interface Props {
    agent: Agent;
    oncontextmenu?: (e: MouseEvent, agent: Agent) => void;
    ondismiss?: (agent: Agent) => void;
    onsummon?: (agent: Agent) => void;
  }
  let { agent, oncontextmenu, ondismiss, onsummon }: Props = $props();

  let live = $derived(liveAgentStore.byAgent.get(agent.id));
  let streaming = $derived(!!live?.isStreaming);
  let archived = $derived(!!agent.archivedAt);
  let active = $derived(agent.id === agentStore.activeTabId);

  let grade = $derived(gradeLetter(agent.shadow?.shadowGrade));
  let model = $derived(agent.model ?? "");

  // Only error gets a pip; working is shown by the animated ring; idle = nothing.
  let presence = $derived(agent.status === "error" ? "var(--status-error)" : null);
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
    working={streaming}
    avatarType={agent.avatarType}
    avatarPath={agent.avatarPath}
  />
  <div class="id">
    <div class="top">
      <span class="nm">{agent.name}</span>
      <span class="gchip mono" style="--gc:var(--grade-{grade.toLowerCase()})">{grade}</span>
      {#if streaming}
        <button class="act stop" title="Stop {agent.name}" aria-label="Stop {agent.name}" onclick={(e) => { e.stopPropagation(); void abortAgent(agent.id); }}>
          <svg viewBox="0 0 24 24" width="9" height="9" fill="currentColor"><rect x="6" y="6" width="12" height="12" rx="1.5" /></svg>
        </button>
      {:else if archived}
        <button class="act rowbtn" title="Summon back" aria-label="Summon {agent.name}" onclick={(e) => { e.stopPropagation(); onsummon?.(agent); }}>↺</button>
      {:else}
        <button class="act rowbtn dismiss" title="Dismiss" aria-label="Dismiss {agent.name}" onclick={(e) => { e.stopPropagation(); ondismiss?.(agent); }}>×</button>
      {/if}
    </div>
    {#if model}<div class="model mono" title={model}>{model}</div>{/if}
  </div>
</div>

<style>
  .row {
    display: flex;
    align-items: flex-start;
    gap: var(--s3);
    padding: var(--s2);
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

  .id { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .top { display: flex; align-items: center; gap: var(--s2); min-width: 0; }
  .nm {
    flex: 1;
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

  .model {
    font-size: 10px; color: var(--text-muted); line-height: 1.4;
    word-break: break-all;
  }

  /* action buttons share a slot at the right of the name row */
  .act { flex: none; }
  .stop {
    display: inline-flex; align-items: center; justify-content: center;
    width: 16px; height: 16px; padding: 0;
    border: 1px solid color-mix(in srgb, var(--status-error) 40%, transparent);
    border-radius: var(--r-sm);
    background: color-mix(in srgb, var(--status-error) 12%, transparent);
    color: var(--status-error); cursor: pointer;
  }
  .stop:hover { background: color-mix(in srgb, var(--status-error) 22%, transparent); }
  .rowbtn {
    width: 18px; height: 18px; display: inline-flex; align-items: center; justify-content: center;
    padding: 0; background: none; border: none; border-radius: var(--r-sm);
    color: var(--text-muted); cursor: pointer; font-size: 14px; line-height: 1;
    opacity: 0; transition: opacity 0.12s, color 0.12s, background 0.12s;
  }
  .row:hover .rowbtn { opacity: 1; }
  .rowbtn:hover { background: var(--bg-raised); color: var(--text-primary); }
  .rowbtn.dismiss:hover { color: var(--status-error); }
</style>
