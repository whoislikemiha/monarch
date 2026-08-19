<script lang="ts">
  /**
   * Agent service record: identity header, experience, lifetime totals,
   * specialization profile (derived from tool usage) and per-tool counts.
   * DB-backed (`db_get_agent_stats`) — renders whether or not the agent has a
   * live session. Built on the design-system atoms (.meter/.drow/.gchip).
   */
  import { invoke } from "$lib/api";
  import { formatCost } from "$lib/format";
  import Avatar from "$lib/ui/Avatar.svelte";
  import { gradeLetter } from "$lib/ui/grade";
  import type { ToolProps } from "../types";
  import type { AgentStats, SpecializationScores } from "$lib/bindings";

  let { agentContext }: ToolProps = $props();

  let stats = $state<AgentStats | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  const SPEC_LABELS: Record<keyof SpecializationScores, string> = {
    coding: "Coding",
    research: "Research",
    testing: "Testing",
    debugging: "Debugging",
    devops: "DevOps",
    documentation: "Docs",
    database: "Database",
    configuration: "Config",
    design: "Design",
    communication: "Comms",
    refactoring: "Refactor",
    security: "Security",
  };

  let grade = $derived(gradeLetter(agentContext?.agent.shadow?.shadowGrade));

  // Non-zero specialization axes, strongest first.
  let activeSpecs = $derived.by(() => {
    if (!stats) return [];
    const entries = Object.entries(stats.specialization) as [keyof SpecializationScores, number][];
    return entries.filter(([, v]) => v > 0.005).sort((a, b) => b[1] - a[1]);
  });

  let primarySpec = $derived(activeSpecs.length > 0 ? SPEC_LABELS[activeSpecs[0][0]] : null);
  let topTools = $derived(stats ? stats.toolUsage.slice(0, 8) : []);
  let maxToolCalls = $derived(topTools.reduce((m, t) => Math.max(m, t.callCount), 1));

  async function loadStats() {
    if (!agentContext) return;
    loading = true;
    error = null;
    try {
      stats = await invoke<AgentStats>("db_get_agent_stats", { agentId: agentContext.agentId });
    } catch (e) {
      error = String(e);
      stats = null;
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (agentContext?.agentId) {
      loadStats();
    } else {
      stats = null;
    }
  });

  function formatTokens(n: number): string {
    if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + "M";
    if (n >= 1_000) return (n / 1_000).toFixed(1) + "K";
    return n.toString();
  }

  const pct = (n: number): string => (n * 100).toFixed(0) + "%";
</script>

