<script lang="ts">
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

  let items = $derived<DisplayItem[]>(agentContext?.live.items ?? []);
  let lastUsage = $derived<Usage | undefined>(agentContext?.live.lastUsage);
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
        label: "Shadow identity",
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
  let usageSource = $derived(lastUsage ? "Live telemetry" : "Estimated from restored content");
</script>

{#if agentContext === null}
  <p class="empty">No agent active.</p>
{:else}
  <div class="inspector">
    <div class="inspector-summary">
      <div class="summary-row">
        <span class="summary-label">Context Snapshot</span>
        <span class="summary-value">{formatTokens(liveContextTokens)} / {formatTokens(resolvedWindow)}</span>
      </div>
      <div class="summary-row sub">
        <span class="summary-label">Headroom</span>
        <span class="summary-value">{formatTokens(freeTokens)} free ({freePct}%)</span>
      </div>
      <div class="summary-row sub">
        <span class="summary-label">Source</span>
        <span class="summary-value">{usageSource}</span>
      </div>
      <div class="health-track" class:warning={healthState === "warning"} class:critical={healthState === "critical"}>
        <div class="health-fill" style:width={`${Math.max((1 - usedRatio) * 100, 0)}%`}></div>
      </div>
      {#if sessionStats && sessionStats.totalTokens > 0}
        <div class="summary-row sub">
          <span class="summary-label">Billing Total</span>
          <span class="summary-value">{formatTokens(sessionStats.totalTokens)} · ${sessionStats.totalCost.toFixed(4)}</span>
        </div>
      {/if}
      {#if sessionStats}
        <div class="summary-row sub">
          <span class="summary-label">Turns</span>
          <span class="summary-value">{sessionStats.turnCount}</span>
        </div>
        <div class="summary-row sub">
          <span class="summary-label">Messages</span>
          <span class="summary-value">{sessionStats.messageCount}</span>
        </div>
      {/if}
      {#if lastUsage && (lastUsage.cacheRead ?? 0) > 0}
        <div class="summary-row sub">
          <span class="summary-label">Cache Read</span>
          <span class="summary-value">{formatTokens(lastUsage.cacheRead ?? 0)}</span>
        </div>
      {/if}
      {#if lastUsage && (lastUsage.cacheWrite ?? 0) > 0}
        <div class="summary-row sub">
          <span class="summary-label">Cache Write</span>
          <span class="summary-value">{formatTokens(lastUsage.cacheWrite ?? 0)}</span>
        </div>
      {/if}
    </div>

    <div class="inspector-categories">
      {#each categories as category (category.id)}
        <div class="category">
          <button class="category-header" onclick={() => toggleCategory(category.id)}>
            <span class="category-chevron">{expandedCategories[category.id] ? "▾" : "▸"}</span>
            <span class="category-label">{category.label}</span>
            <span class="category-count">{category.entries.length}</span>
            <span class="category-tokens">{formatTokens(category.tokens)}</span>
          </button>

          {#if expandedCategories[category.id]}
            <div class="category-entries">
              {#each category.entries as entry (entry.id)}
                <button class="entry" onclick={() => toggleEntry(entry.id)}>
                  <div class="entry-header">
                    <span class="entry-label">{entry.label}</span>
                    <div class="entry-meta">
                      {#if entry.meta}
                        <span class="entry-badge">{entry.meta}</span>
                      {/if}
                      <span class="entry-tokens">{formatTokens(entry.tokens)}</span>
                    </div>
                  </div>
                  <div class="entry-preview">{expandedEntries[entry.id] ? entry.fullText : entry.preview}</div>
                </button>
              {/each}
            </div>
          {/if}
        </div>
      {/each}

      {#if categories.length === 0}
        <div class="empty-categories">No useful context has been captured yet.</div>
      {/if}
    </div>

    <div class="inspector-footer">
      <span class="footer-note">
        Category sizes are estimated from visible content so you can compare what is actually taking up space.
      </span>
    </div>
  </div>
{/if}

<style>
  .empty {
    margin: 0;
    color: var(--text-muted);
    font-size: 11px;
    font-style: italic;
  }

  .inspector {
    display: flex;
    flex-direction: column;
    gap: 0;
    margin: -12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    font-size: 11px;
    color: var(--text-secondary);
  }

  .inspector-summary {
    padding: 12px 14px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .summary-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 6px;
  }

  .summary-row.sub {
    margin-bottom: 2px;
  }

  .summary-label {
    color: var(--text-muted);
    font-size: 10px;
  }

  .summary-value {
    color: var(--text-secondary);
    font-size: 10px;
  }

  .summary-row.sub .summary-label,
  .summary-row.sub .summary-value {
    font-size: 9px;
  }

  .health-track {
    position: relative;
    height: 4px;
    border-radius: 2px;
    overflow: hidden;
    background: var(--active-overlay);
    margin-bottom: 8px;
  }

  .health-fill {
    height: 100%;
    border-radius: 2px;
    background:
      linear-gradient(90deg, color-mix(in srgb, var(--success) 76%, white 10%), var(--success));
    transition: width 0.25s ease, background 0.2s ease;
  }

  .health-track.warning .health-fill {
    background:
      linear-gradient(90deg, color-mix(in srgb, var(--warning) 88%, white 10%), var(--warning));
  }

  .health-track.critical .health-fill {
    background:
      linear-gradient(90deg, color-mix(in srgb, var(--error) 78%, white 8%), var(--error));
  }

  .inspector-categories {
    padding: 8px 0;
  }

  .category {
    border-bottom: 1px solid var(--subtle-divider);
  }

  .category-header {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 8px 14px;
    background: none;
    border: none;
    color: var(--text-secondary);
    font-family: inherit;
    font-size: 11px;
    cursor: pointer;
    text-align: left;
    transition: background 0.12s ease;
  }

  .category-header:hover {
    background: var(--hover-overlay);
  }

  .category-chevron {
    font-size: 9px;
    width: 10px;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .category-label {
    flex: 1;
    min-width: 0;
  }

  .category-count {
    font-size: 9px;
    color: var(--text-muted);
    padding: 1px 5px;
    border-radius: 3px;
    background: var(--hover-overlay);
    flex-shrink: 0;
  }

  .category-tokens {
    font-size: 10px;
    color: color-mix(in srgb, var(--success) 72%, white 12%);
    flex-shrink: 0;
    min-width: 40px;
    text-align: right;
  }

  .category-entries {
    padding: 0 14px 8px 30px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .entry {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    padding: 4px 0;
    border: none;
    border-bottom: 1px solid var(--subtle-divider);
    background: transparent;
    color: inherit;
    font-family: inherit;
    cursor: pointer;
    text-align: left;
    transition: background 0.1s;
  }

  .entry:hover {
    background: var(--hover-overlay);
  }

  .entry:last-child {
    border-bottom: none;
  }

  .entry-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .entry-label {
    font-size: 10px;
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
  }

  .entry-meta {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .entry-badge {
    padding: 2px 6px;
    border-radius: 3px;
    background: var(--active-overlay);
    color: var(--text-muted);
    font-size: 9px;
  }

  .entry-tokens {
    font-size: 9px;
    color: var(--text-muted);
  }

  .entry-preview {
    font-size: 9px;
    color: var(--text-muted);
    line-height: 1.4;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .empty-categories {
    padding: 24px 14px;
    text-align: center;
    color: var(--text-muted);
    font-size: 10px;
  }

  .inspector-footer {
    padding: 8px 14px;
    border-top: 1px solid var(--border-subtle);
  }

  .footer-note {
    font-size: 9px;
    color: var(--text-muted);
    line-height: 1.5;
  }
</style>
