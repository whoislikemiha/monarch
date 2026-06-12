<script lang="ts">
  /**
   * Slim header above the solo workspace. Identity + live status, and the
   * relocated shadow controls (no more floating portrait): thinking level,
   * model, and an actions menu (new session · compact · prompt · history).
   */
  import type { Agent } from "$lib/types";
  import { agentStore } from "$lib/stores/agentStore.svelte";
  import { liveAgentStore } from "$lib/toolbox/liveAgentStore.svelte";
  import { layoutStore } from "$lib/layout/layoutStore.svelte";
  import { formatCost } from "$lib/format";
  import Avatar from "$lib/ui/Avatar.svelte";
  import { gradeLetter } from "$lib/ui/grade";
  import ThinkingPicker from "$lib/ThinkingPicker.svelte";
  import PromptEditor from "$lib/PromptEditor.svelte";
  import type { LiveBinding } from "./liveBinding.svelte";

  interface Props {
    agent: Agent;
    binding: LiveBinding;
    onnewchat?: () => void;
  }
  let { agent, binding, onnewchat }: Props = $props();

  let live = $derived(liveAgentStore.byAgent.get(agent.id));
  let streaming = $derived(!!live?.isStreaming);
  let grade = $derived(gradeLetter(agent.shadow?.shadowGrade));
  let spend = $derived(formatCost(agent.lifetimeCost));
  let model = $derived(agent.model ?? "");
  // Error gets a pip; working shows via the avatar ring; idle = nothing.
  let presence = $derived(agent.status === "error" ? "var(--status-error)" : null);
  // Only narrate when actually working; otherwise the model line carries identity.
  let liveStatus = $derived(streaming ? live?.activityStatus || "Working…" : "");

  let menuOpen = $state(false);
  let showPrompt = $state(false);

  function onThinking(level: string) {
    agentStore.updateAgent(agent.id, (a) => ({ ...a, thinkingLevel: level }));
    binding.setThinkingLevel(agent, level).catch(() => {});
  }
</script>

<header class="head">
  <Avatar name={agent.name} size={28} {grade} {presence} working={streaming} avatarType={agent.avatarType} avatarPath={agent.avatarPath} />
  <div class="who">
    <div class="line1">
      <span class="nm">{agent.name}</span>
      <span class="gchip mono" style="--gc:var(--grade-{grade.toLowerCase()})">{grade}</span>
      {#if liveStatus}
        <span class="status live">{liveStatus}</span>
      {/if}
    </div>
    {#if model}<div class="model mono" title={model}>{model}</div>{/if}
  </div>

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

  <span class="sep" aria-hidden="true"></span>

  <button class="hbtn" title="New chat" aria-label="New chat" onclick={() => onnewchat?.()}>
    <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 4h10v6H7l-3 2.5V10H3z"/></svg>
  </button>
  <button
    class="hbtn"
    title={layoutStore.workspaceOrient === "h" ? "Stack tiles vertically" : "Lay tiles side by side"}
    aria-label="Toggle layout orientation"
    onclick={() => layoutStore.toggleOrient()}
  >
    {#if layoutStore.workspaceOrient === "h"}
      <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.4"><rect x="2.5" y="2.5" width="11" height="4.5" rx="1"/><rect x="2.5" y="9" width="11" height="4.5" rx="1"/></svg>
    {:else}
      <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.4"><rect x="2.5" y="2.5" width="4.5" height="11" rx="1"/><rect x="9" y="2.5" width="4.5" height="11" rx="1"/></svg>
    {/if}
  </button>

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
        <button role="menuitem" onclick={() => { menuOpen = false; if (!layoutStore.isOpen("sessions")) layoutStore.toggle("sessions"); }}>Session history</button>
      </div>
    {/if}
  </div>
</header>

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
  .who { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
  .line1 { display: flex; align-items: center; gap: var(--s2); min-width: 0; }
  .nm { font-size: 13px; font-weight: 600; color: var(--text-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; max-width: 280px; }
  .gchip {
    flex: none;
    display: inline-flex; align-items: center; justify-content: center;
    width: 16px; height: 16px; border-radius: var(--r-sm);
    font-size: 9.5px; font-weight: 700; color: var(--gc);
    border: 1px solid color-mix(in srgb, var(--gc) 40%, transparent);
    background: color-mix(in srgb, var(--gc) 10%, transparent);
  }
  .status { font-size: 11px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .status.live { color: var(--status-info); }
  .model { font-size: 10.5px; color: var(--text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 320px; }
  .grow { flex: 1; min-width: var(--s3); }
  .spend { font-size: 11px; color: var(--text-muted); }
  .sep { width: 1px; height: 18px; background: var(--border-subtle); margin: 0 var(--s1); flex: none; }

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
