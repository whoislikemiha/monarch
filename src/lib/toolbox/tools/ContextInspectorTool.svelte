<script lang="ts">
  /**
   * Context-window inspector: live used/window snapshot with health meter,
   * then the window's contents broken into collapsible categories (setup,
   * user, assistant, thinking, tool calls/results) with per-entry estimates.
   * Live-state panel: sizes come from the in-memory session; when the agent
   * is asleep only the setup layer is estimable and a note says so.
   */
  import type { ToolProps } from "../types";
  import type { DisplayItem, Usage } from "../../types";

  let { agentContext }: ToolProps = $props();

  const CHARS_PER_TOKEN = 4;

  interface ContextEntry {
    id: string;
    label: string;
    tokens: number;
    preview: string;
    fullText: string;
    meta?: string;
  }

  interface ContextCategory {
    id: string;
    label: string;
    tokens: number;
    entries: ContextEntry[];
  }

  let expandedCategories: Record<string, boolean> = $state({
    setup: true,
    "tool-calls": true,
    "tool-results": true,
  });
  let expandedEntries: Record<string, boolean> = $state({});

  function toggleCategory(id: string) {
    expandedCategories = { ...expandedCategories, [id]: !expandedCategories[id] };
  }

  function toggleEntry(id: string) {
    expandedEntries = { ...expandedEntries, [id]: !expandedEntries[id] };
  }

  function estimateTokens(text: string): number {
    return Math.ceil(text.length / CHARS_PER_TOKEN);
  }

  function formatTokens(n: number): string {
    if (n >= 1000000) return (n / 1000000).toFixed(1) + "M";
    if (n >= 1000) return (n / 1000).toFixed(1) + "k";
    return n.toString();
  }

  function truncate(text: string, max: number): string {
    if (text.length <= max) return text;
    return text.slice(0, max) + "...";
  }

  function firstMeaningfulLine(text: string): string {
    const line = text
      .split("\n")
      .map((part) => part.trim())
      .find(Boolean);
    return line || text.trim();
  }

  function textPartsSummary(text: string, fallback: string): { label: string; preview: string } {
    const trimmed = text.trim();
    if (!trimmed) return { label: fallback, preview: "" };
    const firstLine = firstMeaningfulLine(trimmed);
    return {
      label: truncate(firstLine, 56),
      preview: truncate(trimmed, 240),
    };
  }

  function stringifyToolResult(result: unknown): string {
    if (result == null) return "";
    if (typeof result === "string") return result;
    if (typeof result === "object" && result && "content" in result && Array.isArray((result as any).content)) {
      return (result as any).content
        .map((part: any) => (part?.type === "text" ? part.text : `[${part?.type || "content"}]`))
        .join("\n");
    }
    return JSON.stringify(result, null, 2);
  }

  function summarizeToolCall(toolName: string, args: unknown): string {
    const typedArgs = (args && typeof args === "object" ? args : {}) as Record<string, any>;
    switch (toolName) {
      case "bash":
        return typedArgs.command || "";
      case "read":
      case "write":
      case "edit":
        return typedArgs.file_path || typedArgs.path || "";
      case "grep":
        return [typedArgs.pattern, typedArgs.path].filter(Boolean).join(" ");
      case "find":
        return [typedArgs.pattern || typedArgs.glob, typedArgs.path].filter(Boolean).join(" ");
      case "ls":
        return typedArgs.path || ".";
      default:
        return JSON.stringify(args ?? {});
    }
  }

  let items = $derived<DisplayItem[]>(agentContext?.live?.items ?? []);
  let lastUsage = $derived<Usage | undefined>(agentContext?.live?.lastUsage);
  let isLive = $derived(!!agentContext?.live);
  let contextWindow = $derived(agentContext?.agent.contextWindow);
  let sessionStats = $derived(agentContext?.agent.sessionStats);
  let shadow = $derived(agentContext?.agent.shadow);
  let customPrompt = $derived(agentContext?.setup.customPrompt ?? null);
  let projectInstructions = $derived(agentContext?.setup.projectInstructions ?? null);

  let categories = $derived.by<ContextCategory[]>(() => {
    const next: ContextCategory[] = [];

    const setupEntries: ContextEntry[] = [];
    if (customPrompt?.trim()) {
      const text = customPrompt.trim();
      setupEntries.push({
        id: "setup-custom-prompt",
        label: "Custom prompt",
        tokens: estimateTokens(text),
        preview: truncate(text, 240),
        fullText: text,
      });
    }
    if (projectInstructions?.trim()) {
      const text = projectInstructions.trim();
      setupEntries.push({
        id: "setup-project-instructions",
        label: "Project instructions",
        tokens: estimateTokens(text),
        preview: truncate(text, 240),
        fullText: text,
      });
    }
    if (shadow) {
      const text = `${shadow.shadowName} · ${shadow.shadowTitle} · ${shadow.shadowGrade}`;
      setupEntries.push({
        id: "setup-shadow",
        label: "Agent identity",
        tokens: estimateTokens(text),
        preview: text,
        fullText: text,
      });
    }
    if (setupEntries.length > 0) {
      next.push({
        id: "setup",
        label: "Setup",
        tokens: setupEntries.reduce((sum, entry) => sum + entry.tokens, 0),
        entries: setupEntries,
      });
    }

    const userEntries: ContextEntry[] = [];
    let userIndex = 0;
    for (const item of items) {
      if (item.kind !== "user") continue;
      const text = item.content.trim();
      if (!text) continue;
      userIndex++;
      const summary = textPartsSummary(text, `User ${userIndex}`);
      userEntries.push({
        id: `user-${userIndex}`,
        label: summary.label,
        tokens: estimateTokens(text),
        preview: summary.preview,
        fullText: text,
      });
    }
    if (userEntries.length > 0) {
      next.push({
        id: "user-messages",
        label: "User messages",
        tokens: userEntries.reduce((sum, entry) => sum + entry.tokens, 0),
        entries: userEntries,
      });
    }

    const assistantEntries: ContextEntry[] = [];
    const thinkingEntries: ContextEntry[] = [];
    let assistantIndex = 0;
    let thinkingIndex = 0;
    for (const item of items) {
      if (item.kind !== "assistant") continue;

      const assistantText = item.content
        .filter((block): block is Extract<typeof block, { type: "text" }> => block.type === "text" && block.text.trim().length > 0)
        .map((block) => block.text)
        .join("\n\n")
        .trim();

      if (assistantText) {
        assistantIndex++;
        const summary = textPartsSummary(assistantText, `Assistant ${assistantIndex}`);
        assistantEntries.push({
          id: `assistant-${assistantIndex}`,
          label: summary.label,
          tokens: estimateTokens(assistantText),
          preview: summary.preview,
          fullText: assistantText,
          meta: item.model,
        });
      }

      for (const block of item.content) {
        if (block.type !== "thinking" || !block.thinking.trim()) continue;
        thinkingIndex++;
        const summary = textPartsSummary(block.thinking, `Thinking ${thinkingIndex}`);
        thinkingEntries.push({
          id: `thinking-${thinkingIndex}`,
          label: summary.label,
          tokens: estimateTokens(block.thinking),
          preview: summary.preview,
          fullText: block.thinking,
          meta: block.redacted ? "redacted" : undefined,
        });
      }
    }
    if (assistantEntries.length > 0) {
      next.push({
        id: "assistant-messages",
        label: "Assistant messages",
        tokens: assistantEntries.reduce((sum, entry) => sum + entry.tokens, 0),
        entries: assistantEntries,
      });
    }
    if (thinkingEntries.length > 0) {
      next.push({
        id: "thinking",
        label: "Thinking",
        tokens: thinkingEntries.reduce((sum, entry) => sum + entry.tokens, 0),
        entries: thinkingEntries,
      });
    }

    const toolCallEntries: ContextEntry[] = [];
    const toolResultEntries: ContextEntry[] = [];
    let toolIndex = 0;
    for (const item of items) {
      if (item.kind !== "tool-group") continue;
      for (const execution of item.executions) {
        toolIndex++;
        const callText = summarizeToolCall(execution.toolName, execution.args).trim() || JSON.stringify(execution.args ?? {});
        if (callText.trim()) {
          toolCallEntries.push({
            id: `tool-call-${toolIndex}`,
            label: `${execution.toolName} · ${truncate(firstMeaningfulLine(callText), 56)}`,
            tokens: estimateTokens(callText),
            preview: truncate(callText, 240),
            fullText: callText,
            meta: execution.isError ? "error" : undefined,
          });
        }

        const resultText = stringifyToolResult(execution.result).trim();
        if (resultText) {
          toolResultEntries.push({
            id: `tool-result-${toolIndex}`,
            label: `${execution.toolName} result`,
            tokens: estimateTokens(resultText),
            preview: truncate(resultText, 240),
            fullText: resultText,
            meta: execution.isError ? "error" : undefined,
          });
        }
      }
    }
    if (toolCallEntries.length > 0) {
      next.push({
        id: "tool-calls",
        label: "Tool calls",
        tokens: toolCallEntries.reduce((sum, entry) => sum + entry.tokens, 0),
        entries: toolCallEntries,
      });
    }
    if (toolResultEntries.length > 0) {
      next.push({
        id: "tool-results",
        label: "Tool results",
        tokens: toolResultEntries.reduce((sum, entry) => sum + entry.tokens, 0),
        entries: toolResultEntries,
      });
    }

    return next;
  });

  let estimatedTotal = $derived(categories.reduce((sum, category) => sum + category.tokens, 0));

  let liveContextTokens = $derived.by(() => {
    if (!lastUsage) return estimatedTotal;
    const cached = (lastUsage.cacheRead ?? 0) + (lastUsage.cacheWrite ?? 0);
    if (lastUsage.input && lastUsage.input > 0) return lastUsage.input + cached;
    if (lastUsage.totalTokens) {
      const output = lastUsage.output ?? 0;
      return Math.max(lastUsage.totalTokens - output, 0);
    }
    return estimatedTotal;
  });

  let resolvedWindow = $derived(contextWindow ?? 128000);
  let usedRatio = $derived(liveContextTokens > 0 ? Math.min(liveContextTokens / resolvedWindow, 1) : 0);
  let freeTokens = $derived(Math.max(resolvedWindow - liveContextTokens, 0));
  let freePct = $derived(Math.max(100 - Math.round(usedRatio * 100), 0));
  let healthState = $derived(
    usedRatio >= 0.9 ? "critical" : usedRatio >= 0.7 ? "warning" : "healthy"
  );
