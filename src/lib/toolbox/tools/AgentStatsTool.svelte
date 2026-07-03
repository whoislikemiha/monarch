<script lang="ts">
  import { invoke } from "$lib/api";
  import { formatCost } from "$lib/format";
  import type { ToolProps } from "../types";
  import type { AgentStats, SpecializationScores } from "../../bindings";

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

  // Only show specialization categories with non-zero scores, sorted descending
  const activeSpecs = $derived.by(() => {
    if (!stats) return [];
    const entries = Object.entries(stats.specialization) as [keyof SpecializationScores, number][];
    return entries
      .filter(([, v]) => v > 0.005)
      .sort((a, b) => b[1] - a[1]);
  });

  const primarySpec = $derived(
    activeSpecs.length > 0
      ? SPEC_LABELS[activeSpecs[0][0]]
      : null,
  );

  const topTools = $derived(
    stats ? stats.toolUsage.slice(0, 8) : [],
  );

  async function loadStats() {
    if (!agentContext) return;
    loading = true;
    error = null;
    try {
      stats = await invoke<AgentStats>("db_get_agent_stats", {
        agentId: agentContext.agentId,
      });
    } catch (e) {
      error = String(e);
      stats = null;
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (agentContext) {
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

  function pct(n: number): string {
    return (n * 100).toFixed(0) + "%";
  }
</script>

<div class="stats-tool">
  {#if !agentContext}
    <p class="empty">No agent selected.</p>
  {:else if loading && !stats}
    <p class="empty">Loading stats…</p>
  {:else if error}
    <p class="error-msg">{error}</p>
  {:else if stats}
    <!-- Identity + Experience -->
    <div class="section identity">
      <div class="agent-name">
        {agentContext.agent.shadow?.shadowName ?? agentContext.agent.name}
      </div>
      {#if agentContext.agent.shadow?.shadowTitle}
        <div class="agent-title">{agentContext.agent.shadow.shadowTitle}</div>
      {/if}
      {#if agentContext.agent.shadow?.shadowGrade}
        <div class="agent-grade">{agentContext.agent.shadow.shadowGrade}</div>
      {/if}
      {#if primarySpec}
        <div class="primary-spec">{primarySpec} Specialist</div>
      {/if}

      <div class="xp-bar-container">
        <div class="xp-label">
          <span>EXP</span>
          <span class="xp-value">{stats.experience.toFixed(0)}</span>
        </div>
        <div class="xp-track">
          <div class="xp-fill" style="width: {stats.experience}%"></div>
        </div>
      </div>
    </div>

    <!-- Lifetime Numbers -->
    <div class="section">
      <div class="section-title">Lifetime</div>
      <div class="row">
        <span class="label">Tokens (in)</span>
        <span class="value mono">{formatTokens(stats.totalInputTokens)}</span>
      </div>
      <div class="row">
        <span class="label">Tokens (out)</span>
        <span class="value mono">{formatTokens(stats.totalOutputTokens)}</span>
      </div>
      <div class="row">
        <span class="label">Cost</span>
        <span class="value mono">{formatCost(stats.totalCost) ?? "$0"}</span>
      </div>
      <div class="row">
        <span class="label">Sessions</span>
        <span class="value mono">{stats.totalSessions}</span>
      </div>
      <div class="row">
        <span class="label">Messages</span>
        <span class="value mono">{stats.totalMessages}</span>
      </div>
      <div class="row">
        <span class="label">Turns</span>
        <span class="value mono">{stats.totalTurns}</span>
      </div>
    </div>

    <!-- Specialization -->
    {#if activeSpecs.length > 0}
      <div class="section">
        <div class="section-title">Specialization</div>
        {#each activeSpecs as [key, value]}
          <div class="spec-row">
            <span class="spec-label">{SPEC_LABELS[key]}</span>
            <div class="spec-bar-track">
              <div
                class="spec-bar-fill"
                style="width: {value * 100}%"
              ></div>
            </div>
            <span class="spec-pct">{pct(value)}</span>
          </div>
        {/each}
      </div>
    {/if}

    <!-- Tool Usage -->
    {#if topTools.length > 0}
      <div class="section">
        <div class="section-title">Tools</div>
        {#each topTools as tool}
          <div class="row">
            <span class="label tool-name">{tool.toolName}</span>
            <span class="value mono">
              {tool.callCount}{#if tool.errorCount > 0}<span class="error-count"> ({tool.errorCount} err)</span>{/if}
            </span>
          </div>
        {/each}
      </div>
    {/if}

    <!-- Refresh -->
    <button class="refresh-btn" type="button" onclick={loadStats} disabled={loading}>
      {loading ? "Loading…" : "Refresh"}
    </button>
  {:else}
    <p class="empty">No stats yet — use this agent to start tracking.</p>
  {/if}
</div>

<style>
  .stats-tool {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .section-title {
    font-size: 9px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    margin-bottom: 2px;
  }

  /* Identity */
  .identity {
    gap: 2px;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .agent-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .agent-title {
    font-size: 11px;
    color: var(--accent);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .agent-grade {
    font-size: 10px;
    color: var(--text-muted);
  }

  .primary-spec {
    font-size: 10px;
    color: var(--accent);
    font-weight: 500;
    margin-top: 2px;
  }

  /* Experience bar */
  .xp-bar-container {
    margin-top: 6px;
  }

  .xp-label {
    display: flex;
    justify-content: space-between;
    font-size: 9px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin-bottom: 3px;
  }

  .xp-value {
    color: var(--accent);
    font-weight: 600;
  }

  .xp-track {
    position: relative;
    height: 4px;
    border-radius: 2px;
    overflow: hidden;
    background: var(--active-overlay);
  }

  .xp-fill {
    height: 100%;
    border-radius: 2px;
    background: var(--accent);
    transition: width 0.3s ease;
  }

  /* Rows */
  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    font-size: 11px;
  }

  .label {
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-size: 10px;
  }

  .value {
    color: var(--text-primary);
  }

  .value.mono {
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    text-align: right;
  }

  .tool-name {
    text-transform: none;
    letter-spacing: normal;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .error-count {
    color: var(--error);
    font-size: 10px;
  }

  /* Specialization bars */
  .spec-row {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 10px;
  }

  .spec-label {
    width: 56px;
    flex-shrink: 0;
    color: var(--text-muted);
    font-size: 10px;
  }

  .spec-bar-track {
    flex: 1;
    height: 4px;
    border-radius: 2px;
    background: var(--active-overlay);
    overflow: hidden;
  }

  .spec-bar-fill {
    height: 100%;
    border-radius: 2px;
    background: var(--accent);
    transition: width 0.3s ease;
  }

  .spec-pct {
    width: 28px;
    text-align: right;
    color: var(--text-secondary);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    font-size: 10px;
  }

  /* Refresh */
  .refresh-btn {
    margin-top: 4px;
    padding: 5px 10px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: var(--bg-panel-2);
    color: var(--text-primary);
    font-family: inherit;
    font-size: 10px;
    cursor: pointer;
    transition: background 0.15s;
  }

  .refresh-btn:hover:not(:disabled) {
    background: var(--accent-bg-hover);
  }

  .refresh-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .empty {
    margin: 0;
    color: var(--text-muted);
    font-size: 11px;
    font-style: italic;
  }

  .error-msg {
    margin: 0;
    color: var(--error);
    font-size: 11px;
  }
</style>
