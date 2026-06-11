<script lang="ts">
  /**
   * Slim header above the solo workspace. Identity + live status, and the
   * relocated shadow controls (no more floating portrait): thinking level,
   * model, and an actions menu (new session · compact · prompt · history).
   */
  import type { Agent } from "$lib/types";
  import { agentStore } from "$lib/stores/agentStore.svelte";
  import { liveAgentStore } from "$lib/toolbox/liveAgentStore.svelte";
  import { formatCost } from "$lib/format";
  import Avatar from "$lib/ui/Avatar.svelte";
  import { gradeLetter } from "$lib/ui/grade";
  import ThinkingPicker from "$lib/ThinkingPicker.svelte";
  import HistoryPanel from "$lib/HistoryPanel.svelte";
  import PromptEditor from "$lib/PromptEditor.svelte";
  import type { LiveBinding } from "./liveBinding.svelte";

  interface Props {
    agent: Agent;
    binding: LiveBinding;
  }
  let { agent, binding }: Props = $props();

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

  let menuOpen = $state(false);
  let showHistory = $state(false);
  let showPrompt = $state(false);

  function onThinking(level: string) {
    agentStore.updateAgent(agent.id, (a) => ({ ...a, thinkingLevel: level }));
    binding.setThinkingLevel(agent, level).catch(() => {});
  }
</script>

<header class="head">
  <Avatar name={agent.name} size={28} {grade} {presence} avatarType={agent.avatarType} avatarPath={agent.avatarPath} />
  <span class="nm">{agent.name}</span>
  <span class="gchip mono" style="--gc:var(--grade-{grade.toLowerCase()})">{grade}</span>
  <span class="dot" aria-hidden="true">·</span>
  <span class="status" class:live={streaming}>{status}</span>

  <div class="grow"></div>

  {#if agent.model}
    <ThinkingPicker
      provider={agent.provider ?? ""}
      model={agent.model}
      value={agent.thinkingLevel ?? "off"}
      onchange={onThinking}
    />
  {/if}
  {#if spend}<span class="spend mono">{spend}</span>{/if}

  {#if streaming}
    <button class="hbtn danger" title="Stop" aria-label="Stop" onclick={() => binding.abort(agent)}>
      <svg viewBox="0 0 24 24" width="12" height="12" fill="currentColor"><rect x="6" y="6" width="12" height="12" rx="2" /></svg>
    </button>
  {/if}

  <div class="menu-wrap">
    <button class="hbtn" title="Actions" aria-label="Actions" onclick={() => (menuOpen = !menuOpen)}>
      <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor"><circle cx="5" cy="12" r="1.6"/><circle cx="12" cy="12" r="1.6"/><circle cx="19" cy="12" r="1.6"/></svg>
    </button>
    {#if menuOpen}
      <button class="menu-scrim" aria-label="Close menu" onclick={() => (menuOpen = false)}></button>
      <div class="menu" role="menu">
        <button role="menuitem" onclick={() => { menuOpen = false; agentStore.newConversation(agent.id); }}>New session</button>
        <button role="menuitem" onclick={() => { menuOpen = false; binding.compact(agent); }}>Compact context</button>
        <button role="menuitem" onclick={() => { menuOpen = false; showPrompt = true; }}>Edit prompt</button>
        <button role="menuitem" onclick={() => { menuOpen = false; showHistory = true; }}>Session history</button>
      </div>
    {/if}
  </div>
</header>

{#if showHistory}
  <HistoryPanel
    agentId={agent.id}
    sessions={agent.sessions || []}
    currentSessionId={agent.sessionId}
    onload={(session) => { showHistory = false; agentStore.switchSession(agent.id, session.sessionId); }}
    onclose={() => (showHistory = false)}
  />
{/if}

{#if showPrompt}
  <PromptEditor
    agentId={agent.id}
    shadowName={agent.shadow?.shadowName}
    shadowTitle={agent.shadow?.shadowTitle}
    shadowGrade={agent.shadow?.shadowGrade}
    onclose={() => (showPrompt = false)}
  />
{/if}

<style>
  .head {
    display: flex;
    align-items: center;
    gap: var(--s2);
    height: 44px;
    flex: none;
    padding: 0 var(--s3) 0 var(--s4);
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
  .status { font-size: 12px; color: var(--text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .status.live { color: var(--status-info); }
  .grow { flex: 1; min-width: var(--s3); }
  .spend { font-size: 11px; color: var(--text-muted); }

  .hbtn {
    width: 28px; height: 28px; display: inline-flex; align-items: center; justify-content: center;
    background: transparent; border: 1px solid var(--border-subtle); border-radius: var(--r-md);
    color: var(--text-secondary); cursor: pointer; flex: none;
  }
  .hbtn:hover { background: var(--bg-raised); color: var(--text-primary); }
  .hbtn.danger { color: var(--status-error); border-color: color-mix(in srgb, var(--status-error) 40%, transparent); }
  .hbtn.danger:hover { background: color-mix(in srgb, var(--status-error) 14%, transparent); }

  .menu-wrap { position: relative; flex: none; }
  .menu-scrim { position: fixed; inset: 0; z-index: 40; background: none; border: none; cursor: default; }
  .menu {
    position: absolute; top: 32px; right: 0; z-index: 41;
    min-width: 168px; padding: var(--s1);
    background: var(--bg-overlay); border: 1px solid var(--border-strong); border-radius: var(--r-md);
    display: flex; flex-direction: column; gap: 1px;
  }
  .menu button {
    text-align: left; font: inherit; font-size: 12px; color: var(--text-secondary);
    background: none; border: none; border-radius: var(--r-sm); padding: 6px var(--s3); cursor: pointer;
  }
  .menu button:hover { background: var(--bg-raised); color: var(--text-primary); }
</style>