<div class="stats">
  {#if !agentContext}
    <div class="blank">Select an agent to view its service record.</div>
  {:else if loading && !stats}
    <div class="blank">Loading stats…</div>
  {:else if error}
    <div class="err">{error}</div>
  {:else if stats}
    <!-- identity -->
    <header class="who">
      <Avatar
        name={agentContext.agent.name}
        size={38}
        {grade}
        avatarType={agentContext.agent.avatarType}
        avatarPath={agentContext.agent.avatarPath}
        provider={agentContext.agent.provider}
      />
      <div class="who-id">
        <div class="who-top">
          <span class="nm">{agentContext.agent.shadow?.shadowName ?? agentContext.agent.name}</span>
          <span class="gchip mono" style="--gc:var(--grade-{grade.toLowerCase()}); color:var(--gc); border-color:var(--gc)">{grade}</span>
        </div>
        <div class="who-sub">
          {#if agentContext.agent.shadow?.shadowTitle}
            <span class="ti">{agentContext.agent.shadow.shadowTitle}</span>
          {/if}
          {#if primarySpec}
            <span class="spec">{primarySpec} lead</span>
          {/if}
        </div>
      </div>
    </header>

    <!-- experience -->
    <div class="meter">
      <div class="top">
        <span class="lab">Experience</span>
        <span class="val">{stats.experience.toFixed(0)} / 100</span>
      </div>
      <div class="track"><div class="fill" style="width:{Math.min(100, stats.experience)}%"></div></div>
    </div>

    <!-- lifetime totals -->
    <section class="block">
      <div class="bt">Lifetime</div>
      <div class="rows">
        <div class="r"><span class="k">Sessions</span><span class="v mono">{stats.totalSessions.toLocaleString()}</span></div>
        <div class="r"><span class="k">Messages</span><span class="v mono">{stats.totalMessages.toLocaleString()}</span></div>
        <div class="r"><span class="k">Turns</span><span class="v mono">{stats.totalTurns.toLocaleString()}</span></div>
        <div class="r"><span class="k">Tokens in</span><span class="v mono">{formatTokens(stats.totalInputTokens)}</span></div>
        <div class="r"><span class="k">Tokens out</span><span class="v mono">{formatTokens(stats.totalOutputTokens)}</span></div>
        <div class="r"><span class="k">Cost</span><span class="v mono">{formatCost(stats.totalCost) ?? "$0"}</span></div>
      </div>
    </section>

    <!-- specialization profile -->
    {#if activeSpecs.length > 0}
      <section class="block">
        <div class="bt">Specialization</div>
        <div class="rows">
          {#each activeSpecs as [key, value] (key)}
            <div class="r">
              <span class="k">{SPEC_LABELS[key]}</span>
              <span class="cell">
                <span class="track thin"><span class="fill" style="width:{value * 100}%"></span></span>
                <span class="cv mono">{pct(value)}</span>
              </span>
            </div>
          {/each}
        </div>
      </section>
    {/if}

    <!-- tool usage -->
    {#if topTools.length > 0}
      <section class="block">
        <div class="bt">Tools</div>
        <div class="rows">
          {#each topTools as tool (tool.toolName)}
            <div class="r">
              <span class="k mono">{tool.toolName}</span>
              <span class="cell">
                <span class="track thin"><span class="fill f2" style="width:{(tool.callCount / maxToolCalls) * 100}%"></span></span>
                <span class="cv mono">
                  {tool.callCount.toLocaleString()}{#if tool.errorCount > 0}<span class="errct"> · {tool.errorCount}✕</span>{/if}
                </span>
              </span>
            </div>
          {/each}
        </div>
      </section>
    {/if}
  {:else}
    <div class="blank">No stats yet — this agent hasn't worked.</div>
  {/if}
</div>

<style>
  .stats {
    display: flex;
    flex-direction: column;
    gap: var(--s4);
    padding: var(--s3);
  }

  .blank {
    padding: var(--s4);
    text-align: center;
    font-size: 11px;
    color: var(--text-muted);
  }
  .err {
    padding: var(--s2) var(--s3);
    font-size: 11px;
    color: var(--status-error);
  }

  /* identity header */
  .who { display: flex; align-items: center; gap: var(--s3); }
  .who-id { min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .who-top { display: flex; align-items: center; gap: var(--s2); }
  .nm { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .who-sub { display: flex; align-items: baseline; gap: var(--s2); min-width: 0; }
  .ti { font-size: 11px; color: var(--text-secondary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .spec { font-size: 10px; color: var(--accent); white-space: nowrap; }

  /* meters (local copy of the .meter atom internals for scoped markup) */
  .meter { display: flex; flex-direction: column; gap: 5px; }
  .meter .top { display: flex; justify-content: space-between; align-items: baseline; }
  .meter .lab { font-size: 11px; color: var(--text-secondary); font-weight: 500; }
  .meter .val { font-family: "JetBrains Mono", monospace; font-size: 10.5px; color: var(--text-primary); }
  .track {
    display: block; height: 6px; flex: 1;
    background: var(--bg-sink); border: 1px solid var(--border-subtle);
    border-radius: var(--r-full); overflow: hidden;
  }
  .track.thin { height: 4px; }
  .fill { display: block; height: 100%; background: var(--accent); border-radius: var(--r-full); }
  .fill.f2 { background: var(--accent-2); }

  /* data blocks */
  .block { display: flex; flex-direction: column; gap: var(--s2); }
  .bt {
    font-size: 10px; font-weight: 600; letter-spacing: 0.14em;
    text-transform: uppercase; color: var(--text-muted);
  }
  .rows {
    border: 1px solid var(--border-subtle); border-radius: var(--r-md);
    background: var(--bg-panel); overflow: hidden;
  }
  .r {
    display: flex; align-items: center; gap: var(--s3);
    padding: 5px var(--s3); border-bottom: 1px solid var(--border-subtle);
    min-height: 24px;
  }
  .r:last-child { border-bottom: none; }
  .r > .k { font-size: 11px; color: var(--text-muted); flex: none; width: 84px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .r > .k.mono { font-family: "JetBrains Mono", monospace; font-size: 10.5px; color: var(--text-secondary); }
  .r > .v { font-size: 11.5px; color: var(--text-primary); margin-left: auto; }
  .r > .v.mono, .cv.mono { font-family: "JetBrains Mono", monospace; font-size: 10.5px; }
  .cell { display: flex; align-items: center; gap: var(--s2); flex: 1; min-width: 0; }
  .cv { color: var(--text-secondary); flex: none; min-width: 44px; text-align: right; }
  .errct { color: var(--status-error); }
</style>