</script>

{#if agentContext === null}
  <div class="blank">Select an agent to inspect its context window.</div>
{:else}
  <div class="ctx">
    <!-- snapshot -->
    <div class="snap">
      <div class="meter" class:warn={healthState === "warning"} class:crit={healthState === "critical"}>
        <div class="top">
          <span class="lab">Context window</span>
          <span class="val mono">{formatTokens(liveContextTokens)} / {formatTokens(resolvedWindow)}</span>
        </div>
        <div class="track"><div class="fill" style:width={`${Math.max(usedRatio * 100, liveContextTokens > 0 ? 1.5 : 0)}%`}></div></div>
        <div class="under">
          <span class="src">{lastUsage ? "live telemetry" : "estimated from content"}</span>
          <span class="free mono">{formatTokens(freeTokens)} free · {freePct}%</span>
        </div>
      </div>

      {#if sessionStats || (lastUsage && ((lastUsage.cacheRead ?? 0) > 0 || (lastUsage.cacheWrite ?? 0) > 0))}
        <div class="facts">
          {#if sessionStats && sessionStats.totalTokens > 0}
            <div class="fr"><span class="fk">Billing total</span><span class="fv mono">{formatTokens(sessionStats.totalTokens)} · ${sessionStats.totalCost.toFixed(4)}</span></div>
          {/if}
          {#if sessionStats}
            <div class="fr"><span class="fk">Turns</span><span class="fv mono">{sessionStats.turnCount}</span></div>
            <div class="fr"><span class="fk">Messages</span><span class="fv mono">{sessionStats.messageCount}</span></div>
          {/if}
          {#if lastUsage && (lastUsage.cacheRead ?? 0) > 0}
            <div class="fr"><span class="fk">Cache read</span><span class="fv mono">{formatTokens(lastUsage.cacheRead ?? 0)}</span></div>
          {/if}
          {#if lastUsage && (lastUsage.cacheWrite ?? 0) > 0}
            <div class="fr"><span class="fk">Cache write</span><span class="fv mono">{formatTokens(lastUsage.cacheWrite ?? 0)}</span></div>
          {/if}
        </div>
      {/if}

      {#if !isLive}
        <p class="asleep">Agent is asleep — live telemetry appears when a session starts.</p>
      {/if}
    </div>

    <!-- categories -->
    <div class="cats">
      {#each categories as category (category.id)}
        <div class="cat">
          <button class="cat-head" onclick={() => toggleCategory(category.id)} aria-expanded={!!expandedCategories[category.id]}>
            <svg class="chev" class:open={expandedCategories[category.id]} viewBox="0 0 16 16" width="10" height="10" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M6 4l4 4-4 4"/></svg>
            <span class="cat-label">{category.label}</span>
            <span class="cat-count mono">{category.entries.length}</span>
            <span class="cat-tokens mono">{formatTokens(category.tokens)}</span>
          </button>

          {#if expandedCategories[category.id]}
            <div class="entries">
              {#each category.entries as entry (entry.id)}
                <button class="entry" onclick={() => toggleEntry(entry.id)}>
                  <span class="e-head">
                    <span class="e-label">{entry.label}</span>
                    {#if entry.meta}
                      <span class="e-badge" class:err={entry.meta === "error"}>{entry.meta}</span>
                    {/if}
                    <span class="e-tokens mono">{formatTokens(entry.tokens)}</span>
                  </span>
                  {#if entry.preview}
                    <span class="e-preview mono">{expandedEntries[entry.id] ? entry.fullText : entry.preview}</span>
                  {/if}
                </button>
              {/each}
            </div>
          {/if}
        </div>
      {/each}

      {#if categories.length === 0}
        <div class="blank">Nothing in the window yet — send a message to start filling it.</div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .blank { padding: var(--s4); text-align: center; font-size: 11px; color: var(--text-muted); }

  .ctx { display: flex; flex-direction: column; min-height: 0; }
  .mono { font-family: "JetBrains Mono", monospace; }

  /* snapshot */
  .snap {
    padding: var(--s3); border-bottom: 1px solid var(--border-subtle);
    display: flex; flex-direction: column; gap: var(--s2);
  }
  .meter { display: flex; flex-direction: column; gap: 5px; }
  .meter .top { display: flex; justify-content: space-between; align-items: baseline; gap: var(--s2); }
  .meter .lab { font-size: 11px; font-weight: 500; color: var(--text-secondary); }
  .meter .val { font-size: 10.5px; color: var(--text-primary); }
  .meter .track {
    height: 6px; background: var(--bg-sink);
    border: 1px solid var(--border-subtle); border-radius: var(--r-full); overflow: hidden;
  }
  .meter .fill { height: 100%; background: var(--status-success); border-radius: var(--r-full); transition: width .25s ease; }
  .meter.warn .fill { background: var(--status-warning); }
  .meter.crit .fill { background: var(--status-error); }
  .under { display: flex; justify-content: space-between; align-items: baseline; gap: var(--s2); }
  .src { font-size: 10px; color: var(--text-muted); }
  .free { font-size: 10px; color: var(--text-muted); }

  .facts { display: flex; flex-direction: column; }
  .fr { display: flex; align-items: baseline; justify-content: space-between; gap: var(--s3); padding: 1px 0; }
  .fk { font-size: 10.5px; color: var(--text-muted); }
  .fv { font-size: 10px; color: var(--text-secondary); }

  .asleep { margin: 0; font-size: 10.5px; color: var(--text-muted); }

  /* categories */
  .cats { flex: 1; min-height: 0; overflow-y: auto; }
  .cat { border-bottom: 1px solid var(--border-subtle); }
  .cat:last-child { border-bottom: none; }
  .cat-head {
    display: flex; align-items: center; gap: var(--s2);
    width: 100%; padding: 7px var(--s3);
    background: none; border: none; cursor: pointer; text-align: left;
    font: inherit; color: var(--text-primary);
  }
  .cat-head:hover { background: var(--bg-raised); }
  .cat-head:focus-visible { outline: 2px solid var(--focus); outline-offset: -2px; }
  .chev { flex: none; color: var(--text-muted); transition: transform .18s ease; }
  .chev.open { transform: rotate(90deg); }
  .cat-label { flex: 1; min-width: 0; font-size: 12px; font-weight: 500; }
  .cat-count {
    flex: none; font-size: 9.5px; color: var(--text-muted);
    border: 1px solid var(--border-subtle); border-radius: var(--r-sm); padding: 0 5px;
  }
  .cat-tokens { flex: none; font-size: 10.5px; color: var(--accent-2); min-width: 40px; text-align: right; }

  .entries { display: flex; flex-direction: column; padding: 0 var(--s3) var(--s2) var(--s5); }
  .entry {
    display: flex; flex-direction: column; gap: 2px;
    width: 100%; padding: 4px 0; text-align: left; cursor: pointer;
    background: none; border: none; border-bottom: 1px solid var(--border-subtle);
    font: inherit;
  }
  .entry:last-child { border-bottom: none; }
  .entry:hover { background: var(--bg-raised); }
  .e-head { display: flex; align-items: center; gap: var(--s2); min-width: 0; }
  .e-label {
    flex: 1; min-width: 0; font-size: 11px; color: var(--text-secondary);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .e-badge {
    flex: none; font-size: 9px; text-transform: uppercase; letter-spacing: 0.04em;
    color: var(--text-muted); border: 1px solid var(--border-subtle);
    border-radius: var(--r-sm); padding: 0 4px;
  }
  .e-badge.err { color: var(--status-error); border-color: color-mix(in srgb, var(--status-error) 45%, transparent); }
  .e-tokens { flex: none; font-size: 9.5px; color: var(--text-muted); }
  .e-preview {
    font-size: 9.5px; color: var(--text-muted); line-height: 1.5;
    white-space: pre-wrap; word-break: break-word;
  }
</style>
